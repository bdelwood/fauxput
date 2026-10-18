//! `zwlr_output_manager_v1` adapter.

use std::{collections::HashSet, time::Duration};

use indexmap::IndexMap;
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
    backend::ObjectId,
    protocol::{wl_output, wl_registry},
};
use wayland_protocols_wlr::output_management::v1::client::{
    zwlr_output_configuration_head_v1::ZwlrOutputConfigurationHeadV1,
    zwlr_output_configuration_v1::{self, ZwlrOutputConfigurationV1},
    zwlr_output_head_v1::{self, ZwlrOutputHeadV1},
    zwlr_output_manager_v1::{self, ZwlrOutputManagerV1},
    zwlr_output_mode_v1::{self, ZwlrOutputModeV1},
};

use crate::{
    Result,
    compositor::{
        CompositorAdapter, CompositorError, FeatureKind, HeadState, OutputMode, OutputPlan,
        OutputSnapshot, Transform, wayland::WaylandSession,
    },
};

const APPLY_TIMEOUT: Duration = Duration::from_secs(5);
const MANAGER_VERSION: u32 = 4;

pub(crate) struct WlrCompositor {
    session: WaylandSession<State>,
    manager: ZwlrOutputManagerV1,
}

impl WlrCompositor {
    pub(crate) fn connect() -> Result<Option<Self>> {
        let Some(mut session) = WaylandSession::<State>::connect_to_env()? else {
            return Ok(None);
        };
        let _ = session.registry();
        session.roundtrip("wlr: registry")?;
        let Some(manager) = session.state.manager.take() else {
            return Ok(None);
        };
        let mut compositor = Self { session, manager };
        compositor.refresh()?;
        Ok(Some(compositor))
    }

    fn refresh(&mut self) -> Result<u32> {
        self.session.roundtrip("wlr: refresh")?;
        let serial = self
            .session
            .poll_until("wlr output state", APPLY_TIMEOUT, |session| {
                if !session.state.manager_alive {
                    return Err(CompositorError::CompositorWentAway.into());
                }
                Ok(session.state.serial)
            })?;
        // Snapshot/restore needs a mode for each enabled output.
        // TODO: support COSMIC mirroring through its output-management extension:
        // https://github.com/pop-os/cosmic-protocols/blob/main/unstable/cosmic-output-management-unstable-v1.xml
        if self.session.state.devices.values().any(|device| {
            device.enabled
                && device
                    .current_mode
                    .and_then(|id| self.session.state.modes.get(&id))
                    .is_none()
        }) {
            return Err(CompositorError::ApplyRejected {
                reason: "enabled output has no current mode; cannot preserve its layout",
            }
            .into());
        }
        Ok(serial)
    }

    fn submit(&mut self, plan: &OutputPlan, serial: u32) -> Result<ApplyResult> {
        let cfg = self
            .manager
            .create_configuration(serial, self.session.qhandle(), ());

        // wlr requires each head exactly once, including untouched outputs.
        for (id, device) in &self.session.state.devices {
            let head = self.session.state.head(device);
            let target = plan.target_for(&head);
            if !target.enabled {
                cfg.disable_head(&device.proxy);
                continue;
            }
            let config = cfg.enable_head(&device.proxy, self.session.qhandle(), ());
            if let Some(mode) = target.mode
                && (!head.enabled || Some(mode) != head.mode)
            {
                if let Some((_, data)) = self
                    .session
                    .state
                    .modes
                    .values()
                    .find(|(parent, data)| parent == id && data.info == mode)
                    && let Some(proxy) = &data.proxy
                {
                    config.set_mode(proxy);
                } else {
                    config.set_custom_mode(mode.width, mode.height, mode.refresh_mhz);
                }
            }
            if let Some((x, y)) = target.position
                && (!head.enabled || Some((x, y)) != head.position)
            {
                config.set_position(x, y);
            }
            if let Some(scale) = target.scale
                && (!head.enabled || Some(scale) != head.scale)
            {
                config.set_scale(scale);
            }
            if let Some(transform) = target.transform
                && (!head.enabled || Some(transform) != head.transform)
            {
                config.set_transform(wl_output::Transform::try_from(transform as u32).unwrap());
            }
        }

        self.session.state.pending_configuration = Some(cfg.id());
        self.session.state.apply_result = None;
        cfg.apply();
        let result = self
            .session
            .poll_until("wlr apply", APPLY_TIMEOUT, |session| {
                if !session.state.manager_alive {
                    return Err(CompositorError::CompositorWentAway.into());
                }
                Ok(session.state.apply_result.take())
            });
        cfg.destroy();
        self.session.state.pending_configuration = None;
        result
    }
}

