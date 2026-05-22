//! KDE/Plasma `kde_output_management_v2` adapter.

use std::collections::HashSet;

use indexmap::IndexMap;
use std::time::Duration;

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, protocol::wl_registry};
use wayland_protocols_plasma::output_device::v2::client::{
    kde_output_device_mode_v2::{self, KdeOutputDeviceModeV2},
    kde_output_device_registry_v2::{self, KdeOutputDeviceRegistryV2},
    kde_output_device_v2::{self, KdeOutputDeviceV2},
};
use wayland_protocols_plasma::output_management::v2::client::{
    kde_output_configuration_v2::{self, KdeOutputConfigurationV2},
    kde_output_management_v2::KdeOutputManagementV2,
};

use crate::Result;
use crate::compositor::wayland::WaylandSession;
use crate::compositor::{
    CompositorAdapter, CompositorError, FeatureKind, HeadState, ModeInfo, OutputMode, OutputPlan,
    OutputSnapshot, Transform,
};

// polls will wait forever
// we're not doing anything fancy, timeout after short time
const APPLY_TIMEOUT: Duration = Duration::from_secs(5);

const MGMT_VERSION: u32 = 19;
const DEVICE_VERSION: u32 = 20;
const DEVICE_REGISTRY_VERSION: u32 = 23;

pub struct KdeCompositor {
    session: WaylandSession<State>,
    manager: KdeOutputManagementV2,
}

impl KdeCompositor {
    /// connect to compositor
    pub fn connect() -> Result<Option<Self>> {
        let Some(mut session) = WaylandSession::<State>::connect_to_env()? else {
            return Ok(None);
        };
        let _ = session.registry();

        session.roundtrip("kde: registry")?;

        // if none, we're not wayland
        let Some(manager) = session.state.manager.take() else {
            return Ok(None);
        };

        // Fail loud if we're missing protocols
        if session.state.device_registry.is_none() && session.state.devices.is_empty() {
            return Err(CompositorError::MissingProtocol {
                compositor: "KWin",
                manager: "kde_output_management_v2",
                expected: "kde_output_device_registry_v2 (Plasma 6.7+) or \
                           kde_output_device_v2 (Plasma ≤6.6)",
                hint: "kde_output_",
            }
            .into());
        }

        session.state.manager_alive = true;

        // get initial device event burst
        session.roundtrip("kde: device burst")?;

        Ok(Some(Self { session, manager }))
    }

    /// Submits a configuration to KWin and blocks until it succeeds or
    /// fails. Skips the round-trip entirely when the populate closure
    /// reports no changes.
    fn submit<F>(&mut self, ctx: &'static str, populate: F) -> Result<()>
    where
        F: FnOnce(&KdeOutputConfigurationV2, &State) -> Result<usize>,
    {
        // pass in empty userdata; don't need it
        let cfg = self
            .manager
            .create_configuration(self.session.qhandle(), ());
        let touched = populate(&cfg, &self.session.state)?;

        // case where we have nothing to commit.
        if touched == 0 {
            cfg.destroy();
            return Ok(());
        }

        // Reset the result slot before triggering apply
        // the configuration object's Dispatch impl will fill it in from the server reply.
        self.session.state.apply_result = ApplyResult::Pending;
        self.session.state.pending_failure_reason = None;
        cfg.apply();
        self.session.flush(ctx)?;

        // Poll for a non-pending result, bailing early if the manager
        // global disappears mid-wait.
        let result =
            self.session
                .poll_until(format!("kde apply ({ctx})"), APPLY_TIMEOUT, |session| {
                    session.roundtrip(ctx)?;
                    // probably overly defensive, but things happen
                    if !session.state.manager_alive {
                        return Err(CompositorError::CompositorWentAway.into());
                    }
                    Ok(match session.state.apply_result {
                        ApplyResult::Pending => None,
                        _ => Some(std::mem::take(&mut session.state.apply_result)),
                    })
                });

        // KWin won't accept a second apply on the same configuration object,
        //  so destroy it regardless of how the poll resolved.
        cfg.destroy();

        match result? {
            ApplyResult::Applied => Ok(()),
            ApplyResult::Failed { reason } => Err(CompositorError::ApplyFailed { reason }.into()),
            ApplyResult::Pending => unreachable!(),
        }
    }
}

impl CompositorAdapter for KdeCompositor {
    fn name(&self) -> &'static str {
        "kde"
    }

