//! Shape's F1 help: what the keys on screen do now, then how Shape works and what it never does.

use super::*;

impl Shape {
    /// Help for what is on screen now: what to do, then how it works with the paths it reads.
    pub fn help_text(&self) -> String {
        let agent = self.agent.as_deref();
        let back = match &self.open {
            Some(open) if open.running.is_some() => "Esc stop",
            Some(Opened {
                aside: Some((Aside::TeachBack, _)),
                ..
            }) => "Esc cancel teach-back",
            Some(Opened {
                aside: Some((Aside::Change, _)),
                ..
            }) => "Esc cancel change",
            Some(open) if open.form.is_some() => "Esc cancel form",
            _ => "Esc back",
        };
        let (now, how) = match &self.open {
            Some(open) => {
                let failure = match &open.reading {
                    Reading::Done(Err(error)) => format!("What went wrong:\n{error}\n\n"),
                    _ => String::new(),
                };
                let model = agent.map_or_else(
                    || {
                        if self.held {
                            HELD
                        } else {
                            "No model is set up, so Shape only reads."
                        }
                        .into()
                    },
                    |a| {
                        let step = if open.offers_teach_back() {
                            "Each teach-back comparison uses one drafting run, without repository access."
                        } else {
                            "Asking the model uses one drafting run. Read-only tools may need several provider requests; manual actions may use none."
                        };
                        format!(
                            "{} {step}\n\
                             Tinkery saves every change; the model never writes the session.",
                            a.left()
                        )
                    },
                );
                let reads = match (agent, &open.folder) {
                    (Some(_), Folder::Known(folder)) => {
                        format!("\nThe next step reads files in: {}", folder.display())
                    }
                    _ => String::new(),
                };
                let now = match (agent, open.typing()) {
                    _ if open.running.is_some() => "A step is running. Esc stops it.",
                    _ if open.form.is_some() => {
                        "Edit the form. Enter adds a line; Esc cancels without saving."
                    }
                    _ if open.writing.is_some() => "Tinkery is saving a change.",
                    _ if open.objecting => {
                        "Press the number of the assumption you object to; it becomes an open question. Esc keeps them all."
                    }
                    _ if open.asking == Some(Ending::Completed) => {
                        "F2 ends this session as completed, for good; Esc keeps it open."
                    }
                    _ if open.asking.is_some() => {
                        "F2 stops this session for good; Esc keeps it open."
                    }
                    _ if open.choosing.is_some() => {
                        "Type the folder the next step should read. Enter adds a line; F2 uses it; Esc keeps the one shown."
                    }
                    (
                        Some(_),
                        Some(Typing {
                            act: Act::TeachBack { .. },
                            ..
                        }),
                    ) => {
                        "Say what will and won't be built in your own words. F2 saves and compares (one drafting run); Esc cancels. Mismatches are guesses and never block confirming."
                    }
                    (
                        Some(_),
                        Some(Typing {
                            act: Act::Change { .. },
                            ..
                        }),
                    ) => {
                        "Say what should change in the draft. F2 saves your words as your decision and redrafts (one drafting run); Esc cancels. Nothing is confirmed."
                    }
                    (
                        Some(_),
                        Some(Typing {
                            act: Act::Keep { .. },
                            ..
                        }),
                    ) => {
                        "Type why you're keeping this answer. F2 keeps it unchanged with your reason; no model runs."
                    }
                    (
                        Some(_),
                        Some(Typing {
                            act: Act::Answer { .. },
                            ..
                        }),
                    ) => {
                        concat!(
                            "Type your answer. Enter adds a line. F2 saves it and looks into the next question. A model step uses one drafting run. ",
                            "PgUp/PgDn scroll the reading."
                        )
                    }
                    (
                        Some(_),
                        Some(Typing {
                            act: Act::Confirm { .. },
                            ..
                        }),
                    ) => {
                        concat!(
                            "Read the seed, type confirm, then F2 confirms it. To change the draft instead, ",
                            "F1 Revise the draft. It is not approval to build. ",
                            "Without a completed teach-back, confirming records: Skipped teach-back."
                        )
                    }
                    (Some(_), Some(_)) => {
                        "Edit the form. Enter adds a line; Esc cancels without saving."
                    }
                    _ => "Read where this goal's seed stands. ↑↓ or PgUp/PgDn scroll.",
                };
                let form = open
                    .form
                    .as_ref()
                    .map(|f| format!("\n{}", f.description()))
                    .unwrap_or_default();
                (
                    now,
                    format!(
                        "{failure}{model}{reads}{form}\nSession: {}",
                        open.session.display()
                    ),
                )
            }
            None => (
                "↑↓ / j k choose a goal waiting for a seed · Enter opens it.",
                match &self.list {
                    Some(Some(Err(error))) => format!("What went wrong:\n{error}"),
                    Some(Some(Ok(_))) if self.unreadable > 0 => self.discovery_help(),
                    _ => "Goals whose seed is ready, ended or unknown are in tend.".into(),
                },
            ),
        };
        let promise = if self.held {
            "Shape reads and calls no model. An agent answers Shape with --shape-pi; Esc goes back."
                .to_owned()
        } else if agent.is_some() {
            "Opening, scrolling and typing write nothing. F2 runs the one action the key bar names; F1 lists the rest by number. \
                 Saving an answer, a deferral or a delegation then looks into the next; only F2 on a drafted seed confirms it, with your words. \
                 Revision saves and looks, asking recorded dependents before a model call. \
                 Ending a session, from F1, asks once more, then ends it for good."
                .to_owned()
        } else {
            "Shape reads and calls no model; its only write is ending a session, from F1, which asks once more first. \
                 To set up Shape, press Esc until the menu, then m: choose a model and runs, and s saves them. \
                 Then q quits; start Tinkery again, keeping the same --seed-session-root if you supplied one. \
                 Open Tend (2), select this saved goal, press Enter, then s to reopen Shape."
                .to_owned()
        };
        let editing = if self.takes_text() {
            "\nArrows, Home/End and Ctrl+Left/Right move; Shift selects; Ctrl+W or Alt+Backspace deletes a word.\n\
             Ctrl+U undoes, Ctrl+R redoes; PgUp/PgDn scroll the reading."
        } else {
            ""
        };
        // The numbered actions come first, so even a short panel shows them without scrolling.
        let rows = self.actions.text(&self.rows());
        let rows = if rows.is_empty() {
            rows
        } else {
            format!("{rows}\n\n")
        };
        format!(
            "Right now\n{rows}{now}{editing}\n{back}\n\nF1 close\n\nHow it works\n{how}\n\n{promise}"
        )
    }
}
