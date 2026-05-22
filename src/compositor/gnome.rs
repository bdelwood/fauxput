mod display_config;

use self::display_config::{
    CurrentState, DisplayConfigProxyBlocking, LogicalMonitorConfig, Method, MonitorConfig,
};
use crate::Result;
use crate::compositor::{
    CompositorAdapter, CompositorError, FeatureKind, HeadState, ModeInfo, OutputPlan,
    OutputSnapshot, Transform,
};

use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use zbus::blocking::Connection;

impl From<CurrentState> for OutputSnapshot {
    fn from(state: CurrentState) -> Self {
        let heads = state
            .monitors
            .iter()
            .map(|monitor| {
                // get logical monitors attached to this head
                let logical = state.logical_monitors.iter().find(|lm| {
                    lm.monitors
                        .iter()
                        .any(|id| id.connector == monitor.id.connector)
                });

                HeadState {
                    name: monitor.id.connector.clone(),
                    enabled: logical.is_some(),
                    mode: monitor
                        .modes
                        .iter()
                        .find(|m| m.is_current())
                        .map(|m| ModeInfo {
                            width: m.width,
                            height: m.height,
                            refresh_mhz: (m.refresh_rate * 1000.0).round() as i32,
                        }),
                    position: logical.map(|l| (l.x, l.y)),
                    scale: logical.map(|l| l.scale),
                    transform: logical.and_then(|l| Transform::try_from(l.transform).ok()),
                }
            })
            .collect();

        OutputSnapshot { heads }
    }
}

/// Convert the the output plan into a form that the mutter interface understands
impl OutputPlan {
    pub(crate) fn to_mutter_config(&self, state: &CurrentState) -> Vec<LogicalMonitorConfig> {
        let enable_names: HashSet<&str> = self.enables.iter().map(|e| e.name.as_str()).collect();
        let disable_names: HashSet<&str> = self.disables.iter().map(|s| s.as_str()).collect();

        // If the plan declares a primary, it overrides any existing primary
        // marker carried forward from mutter's current layout
        let plan_primary = self.primary();
        let plan_sets_primary = plan_primary.is_some();

        let mut result: Vec<LogicalMonitorConfig> = Vec::new();

        // Carry forward logical monitors the plan doesn't touch.
        // A logical monitor is "touched" if any of its connectors is in enables
        for lm in &state.logical_monitors {
            let touched = lm.monitors.iter().any(|id| {
                enable_names.contains(id.connector.as_str())
                    || disable_names.contains(id.connector.as_str())
            });
            if touched {
                continue;
            }

            // Translate LogicalMonitor (read side) > LogicalMonitorConfig (write side).
            let monitors: Vec<MonitorConfig> = lm
                .monitors
                .iter()
                .filter_map(|id| {
                    let monitor = state
                        .monitors
                        .iter()
                        .find(|m| m.id.connector == id.connector)?;
                    let current = monitor.modes.iter().find(|m| m.is_current())?;
                    Some(MonitorConfig {
                        connector: id.connector.clone(),
                        id: current.id.clone(),
                        properties: HashMap::new(),
                    })
                })
                .collect();

            if monitors.is_empty() {
                // Lost every connector to lookup failure
                //  skip rather than submit a logical monitor with no outputs
                continue;
            }

            result.push(LogicalMonitorConfig {
                x: lm.x,
                y: lm.y,
                scale: lm.scale,
                transform: lm.transform,
                // Plan-declared primary clears every other primary marker.
                primary: if plan_sets_primary { false } else { lm.primary },
                monitors,
            });
        }

        // New logical monitor per EnableOutput.
        for enable in &self.enables {
            let Some(monitor) = state
                .monitors
                .iter()
                .find(|m| m.id.connector == enable.name)
            else {
                eprintln!(
                    "gnome: enable target {:?} not advertised by Mutter; skipping",
                    enable.name
                );
                continue;
            };

            // Pick a mode.
            // The lookup ended up being a bit contrived, but should work fine:
            //   1. exact w/h/refresh match
            //   2. w/h match, refresh closest to requested
            //   3. preferred mode -> first as a last resort
            let mode = match &enable.mode {
                Some(info) => monitor
                    .modes
                    .iter()
                    // exact match
                    .find(|m| {
                        m.width == info.width
                            && m.height == info.height
                            && (m.refresh_rate * 1000.0).round() as i32 == info.refresh_mhz
                    })
                    // if not exact...
                    .or_else(|| {
                        monitor
                            .modes
                            .iter()
                            // do width and height match?
                            .filter(|m| m.width == info.width && m.height == info.height)
                            .min_by_key(|m| {
                                let mhz = (m.refresh_rate * 1000.0).round() as i32;
                                (mhz - info.refresh_mhz).abs()
                            })
                    }),
                // otherwise, check if preferred or first
                None => monitor
                    .modes
                    .iter()
                    .find(|m| m.is_preferred())
                    .or_else(|| monitor.modes.first()),
            };

            let Some(mode) = mode else {
                eprintln!(
                    "gnome: no matching mode on {:?} for {:?}; skipping",
                    enable.name, enable.mode
                );
                continue;
            };

            // Position priority:
            //   1. Explicit position in the plan
            //   2. Current live position from mutter
            //   3. Pack to the right of monitors already placed
            let existing = state
                .logical_monitors
                .iter()
                .find(|lm| lm.monitors.iter().any(|m| m.connector == enable.name));

            let (x, y) = match (enable.position, existing) {
                (Some((px, py)), _) => (px, py),
                (None, Some(lm)) => (lm.x, lm.y),
                (None, None) => (right_edge(&result, state), 0),
            };

            // Primary rule
            // If plan sets primary, set it
            // otherwise preserve
            let primary = match plan_primary {
                Some(name) => name == enable.name.as_str(),
                None => existing.map(|lm| lm.primary).unwrap_or(false),
            };

            result.push(LogicalMonitorConfig {
                x,
                y,
                // don't touch scaling or transform... for now.
                scale: 1.0,
                transform: 0,
                primary,
                monitors: vec![MonitorConfig {
                    connector: enable.name.clone(),
                    id: mode.id.clone(),
                    properties: HashMap::new(),
                }],
            });
        }

        // Anything not in result is disabled by omission

        // Safety net: Mutter rejects non-origin-anchored layouts with
        // "org.freedesktop.DBus.Error.InvalidArgs: Logical monitors positions
        // are offset". Need to preserve correct offsets.
        if let Some(min_x) = result.iter().map(|lm| lm.x).min()
            && min_x != 0
        {
            for lm in &mut result {
                lm.x -= min_x;
            }
        }
        if let Some(min_y) = result.iter().map(|lm| lm.y).min()
            && min_y != 0
        {
            for lm in &mut result {
                lm.y -= min_y;
            }
        }
        result
    }
}

