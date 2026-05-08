//! Shared Wayland-client glue (connection, registry, dispatch with context, bounded poll) reused by the wlr and KDE adapters.

use std::thread;
use std::time::{Duration, Instant};

use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, protocol::wl_registry};

use crate::Result;
use crate::compositor::CompositorError;

pub struct WaylandSession<S> {
    #[allow(dead_code)]
    conn: Connection,
    qhandle: QueueHandle<S>,
    eq: EventQueue<S>,
    pub state: S,
}

impl<S: Default + 'static> WaylandSession<S> {
    pub fn connect_to_env() -> Result<Option<Self>> {
        let conn = match Connection::connect_to_env() {
            Ok(conn) => conn,
            Err(_) => return Ok(None),
        };
        let eq = conn.new_event_queue::<S>();
        let qhandle = eq.handle();
        Ok(Some(Self {
            conn,
            qhandle,
            eq,
            state: S::default(),
        }))
    }
}

impl<S: 'static> WaylandSession<S> {
    pub fn registry(&self) -> wl_registry::WlRegistry
    where
        S: Dispatch<wl_registry::WlRegistry, ()>,
    {
        self.conn.display().get_registry(&self.qhandle, ())
    }

    pub fn qhandle(&self) -> &QueueHandle<S> {
        &self.qhandle
    }

    pub fn roundtrip(&mut self, ctx: &'static str) -> Result<()> {
        self.eq
            .roundtrip(&mut self.state)
            .map_err(|source| CompositorError::Dispatch {
                context: ctx,
                source,
            })?;
        Ok(())
    }

    pub fn flush(&mut self, ctx: &'static str) -> Result<()> {
        self.eq
            .flush()
            .map_err(|source| CompositorError::Dispatch {
                context: ctx,
                source: wayland_client::DispatchError::Backend(source),
            })?;
        Ok(())
    }

    pub fn poll_until<T, F>(
        &mut self,
        reason: impl Into<String>,
        timeout: Duration,
        interval: Duration,
        mut poll: F,
    ) -> Result<T>
    where
        F: FnMut(&mut Self) -> Result<Option<T>>,
    {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(value) = poll(self)? {
                return Ok(value);
            }
            if Instant::now() >= deadline {
                return Err(CompositorError::Timeout {
                    reason: reason.into(),
                    timeout,
                }
                .into());
            }
            if !interval.is_zero() {
                thread::sleep(interval);
            }
        }
    }
}
