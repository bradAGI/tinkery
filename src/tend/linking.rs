//! Explicit link picking, safe refusals and derived garden wording, independent of drawing.

use super::{Session, Tend, row};
use crate::clipped;
use unicode_width::UnicodeWidthStr;

#[cfg(test)]
mod tests;

pub(super) const UNAVAILABLE: &str =
    "Links couldn't be read, so nothing changed. F1 then h for details.";

impl Tend {
    pub(crate) fn is_linking(&self) -> bool {
        self.linking.is_some()
    }

    pub(crate) fn start_link(&mut self) {
        if !self.garden_showing() || self.linking.is_some() {
            return;
        }
        self.notice.clear();
        self.last_press = None;
        self.link_error = None;
        if let Err(reason) = self.link_id(self.current().unwrap()) {
            self.refuse_link("This goal can't be linked yet.", reason);
            return;
        }
        self.links = self.needs.read().into_links();
        self.prepare_graph();
        if let Err(reason) = &self.links {
            self.refuse_link(UNAVAILABLE, reason.clone());
            return;
        }
        self.linking = self.current().map(|s| s.path.clone());
    }

    pub(crate) fn cancel_link(&mut self) {
        if let Some(path) = self.linking.take() {
            if let Some(index) = self.sessions.iter().position(|s| s.path == path) {
                self.selected = index;
            }
            self.last_press = None;
        }
    }

    fn source(&self) -> Option<&Session> {
        let path = self.linking.as_ref()?;
        self.sessions.iter().find(|s| s.path == *path)
    }

    fn link_id<'a>(&self, session: &'a Session) -> Result<&'a str, String> {
        let id = session
            .id
            .as_deref()
            .filter(|id| !id.is_empty() && !id.chars().any(char::is_control))
            .ok_or_else(|| {
                format!(
                    "Cannot link {}: no valid ledger session id.",
                    session.path.display()
                )
            })?;
        if self
            .all_sessions
            .iter()
            .filter(|s| s.id.as_deref() == Some(id))
            .count()
            != 1
        {
            return Err(format!(
                "Cannot link {}: its ledger session id is not unique.",
                session.path.display()
            ));
        }
        Ok(id)
    }

    pub(crate) fn commit_link(&mut self) {
        let Some(source) = self.source() else { return };
        let Some(target) = self.current() else { return };
        if source.path == target.path {
            return;
        }
        let ids = self.link_id(source).and_then(|source| {
            self.link_id(target)
                .map(|target| (source.to_owned(), target.to_owned()))
        });
        let (source, target) = match ids {
            Ok(ids) => ids,
            Err(reason) => {
                self.refuse_link("This goal can't be linked yet.", reason);
                return;
            }
        };
        let linked = self.link_verb() == "link";
        match self.needs.set(&source, &target, linked) {
            Ok((links, changed)) => {
                self.links = Ok(links);
                self.prepare_graph();
                self.notice = match (changed, linked) {
                    (true, _) => "",
                    (false, true) => "Already linked.",
                    (false, false) => "Already unlinked.",
                }
                .into();
                self.link_error = None;
                self.cancel_link();
            }
            Err(reason) => {
                self.links = self.needs.read().into_links();
                self.prepare_graph();
                let message = if self.links.is_err() {
                    UNAVAILABLE
                } else {
                    "Links couldn't be saved, so nothing changed. F1 then h for details."
                };
                self.refuse_link(message, reason);
            }
        }
    }

    fn refuse_link(&mut self, message: &str, reason: String) {
        self.cancel_link();
        self.notice = message.into();
        self.link_error = Some(reason);
    }

    pub(super) fn link_verb(&self) -> &'static str {
        if let (Some(source), Some(target), Ok(links)) =
            (self.source(), self.current(), &self.links)
            && let (Some(source), Some(target)) = (&source.id, &target.id)
            && links.contains(source, target)
        {
            "unlink"
        } else {
            "link"
        }
    }

    pub(super) fn source_marked(&self, index: usize) -> bool {
        self.linking
            .as_ref()
            .map_or(index == self.selected, |path| {
                self.sessions[index].path == *path
            })
    }

    pub(super) fn selected_line(&self, width: usize, now: Option<u64>) -> (String, String) {
        if let Some(source) = self.source() {
            let target = self
                .current()
                .filter(|s| s.path != source.path)
                .map_or("…", |s| s.goal.lines().next().unwrap_or(""));
            let source = source.goal.lines().next().unwrap_or("");
            return (
                String::new(),
                clipped(&format!("{source} needs {target}"), width),
            );
        }
        let session = &self.sessions[self.selected];
        let suffix = self.needs_text();
        let (goal, status) = row(session, width.saturating_sub(suffix.width()), now);
        let minimum = session.goal.lines().next().unwrap_or("").width().min(12);
        if !suffix.is_empty()
            && (goal.trim_end().width() < minimum
                || goal.width() + status.width() + suffix.width() > width)
        {
            let readiness = suffix.rsplit(" · ").next().unwrap_or("");
            let suffix = format!(" · {readiness} · F1 needs");
            return (
                clipped(
                    session.goal.lines().next().unwrap_or(""),
                    width.saturating_sub(suffix.width()),
                ),
                suffix,
            );
        }
        (goal, format!("{status}{suffix}"))
    }

    pub(super) fn needs_text(&self) -> String {
        let Some(session) = self.current() else {
            return String::new();
        };
        let mut suffix = String::new();
        if let (Some(id), Ok(links)) = (&session.id, &self.links) {
            if self.link_id(session).is_err() {
                return " · readiness unknown".into();
            }
            let summary = links.summary(id, &self.all_sessions);
            if !summary.titles.is_empty() {
                suffix.push_str(&format!(" · needs {}", summary.titles.join(", ")));
            }
            if summary.missing > 0 {
                suffix.push_str(&format!(
                    " · {} need{} not found",
                    summary.missing,
                    if summary.missing == 1 { "" } else { "s" }
                ));
            }
            if !suffix.is_empty() {
                suffix.push_str(if summary.ready {
                    " · ready"
                } else {
                    " · not ready"
                });
            }
        }
        suffix
    }
}