/// Largest `x + width` across already-placed logical monitors
fn right_edge(placed: &[LogicalMonitorConfig], state: &CurrentState) -> i32 {
    placed
        .iter()
        .filter_map(|lmc| {
            let m = lmc.monitors.first()?;
            let monitor = state
                .monitors
                .iter()
                .find(|s| s.id.connector == m.connector)?;
            let mode = monitor.modes.iter().find(|md| md.id == m.id)?;
            Some(lmc.x + mode.width)
        })
        .max()
        .unwrap_or(0)
}

pub(crate) struct GnomeCompositor {
    conn: Connection,
}

impl GnomeCompositor {
    pub(crate) fn connect() -> Result<Option<Self>> {
        let conn = Connection::session().map_err(|source| CompositorError::Dbus {
            context: "gnome: connect session bus",
            source,
        })?;
        let proxy =
            DisplayConfigProxyBlocking::new(&conn).map_err(|source| CompositorError::Dbus {
                context: "gnome: create display config proxy",
                source,
            })?;
        let state = match proxy.get_current_state() {
            Ok(state) => state,
            Err(source)
                if matches!(
                    &source,
                    zbus::Error::MethodError(name, _, _)
                        if name.as_str() == "org.freedesktop.DBus.Error.ServiceUnknown"
                ) =>
            {
                return Ok(None);
            }
            Err(source) => {
                return Err(CompositorError::Dbus {
                    context: "gnome: get current state",
                    source,
                }
                .into());
            }
        };
        log::debug!("gnome: GetCurrentState returned {state:#?}");

        Ok(Some(Self { conn }))
    }

    fn current_state(&self) -> Result<CurrentState> {
        let proxy = self.proxy()?;
        proxy
            .get_current_state()
            .map_err(|source| CompositorError::Dbus {
                context: "gnome: get current state",
                source,
            })
            .map_err(Into::into)
    }