    /// Features KWin natively honors
    fn supported_features(&self) -> HashSet<FeatureKind> {
        HashSet::from([FeatureKind::Primary])
    }

    /// Force a fresh roundtrip so
    ///  the snapshot reflects KWin's current state
    fn snapshot(&mut self) -> Result<OutputSnapshot> {
        self.session.roundtrip("kde: snapshot refresh")?;
        Ok(OutputSnapshot {
            heads: self.session.state.live_heads(),
        })
    }

    /// Polls until a head not in `baseline` appears. KWin advertises new
    /// connectors asynchronously after the kernel hot-plug.
    fn wait_for_new_head(
        &mut self,
        baseline: &HashSet<String>,
        timeout: Duration,
    ) -> Result<HeadState> {
        let start = std::time::Instant::now();
        self.session.poll_until("new kde head", timeout, |session| {
            session.roundtrip("kde: head poll")?;
            let heads = session.state.live_heads();
            let names: Vec<String> = heads.iter().map(|h| h.name.clone()).collect();
            let new = heads
                .into_iter()
                .find(|head| !baseline.contains(&head.name));
            log::debug!(
                "kde head poll t={}ms heads={:?} new={:?}",
                start.elapsed().as_millis(),
                names,
                new.as_ref().map(|h| h.name.as_str()),
            );
            Ok(new)
        })
    }

    fn apply(&mut self, plan: &OutputPlan) -> Result<()> {
        // Refresh state before computing the diff so we're working with
        // KWin's current view of the world.
        self.session.roundtrip("kde: apply refresh")?;
        let live = self.session.state.live_heads();
        let plan = plan.clone();

        // KDE's `set_priority` is 1-indexed
        // 0 is the unranked sentinel
        let primary_name = plan.primary();
        let mut next: u32 = if primary_name.is_some() { 2 } else { 1 };
        let mut priorities = IndexMap::with_capacity(live.len());
        for head in &live {
            let priority = if Some(head.name.as_str()) == primary_name {
                1
            } else {
                let p = next;
                next += 1;
                p
            };
            priorities.insert(head.name.clone(), priority);
        }

        self.submit("apply", move |cfg, state| {
            let mut touched = 0;

            // Walk every live head and write only the properties that
            // actually changed. KWin treats unmentioned heads as "leave
            // alone" for most properties.
            for head in &live {
                // skip heads we don't have a bound proxy for
                let Some(proxy) = state.device_by_name(&head.name) else {
                    continue;
                };
                let target = Target::for_head(head, &plan);

                // enable/disable diff
                if target.enabled != head.enabled {
                    cfg.enable(&proxy, target.enabled.into());
                    touched += 1;
                }

                // mode diff
                // skipped if no matching mode proxy is on file
                if target.enabled
                    && let Some(mode) = target.mode
                    && Some(mode) != head.mode
                    && let Some(mp) = state.find_mode_proxy(&head.name, mode)
                {
                    cfg.mode(&proxy, &mp);
                    touched += 1;
                }

                // position diff
                if target.enabled
                    && let Some(pos) = target.position
                    && Some(pos) != head.position
                {
                    cfg.position(&proxy, pos.0, pos.1);
                    touched += 1;
                }

                // priority resets to 0 unless re-asserted on every apply
                cfg.set_priority(&proxy, priorities[&head.name]);
                touched += 1;
            }
            Ok(touched)
        })
    }
}

/// What a single head should look like after the plan is applied.
struct Target {
    enabled: bool,
    mode: Option<ModeInfo>,
    position: Option<(i32, i32)>,
}

impl Target {
    fn for_head(head: &HeadState, plan: &OutputPlan) -> Self {
        let enable_entry = plan.enables.iter().find(|entry| entry.name == head.name);
        let disabled = plan.disables.iter().any(|name| name == &head.name);
        Self {
            // Disable wins outright; otherwise an explicit enable forces the
            // head on, and unmentioned heads keep whatever they had.
            enabled: !disabled && (enable_entry.is_some() || head.enabled),
            // Plan-supplied mode/position override the live values
            // absent fields fall back to what the compositor reported.
            mode: enable_entry.and_then(|entry| entry.mode).or(head.mode),
            position: enable_entry
                .and_then(|entry| entry.position)
                .or(head.position),
        }
    }
}

