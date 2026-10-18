mod display_config;

use self::display_config::{
    ApplyPropertyMap, CurrentState, DisplayConfigProxyBlocking, LogicalMonitorConfig, Method,
    MonitorConfig,
};
use crate::Result;
use crate::compositor::{
    ColorMode, CompositorAdapter, CompositorError, FeatureKind, HeadState, ModeInfo, OutputPlan,
    OutputSnapshot, Transform,
};

use std::collections::{HashMap, HashSet};
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

                let color_mode = monitor
                    .properties
                    .get("color-mode")
                    .and_then(|v| u32::try_from(v).ok())
                    .and_then(|mode| match mode {
                        0 => Some(ColorMode::Sdr),
                        1 => Some(ColorMode::Hdr),
                        2 => Some(ColorMode::SdrNative),
                        _ => None,
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
                    color_mode,
                }
            })
            .collect();

        // get layout state too
        let physical_layout = state
            .properties
            .get("layout-mode")
            .and_then(|v| u32::try_from(v).ok())
            == Some(2);
        OutputSnapshot {
            heads,
            physical_layout,
        }
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
            // Carry forward the existing color-mode for untouched logical
            // monitors so that re-applying a plan doesn't silently revert
            // HDR-enabled monitors to SDR.
            let monitors: Vec<MonitorConfig> = lm
                .monitors
                .iter()
                .filter_map(|id| {
                    let monitor = state
                        .monitors
                        .iter()
                        .find(|m| m.id.connector == id.connector)?;
                    let current = monitor.modes.iter().find(|m| m.is_current())?;
                    let mut properties: ApplyPropertyMap = HashMap::new();
                    if let Some(cm) = monitor.properties.get("color-mode")
                        && let Ok(cm) = cm.try_clone()
                    {
                        properties.insert("color-mode".to_string(), cm);
                    }
                    Some(MonitorConfig {
                        connector: id.connector.clone(),
                        id: current.id.clone(),
                        properties,
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

            // 0 = SDR default, 1 = bt2100 HDR, 2 = SDR native.
            // An absent key resets to SDR, so carry forward the current value.
            let mut monitor_properties: ApplyPropertyMap = HashMap::new();
            if let Some(color_mode) = enable.color_mode {
                let color_mode = match color_mode {
                    ColorMode::Sdr => 0,
                    ColorMode::Hdr => 1,
                    ColorMode::SdrNative => 2,
                };
                monitor_properties.insert(
                    "color-mode".to_string(),
                    zbus::zvariant::Value::U32(color_mode)
                        .try_to_owned()
                        .unwrap(),
                );
            } else if let Some(cm) = monitor.properties.get("color-mode")
                && let Ok(cm) = cm.try_clone()
            {
                monitor_properties.insert("color-mode".into(), cm);
            }
            result.push(LogicalMonitorConfig {
                x,
                y,
                scale: enable
                    .scale
                    .or_else(|| existing.map(|lm| lm.scale))
                    .unwrap_or(mode.preferred_scale),
                transform: enable
                    .transform
                    .map(|t| t as u32)
                    .or_else(|| existing.map(|lm| lm.transform))
                    .unwrap_or(0),
                primary,
                monitors: vec![MonitorConfig {
                    connector: enable.name.clone(),
                    id: mode.id.clone(),
                    properties: monitor_properties,
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
            let width = if Transform::try_from(lmc.transform).is_ok_and(|t| t.swaps_axes()) {
                mode.height
            } else {
                mode.width
            };
            let physical = state
                .properties
                .get("layout-mode")
                .and_then(|v| u32::try_from(v).ok())
                == Some(2);
            let scale = if physical { 1.0 } else { lmc.scale };
            Some(lmc.x + (f64::from(width) / scale).round() as i32)
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
            source: Box::new(source),
        })?;
        let proxy =
            DisplayConfigProxyBlocking::new(&conn).map_err(|source| CompositorError::Dbus {
                context: "gnome: create display config proxy",
                source: Box::new(source),
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
                    source: Box::new(source),
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
                source: Box::new(source),
            })
            .map_err(Into::into)
    }

    fn proxy(&self) -> Result<DisplayConfigProxyBlocking<'_>> {
        let proxy = DisplayConfigProxyBlocking::new(&self.conn).map_err(|source| {
            CompositorError::Dbus {
                context: "gnome: create display config proxy",
                source: Box::new(source),
            }
        })?;
        Ok(proxy)
    }
}

impl CompositorAdapter for GnomeCompositor {
    fn name(&self) -> &'static str {
        "gnome"
    }

    /// Features mutter natively honors.
    fn supported_features(&self) -> HashSet<FeatureKind> {
        HashSet::from([
            FeatureKind::Primary,
            FeatureKind::Hdr,
            FeatureKind::SdrNative,
        ])
    }

    fn snapshot(&mut self) -> Result<OutputSnapshot> {
        Ok(self.current_state()?.into())
    }

    fn apply(&mut self, plan: &OutputPlan) -> Result<()> {
        let plan = plan.for_adapter(self);
        // Refresh the serial
        let state = self.current_state()?;
        let logical = plan.to_mutter_config(&state);

        let mut properties = HashMap::new();
        if state
            .properties
            .get("supports-changing-layout-mode")
            .and_then(|v| bool::try_from(v).ok())
            == Some(true)
            && let Some(layout_mode) = state.properties.get("layout-mode")
            && let Ok(layout_mode) = layout_mode.try_clone()
        {
            properties.insert("layout-mode".into(), layout_mode);
        }

        self.proxy()?
            .apply_monitors_config(state.serial, Method::Temporary, &logical, properties)
            .map_err(|source| CompositorError::Dbus {
                context: "gnome: apply monitors config",
                source: Box::new(source),
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compositor::EnableOutput;
    use display_config::{LogicalMonitor, Monitor, MonitorIdentity, MonitorMode};

    fn state() -> CurrentState {
        let id = MonitorIdentity {
            connector: "DP-1".into(),
            vendor: "test".into(),
            product: "panel".into(),
            serial: "1".into(),
        };
        CurrentState {
            serial: 1,
            monitors: vec![Monitor {
                id: id.clone(),
                modes: vec![MonitorMode {
                    id: "mode".into(),
                    width: 2560,
                    height: 1440,
                    refresh_rate: 60.0,
                    preferred_scale: 1.0,
                    supported_scales: vec![1.0, 1.5],
                    properties: HashMap::from([("is-current".into(), true.into())]),
                }],
                properties: HashMap::from([("color-mode".into(), 1u32.into())]),
            }],
            logical_monitors: vec![LogicalMonitor {
                x: 0,
                y: 0,
                scale: 1.5,
                transform: 1,
                primary: true,
                monitors: vec![id],
                properties: HashMap::new(),
            }],
            properties: HashMap::from([("layout-mode".into(), 1u32.into())]),
        }
    }

    fn color_mode(config: &LogicalMonitorConfig) -> u32 {
        u32::try_from(&config.monitors[0].properties["color-mode"]).unwrap()
    }

    #[test]
    fn snapshot_reads_physical_monitor_hdr_and_restores_geometry() {
        for (original_mode, expected) in [
            (0u32, ColorMode::Sdr),
            (1, ColorMode::Hdr),
            (2, ColorMode::SdrNative),
        ] {
            let mut state = state();
            state.monitors[0]
                .properties
                .insert("color-mode".into(), original_mode.into());
            let snapshot: OutputSnapshot = state.clone().into();
            assert_eq!(snapshot.heads[0].color_mode, Some(expected));
            assert_eq!(snapshot.right_edge(), 960);
            state.logical_monitors.clear();
            state.monitors[0]
                .properties
                .insert("color-mode".into(), 0u32.into());
            let plan = OutputPlan::builder()
                .enable_all(snapshot.active_enables())
                .unwrap()
                .build();
            let config = plan.to_mutter_config(&state);
            assert_eq!(config[0].scale, 1.5);
            assert_eq!(config[0].transform, 1);
            assert_eq!(color_mode(&config[0]), original_mode);
        }

        let plan = OutputPlan::builder()
            .enable(EnableOutput {
                name: "DP-1".into(),
                color_mode: Some(ColorMode::Sdr),
                ..Default::default()
            })
            .unwrap()
            .build();
        assert_eq!(color_mode(&plan.to_mutter_config(&state())[0]), 0);
    }

    #[test]
    fn placement_uses_rotated_scaled_size_and_layout_mode() {
        let mut state = state();
        let placed = OutputPlan::default().to_mutter_config(&state);
        assert_eq!(right_edge(&placed, &state), 960);
        state.properties.insert("layout-mode".into(), 2u32.into());
        assert_eq!(right_edge(&placed, &state), 1440);
        let snapshot: OutputSnapshot = state.into();
        assert_eq!(snapshot.right_edge(), 1440);
    }

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
