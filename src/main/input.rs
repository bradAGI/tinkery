//! Ordered terminal input that wakes its owning UI thread. The bounded queue preserves events;
//! mouse input stays held after resize until the source acknowledges that resize's drain.
//! A hangup-only watchdog reports terminal loss without touching key readiness. Drop joins the
//! workers on normal shutdown, but detaches a reader trapped in crossterm after terminal loss.

use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crossterm::event::{self, Event};

#[cfg(unix)]
type Terminal = std::os::fd::OwnedFd;
#[cfg(not(unix))]
type Terminal = ();

enum Message {
    Event(io::Result<Event>),
    Drained(u64),
}

/// One terminal reader; only its worker calls crossterm's input APIs.
pub(super) struct Input {
    events: Option<Receiver<Message>>,
    pending: Option<io::Result<Event>>,
    drain: Arc<AtomicU64>,
    mouse_ready: bool,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Input {
    /// Read terminal events in order, waking the caller without shortening its heartbeat.
    pub(super) fn new() -> io::Result<Self> {
        #[cfg(unix)]
        let terminal = {
            use std::{io::IsTerminal, os::fd::AsFd};
            let stdin = io::stdin();
            Some(if stdin.is_terminal() {
                stdin.as_fd().try_clone_to_owned()?
            } else {
                std::fs::File::options()
                    .read(true)
                    .write(true)
                    .open("/dev/tty")?
                    .into()
            })
        };
        #[cfg(not(unix))]
        let terminal = None;
        Self::start(terminal, |timeout| {
            if event::poll(timeout)? {
                event::read().map(Some)
            } else {
                Ok(None)
            }
        })
    }

    fn start(
        terminal: Option<Terminal>,
        mut read: impl FnMut(Duration) -> io::Result<Option<Event>> + Send + 'static,
    ) -> io::Result<Self> {
        let (send, events) = mpsc::sync_channel(64);
        #[cfg(unix)]
        let failure = send.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let drain = Arc::new(AtomicU64::new(0));
        let requested = drain.clone();
        let owner = thread::current();
        #[cfg(unix)]
        let watchdog_owner = owner.clone();
        let worker = thread::Builder::new()
            .name("terminal-input".into())
            .spawn(move || {
                let mut acknowledged = 0;
                while !cancelled.load(Ordering::SeqCst) {
                    // Snapshot before polling: an earlier empty observation cannot acknowledge
                    // a resize requested while the source read was in flight.
                    let generation = requested.load(Ordering::SeqCst);
                    let timeout = if generation == acknowledged {
                        Duration::from_millis(100)
                    } else {
                        Duration::ZERO
                    };
                    let message = match read(timeout) {
                        Ok(Some(event)) => Message::Event(Ok(event)),
                        Ok(None) if generation != acknowledged => {
                            acknowledged = generation;
                            Message::Drained(generation)
                        }
                        Ok(None) => continue,
                        Err(error) => Message::Event(Err(error)),
                    };
                    let failed = matches!(&message, Message::Event(Err(_)));
                    if send.send(message).is_err() {
                        break;
                    }
                    owner.unpark();
                    if failed {
                        break;
                    }
                }
            })?;
        let mut input = Self {
            events: Some(events),
            pending: None,
            drain,
            mouse_ready: true,
            stop,
            worker: Some(worker),
        };
        #[cfg(unix)]
        if let Some(terminal) = terminal {
            let worker = input.worker.take().unwrap();
            let stop = input.stop.clone();
            input.worker = Some(
                thread::Builder::new()
                    .name("terminal-hangup".into())
                    .spawn(move || {
                        watch_terminal(terminal, worker, failure, stop, watchdog_owner)
                    })?,
            );
        }
        #[cfg(not(unix))]
        let _ = terminal;
        Ok(input)
    }

    /// Hold mouse coordinates until the source drains input after this resize request.
    pub(super) fn invalidate_mouse(&mut self) {
        self.mouse_ready = false;
        self.drain.fetch_add(1, Ordering::SeqCst);
    }

    /// Whether a source-level drain acknowledged the latest resize, not a channel gap.
    pub(super) fn mouse_ready(&self) -> bool {
        self.mouse_ready
    }

    fn accept(&mut self, message: Message) -> Option<io::Result<Event>> {
        match message {
            Message::Event(event) => Some(event),
            Message::Drained(generation) => {
                if generation == self.drain.load(Ordering::SeqCst) {
                    self.mouse_ready = true;
                }
                None
            }
        }
    }

    /// Whether an event or input failure is ready; never blocks or discards input.
    pub(super) fn poll(&mut self) -> bool {
        while self.pending.is_none() {
            match self.events.as_ref().unwrap().try_recv() {
                Ok(message) => self.pending = self.accept(message),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.pending = Some(Err(disconnected()));
                }
            }
        }
        self.pending.is_some()
    }