/// Per-session bookkeeping. The wayland event loop mutates this in response
/// to server messages
/// the adapter reads it to compute snapshots and to
/// observe the result of a submitted configuration.
#[derive(Default)]
struct State {
    /// Bound manager proxy.
    /// Taken out of the option once `connect` has claimed it
    manager: Option<KdeOutputManagementV2>,
    /// Numeric registry name for the manager.
    manager_registry_name: Option<u32>,
    /// Sticky liveness flag.
    manager_alive: bool,
    /// Bound output-device registry proxy (Plasma 6.7+).
    device_registry: Option<KdeOutputDeviceRegistryV2>,
    /// Numeric registry name for the device registry.
    device_registry_name: Option<u32>,
    /// Devices keyed by their wayland protocol id.
    devices: IndexMap<u32, Device>,
    /// Modes keyed by protocol id, paired with the parent device id
    modes: IndexMap<u32, (u32, ModeData)>,
    /// Result of the most recent `apply()`
    apply_result: ApplyResult,
    /// Optional reason for an apply failure. The protocol may send this
    /// before the `Failed` event.
    pending_failure_reason: Option<String>,
}

#[derive(Default)]
struct Device {
    registry_name: Option<u32>,
    proxy: Option<KdeOutputDeviceV2>,
    name: Option<String>,
    enabled: bool,
    current_mode: Option<u32>,
    position: Option<(i32, i32)>,
    scale: Option<f64>,
    transform: Option<Transform>,
    // to prevent races against initialization
    done_seen: bool,
}

type ModeData = OutputMode<KdeOutputDeviceModeV2>;

/// Outcome slot for a single apply round-trip.
#[derive(Default, PartialEq, Eq)]
enum ApplyResult {
    #[default]
    Pending,
    Applied,
    Failed {
        reason: Option<String>,
    },
}

impl State {
    /// Projects every device that has finished its initial event burst into
    /// a public state of the head.
    fn live_heads(&self) -> Vec<HeadState> {
        self.devices
            .values()
            // skip devices KWin hasn't finished describing yet
            .filter(|device| device.name.is_some() && device.done_seen)
            // resolve each into the public shape with its mode dereffed
            .map(|device| self.head(device))
            .collect()
    }

    /// Snapshot a single device into the public head shape, resolving the
    /// `current_mode` id back to its `ModeInfo`.
    fn head(&self, device: &Device) -> HeadState {
        let mode = device
            .current_mode
            // dereference the id into the modes table
            .and_then(|id| self.modes.get(&id))
            // discard the parent-id pairing, keep just the ModeInfo
            .map(|(_, mode_data)| mode_data.info);

        HeadState {
            name: device.name.clone().unwrap_or_default(),
            enabled: device.enabled,
            mode,
            position: device.position,
            scale: device.scale,
            transform: device.transform,
        }
    }

    /// Look up a device proxy by the connector name KWin advertised.
    fn device_by_name(&self, name: &str) -> Option<KdeOutputDeviceV2> {
        self.devices
            .values()
            // find the device whose name matches
            .find(|device| device.name.as_deref() == Some(name))
            // hand back its bound proxy so the caller can issue requests on it
            .and_then(|device| device.proxy.clone())
    }

    /// Find the mode proxy on a given device whose dimensions and refresh
    /// match `want`.
    /// Exact equality on refresh since we compute it self-consistently throughout
    fn find_mode_proxy(&self, device_name: &str, want: ModeInfo) -> Option<KdeOutputDeviceModeV2> {
        // resolve the connector name into the device's protocol id
        let device_id = self
            .devices
            .iter()
            .find(|(_, device)| device.name.as_deref() == Some(device_name))
            .map(|(id, _)| *id)?;

        let matches_device = |parent: &u32| *parent == device_id;
        let matches_dims = |info: &ModeInfo| info.width == want.width && info.height == want.height;

        self.modes
            .iter()
            .find(|(_, (parent, mode_data))| {
                matches_device(parent)
                    && matches_dims(&mode_data.info)
                    && mode_data.info.refresh_mhz == want.refresh_mhz
            })
            .or_else(|| {
                self.modes
                    .iter()
                    .filter(|(_, (parent, mode_data))| {
                        matches_device(parent) && matches_dims(&mode_data.info)
                    })
                    .min_by_key(|(_, (_, mode_data))| {
                        (mode_data.info.refresh_mhz - want.refresh_mhz).abs()
                    })
            })
            .and_then(|(_, (_, mode_data))| mode_data.proxy.clone())
    }
}

