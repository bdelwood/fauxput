//! Shared Wayland-client glue (connection, registry, dispatch with context, bounded poll) reused by the wlr and KDE adapters.

use std::thread;
use std::time::{Duration, Instant};

use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle, protocol::wl_registry};

use crate::Result;
use crate::compositor::CompositorError;

/// Owns the wayland-client connection, its event queue, and the
/// per-adapter state that keeps track of globally mutated state.
/// Generic state S must implement dispatch for every protocol used
pub struct WaylandSession<S> {
    #[allow(dead_code)]
    conn: Connection,
    qhandle: QueueHandle<S>,
    eq: EventQueue<S>,
    pub state: S,
}

impl<S: Default + 'static> WaylandSession<S> {
    /// Connect to the wayland server.
    /// Returns `Ok(None)` when no socket is reachable, so the caller can
    /// fall through to the next adapter without bubbling up an error.
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
    /// Bind `wl_registry` and start receiving global advertisements.
    /// Caller must follow up with a `roundtrip` to actually drain the
    /// initial burst of `Global` events.
    pub fn registry(&self) -> wl_registry::WlRegistry
    where
        S: Dispatch<wl_registry::WlRegistry, ()>,
    {
        self.conn.display().get_registry(&self.qhandle, ())
    }

    /// Queue handle for binding new globals or creating child proxies.
    pub fn qhandle(&self) -> &QueueHandle<S> {
        &self.qhandle
    }

    /// Flush pending requests, block until the server has processed them,
    /// then dispatch any incoming events into the state. Use after writes
    /// that need a server ACK before reading state.
    pub fn roundtrip(&mut self, ctx: &'static str) -> Result<()> {
        self.eq
            .roundtrip(&mut self.state)
            .map_err(|source| CompositorError::Dispatch {
                context: ctx,
                source,
            })?;
        Ok(())
    }

    /// Send pending requests to the server without waiting for a reply.
    /// Use when issuing a fire-and-forget like `apply()` followed by a
    /// poll loop that watches for the server-side completion event.
    pub fn flush(&mut self, ctx: &'static str) -> Result<()> {
        self.eq
            .flush()
            .map_err(|source| CompositorError::Dispatch {
                context: ctx,
                source: wayland_client::DispatchError::Backend(source),
            })?;
        Ok(())
    }

    /// Bounded retry loop. Calls `poll` until it returns `Some(value)` or
    /// `timeout` elapses, sleeping `interval` between attempts. Used to
    /// wait for asynchronous server events (e.g. a new head appearing
    /// after a kernel hot-plug) without blocking forever.
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
            // Check the deadline after the poll so a single attempt always
            // gets to run even if the timeout has already elapsed.
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