impl CompositorAdapter for WlrCompositor {
    fn name(&self) -> &'static str {
        "wlr"
    }

    fn supported_features(&self) -> HashSet<FeatureKind> {
        HashSet::new()
    }

    fn snapshot(&mut self) -> Result<OutputSnapshot> {
        self.refresh()?;
        Ok(OutputSnapshot {
            heads: self
                .session
                .state
                .devices
                .values()
                .map(|device| self.session.state.head(device))
                .collect(),
            physical_layout: false,
        })
    }

    fn apply(&mut self, plan: &OutputPlan) -> Result<()> {
        let plan = plan.for_adapter(self);

        // Hotplug can invalidate the serial between refresh and apply.
        for attempt in 0..2 {
            let serial = self.refresh()?;
            if plan.enables.iter().any(|enable| {
                !self
                    .session
                    .state
                    .devices
                    .values()
                    .any(|device| device.name == enable.name)
            }) {
                return Err(CompositorError::ApplyRejected {
                    reason: "requested output is no longer available",
                }
                .into());
            }
            match self.submit(&plan, serial)? {
                ApplyResult::Applied => return Ok(()),
                ApplyResult::Failed => {
                    return Err(CompositorError::ApplyFailed { reason: None }.into());
                }
                ApplyResult::Cancelled if attempt == 0 => continue,
                ApplyResult::Cancelled => {
                    return Err(CompositorError::ApplyRejected {
                        reason: "output configuration changed during apply",
                    }
                    .into());
                }
            }
        }
        unreachable!()
    }
}

#[derive(Default)]
struct State {
    manager: Option<ZwlrOutputManagerV1>,
    manager_name: Option<u32>,
    manager_alive: bool,
    // Invalidated by updates until the manager sends done.
    serial: Option<u32>,
    devices: IndexMap<u32, Device>,
    modes: IndexMap<u32, (u32, OutputMode<ZwlrOutputModeV1>)>,
    pending_configuration: Option<ObjectId>,
    apply_result: Option<ApplyResult>,
}

struct Device {
    proxy: ZwlrOutputHeadV1,
    name: String,
    enabled: bool,
    current_mode: Option<u32>,
    position: Option<(i32, i32)>,
    scale: Option<f64>,
    transform: Option<Transform>,
}

enum ApplyResult {
    Applied,
    Failed,
    Cancelled,
}