/// Binds the manager and per-output globals as KWin advertises them, and
/// reacts to globals disappearing mid-session.
impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: <wl_registry::WlRegistry as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                // The protocol entry point. Bound once and reused for
                // every `submit` to build configuration objects.
                "kde_output_management_v2" => {
                    let bind_version = version.min(MGMT_VERSION);
                    state.manager = Some(registry.bind::<KdeOutputManagementV2, _, _>(
                        name,
                        bind_version,
                        qhandle,
                        (),
                    ));
                    state.manager_registry_name = Some(name);
                }
                // Output-device registry (Plasma 6.7+).
                // One global that emits an `output` event per existing/new device
                // replaces the per-device-global pattern below.
                "kde_output_device_registry_v2" => {
                    let bind_version = version.min(DEVICE_REGISTRY_VERSION);
                    let proxy = registry.bind::<KdeOutputDeviceRegistryV2, _, _>(
                        name,
                        bind_version,
                        qhandle,
                        (),
                    );
                    state.device_registry = Some(proxy);
                    state.device_registry_name = Some(name);
                }
                // Legacy: one device per output as a wl_registry global.
                // Kept for Plasma <=6.6 compatibility
                // PAssed 6.7 this isn't advertised
                "kde_output_device_v2" => {
                    let bind_version = version.min(DEVICE_VERSION);
                    let device =
                        registry.bind::<KdeOutputDeviceV2, _, _>(name, bind_version, qhandle, ());
                    let id = device.id().protocol_id();
                    let entry = state.devices.entry(id).or_default();
                    entry.registry_name = Some(name);
                    entry.proxy = Some(device);
                }
                _ => {}
            },
            wl_registry::Event::GlobalRemove { name } => {
                // Manager removed: KWin restarted or crashed.
                if state.manager_registry_name == Some(name) {
                    state.manager_alive = false;
                    state.manager_registry_name = None;
                    return;
                }
                // Device registry removed
                if state.device_registry_name == Some(name) {
                    state.device_registry = None;
                    state.device_registry_name = None;
                    return;
                }
                // Device removed: a real monitor was unplugged. Drop the
                // device record along with any modes that referenced it
                // so subsequent snapshots don't surface stale state.
                if let Some(device_id) = state
                    .devices
                    .iter()
                    // match by registry name to find the protocol id
                    .find(|(_, device)| device.registry_name == Some(name))
                    .map(|(id, _)| *id)
                {
                    // drop the device record
                    state.devices.shift_remove(&device_id);
                    // drop any modes that hung off it
                    state.modes.retain(|_, (parent, _)| *parent != device_id);
                }
            }
            _ => {}
        }
    }
}

/// The manager itself emits no events; this impl exists only to satisfy
/// the trait bound on bound proxies.
impl Dispatch<KdeOutputManagementV2, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &KdeOutputManagementV2,
        _event: <KdeOutputManagementV2 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        // do nothing here
    }
}

/// Receives a `KdeOutputDeviceV2` proxy for every existing and newly connected output.
impl Dispatch<KdeOutputDeviceRegistryV2, ()> for State {
    // Opcode 1 because `finished` (destructor) takes opcode 0.
    wayland_client::event_created_child!(State, KdeOutputDeviceRegistryV2, [
        1 => (KdeOutputDeviceV2, ()),
    ]);

    fn event(
        state: &mut Self,
        _registry: &KdeOutputDeviceRegistryV2,
        event: <KdeOutputDeviceRegistryV2 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        use kde_output_device_registry_v2::Event::*;
        if let Output { output } = event {
            // No registry_name because this device isn't a wl_registry global
            let id = output.id().protocol_id();
            let entry = state.devices.entry(id).or_default();
            entry.proxy = Some(output);
        }
    }
}

/// Records per-device state as KWin streams it in. The device only becomes
/// visible to callers once the server signals its initial burst is complete.
impl Dispatch<KdeOutputDeviceV2, ()> for State {
    // The `mode` event spawns a child proxy; wayland-rs needs to know
    // what type to construct for it.
    wayland_client::event_created_child!(State, KdeOutputDeviceV2, [
        2 => (KdeOutputDeviceModeV2, ()),
    ]);

