//! Top-level orchestration: ties the display backend, the compositor
//! adapter, and the persistent state file together.

use log;
use std::{collections::HashSet, time::Duration};

use crate::{
    Result,
    backend::{CreateOutcome, DisplayBackend, DisplayHandle, pick_backend},
    compositor::{CompositorAdapter, EnableOutput, ModeInfo, OutputPlan, OutputSnapshot},
    edid::EdidSpec,
    state::{ActiveState, InstanceRecord, LayoutChanges, StateStore},
};

#[cfg(feature = "kde")]
use crate::compositor::kde::KdeCompositor;

#[cfg(feature = "gnome")]
use crate::compositor::gnome::GnomeCompositor;

/// How long to wait for the kernel hot-plug to surface as a Wayland head.
const HEAD_APPEAR_TIMEOUT: Duration = Duration::from_secs(1);

/// Try each adapter in order and return the first one that connects.
fn connect_compositor() -> Option<Box<dyn CompositorAdapter>> {
    #[cfg(feature = "kde")]
    {
        match KdeCompositor::connect() {
            Ok(Some(c)) => return Some(Box::new(c)),
            Ok(None) => log::debug!("kde_output_management_v2 not advertised"),
            Err(e) => log::warn!("kde connect failed: {e:#}"),
        }
    }
    #[cfg(feature = "gnome")]
    {
        match GnomeCompositor::connect() {
            Ok(Some(c)) => return Some(Box::new(c)),
            Ok(None) => log::debug!("org.gnome.Mutter.DisplayConfig not available"),
            Err(e) => log::warn!("gnome connect failed: {e:#}"),
        }
    }
    None
}

/// True if `name` looks like a hardware connector we hould treat as "real"
/// Used to decide which heads `--disable-real-outputs`
/// should turn off.
fn is_real_output(name: &str) -> bool {
    matches!(
        name.split('-').next().unwrap_or(""),
        "DP" | "HDMI" | "eDP" | "DVI" | "VGA" | "DSI" | "LVDS"
    )
}

/// Caller-supplied parameters for `up`. The lifecycle layer translates
/// these into a backend create + a compositor plan.
#[derive(Debug, Clone)]
pub struct UpRequest {
    pub spec: EdidSpec,
    /// Mark the new fauxput head as the compositor's primary output.
    pub make_primary: bool,
    /// Turn off every real output while the fauxput head is active.
    /// Useful for streaming workflows that want windows to land on the
    /// virtual display by default.
    pub disable_real_outputs: bool,
}

impl UpRequest {
    /// Map the requested resolution + refresh into the compositor's `ModeInfo`
    fn mode(&self) -> ModeInfo {
        ModeInfo {
            width: self.spec.width as i32,
            height: self.spec.height as i32,
            refresh_mhz: (self.spec.refresh_hz as i32) * 1000,
        }
    }

    /// Build the enable request for this spec at the given position.
    fn as_enable(&self, name: &str, position: (i32, i32)) -> EnableOutput {
        EnableOutput {
            name: name.to_string(),
            mode: Some(self.mode()),
            position: Some(position),
        }
    }
}

/// What `up` produced, including any partial-success signals the CLI should warn us about
#[derive(Debug)]
pub struct UpOutcome {
    pub handle: DisplayHandle,
    pub edid_applied: bool,
    pub compositor_configured: bool,
    pub compositor_position: Option<(i32, i32)>,
}

/// Orchestrator for the `up` flow.
struct Up<'a> {
    req: &'a UpRequest,
    backend: Box<dyn DisplayBackend>,
    store: StateStore,
    compositor: Option<Box<dyn CompositorAdapter>>,
    /// Compositor layout taken before the kernel-side create.
    pre_create: OutputSnapshot,
}

impl<'a> Up<'a> {
    pub fn run(req: &'a UpRequest) -> Result<UpOutcome> {
        let backend = pick_backend();
        // Fast-fail before touching state: if configfs isn't mounted or
        // vkms isn't loaded, bail here so we don't write a state record
        // we can't act on later.
        backend.check_available()?;
        let store = StateStore::new();

        let mut compositor = connect_compositor();
        // Snapshot before the kernel-side create
        let pre_create = compositor
            .as_mut()
            .and_then(|c| c.snapshot().ok())
            .unwrap_or_default();

        let mut up = Self {
            req,
            backend,
            store,
            compositor,
            pre_create,
        };

        // attempt to create the kernel hot-plug
        let outcome = up.create_kernel_side()?;

        let mut compositor_configured = false;
        let mut compositor_position = None;
        if up.compositor.is_some() {
            match up.attach_compositor(&outcome.handle) {
                Ok(pos) => {
                    compositor_configured = true;
                    compositor_position = Some(pos);
                }
                // Compositor failure here doesn't undo the kernel-side success
                // warn and return the partial outcome so the
                // CLI can tell us what to fix manually.
                Err(e) => {
                    log::warn!("failed to configure compositor: {e:#}");
                }
            }
        }

        // The compositor_* fields and edid_applied let the CLI report
        // which parts of the request actually took effect.
        Ok(UpOutcome {
            handle: outcome.handle,
            edid_applied: outcome.feature_acceptance.edid_applied,
            compositor_configured,
            compositor_position,
        })
    }

