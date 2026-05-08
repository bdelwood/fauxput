//! KDE/Plasma `kde_output_management_v2` driver.

use std::collections::HashSet;

use indexmap::IndexMap;
use std::time::Duration;

use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, protocol::wl_registry};
use wayland_protocols_plasma::output_device::v2::client::{
    kde_output_device_mode_v2::{self, KdeOutputDeviceModeV2},
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
const HEAD_POLL_INTERVAL: Duration = Duration::from_millis(50);

const MGMT_VERSION: u32 = 19;
const DEVICE_VERSION: u32 = 20;

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

        session.state.manager_alive = true;

        // get initial device event burst
        session.roundtrip("kde: device burst")?;

        Ok(Some(Self { session, manager }))
    }

    fn submit<F>(&mut self, ctx: &'static str, populate: F) -> Result<()>
    where
        F: FnOnce(&KdeOutputConfigurationV2, &State) -> Result<usize>,
    {
        // pass in empty userdata; don't need it
        let cfg = self
            .manager
            .create_configuration(self.session.qhandle(), ());
        let touched = populate(&cfg, &self.session.state)?;

        if touched == 0 {
            cfg.destroy();
            return Ok(());
        }

        self.session.state.apply_result = ApplyResult::Pending;
        self.session.state.pending_failure_reason = None;
        cfg.apply();
        self.session.flush(ctx)?;

        let result = self.session.poll_until(
            format!("kde apply ({ctx})"),
            APPLY_TIMEOUT,
            Duration::ZERO,
            |session| {
                session.roundtrip(ctx)?;
                // probably overly defensive, but things happen
                if !session.state.manager_alive {
                    return Err(CompositorError::CompositorWentAway.into());
                }
                Ok(match session.state.apply_result {
                    ApplyResult::Pending => None,
                    _ => Some(std::mem::take(&mut session.state.apply_result)),
                })
            },
        );

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

    fn supported_features(&self) -> HashSet<FeatureKind> {
        HashSet::from([FeatureKind::Primary])
    }

    fn snapshot(&mut self) -> Result<OutputSnapshot> {
        self.session.roundtrip("kde: snapshot refresh")?;
        Ok(OutputSnapshot {
            heads: self.session.state.live_heads(),
        })
    }

    fn wait_for_new_head(
        &mut self,
        baseline: &HashSet<String>,
        timeout: Duration,
    ) -> Result<HeadState> {
        self.session
            .poll_until("new kde head", timeout, HEAD_POLL_INTERVAL, |session| {
                session.roundtrip("kde: head poll")?;
                Ok(session
                    .state
                    .live_heads()
                    .into_iter()
                    .find(|head| !baseline.contains(&head.name)))
            })
    }

    fn apply(&mut self, plan: &OutputPlan) -> Result<()> {
        self.session.roundtrip("kde: apply refresh")?;
        let live = self.session.state.live_heads();
        let plan = plan.clone();

        self.submit("apply", move |cfg, state| {
            let mut touched = 0;

            let primary_name = plan.primary();
            let mut next: u32 = if primary_name.is_some() { 2 } else { 1 };
            for head in &live {
                let Some(proxy) = state.device_by_name(&head.name) else {
                    continue;
                };
                let target = Target::for_head(head, &plan);

                if target.enabled != head.enabled {
                    cfg.enable(&proxy, target.enabled.into());
                    touched += 1;
                }

                if target.enabled
                    && let Some(mode) = target.mode
                    && Some(mode) != head.mode
                    && let Some(mp) = state.find_mode_proxy(&head.name, mode)
                {
                    cfg.mode(&proxy, &mp);
                    touched += 1;
                }

                if target.enabled
                    && let Some(pos) = target.position
                    && Some(pos) != head.position
                {
                    cfg.position(&proxy, pos.0, pos.1);
                    touched += 1;
                }

                let priority = if Some(head.name.as_str()) == primary_name {
                    1
                } else {
                    let p = next;
                    next += 1;
                    p
                };

                cfg.set_priority(&proxy, priority);
                touched += 1;
            }
            Ok(touched)
        })
    }
}

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
            enabled: !disabled && (enable_entry.is_some() || head.enabled),
            mode: enable_entry.and_then(|entry| entry.mode).or(head.mode),
            position: enable_entry
                .and_then(|entry| entry.position)
                .or(head.position),
        }
    }
}

#[derive(Default)]
struct State {
    manager: Option<KdeOutputManagementV2>,
    manager_registry_name: Option<u32>,
    manager_alive: bool,
    devices: IndexMap<u32, Device>,
    modes: IndexMap<u32, (u32, ModeData)>,
    apply_result: ApplyResult,
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
    fn live_heads(&self) -> Vec<HeadState> {
        self.devices
            .values()
            .filter(|device| device.name.is_some() && device.done_seen)
            .map(|device| self.head(device))
            .collect()
    }

    fn head(&self, device: &Device) -> HeadState {
        let mode = device
            .current_mode
            .and_then(|id| self.modes.get(&id))
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

    fn device_by_name(&self, name: &str) -> Option<KdeOutputDeviceV2> {
        self.devices
            .values()
            .find(|device| device.name.as_deref() == Some(name))
            .and_then(|device| device.proxy.clone())
    }

    fn find_mode_proxy(&self, device_name: &str, want: ModeInfo) -> Option<KdeOutputDeviceModeV2> {
        let device_id = self
            .devices
            .iter()
            .find(|(_, device)| device.name.as_deref() == Some(device_name))
            .map(|(id, _)| *id)?;

        self.modes
            .iter()
            .find(|(_, (parent, mode_data))| {
                *parent == device_id
                    && mode_data.info.width == want.width
                    && mode_data.info.height == want.height
                    && (mode_data.info.refresh_mhz - want.refresh_mhz) < 100
            })
            .and_then(|(_, (_, mode_data))| mode_data.proxy.clone())
    }
}

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
                if state.manager_registry_name == Some(name) {
                    state.manager_alive = false;
                    state.manager_registry_name = None;
                    return;
                }
                if let Some(device_id) = state
                    .devices
                    .iter()
                    .find(|(_, device)| device.registry_name == Some(name))
                    .map(|(id, _)| *id)
                {
                    state.devices.shift_remove(&device_id);
                    state.modes.retain(|_, (parent, _)| *parent != device_id);
                }
            }
            _ => {}
        }
    }
}

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

impl Dispatch<KdeOutputDeviceV2, ()> for State {
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

                if entry.current_mode.is_none() {
                    entry.current_mode = Some(mode_id);
                }
            }
            Done => entry.done_seen = true,
            _ => {}
        }
    }
}

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

impl TryFrom<i32> for Transform {
    type Error = i32;
    fn try_from(value: i32) -> std::result::Result<Self, Self::Error> {
        Ok(match value {
            0 => Transform::Normal,
            1 => Transform::Rot90,
            2 => Transform::Rot180,
            3 => Transform::Rot270,
            4 => Transform::Flipped,
            5 => Transform::FlippedRot90,
            6 => Transform::FlippedRot180,
            7 => Transform::FlippedRot270,
            _ => return Err(value),
        })
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
        let mut plan = OutputPlan::builder();
        plan.disable("DP-1").unwrap();
        plan.enable(EnableOutput {
            name: "fauxput-0".into(),
            mode: Some(mode),
            position: Some((100, 0)),
        })
        .unwrap();
        plan.set_primary("DP-2").unwrap();
        let plan = plan.build();

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
