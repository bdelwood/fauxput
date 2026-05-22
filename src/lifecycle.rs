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

/// True if `name` looks like a hardware connector we should treat as "real"
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
    /// Map the requested resolution + refresh into the compositor's `ModeInfo`.
    /// Refresh is the exact value the EDID encodes
    fn mode(&self) -> ModeInfo {
        ModeInfo {
            width: self.spec.width as i32,
            height: self.spec.height as i32,
            refresh_mhz: self.spec.refresh_mhz(),
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
    /// True iff `--hdr` was set AND the backend wired up the kernel correctly
    pub hdr_properties_attached: bool,
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

        let compositor = connect_compositor();

        let mut up = Self {
            req,
            backend,
            store,
            compositor,
            // Filled in by `create_kernel_side` immediately before backend create
            pre_create: OutputSnapshot::default(),
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
            hdr_properties_attached: outcome.feature_acceptance.hdr_applied,
            compositor_configured,
            compositor_position,
        })
    }

    pub fn create_kernel_side(&mut self) -> Result<CreateOutcome> {
        // Snapshot the compositor immediately before the kernel-side create
        // so the "new heads" diff against this baseline isn't poisoned by
        // possibly unrelated state shifts
        self.pre_create = self
            .compositor
            .as_mut()
            .and_then(|c| c.snapshot().ok())
            .unwrap_or_default();

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
        let (compositor_name, initial_pos) = self.place_new_head()?;

        // Place at the origin: otherwise the cursor gets pinned off-screen,
        // at the coordinates the disabled real outputs used to occupy.
        // Normal apps don't notice:
        // clicks still land where the visible cursor is
        // but for full screen apps like games which use `zwp_locked_pointer_v1`, they refuse to engage when the
        // cursor sits outside the requesting surface.
        let pos = if self.req.disable_real_outputs {
            (0, 0)
        } else {
            initial_pos
        };

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

        // One compositor restore covering all instances.
        // Found that per-instance restores would each
        // disable a single fauxput head and leave the
        // remaining ones at their original positions
        // which caused gaps that made mutter mad.
        if !state.instances.is_empty() {
            restore_compositor(&mut down.compositor, &state.instances);
        }

        // Kernel-side destroys can happen in any order now that the
        // compositor's view has been settled in a single apply.
        // just do newest first.
        for rec in state.instances.iter().rev() {
            if down.backend.destroy(&rec.handle).is_ok() {
                removed += 1
            }
        }

        down.store.clear()?;
        Ok(removed)
    }
}

/// Build one combined plan that disables every fauxput head, re-enables
/// the union of real outputs that any instance had disabled, and sets
/// primary back to the original primary.
fn restore_compositor(
    compositor: &mut Option<Box<dyn CompositorAdapter>>,
    instances: &[InstanceRecord],
) {
    let Some(comp) = compositor.as_mut() else {
        return;
    };

    // Every fauxput head we ever brought up.
    let to_disable: Vec<String> = instances
        .iter()
        .filter(|r| r.compositor_configured)
        .map(|r| {
            r.compositor_head_name
                .clone()
                .unwrap_or_else(|| r.handle.local_id.clone())
        })
        .collect();

    // Union of every real output that we disable across any instance.
    // Need to dedupe if multiple instances disabled the same output.
    let to_reenable: HashSet<String> = instances
        .iter()
        .flat_map(|r| r.layout_changes.disabled_outputs.iter().cloned())
        .collect();

    // The oldest instance's `previous_primary` represents what was
    // primary before any action.
    let previous_primary = instances
        .iter()
        .find_map(|r| r.layout_changes.previous_primary.clone());

    // Take snapshot and restore back-to-back.
    let original_snapshot: Option<&OutputSnapshot> = instances
        .iter()
        .find_map(|r| r.compositor_snapshot.as_ref());
    let live = comp.snapshot().ok();
    let enables: Vec<EnableOutput> = to_reenable
        .into_iter()
        .map(|name| {
            let snap_head = original_snapshot.and_then(|s| s.heads.iter().find(|h| h.name == name));
            let live_head = live
                .as_ref()
                .and_then(|s| s.heads.iter().find(|h| h.name == name));
            EnableOutput {
                name,
                mode: snap_head
                    .and_then(|h| h.mode)
                    .or_else(|| live_head.and_then(|h| h.mode)),
                position: snap_head
                    .and_then(|h| h.position)
                    .or_else(|| live_head.and_then(|h| h.position)),
            }
        })
        .collect();

    // Two-phase apply.
    if !enables.is_empty() || previous_primary.is_some() {
        let plan: Result<OutputPlan> = (|| {
            let mut builder = OutputPlan::builder().enable_all(enables)?;
            if let Some(name) = previous_primary {
                builder = builder.set_primary(name)?;
            }
            Ok(builder.build())
        })();
        match plan {
            Ok(plan) => {
                if let Err(e) = comp.apply(&plan) {
                    log::warn!("cleanup: phase 1 (re-enable real outputs) failed: {e:#}");
                }
            }
            Err(e) => log::warn!("cleanup: invalid phase 1 plan: {e:#}"),
        }
    }

    let plan: Result<OutputPlan> = (|| {
        Ok(OutputPlan::builder()
            .disable_all(to_disable.iter().cloned())?
            .build())
    })();
    match plan {
        Ok(plan) => {
            if let Err(e) = comp.apply(&plan) {
                log::warn!("cleanup: phase 2 (disable fauxput heads) failed: {e:#}");
            }
        }
        Err(e) => log::warn!("cleanup: invalid phase 2 plan: {e:#}"),
    }
}

/// Walks both the state file and the configfs root, so a wedged previous
/// run that never made it to push_instance still gets cleaned up.
struct Reset {
    backend: Box<dyn DisplayBackend>,
    store: StateStore,
    compositor: Option<Box<dyn CompositorAdapter>>,
}

impl Reset {
    pub fn run() -> Result<usize> {
        let reset = Self {
            backend: pick_backend(),
            store: StateStore::new(),
            compositor: connect_compositor(),
        };
        let mut reset = reset;

        let mut removed: usize = 0;

        // Default-on-load lets reset proceed even when the state file is
        // missing or unreadable
        let state = reset.store.load().unwrap_or_default();
        if !state.instances.is_empty() {
            restore_compositor(&mut reset.compositor, &state.instances);
        }
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