    pub fn create_kernel_side(&mut self) -> Result<CreateOutcome> {
        let outcome = self.backend.create(&self.req.spec)?;

        // Persist the record before we return so a crash anywhere downstream
        // still leaves `down`/`reset` enough to find the kernel-side instance.
        self.store.push_instance(InstanceRecord {
            handle: outcome.handle.clone(),
            compositor_head_name: None,
            spec: self.req.spec.clone(),
            compositor_snapshot: Some(self.pre_create.clone()),
            compositor_configured: false,
            layout_changes: LayoutChanges::default(),
        })?;

        Ok(outcome)
    }

    pub fn attach_compositor(&mut self, handle: &DisplayHandle) -> Result<(i32, i32)> {
        let (compositor_name, pos) = self.place_new_head()?;

        log::info!(
            "compositor identified new output as {compositor_name:?}, slug {:?}",
            handle.local_id
        );

        // Swallow partial-apply failures: the head is up and recorded, and
        // the user can manually fix or `down` to clean up. Bubbling the
        // error here would suggest the whole `up` failed when it didn't.
        let layout_changes = self
            .apply_layout(&compositor_name, pos)
            .unwrap_or_else(|e| {
                log::warn!("layout partially applied: {e:#}");
                LayoutChanges::default()
            });

        self.store.update_instance(&handle.local_id, |rec| {
            rec.compositor_head_name = Some(compositor_name);
            rec.compositor_configured = true;
            rec.layout_changes = layout_changes;
        })?;

        Ok(pos)
    }

    pub fn place_new_head(&mut self) -> Result<(String, (i32, i32))> {
        let comp = self.compositor.as_mut().expect("compositor must be Some");

        // Anything not in the pre-create snapshot must be the head we just
        // created; this filters out the existing real outputs.
        let baseline_names: HashSet<String> = self
            .pre_create
            .heads
            .iter()
            .map(|h| h.name.clone())
            .collect();
        let new_head = comp.wait_for_new_head(&baseline_names, HEAD_APPEAR_TIMEOUT)?;
        let head_name = new_head.name.clone();

        // Append to the right of the existing layout so we never overlap
        // a real output and confuse the compositor's window placement.
        let max_x = self
            .pre_create
            .heads
            .iter()
            .filter(|h| h.enabled)
            // skip heads missing position or mode; we can't compute their
            // right edge so they contribute nothing to the placement
            .filter_map(|h| {
                let (x, _) = h.position?;
                let mode = h.mode?;
                Some(x + mode.width)
            })
            .max()
            .unwrap_or(0);

        // First apply just enables the head at its slot. The full layout
        // pass (disable real outputs, set primary) happens in apply_layout
        // once the compositor has acknowledged the head exists.
        let mut enable = self.pre_create.active_enables();
        enable.push(self.req.as_enable(&head_name, (max_x, 0)));

        let plan = OutputPlan::builder().enable_all(enable)?.build();
        comp.apply(&plan)?;

        Ok((head_name, (max_x, 0)))
    }

    pub fn apply_layout(
        &mut self,
        new_head_name: &str,
        new_head_position: (i32, i32),
    ) -> Result<LayoutChanges> {
        // Best-effort guess at "the previous primary" so `down` can restore it.
        let previous_primary = if self.req.make_primary {
            self.pre_create
                .heads
                .iter()
                .find(|h| h.enabled && is_real_output(&h.name))
                .map(|h| h.name.clone())
        } else {
            None
        };

        // Names of every currently-on real output.
        let disabled_outputs: Vec<String> = if self.req.disable_real_outputs {
            self.pre_create
                .heads
                .iter()
                .filter(|h| h.enabled && is_real_output(&h.name))
                .map(|h| h.name.clone())
                .collect()
        } else {
            Vec::new()
        };

        // Drop the disabled outputs from the enable list so we don't tell
        // the compositor to enable and disable the same head in one plan.
        let mut enable: Vec<EnableOutput> = self
            .pre_create
            .active_enables()
            .into_iter()
            .filter(|e| !disabled_outputs.contains(&e.name))
            .collect();

        enable.push(self.req.as_enable(new_head_name, new_head_position));

        let mut builder = OutputPlan::builder()
            .enable_all(enable)?
            .disable_all(disabled_outputs.iter().cloned())?;

        if self.req.make_primary {
            builder = builder.set_primary(new_head_name.to_string())?;
        }

        let plan = builder.build();

        let comp = self
            .compositor
            .as_mut()
            .expect("apply_layout only called from attach_compositor");

        // warn if a feature was requested that the compositor doesn't support
        for kind in plan.unsupported_by(comp.as_ref()) {
            log::warn!(
                "compositor `{}` does not support `{}`. Ignoring.",
                comp.name(),
                kind
            )
        }

        comp.apply(&plan)?;

        Ok(LayoutChanges {
            disabled_outputs,
            previous_primary,
        })
    }
}