    fn event(
        state: &mut Self,
        device: &KdeOutputDeviceV2,
        event: <KdeOutputDeviceV2 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandleandle: &QueueHandle<Self>,
    ) {
        use kde_output_device_v2::Event::*;

        let device_id = device.id().protocol_id();
        let entry = state.devices.entry(device_id).or_default();
        match event {
            Name { name } => entry.name = Some(name),
            Enabled { enabled } => entry.enabled = enabled != 0,
            CurrentMode { mode } => entry.current_mode = Some(mode.id().protocol_id()),
            Geometry {
                x, y, transform, ..
            } => {
                entry.position = Some((x, y));
                entry.transform = Transform::try_from(transform).ok();
            }
            Scale { factor } => {
                // Snap to the 1/120 grid that fractional-scale-v1 negotiates
                entry.scale = {
                    let snap_scale = (factor * 120.0).round() / 120.0;
                    Some(snap_scale)
                }
            }
            Mode { mode } => {
                let mode_id = mode.id().protocol_id();
                let mode_entry = state
                    .modes
                    .entry(mode_id)
                    .or_insert_with(|| (device_id, ModeData::default()));
                mode_entry.0 = device_id;
                mode_entry.1.proxy = Some(mode);

                // Older KWin sometimes never sends `current_mode`; per
                // libkscreen the last `mode` event is implicitly current.
                if entry.current_mode.is_none() {
                    entry.current_mode = Some(mode_id);
                }
            }
            Done => entry.done_seen = true,
            _ => {}
        }
    }
}

/// Records mode size and refresh as the server streams them in, and
/// cleans up when the server retires a mode.
impl Dispatch<KdeOutputDeviceModeV2, ()> for State {
    fn event(
        state: &mut Self,
        mode: &KdeOutputDeviceModeV2,
        event: <KdeOutputDeviceModeV2 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandleandle: &QueueHandle<Self>,
    ) {
        use kde_output_device_mode_v2::Event::*;
        let mode_id = mode.id().protocol_id();

        // Drop the mode
        // if it was a device's current_mode, repoint to a sibling.
        if matches!(event, Removed) {
            let parent_id = state.modes.get(&mode_id).map(|(parent, _)| *parent);
            state.modes.shift_remove(&mode_id);
            if let Some(parent_id) = parent_id
                && let Some(device) = state.devices.get_mut(&parent_id)
                && device.current_mode == Some(mode_id)
            {
                device.current_mode = state
                    .modes
                    .iter()
                    .find(|(_, (parent, _))| *parent == parent_id)
                    .map(|(other_mode_id, _)| *other_mode_id);
            }
            return;
        }
        let entry = &mut state
            .modes
            .entry(mode_id)
            .or_insert_with(|| (0, ModeData::default()))
            .1;

        match event {
            Size { width, height } => {
                entry.info.width = width;
                entry.info.height = height;
            }
            Refresh { refresh } => entry.info.refresh_mhz = refresh,

            Removed => unreachable!(),
            _ => {}
        }
    }
}

