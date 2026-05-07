//! KDE/Plasma `kde_output_management_v2` driver.

use std::collections::{HashMap, HashSet};

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
    CompositorAdapter, CompositorError, FeatureKind, HeadState, ModeInfo, OutputPlan,
    OutputSnapshot, Transform,
};

// polls will wait forever
// we're not doing anything fancy, timeout after short time
const APPLY_TIMEOUT: Duration = Duration::from_secs(5);
const HEAD_POLL_INTERVAL: Duration = Duration::from_millis(50);

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
        let cfg = self.manager.create_configuration(self.session.qh(), ());
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
        todo!()
    }
}

// TODO: refactor this
fn compute_priorities(plan: &OutputPlan, live: &[HeadState]) -> HashMap<String, u32> {
    todo!("implement")
}

#[derive(Default)]
struct State {
    manager: Option<KdeOutputManagementV2>,
    managet_registry_name: Option<u32>,
    manager_alive: bool,
    devices: IndexMap<u32, Device>,
    modes: HashMap<u32, (u32, ModeData)>,
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

#[derive(Default)]
struct ModeData {
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
            .map(|(_, mode_data)| ModeInfo {
                width: mode_data.width,
                height: mode_data.height,
                refresh_mhz: mode_data.refresh_mhz,
            });

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
        todo!()
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_registry::WlRegistry,
        event: <wl_registry::WlRegistry as Proxy>::Event,
        data: &(),
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<KdeOutputConfigurationV2, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &KdeOutputConfigurationV2,
        event: <KdeOutputConfigurationV2 as Proxy>::Event,
        data: &(),
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
    }
}