/// Compositor is optional so a `down` still removes kernel-side instances
/// when run from outside the Wayland session.
struct Down {
    backend: Box<dyn DisplayBackend>,
    store: StateStore,
    compositor: Option<Box<dyn CompositorAdapter>>,
}

impl Down {
    pub fn run() -> Result<usize> {
        let mut down = Self {
            backend: pick_backend(),
            store: StateStore::new(),
            compositor: connect_compositor(),
        };

        let state = down.store.load()?;
        let mut removed: usize = 0;

        // Newest-first so layout restoration unwinds in the order `up`s
        // applied their changes.
        for rec in state.instances.iter().rev() {
            down.restore_compositor(rec);
            if down.backend.destroy(&rec.handle).is_ok() {
                removed += 1
            }
        }

        down.store.clear()?;
        Ok(removed)
    }

    fn restore_compositor(&mut self, rec: &InstanceRecord) {
        if !rec.compositor_configured {
            return;
        }

        let Some(comp) = self.compositor.as_mut() else {
            return;
        };

        // Prefer current geometry over the stored snapshot: modes and
        // positions may have shifted since `up`, and stale values are
        // more likely to produce a plan the compositor rejects.
        let live = comp.snapshot().ok();

        let mut enables = Vec::new();
        for name in &rec.layout_changes.disabled_outputs {
            // Look up this name in the live snapshot, if there is one
            // missing live data leaves mode/position empty and lets the
            // compositor fall back to its own defaults.
            let live_head = live
                .as_ref()
                .and_then(|s| s.heads.iter().find(|h| h.name == *name));
            enables.push(EnableOutput {
                name: name.clone(),
                mode: live_head.and_then(|h| h.mode),
                position: live_head.and_then(|h| h.position),
            });
        }

        // kernel slug is a workable fallback target
        let compositor_head_name = rec
            .compositor_head_name
            .clone()
            .unwrap_or_else(|| rec.handle.local_id.clone());

        // Wrap in an inline closure so `?` lands in this local Result
        // we want to warn and keep going, not abort the whole down run
        // Seems a bit hacky, but it works.
        let plan: Result<OutputPlan> = (|| {
            let mut builder = OutputPlan::builder()
                .enable_all(enables)?
                .disable(compositor_head_name)?;
            if let Some(name) = &rec.layout_changes.previous_primary {
                builder = builder.set_primary(name.clone())?;
            }
            Ok(builder.build())
        })();

        // Plan or apply failure here doesn't block the kernel-side destroy
        // warn so the user sees what happened and move on
        let plan = match plan {
            Ok(plan) => plan,
            Err(e) => {
                log::warn!("down: invalid plan for {}: {e:#}", rec.handle.local_id);
                return;
            }
        };

        if let Err(e) = comp.apply(&plan) {
            log::warn!(
                "layout restore & graceful disable failed for {}: {e:#}",
                rec.handle.local_id
            );
        }
    }
}

/// Walks both the state file and the configfs root, so a wedged previous
/// run that never made it to push_instance still gets cleaned up.
struct Reset {
    backend: Box<dyn DisplayBackend>,
    store: StateStore,
}

impl Reset {
    pub fn run() -> Result<usize> {
        let reset = Self {
            backend: pick_backend(),
            store: StateStore::new(),
        };

        let mut removed: usize = 0;

        // Default-on-load lets reset proceed even when the state file is
        // missing or unreadable
        let state = reset.store.load().unwrap_or_default();
        for rec in state.instances.iter().rev() {
            if reset.backend.destroy(&rec.handle).is_ok() {
                removed += 1;
            }
        }

        // Catches instances the state file didn't name
        for handle in reset.backend.list().unwrap_or_default() {
            if reset.backend.destroy(&handle).is_ok() {
                removed += 1;
            }
        }

        reset.store.clear()?;

        Ok(removed)
    }
}

// simple wrappers around methods for public interface
pub fn up(req: &UpRequest) -> Result<UpOutcome> {
    Up::run(req)
}

pub fn down() -> Result<usize> {
    Down::run()
}

pub fn reset() -> Result<usize> {
    Reset::run()
}

pub fn status() -> Result<ActiveState> {
    StateStore::new().load()
}
