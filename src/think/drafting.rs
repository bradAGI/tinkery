//! Think's model host: Pi or Claude Code configured for the reading and the decision review.
//!
//! A request runs the host with no tools, session, extensions or context files, so the model sees
//! only the prompt and the words sent. The process boundary (caps, timeout, cancellation) and the
//! call budget are shared in [`crate::pi`]; this module adds think's instructions and settings.
//! Construction requires a [`CallBudget`]; no request starts once its limit is reached.
//! Bundled guidance is verbatim from Seed Me c9d766760dbeac0c05c92108165f9e968bec0a66,
//! between its title and Size gate; no installed skill is read.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use serde::{Deserialize, Serialize};

pub use crate::pi::{Attempt, CallBudget, Harness, REQUEST_LIMIT, Request};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
/// One way to reach the goal, as the reading proposes it. A proposal only: saving records it as
/// an option, never as a choice.
pub struct Approach {
    /// The approach in a few words.
    pub label: String,
    /// What it gains.
    pub benefit: String,
    /// What it costs to do.
    pub cost: String,
    /// What it costs to back out of later.
    pub undo_cost: String,
}

/// Pi or Claude Code, configured for think's shaping calls: an explicit model, a thinking level,
/// and the bundled working-draft guidance as standing instructions.
pub struct ModelHost {
    pub(super) harness: Harness,
    program: PathBuf,
    pub(super) model: String,
    pub(super) thinking: String,
    timeout: Duration,
    budget: Arc<CallBudget>,
    pub(super) brain_history: std::sync::Mutex<Vec<String>>,
    pub(super) active_call: std::sync::Mutex<Option<Request>>,
}

impl ModelHost {
    /// `harness` at `program` with a model it accepts, bundled working-draft guidance and the
    /// authorized allowance. Starts nothing; zero refuses every request before the host starts.
    ///
    /// An unbudgeted host cannot be constructed:
    /// ```compile_fail,E0061
    /// use tinkery::{pi::Harness, think::drafting::ModelHost};
    /// let host = ModelHost::new(Harness::Pi, "TEST-pi".into(), "TEST/stub".into());
    /// ```
    pub fn new(
        harness: Harness,
        program: PathBuf,
        model: String,
        budget: Arc<CallBudget>,
    ) -> Result<Self, String> {
        harness.check_model(&model)?;
        Ok(Self {
            harness,
            program,
            model,
            thinking: "low".into(),
            timeout: Duration::from_secs(90),
            budget,
            brain_history: std::sync::Mutex::new(Vec::new()),
            active_call: std::sync::Mutex::new(None),
        })
    }

    /// Reuses the configuration for a separate goal, with an empty check history and the same
    /// budget. No process, provider request or file read occurs; an older goal's running job keeps
    /// its own history.
    pub fn new_session(&self) -> Self {
        Self {
            harness: self.harness,
            program: self.program.clone(),
            model: self.model.clone(),
            thinking: self.thinking.clone(),
            timeout: self.timeout,
            budget: self.budget.clone(),
            brain_history: std::sync::Mutex::new(Vec::new()),
            active_call: std::sync::Mutex::new(None),
        }
    }

    /// Sets the thinking level; refuses one the harness doesn't name. The default is `low`.
    pub fn with_thinking(mut self, level: &str) -> Result<Self, String> {
        self.harness.check_thinking(level)?;
        self.thinking = level.into();
        Ok(self)
    }

    pub(super) fn working_instructions(&self) -> &str {
        include_str!("working-draft.txt")
            .split_once("### Shape the working draft")
            .expect("Bundled working-draft boundary")
            .1
    }

    pub(super) fn budget(&self) -> &CallBudget {
        &self.budget
    }

    /// Leaves the thinking level to the host's own default.
    pub fn with_default_thinking(mut self) -> Self {
        self.thinking.clear();
        self
    }

    pub(super) fn complete(
        &self,
        kind: Request,
        input: Vec<u8>,
        prompt: String,
        cancelled: &AtomicBool,
    ) -> Result<String, String> {
        *self.active_call.lock().unwrap() = Some(kind);
        let result = crate::pi::Call {
            harness: self.harness,
            program: self.program.clone(),
            model: self.model.clone(),
            thinking: self.thinking.clone(),
            read_only_tools: false,
            folder: None,
            timeout: self.timeout,
        }
        .complete(&self.budget, kind, input, prompt, cancelled);
        *self.active_call.lock().unwrap() = None;
        result
    }
}

pub(super) struct Job<T> {
    cancelled: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<T, String>>,
    worker: Option<JoinHandle<()>>,
}

impl<T: Send + 'static> Job<T> {
    pub(super) fn launch(
        work: impl FnOnce(&AtomicBool) -> Result<T, String> + Send + 'static,
    ) -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancelled.clone();
        let (sender, result) = mpsc::channel();
        let worker = thread::spawn(move || {
            let _ = sender.send(work(&worker_cancel));
        });
        Self {
            cancelled,
            result,
            worker: Some(worker),
        }
    }

    pub(super) fn poll(&self) -> Option<Result<T, String>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Draft worker stopped unexpectedly.".into()))
            }
        }
    }
}

impl<T> Drop for Job<T> {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
#[path = "drafting_tests.rs"]
mod tests;