/// Receives the server's verdict on a submitted configuration and stashes
/// any failure reason for the apply machinery to consume.
impl Dispatch<KdeOutputConfigurationV2, ()> for State {
    fn event(
        state: &mut Self,
        _proxy: &KdeOutputConfigurationV2,
        event: <KdeOutputConfigurationV2 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qhandleandle: &QueueHandle<Self>,
    ) {
        use kde_output_configuration_v2::Event::*;
        match event {
            FailureReason { reason } => state.pending_failure_reason = Some(reason),
            Applied => state.apply_result = ApplyResult::Applied,
            Failed => {
                state.apply_result = ApplyResult::Failed {
                    reason: state.pending_failure_reason.take(),
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compositor::EnableOutput;

    fn head(name: &str, enabled: bool) -> HeadState {
        HeadState {
            name: name.into(),
            enabled,
            mode: None,
            position: None,
            scale: None,
            transform: None,
        }
    }

    // No Wayland socket should be a soft miss (Ok(None)), not an error,
    // so the lifecycle can fall through to the next adapter.
    #[test]
    fn connect_returns_none_when_no_wayland_display() {
        let prev = std::env::var("WAYLAND_DISPLAY").ok();
        unsafe {
            std::env::remove_var("WAYLAND_DISPLAY");
            std::env::remove_var("WAYLAND_SOCKET");
        }
        let result = KdeCompositor::connect();
        // Restore before asserting so a panic doesn't leak into other tests.
        unsafe {
            if let Some(v) = prev {
                std::env::set_var("WAYLAND_DISPLAY", v);
            }
        }
        assert!(matches!(result, Ok(None)));
    }

    fn mode(width: i32, height: i32) -> ModeInfo {
        ModeInfo {
            width,
            height,
            refresh_mhz: 60_000,
        }
    }

    // Three resolution rules in `Target::for_head`:
    //   disable beats carry,
    //   unmentioned heads carry their current state,
    //   explicit enable overrides a head that's currently off.
    #[test]
    fn plan_policy_resolves_targets() {
        let mode = ModeInfo {
            width: 1920,
            height: 1080,
            refresh_mhz: 60_000,
        };

        // Two real heads on, one virtual head off.
        let mut primary = head("DP-1", true);
        primary.mode = Some(mode);
        primary.position = Some((0, 0));
        let mut secondary = head("DP-2", true);
        secondary.mode = Some(mode);
        secondary.position = Some((1920, 0));
        let live = [primary.clone(), secondary.clone(), head("fauxput-0", false)];

        // disable DP-1, enable fauxput-0, leave DP-2 unmentioned.
        let plan = OutputPlan::builder()
            .disable("DP-1")
            .unwrap()
            .enable(EnableOutput {
                name: "fauxput-0".into(),
                mode: Some(mode),
                position: Some((100, 0)),
            })
            .unwrap()
            .set_primary("DP-2")
            .unwrap()
            .build();

        // DP-1: disable wins over current-on.
        let target_primary = Target::for_head(&primary, &plan);
        assert!(!target_primary.enabled, "disable should win over carry");

        // DP-2: unmentioned, keep current state.
        let target_secondary = Target::for_head(&secondary, &plan);
        assert!(
            target_secondary.enabled,
            "unmentioned enabled head should carry"
        );
        assert_eq!(target_secondary.mode, secondary.mode);
        assert_eq!(target_secondary.position, secondary.position);

        // fauxput-0: explicit enable wins over current-off, plan's
        // mode/position replace the head's defaults.
        let target_virtual = Target::for_head(&live[2], &plan);
        assert!(
            target_virtual.enabled,
            "explicit enable should override current disabled state"
        );
        assert_eq!(target_virtual.mode, Some(mode));
        assert_eq!(target_virtual.position, Some((100, 0)));
    }

    // KDE carries transform as a raw i32. 0-7 maps to wl_output.transform;
    // anything else round-trips as Err(value) instead of panicking.
    #[test]
    fn transform_decode_returns_err_for_unknown() {
        assert_eq!(Transform::try_from(0), Ok(Transform::Normal));
        assert_eq!(Transform::try_from(7), Ok(Transform::FlippedRot270));
        assert_eq!(Transform::try_from(8), Err(8));
        assert_eq!(Transform::try_from(-1), Err(-1));
    }

    // Device properties trickle in across events; only after `done` is the
    // snapshot coherent. `live_heads` filters on `done_seen` so callers
    // never observe a half-initialized device.
    #[test]
    fn live_heads_excludes_devices_without_done() {
        let mut s = State::default();
        // A: fully initialized.
        s.devices.insert(
            1,
            Device {
                name: Some("A".into()),
                done_seen: true,
                current_mode: Some(10),
                ..Default::default()
            },
        );
        // B: mid-initialization, no `done` yet.
        s.devices.insert(
            2,
            Device {
                name: Some("B".into()),
                done_seen: false,
                ..Default::default()
            },
        );
        // Mode 10's parent is device 1; `head` resolves current_mode via this map.
        s.modes.insert(
            10,
            (
                1,
                ModeData {
                    proxy: None,
                    info: ModeInfo {
                        width: 1920,
                        height: 1080,
                        refresh_mhz: 60_000,
                    },
                },
            ),
        );

        let heads = s.live_heads();
        // B filtered out, A surfaces with its mode resolved.
        assert_eq!(
            heads.len(),
            1,
            "only done devices should project as live heads"
        );
        assert_eq!(heads[0].name, "A");
        assert_eq!(heads[0].mode, Some(mode(1920, 1080)));
    }
}
