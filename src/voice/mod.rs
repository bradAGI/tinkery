//! Local dictation, owned end to end: consent, setup, takes and their text.
//!
//! Think sees only [`Voice`]: start or stop a take, read its live partial text, and receive one
//! [`Final`] per take. Behind [`VoiceHost`] sit the helper process, its JSON-lines protocol and
//! the engine install, so think never handles a process, a microphone or a download. Nothing
//! records until the operator starts a take, nothing installs without a yes, no audio is kept,
//! and no text leaves the machine from here. `docs/design/voice.md` has the lifecycle.

mod process;
mod protocol;
mod setup;
pub use setup::{Consent, key_label};

pub use process::ProcessHost;
pub use protocol::{Command, Event, Failure};
use std::time::{Duration, Instant};

/// Where takes actually run. [`ProcessHost`] runs the Node helper; tests use fakes, so no test
/// needs a microphone or a native engine.
pub trait VoiceHost {
    /// The consent record answers are kept in. A host that shares one across visits makes a
    /// "no" stick for the whole run; the default forgets it with the host.
    fn setup_session(&self) -> std::sync::Arc<std::sync::Mutex<Consent>> {
        Default::default()
    }
    /// Hands one command to the helper without waiting for its answer. An error means the
    /// helper can't be reached, and voice closes.
    fn send(&mut self, command: Command) -> Result<(), Failure>;
    /// The next event the helper sent, if any. Never blocks.
    fn poll(&mut self) -> Option<Event>;
    /// Stops the helper and releases the microphone.
    fn shutdown(&mut self);
    /// Starts a fresh helper after a shutdown or failure.
    fn restart(&mut self);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Loading,
    Setup,
    Installing,
    Idle,
    Starting,
    Listening,
    Finalizing,
    Closed,
}

/// The finished text of one take.
pub struct Final {
    /// What was heard, to insert at the cursor. Still editable; it is never sent by itself.
    pub text: String,
    /// The operator asked to send during the take, so the caller sends once this is inserted.
    pub send: bool,
}

/// One think visit's dictation: whether voice is ready, the current take, and the one line that
/// says what's wrong when something is. Take ids keep a late event from an old take out of a
/// new one, and every wait has a deadline, so voice can fail but never hang think.
pub struct Voice {
    host: Box<dyn VoiceHost>,
    phase: Phase,
    available: bool,
    next_take: u64,
    take: Option<u64>,
    partial: String,
    pending_send: bool,
    deadline: Option<Instant>,
    notice: String,
    failure: Option<Failure>,
    setup: setup::Setup,
}