    /// Take the next event, preserving an input failure rather than silently ignoring it.
    pub(super) fn read(&mut self) -> io::Result<Event> {
        loop {
            if let Some(event) = self.pending.take() {
                return event;
            }
            let message = self
                .events
                .as_ref()
                .unwrap()
                .recv()
                .map_err(|_| disconnected())?;
            if let Some(event) = self.accept(message) {
                return event;
            }
        }
    }
}

#[cfg(unix)]
fn watch_terminal(
    terminal: Terminal,
    worker: JoinHandle<()>,
    send: mpsc::SyncSender<Message>,
    stop: Arc<AtomicBool>,
    owner: thread::Thread,
) {
    use std::os::fd::AsRawFd;
    let mut descriptor = libc::pollfd {
        fd: terminal.as_raw_fd(),
        events: libc::POLLHUP,
        revents: 0,
    };
    while !worker.is_finished() {
        let result = unsafe { libc::poll(&mut descriptor, 1, 100) };
        let error = if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            Some(error)
        } else if descriptor.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
            Some(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Terminal input hung up.",
            ))
        } else {
            None
        };
        if let Some(error) = error {
            stop.store(true, Ordering::SeqCst);
            owner.unpark();
            let _ = send.send(Message::Event(Err(error)));
            owner.unpark();
            return;
        }
    }
    let _ = worker.join();
}

fn disconnected() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "Terminal input stopped.")
}

