// stubbed

use std::collections::HashMap;
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
    CompositorAdaptor, CompositorError, HeadState, ModeInfo, OutputPlanBuilder, OutputSnapshot,
    Transform,
};

pub struct KdeCompositor {
    session: WaylandSession<State>,
    manager: KdeOutputManagementV2,
}


impl KdeCompositor {
    pub fn connect() -> Result<Option<Self>> {
        let Some(mut session) = WaylandSession::<State>::connect_to_env()? else {
            return Ok(None)
        }
    }
}

#[derive(Default)]
struct State {
    manager: Option<KdeOutputManagementV2>,
    devices: HashMap<u32, DeviceInProgress>,
    modes: HashMap<u32, (u32, ModeInProgress),
    apply_result: ApplyResult,
}


#[derive(Default)]
struct DeviceInProgress {
    registry_name: Option<u32>,
    proxy: Option<KdeOutputDeviceV2>,
    name: Option<String>,
    enabled: bool, 
    current_mode: Option<u32>,
}

#[derive(Default)]
struct ModeInProgress {
    proxy: Option<KdeOutputDeviceModeV2>,
    width: i32, 
    height: i32, 
    refresh_mhz: i32,
    finished: bool,
}

#[derive(Default, PartialEq, Eq)]
enum ApplyResult {
    #[default]
    Pending, 
    Applied, 
    Failed
}


impl State {
    fn live_heads() {}
}