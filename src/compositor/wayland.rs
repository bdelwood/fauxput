//! Shared Wayland-client glue (connection, registry, dispatch with context, bounded poll) reused by the wlr and KDE adapters.

use std::io::ErrorKind;
use std::os::fd::OwnedFd;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use rustix::event::{self, PollFd, PollFlags, Timespec};
use rustix::io::Errno;
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle,
    backend::{Backend, ObjectData, ObjectId, WaylandError, protocol::Message},
    protocol::{wl_display, wl_registry},
};

use crate::Result;
use crate::compositor::CompositorError;

/// Owns the wayland-client connection, its event queue, and the
/// per-adapter state that keeps track of globally mutated state.
/// Generic state S must implement dispatch for every protocol used
pub struct WaylandSession<S> {
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

    /// Wait up to five seconds for the server to process pending requests,
    /// dispatching incoming events into the state.
    pub fn roundtrip(&mut self, ctx: &'static str) -> Result<()> {
        let done = Arc::new(SyncDone::default());
        self.conn
            .send_request(
                &self.conn.display(),
                wl_display::Request::Sync {},
                Some(done.clone()),
            )
            .map_err(|_| CompositorError::CompositorWentAway)?;
        self.poll_until(ctx, Duration::from_secs(5), |_| {
            Ok(done.0.load(Ordering::Relaxed).then_some(()))
        })
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

            // Prepare before polling so pending events cannot race the socket read.
            let Some(reader) = self.eq.prepare_read() else {
                continue;
            };
            self.flush("wayland: poll flush")?;
            let fd = reader.connection_fd();
            let ts = Timespec {
                tv_sec: remaining.as_secs() as i64,
                tv_nsec: remaining.subsec_nanos() as i64,
            };
            let mut pollfds = [PollFd::new(&fd, PollFlags::IN)];
            match event::poll(&mut pollfds, Some(&ts)) {
                Ok(0) | Err(Errno::INTR) => continue,
                Ok(_) => {}
                Err(err) => return Err(std::io::Error::from(err).into()),
            }
            match reader.read() {
                Ok(_) => {}
                Err(WaylandError::Io(err)) if err.kind() == ErrorKind::WouldBlock => {}
                Err(source) => {
                    return Err(CompositorError::Dispatch {
                        context: "wayland: read events",
                        source: wayland_client::DispatchError::Backend(source),
                    }
                    .into());
                }
            }
        }
    }
}

// Handle wl_display.sync centrally for every adapter's event queue.
#[derive(Default)]
struct SyncDone(AtomicBool);

impl ObjectData for SyncDone {
    fn event(
        self: Arc<Self>,
        _: &Backend,
        _: Message<ObjectId, OwnedFd>,
    ) -> Option<Arc<dyn ObjectData>> {
        self.0.store(true, Ordering::Relaxed);
        None
    }

    fn destroyed(&self, _: ObjectId) {}
}