impl Drop for Input {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Unblock a producer waiting on a full queue before joining it.
        self.events.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(character: char) -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE))
    }

    #[test]
    fn input_wakes_the_owner_and_preserves_event_order() {
        thread::spawn(|| {
            let (send, receive) = mpsc::channel();
            let mut input =
                Input::start(None, move |timeout| match receive.recv_timeout(timeout) {
                    Ok(event) => Ok(Some(event)),
                    Err(_) => Ok(None),
                })
                .unwrap();
            let producer = thread::spawn(move || {
                thread::sleep(Duration::from_millis(30));
                send.send(key('a')).unwrap();
                send.send(key('b')).unwrap();
            });
            let started = std::time::Instant::now();
            thread::park_timeout(Duration::from_secs(2));
            assert!(started.elapsed() < Duration::from_secs(1));
            assert!(input.poll());
            assert!(input.poll(), "readiness must not consume an event");
            assert_eq!(input.read().unwrap(), key('a'));
            assert_eq!(input.read().unwrap(), key('b'));
            producer.join().unwrap();
        })
        .join()
        .unwrap();
    }

    #[test]
    fn input_errors_reach_the_owner() {
        let mut input =
            Input::start(None, |_| Err(io::Error::other("TEST input failure"))).unwrap();
        assert_eq!(input.read().unwrap_err().to_string(), "TEST input failure");
    }

    struct SourceBarrier {
        entered: Receiver<Duration>,
        release: mpsc::Sender<Option<Event>>,
    }

    impl SourceBarrier {
        fn entered(&self) -> Duration {
            self.entered.recv_timeout(Duration::from_secs(2)).unwrap()
        }

        fn release(&self, event: Option<Event>) {
            self.release.send(event).unwrap();
        }
    }

    fn controlled_source() -> (Input, SourceBarrier) {
        let (entered, observation) = mpsc::channel();
        let (release, source) = mpsc::channel();
        let input = Input::start(None, move |timeout| {
            entered.send(timeout).map_err(|_| disconnected())?;
            source
                .recv_timeout(Duration::from_secs(2))
                .map_err(|_| disconnected())
        })
        .unwrap();
        (
            input,
            SourceBarrier {
                entered: observation,
                release,
            },
        )
    }

    fn mouse() -> Event {
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 40,
            row: 21,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn resize_keeps_mouse_held_across_transfer_gaps_until_the_source_drains() {
        let (mut input, source) = controlled_source();
        source.entered();
        source.release(Some(Event::Resize(80, 24)));
        assert_eq!(input.read().unwrap(), Event::Resize(80, 24));
        input.invalidate_mouse();

        // The reader is held before transferring each queued source event. In particular,
        // an empty UI channel between Resize and the old-layout click is not a drain.
        for event in
            std::iter::repeat_n(key('a'), 80).chain([Event::Paste("TEST café".into()), mouse()])
        {
            source.entered();
            assert!(!input.poll());
            assert!(
                !input.mouse_ready(),
                "a transfer gap admitted stale coordinates"
            );
            source.release(Some(event.clone()));
            assert_eq!(input.read().unwrap(), event, "input was lost or reordered");
            assert!(!input.mouse_ready());
        }
        assert_eq!(source.entered(), Duration::ZERO);
        source.release(None);
        source.entered(); // The acknowledgement was queued before the next source read.
        assert!(!input.poll());
        assert!(input.mouse_ready());
        source.release(Some(mouse()));
        assert_eq!(
            input.read().unwrap(),
            mouse(),
            "fresh clicks must still work"
        );
        assert!(input.mouse_ready());
        drop(source);
        drop(input);
    }

    #[test]
    fn only_a_source_observation_after_the_latest_resize_can_release_mouse() {
        let (mut input, source) = controlled_source();
        source.entered();
        input.invalidate_mouse();
        source.release(None); // This read began before the request; it cannot acknowledge it.
        assert_eq!(source.entered(), Duration::ZERO);
        assert!(!input.poll());
        assert!(!input.mouse_ready());
        source.release(None);
        source.entered(); // First resize's acknowledgement is queued but not consumed.
        input.invalidate_mouse();
        assert!(!input.poll());
        assert!(
            !input.mouse_ready(),
            "an older acknowledgement released a newer resize"
        );
        source.release(None); // Another in-flight observation predates the latest request.
        assert_eq!(source.entered(), Duration::ZERO);
        assert!(!input.poll());
        assert!(!input.mouse_ready());
        source.release(None);
        source.entered();
        assert!(!input.poll());
        assert!(input.mouse_ready());
        drop(source);
        drop(input);
    }

    #[cfg(unix)]
    #[test]
    fn hangup_wakes_the_owner_and_drop_detaches_a_stuck_reader() {
        use std::os::unix::net::UnixStream;
        thread::spawn(|| {
            let (terminal, peer) = UnixStream::pair().unwrap();
            let (entered, reading) = mpsc::channel();
            let (release, blocked) = mpsc::channel();
            let mut input = Input::start(Some(terminal.into()), move |_| {
                entered.send(()).unwrap();
                blocked
                    .recv_timeout(Duration::from_secs(2))
                    .map_err(|_| disconnected())
            })
            .unwrap();
            reading.recv_timeout(Duration::from_secs(2)).unwrap();
            let closer = thread::spawn(move || {
                thread::sleep(Duration::from_millis(30));
                drop(peer);
            });
            let started = std::time::Instant::now();
            thread::park_timeout(Duration::from_secs(2));
            assert!(started.elapsed() < Duration::from_secs(1));
            assert_eq!(input.read().unwrap_err().kind(), io::ErrorKind::BrokenPipe);
            let started = std::time::Instant::now();
            drop(input);
            assert!(started.elapsed() < Duration::from_secs(1));
            release.send(None).unwrap();
            closer.join().unwrap();
        })
        .join()
        .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn hangup_during_drop_does_not_wait_for_the_reader() {
        use std::os::unix::net::UnixStream;
        let (terminal, peer) = UnixStream::pair().unwrap();
        let (entered, reading) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        let input = Input::start(Some(terminal.into()), move |_| {
            entered.send(()).unwrap();
            blocked
                .recv_timeout(Duration::from_secs(2))
                .map_err(|_| disconnected())
        })
        .unwrap();
        reading.recv_timeout(Duration::from_secs(2)).unwrap();
        let closer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            drop(peer);
        });
        let started = std::time::Instant::now();
        drop(input);
        assert!(started.elapsed() < Duration::from_secs(1));
        release.send(None).unwrap();
        closer.join().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn watchdog_ignores_readability_and_normal_drop_joins_the_reader() {
        use std::{io::Write, os::unix::net::UnixStream};
        let (mut terminal, mut peer) = UnixStream::pair().unwrap();
        let (send, receive) = mpsc::channel();
        let mut input =
            Input::start(
                Some(terminal.try_clone().unwrap().into()),
                move |timeout| match receive.recv_timeout(timeout) {
                    Ok(event) => Ok(Some(event)),
                    Err(_) => Ok(None),
                },
            )
            .unwrap();
        peer.write_all(b"key").unwrap();
        thread::sleep(Duration::from_millis(150));
        assert!(!input.poll());
        send.send(key('a')).unwrap();
        assert_eq!(input.read().unwrap(), key('a'));
        let mut bytes = [0; 3];
        io::Read::read_exact(&mut terminal, &mut bytes).unwrap();
        assert_eq!(&bytes, b"key");
        drop(input);
        assert!(send.send(key('b')).is_err());
    }

    #[test]
    fn dropping_a_full_input_queue_stops_and_joins_the_reader() {
        let input = Input::start(None, |_| Ok(Some(key('x')))).unwrap();
        thread::sleep(Duration::from_millis(30));
        let started = std::time::Instant::now();
        drop(input);
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