    fn proxy(&self) -> Result<DisplayConfigProxyBlocking<'_>> {
        let proxy = DisplayConfigProxyBlocking::new(&self.conn).map_err(|source| {
            CompositorError::Dbus {
                context: "gnome: create display config proxy",
                source,
            }
        })?;
        Ok(proxy)
    }
}

impl CompositorAdapter for GnomeCompositor {
    fn name(&self) -> &'static str {
        "gnome"
    }

    /// Features mutter natively honors
    fn supported_features(&self) -> HashSet<FeatureKind> {
        HashSet::from([FeatureKind::Primary])
    }

    fn snapshot(&mut self) -> Result<OutputSnapshot> {
        Ok(self.current_state()?.into())
    }

    fn wait_for_new_head(
        &mut self,
        baseline: &HashSet<String>,
        timeout: Duration,
    ) -> Result<HeadState> {
        // Subscribe to `MonitorsChanged` BEFORE the first state check so we
        // can't lose an edge that fires between the check and the subscribe.
        // Mutter tells us monitors changed  `MonitorsChanged` exactly when `manager->monitors` is
        // rebuilt,
        // which is also the moment the hot-added connector becomes
        // visible to the state monitor.
        //  The kernel-side HPD toggle in the vkms commit triggers mutter to rebuild
        // here we just need to wake up the moment Mutter rings the bell.
        let signal_iter =
            self.proxy()?
                .receive_monitors_changed()
                .map_err(|source| CompositorError::Dbus {
                    context: "gnome: subscribe MonitorsChanged",
                    source,
                })?;

        // Pump signal arrivals through an mpsc channel so the main thread
        let (tx, rx) = mpsc::channel::<()>();
        thread::spawn(move || {
            for _ev in signal_iter {
                if tx.send(()).is_err() {
                    break;
                }
            }
        });

        // Captures `baseline` and `timeout`
        let make_timeout = |last_seen: &[String]| -> crate::Error {
            CompositorError::Timeout {
                reason: format!(
                    "GNOME never surfaced a new head; \
                     baseline={baseline:?}, last seen={last_seen:?}"
                ),
                timeout,
            }
            .into()
        };

        let deadline = Instant::now() + timeout;
        let mut last_seen: Vec<String>;
        loop {
            let snap = self.snapshot()?;
            last_seen = snap.heads.iter().map(|h| h.name.clone()).collect();
            if let Some(head) = snap.heads.into_iter().find(|h| !baseline.contains(&h.name)) {
                return Ok(head);
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(make_timeout(&last_seen));
            };
            match rx.recv_timeout(remaining) {
                Ok(()) => continue,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(make_timeout(&last_seen));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    // Worker exited
                    //  One last snapshot in case the rebuild raced past us, then fail.
                    let snap = self.snapshot()?;
                    last_seen = snap.heads.iter().map(|h| h.name.clone()).collect();
                    if let Some(head) = snap.heads.into_iter().find(|h| !baseline.contains(&h.name))
                    {
                        return Ok(head);
                    }
                    return Err(make_timeout(&last_seen));
                }
            }
        }
    }

    fn apply(&mut self, plan: &OutputPlan) -> Result<()> {
        // Refresh the serial
        let state = self.current_state()?;
        let logical = plan.to_mutter_config(&state);

        self.proxy()?
            .apply_monitors_config(state.serial, Method::Temporary, &logical, HashMap::new())
            .map_err(|source| CompositorError::Dbus {
                context: "gnome: apply monitors config",
                source,
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end live test against a running GNOME Wayland session: connects
    /// to the user session bus, reads Mutter's state, runs it through the
    /// snapshot conversion, and confirms it produced a usable result.
    ///
    /// Requires a GDM/Mutter session and access to that user's session bus.
    ///
    /// Run with:
    ///   `cargo test -- --ignored --nocapture gnome_snapshot`
    #[test]
    #[ignore]
    fn gnome_snapshot() {
        let Some(mut compositor) = GnomeCompositor::connect().expect("GNOME connect failed") else {
            eprintln!("skipping: org.gnome.Mutter.DisplayConfig unavailable in this session");
            return;
        };

        let snap = compositor.snapshot().expect("snapshot failed");
        eprintln!("gnome: snapshot = {snap:#?}");

        assert!(
            snap.heads.iter().any(|h| h.enabled),
            "GNOME session must have at least one enabled head"
        );
    }
}
