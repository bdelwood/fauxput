//! Shared Wayland-client glue (connection, registry, dispatch with context, bounded poll) reused by the wlr and KDE adapters.

use std::os::fd::AsFd;
use std::thread;
use std::time::{Duration, Instant};

use rustix::event::{self, PollFd, PollFlags, Timespec};
use rustix::io::Errno;
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

    /// Bounded event-driven wait. Drains anything sitting on the queue,
    /// calls `poll`, and, if `poll` returns `None`, blocks on the wayland
    /// socket itself until either the server sends new events
    /// or the deadline elapses.
    /// wake up the moment the compositor writes to its socket
    pub fn poll_until<T, F>(
        &mut self,
        reason: impl Into<String>,
        timeout: Duration,
        mut poll: F,
    ) -> Result<T>
    where
        F: FnMut(&mut Self) -> Result<Option<T>>,
    {
        let reason = reason.into();
        let deadline = Instant::now() + timeout;
        loop {
            self.eq
                .dispatch_pending(&mut self.state)
                .map_err(|source| CompositorError::Dispatch {
                    context: "wayland: dispatch_pending",
                    source,
                })?;
            if let Some(value) = poll(self)? {
                return Ok(value);
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(CompositorError::Timeout { reason, timeout }.into());
            };

            // Block on the wayland fd until events arrive or `remaining` elapses.
            let fd = self.conn.as_fd();
            let ts = Timespec {
                tv_sec: remaining.as_secs() as i64,
                tv_nsec: remaining.subsec_nanos() as i64,
            };
            let mut pollfds = [PollFd::new(&fd, PollFlags::IN)];
            loop {
                match event::poll(&mut pollfds, Some(&ts)) {
                    Ok(_) => break,
                    Err(Errno::INTR) => continue,
                    Err(err) => {
                        log::warn!("wayland fd poll failed: {err}; sleeping briefly and retrying");
                        thread::sleep(Duration::from_millis(50).min(remaining));
                        break;
                    }
                }
            }
        }
    }
}
