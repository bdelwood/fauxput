mod display_config;

use self::display_config::{CurrentState, DisplayConfigProxyBlocking};
use crate::Result;
use crate::compositor::{
    CompositorAdapter, CompositorError, FeatureKind, HeadState, ModeInfo, OutputSnapshot, Transform,
};

use std::collections::HashSet;
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
        eprintln!("gnome: GetCurrentState returned {state:#?}");

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
        timeout: std::time::Duration,
    ) -> Result<super::HeadState> {
        todo!()
    }

    fn apply(&mut self, plan: &super::OutputPlan) -> Result<()> {
        todo!()
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