impl State {
    fn head(&self, device: &Device) -> HeadState {
        HeadState {
            name: device.name.clone(),
            enabled: device.enabled,
            mode: device
                .current_mode
                .and_then(|id| self.modes.get(&id))
                .map(|(_, mode)| mode.info),
            position: device.position,
            scale: device.scale,
            transform: device.transform,
            color_mode: None,
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == "zwlr_output_manager_v1" && state.manager_name.is_none() => {
                state.manager =
                    Some(registry.bind(name, version.min(MANAGER_VERSION), qhandle, ()));
                state.manager_name = Some(name);
                state.manager_alive = true;
            }
            wl_registry::Event::GlobalRemove { name } if state.manager_name == Some(name) => {
                state.manager_alive = false;
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrOutputManagerV1, ()> for State {
    wayland_client::event_created_child!(State, ZwlrOutputManagerV1, [
        0 => (ZwlrOutputHeadV1, ()),
    ]);

    fn event(
        state: &mut Self,
        _: &ZwlrOutputManagerV1,
        event: zwlr_output_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_output_manager_v1::Event::Head { head } => {
                state.serial = None;
                state.devices.insert(
                    head.id().protocol_id(),
                    Device {
                        proxy: head,
                        name: String::new(),
                        enabled: false,
                        current_mode: None,
                        position: None,
                        scale: None,
                        transform: None,
                    },
                );
            }
            zwlr_output_manager_v1::Event::Done { serial } => state.serial = Some(serial),
            zwlr_output_manager_v1::Event::Finished => state.manager_alive = false,
            _ => {}
        }
    }
}

impl Dispatch<ZwlrOutputHeadV1, ()> for State {
    wayland_client::event_created_child!(State, ZwlrOutputHeadV1, [
        3 => (ZwlrOutputModeV1, ()),
    ]);

    fn event(
        state: &mut Self,
        proxy: &ZwlrOutputHeadV1,
        event: zwlr_output_head_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwlr_output_head_v1::Event::*;
        let id = proxy.id().protocol_id();
        state.serial = None;
        if matches!(event, Finished) {
            state.devices.shift_remove(&id);
            state.modes.retain(|_, (parent, _)| *parent != id);
            if proxy.version() >= 3 {
                proxy.release();
            }
            return;
        }
        let Some(device) = state.devices.get_mut(&id) else {
            return;
        };
        match event {
            Name { name } => device.name = name,
            Enabled { enabled } => device.enabled = enabled != 0,
            Mode { mode } => {
                state.modes.insert(
                    mode.id().protocol_id(),
                    (
                        id,
                        OutputMode {
                            proxy: Some(mode),
                            ..Default::default()
                        },
                    ),
                );
            }
            CurrentMode { mode } => device.current_mode = Some(mode.id().protocol_id()),
            Position { x, y } => device.position = Some((x, y)),
            Scale { scale } => device.scale = Some(scale),
            Transform { transform } => {
                device.transform = match transform {
                    WEnum::Value(transform) => {
                        crate::compositor::Transform::try_from(transform as u32).ok()
                    }
                    WEnum::Unknown(_) => None,
                };
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrOutputModeV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &ZwlrOutputModeV1,
        event: zwlr_output_mode_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwlr_output_mode_v1::Event::*;
        let id = proxy.id().protocol_id();
        state.serial = None;
        if matches!(event, Finished) {
            state.modes.shift_remove(&id);
            for device in state.devices.values_mut() {
                if device.current_mode == Some(id) {
                    device.current_mode = None;
                }
            }
            if proxy.version() >= 3 {
                proxy.release();
            }
            return;
        }
        let Some((_, mode)) = state.modes.get_mut(&id) else {
            return;
        };
        match event {
            Size { width, height } => {
                mode.info.width = width;
                mode.info.height = height;
            }
            Refresh { refresh } => mode.info.refresh_mhz = refresh,
            _ => {}
        }
    }
}

impl Dispatch<ZwlrOutputConfigurationV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &ZwlrOutputConfigurationV1,
        event: zwlr_output_configuration_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if state.pending_configuration.as_ref() != Some(&proxy.id()) {
            return;
        }
        state.apply_result = match event {
            zwlr_output_configuration_v1::Event::Succeeded => Some(ApplyResult::Applied),
            zwlr_output_configuration_v1::Event::Failed => Some(ApplyResult::Failed),
            zwlr_output_configuration_v1::Event::Cancelled => Some(ApplyResult::Cancelled),
            _ => None,
        };
    }
}

wayland_client::delegate_noop!(State: ignore ZwlrOutputConfigurationHeadV1);
