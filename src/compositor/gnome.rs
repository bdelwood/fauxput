mod display_config;

use self::display_config::DisplayConfigProxyBlocking;
use crate::Result;
use crate::compositor::{CompositorAdapter, CompositorError, FeatureKind, OutputSnapshot};
use std::collections::HashSet;

use zbus::blocking::Connection;

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

    fn current_state(&self) -> Result<display_config::CurrentState> {
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
        todo!()
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

    /// Live GNOME/Mutter smoke test for the blocking D-Bus binding.
    ///
    /// Requires:
    ///   - a running GNOME Wayland session
    ///   - access to that session's D-Bus user bus
    ///
    /// Run with:
    ///   `cargo test -- --ignored gnome_connects_and_reads_current_state`
    #[test]
    #[ignore]
    fn gnome_connects_and_reads_current_state() {
        let Some(compositor) = GnomeCompositor::connect().expect("GNOME connect failed") else {
            eprintln!("skipping: org.gnome.Mutter.DisplayConfig unavailable in this session");
            return;
        };

        let state = compositor.current_state().expect("GetCurrentState failed");

        assert!(
            !state.monitors.is_empty(),
            "GNOME should report at least one monitor"
        );
        assert!(
            state
                .logical_monitors
                .iter()
                .any(|logical| !logical.monitors.is_empty()),
            "GNOME should report at least one logical monitor with attached outputs"
        );
    }
}