impl Voice {
    /// Voice on `host`, loading. It reports ready, or what it needs, through [`Voice::tick`].
    pub fn new(host: Box<dyn VoiceHost>) -> Self {
        let setup = setup::Setup {
            session: host.setup_session(),
            ..Default::default()
        };
        Self {
            host,
            setup,
            phase: Phase::Loading,
            available: false,
            next_take: 0,
            take: None,
            partial: String::new(),
            pending_send: false,
            deadline: Some(Instant::now() + Duration::from_secs(30)),
            notice: String::new(),
            failure: None,
        }
    }
    /// Ready to start a take.
    pub fn available(&self) -> bool {
        self.available
    }
    /// Waiting on a yes or no to an install or model download.
    pub fn setup_prompt(&self) -> bool {
        self.setup.prompt
    }
    /// The operator's answer to the setup prompt. Yes starts the install; no is remembered
    /// for the rest of the run, so the prompt doesn't come back.
    pub fn answer_setup(&mut self, yes: bool, now: Instant) {
        if let Some(command) = self.setup.answer(yes) {
            self.begin_setup(command, now);
        } else {
            self.notice.clear();
        }
    }
    fn begin_setup(&mut self, command: Command, now: Instant) {
        self.phase = Phase::Installing;
        self.setup.prompt = false;
        self.notice = self.setup.need.unwrap().progress().into();
        self.failure = None;
        self.deadline = Some(now + Duration::from_secs(600));
        self.command(command);
    }
    fn request_setup(&mut self, now: Instant) {
        if let Some(command) = self.setup.request() {
            self.begin_setup(command, now);
        } else if self.setup.prompt {
            self.notice = self.setup.message.clone();
        } else {
            self.notice.clear();
        }
    }
    /// A take is running or finishing.
    pub fn active(&self) -> bool {
        self.take.is_some()
    }
    /// The running take's text so far, shown muted; empty when there is none.
    pub fn partial(&self) -> &str {
        &self.partial
    }
    /// The one line to show about voice now. Missing Node is mentioned only on request.
    pub fn notice(&self) -> &str {
        let voice_requested = self.setup.requested || self.next_take > 0;
        if !voice_requested && self.failure().is_some_and(|error| error.kind == "no-node") {
            return "";
        }
        &self.notice
    }
    /// The last failure, kept until voice next works.
    pub fn failure(&self) -> Option<&Failure> {
        self.failure.as_ref()
    }
    /// Ends the take, if there is one, and returns to Idle. Without a take the phase is not
    /// a take's, so it stays as it is.
    fn clear_take(&mut self) {
        if self.take.take().is_none() {
            return;
        }
        self.partial.clear();
        self.pending_send = false;
        self.deadline = None;
        self.phase = Phase::Idle;
    }
    /// Ends any take and shows `failure` as one plain line.
    pub fn reject(&mut self, failure: Failure) {
        self.cancel();
        self.notice = failure
            .message
            .replace(['\r', '\n'], " ")
            .chars()
            .filter(|c| !c.is_control())
            .take(1024)
            .collect();
        self.failure = Some(failure);
    }
    fn command(&mut self, command: Command) {
        if let Err(failure) = self.host.send(command) {
            self.clear_take();
            self.reject(failure);
            self.setup.prompt = false;
            self.shut();
        }
    }
    /// Stops the helper: Closed, unavailable, with no deadline left to fire. The voice key
    /// restarts it.
    fn shut(&mut self) {
        self.host.shutdown();
        self.available = false;
        self.deadline = None;
        self.phase = Phase::Closed;
    }
    /// The voice key: starts a take, or stops the running one. When voice isn't ready it says
    /// why, or asks to set up, instead.
    pub fn toggle(&mut self, now: Instant) {
        if self.active() {
            self.stop(false, now);
            return;
        }
        if self.phase == Phase::Installing {
            return;
        }
        if self.phase == Phase::Closed {
            self.setup.requested = true;
            self.host.restart();
            self.phase = Phase::Loading;
            self.deadline = Some(now + Duration::from_secs(30));
        }
        if self.setup.need.is_some() && self.phase == Phase::Setup {
            self.request_setup(now);
            return;
        }
        if !self.available {
            self.setup.requested = true;
            if self.failure.is_none() {
                self.notice = "Voice is loading. Press the voice key when ready.".into();
            }
            return;
        }
        self.next_take += 1;
        self.take = Some(self.next_take);
        self.phase = Phase::Starting;
        self.notice.clear();
        self.failure = None;
        self.deadline = Some(now + Duration::from_secs(30));
        self.command(Command::Start {
            take: self.next_take,
        });
    }
    /// Stops the take and waits for its final text. `send` asks for that text to be sent once it
    /// arrives (F2 during a take).
    pub fn stop(&mut self, send: bool, now: Instant) {
        let Some(take) = self.take else {
            return;
        };
        self.pending_send |= send;
        if self.phase != Phase::Finalizing {
            self.phase = Phase::Finalizing;
            self.deadline = Some(now + Duration::from_secs(30));
            self.command(Command::Stop { take });
        }
    }
    /// Discards the take; no text arrives from it.
    pub fn cancel(&mut self) {
        let take = self.take;
        self.clear_take();
        if let Some(take) = take {
            self.command(Command::Cancel { take });
        }
    }
    /// Ends voice for this visit: cancels the take and stops the helper. Also runs on drop.
    pub fn close(&mut self) {
        self.cancel();
        self.shut();
        self.setup.reset();
        self.notice.clear();
        self.failure = None;
    }
    /// Reads what the helper sent and enforces deadlines. Returns the take's text once it
    /// is final.
    pub fn tick(&mut self, now: Instant) -> Option<Final> {
        if self.deadline.is_some_and(|end| now >= end) {
            self.close();
            self.reject(Failure::new(
                "timeout",
                "Voice timed out. Press the voice key to retry.",
            ));
            return None;
        }
        for _ in 0..64 {
            let Some(event) = self.host.poll() else {
                break;
            };
            let needed = match &event {
                Event::NeedsInstall { .. } => Some(setup::Need::Install),
                Event::NeedsModel { .. } => Some(setup::Need::Model),
                _ => None,
            };
            match event {
                Event::NeedsInstall { size_mb, message }
                | Event::NeedsModel { size_mb, message }
                    if !self.active()
                        && matches!(
                            self.phase,
                            Phase::Loading | Phase::Setup | Phase::Installing
                        ) =>
                {
                    if size_mb == 0 || message.len() > 1024 {
                        self.close();
                        self.reject(Failure::new("protocol", "Invalid voice setup response."));
                        continue;
                    }
                    self.setup.need = needed;
                    self.setup.message = message;
                    self.phase = Phase::Setup;
                    self.available = false;
                    self.deadline = None;
                    self.failure = None;
                    self.notice.clear();
                    if self.setup.requested {
                        self.request_setup(now);
                    }
                }
                Event::Loading
                    if !self.active()
                        && matches!(self.phase, Phase::Loading | Phase::Installing) =>
                {
                    self.phase = Phase::Loading;
                    self.deadline = Some(now + Duration::from_secs(30));
                }
                Event::Ready { version } if self.phase == Phase::Loading => {
                    if version != protocol::VERSION {
                        self.close();
                        self.reject(Failure::new(
                            "protocol",
                            "Incompatible voice protocol. Restart think.",
                        ));
                    } else {
                        self.available = true;
                        self.setup.reset();
                        self.phase = Phase::Idle;
                        self.deadline = None;
                        self.notice.clear();
                        self.failure = None;
                    }
                }
                Event::Listening { take } if self.take == Some(take) => {
                    if self.phase != Phase::Finalizing {
                        self.phase = Phase::Listening;
                        self.deadline = None;
                    }
                }
                Event::Partial { take, text } if self.take == Some(take) => {
                    if text.len() > 4096 {
                        self.reject(Failure::new(
                            "capture",
                            "Voice text exceeds 4096 bytes. Try a shorter take.",
                        ));
                    } else {
                        self.partial = text;
                    }
                }
                Event::Final { take, text } if self.take == Some(take) => {
                    let send = self.pending_send;
                    self.clear_take();
                    return Some(Final { text, send });
                }
                Event::Error {
                    take,
                    kind,
                    message,
                } if take.is_none() || take == self.take => {
                    let global = take.is_none();
                    self.reject(Failure::new(&kind, &message));
                    if global {
                        self.setup.prompt = false;
                        self.shut();
                    }
                }
                _ => {}
            }
        }
        None
    }
    /// Says a take heard nothing worth inserting.
    pub fn missed(&mut self) {
        self.notice = "Didn't catch that.".into();
    }
}
impl Drop for Voice {
    fn drop(&mut self) {
        self.close();
    }
}

/// Text speech engines produce from silence ("Thank you.", "you"). Inserting it would put
/// words in the operator's mouth, so it is treated as nothing heard.
pub fn silence_filler(text: &str) -> bool {
    matches!(
        text.trim()
            .to_lowercase()
            .trim_end_matches(['.', '!', '?'])
            .trim(),
        "" | "thank you" | "you"
    )
}

#[cfg(test)]
mod setup_tests;
#[cfg(test)]
mod tests;
