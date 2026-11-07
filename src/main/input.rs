//! Ordered terminal input that wakes its owning UI thread. The bounded queue preserves events;
//! mouse input stays held after resize until the source acknowledges that resize's drain.
//! Dropping the reader stops and joins its worker before terminal mode is restored.

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
        Self::start(|timeout| {
            if event::poll(timeout)? {
                event::read().map(Some)
            } else {
                Ok(None)
            }
        })
    }

    fn start(
        mut read: impl FnMut(Duration) -> io::Result<Option<Event>> + Send + 'static,
    ) -> io::Result<Self> {
        let (send, events) = mpsc::sync_channel(64);
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let drain = Arc::new(AtomicU64::new(0));
        let requested = drain.clone();
        let owner = thread::current();
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
        Ok(Self {
            events: Some(events),
            pending: None,
            drain,
            mouse_ready: true,
            stop,
            worker: Some(worker),
        })
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
            let mut input = Input::start(move |timeout| match receive.recv_timeout(timeout) {
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
        let mut input = Input::start(|_| Err(io::Error::other("TEST input failure"))).unwrap();
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
        let input = Input::start(move |timeout| {
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

    #[test]
    fn dropping_a_full_input_queue_stops_and_joins_the_reader() {
        let input = Input::start(|_| Ok(Some(key('x')))).unwrap();
        thread::sleep(Duration::from_millis(30));
        let started = std::time::Instant::now();
        drop(input);
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
