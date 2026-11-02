//! The header's model label and the Model panel it opens. The label shows real state: runs left
//! when this launch drafts with a model, else `practice`. The panel names this launch's recipient
//! and edits settings for later launches; `s` saves them, `esc` drops them.
//! `enter` on Model picks the next model the harness offers:
//! Claude Code's fixed list, or the list Pi prints (asked once, off the paint path, with no model
//! request); `t` types any model instead. When this launch drafts, its Now row adds runs at once,
//! for this launch only. Nothing here starts a model request.
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap},
};

use super::{CALLS, Harness, Setup};
use crate::{Palette, pi::CallBudget};

/// Drafting runs one `enter` on the Now row adds.
const TOP_UP: usize = 5;
/// Why an agent's panel only shows, and where its runs come from instead.
const AGENT: &str =
    "An agent can't save a setup or add runs; the operator grants them with --shape-budget CALLS.";
/// The note while Pi lists its models.
const LISTING: &str = "Listing Pi's models…";

/// This launch's model state: where the setup saves, what was saved, the launch's runs when it
/// drafts, and the panel while it is open.
pub struct Controls {
    path: Option<PathBuf>,
    saved: Result<Option<Setup>, String>,
    active: Option<(Setup, Arc<CallBudget>)>,
    panel: Option<Panel>,
    agent: bool,
    /// The Pi executable that lists models; `None` types Pi models.
    pi: Option<PathBuf>,
    /// Pi's models once listed, or why not; asked at most once per launch.
    pi_models: Arc<OnceLock<Result<Vec<String>, String>>>,
    pi_asked: bool,
}

struct Panel {
    draft: Setup,
    row: usize,
    typing: Option<String>,
    note: String,
}

impl Controls {
    /// Controls saving to `path` (none without an absolute HOME), starting from what launch read
    /// there. `active` pairs this launch's resolved setup with its shared budget, or is `None`
    /// for practice. Editing or saving later settings never changes the active recipient.
    pub fn new(
        path: Option<PathBuf>,
        saved: Result<Option<Setup>, String>,
        active: Option<(Setup, Arc<CallBudget>)>,
    ) -> Self {
        Self {
            path,
            saved,
            active,
            panel: None,
            agent: false,
            pi: None,
            pi_models: Arc::default(),
            pi_asked: false,
        }
    }

    /// Lets `enter` on Model pick from the models Pi at `program` lists.
    pub fn listing_pi(mut self, program: PathBuf) -> Self {
        self.pi = Some(program);
        self
    }

    /// For an agent operator: the panel shows the setup but refuses to save or add runs.
    pub fn for_agent(mut self) -> Self {
        self.agent = true;
        self
    }

    /// The header label: runs left when this launch drafts, else `practice`.
    pub fn label(&self) -> String {
        match &self.active {
            Some((_, budget)) if budget.remaining() == 1 => "1 run left".into(),
            Some((_, budget)) => format!("{} runs left", budget.remaining()),
            None => "practice".into(),
        }
    }

    /// Whether the panel is open and takes every key.
    pub fn is_open(&self) -> bool {
        self.panel.is_some()
    }

    /// Opens the panel on the saved setup, or on practice values when there is none.
    pub fn open(&mut self) {
        let note = match &self.saved {
            Err(why) => why.clone(),
            Ok(_) if self.agent => AGENT.into(),
            Ok(_) => "Saved changes start with the next launch.".into(),
        };
        self.panel = Some(Panel {
            draft: self
                .saved
                .clone()
                .ok()
                .flatten()
                .unwrap_or_else(Setup::practice),
            row: 0,
            typing: None,
            note,
        });
    }

    /// Handles `key` while the panel is open and reports whether it took it.
    pub fn key(&mut self, key: KeyEvent) -> bool {
        if self.panel.is_none() {
            return false;
        }
        if key.kind == KeyEventKind::Release {
            return true;
        }
        let rows = self.rows();
        let panel = self.panel.as_mut().expect("checked open");
        let draft = &mut panel.draft;
        if let Some(text) = &mut panel.typing {
            match key.code {
                KeyCode::Char(c) if !key.modifiers.intersects(!KeyModifiers::SHIFT) => text.push(c),
                KeyCode::Backspace => {
                    text.pop();
                }
                KeyCode::Esc => panel.typing = None,
                KeyCode::Enter => {
                    let model = text.trim().to_owned();
                    match draft.harness.check_model(&model) {
                        Err(why) if !model.is_empty() => panel.note = why,
                        _ => {
                            draft.model = model;
                            panel.typing = None;
                        }
                    }
                }
                _ => {}
            }
            return true;
        }
        if key.kind == KeyEventKind::Repeat && key.code != KeyCode::Up && key.code != KeyCode::Down
        {
            return true;
        }
        match key.code {
            KeyCode::Esc => self.panel = None,
            KeyCode::Up => panel.row = (panel.row + rows - 1) % rows,
            KeyCode::Down => panel.row = (panel.row + 1) % rows,
            KeyCode::Enter => match panel.row {
                0 => {
                    // Another harness can't run this one's model or every thinking level.
                    draft.harness = next(&Harness::ALL, &draft.harness);
                    draft.model = draft.harness.models().first().map_or("", |m| m).into();
                    if draft.harness.check_thinking(&draft.thinking).is_err() {
                        draft.thinking = "low".into();
                    }
                }
                1 if draft.harness == Harness::ClaudeCode => {
                    draft.model = next(draft.harness.models(), &draft.model.as_str()).into()
                }
                1 => match (self.pi_models.get(), &self.pi) {
                    (Some(Ok(models)), _) => {
                        let models: Vec<&str> = models.iter().map(String::as_str).collect();
                        draft.model = next(&models, &draft.model.as_str()).into();
                    }
                    (Some(Err(why)), _) => {
                        panel.note = format!("{why} Type the model instead.");
                        panel.typing = Some(draft.model.clone());
                    }
                    (None, None) => panel.typing = Some(draft.model.clone()),
                    (None, Some(program)) => {
                        panel.note = LISTING.into();
                        if !self.pi_asked {
                            self.pi_asked = true;
                            let (program, models) = (program.clone(), self.pi_models.clone());
                            std::thread::spawn(move || {
                                let _ = models.set(crate::pi::pi_models(&program));
                            });
                        }
                    }
                },
                2 => {
                    draft.thinking = next(draft.harness.thinking(), &draft.thinking.as_str()).into()
                }
                3 => draft.calls = next(&CALLS, &draft.calls),
                _ if self.agent => panel.note = AGENT.into(),
                _ => {
                    if let Some((_, budget)) = &self.active {
                        budget.add(TOP_UP);
                    }
                }
            },
            KeyCode::Char('t') if panel.row == 1 && key.modifiers.is_empty() => {
                panel.typing = Some(draft.model.clone())
            }
            KeyCode::Char('s') if key.modifiers.is_empty() => self.save(),
            _ => {}
        }
        true
    }

    /// Types a paste into the model while it is being typed; otherwise the open panel ignores it.
    /// Reports whether the panel took it.
    pub fn paste(&mut self, pasted: &str) -> bool {
        let Some(panel) = &mut self.panel else {
            return false;
        };
        if let Some(text) = &mut panel.typing {
            text.extend(pasted.chars().filter(|c| !c.is_control()));
        }
        true
    }

    /// Opens the panel on a left click on the label; while it is open, takes every click.
    pub fn click(&mut self, event: MouseEvent, area: Rect) -> bool {
        if self.is_open() {
            return true;
        }
        let room = crate::label_area(area);
        let width = (self.label().chars().count() as u16).min(room.width);
        let hit = event.kind == crossterm::event::MouseEventKind::Down(MouseButton::Left)
            && event.row == room.y
            && (room.right() - width..room.right()).contains(&event.column);
        if hit {
            self.open();
        }
        hit
    }

    /// Paints the label and, when open, the panel over the screen's upper right.
    pub fn render(&self, frame: &mut Frame, palette: Palette) {
        let area = frame.area();
        frame.render_widget(
            Paragraph::new(self.label())
                .alignment(Alignment::Right)
                .style(palette.muted),
            crate::label_area(area),
        );
        let Some(panel) = &self.panel else {
            return;
        };
        let row = |i: usize, name: &str, value: String| {
            let style = if i == panel.row {
                palette.ink.bold()
            } else {
                palette.ink
            };
            Line::from(vec![
                Span::styled(if i == panel.row { "› " } else { "  " }, palette.jade),
                Span::styled(format!("{name:<10}"), style),
                Span::styled(value, palette.muted),
            ])
        };
        let model = match &panel.typing {
            Some(text) => format!("{text}▏"),
            None if panel.draft.model.is_empty() => "none".into(),
            None => panel.draft.model.clone(),
        };
        let recipient = match &self.active {
            Some((setup, _)) => format!("F2 sends to {}: {}.", setup.harness.label(), setup.model),
            None => "This launch: practice; no model requests.".into(),
        };
        let mut lines = vec![
            Line::styled(recipient, palette.ink),
            Line::default(),
            Line::styled("For later launches", palette.muted),
            row(0, "Harness", panel.draft.harness.label().into()),
            row(1, "Model", model),
            row(2, "Thinking", panel.draft.thinking.clone()),
            row(3, "Runs", format!("{} each launch", panel.draft.calls)),
        ];
        if let Some((_, budget)) = &self.active {
            lines.push(row(
                4,
                "Now",
                format!("{} left · enter adds {TOP_UP}", budget.remaining()),
            ));
        }
        lines.extend([
            Line::default(),
            Line::styled(
                "Runs count drafting attempts, including failures and cancellations.",
                palette.ink,
            ),
            Line::styled(
                "One run may make several model responses; nothing caps responses or spending.",
                palette.ink,
            ),
            Line::styled(self.note(panel), palette.muted),
            Line::default(),
        ]);
        let width = 50.min(area.width.saturating_sub(4));
        let keys = Paragraph::new(match (&panel.typing, panel.row) {
            (Some(_), _) => "enter done   esc undo",
            (None, 1) => "↑↓ choose   enter next   t type   s save   esc",
            (None, _) => "↑↓ choose   enter change   s save   esc close",
        })
        .style(palette.muted)
        .wrap(Wrap { trim: false });
        let keys_height = keys.line_count(width.saturating_sub(4)) as u16;
        let text = Paragraph::new(lines)
            .style(palette.ink)
            .wrap(Wrap { trim: false });
        let height = (text.line_count(width.saturating_sub(4)) as u16 + keys_height + 2)
            .min(area.height.saturating_sub(4));
        let at =
            Rect::new(area.width.saturating_sub(width + 2), 3, width, height).intersection(area);
        let block = Block::default()
            .title(Span::styled(" Model ", palette.jade))
            .borders(Borders::ALL)
            .border_style(palette.muted)
            .padding(Padding::new(1, 1, 0, 0));
        let [body, footer] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(keys_height)])
                .areas(block.inner(at));
        frame.render_widget(Clear, at);
        frame.render_widget(block, at);
        frame.render_widget(text, body);
        frame.render_widget(keys, footer);
    }

    /// The panel's note, saying how listing Pi's models went once it has.
    fn note(&self, panel: &Panel) -> String {
        match self.pi_models.get() {
            Some(Ok(models)) if panel.note == LISTING => {
                format!("Pi lists {} models; enter picks the next.", models.len())
            }
            Some(Err(why)) if panel.note == LISTING => {
                format!("{why} Type the model instead.")
            }
            _ => panel.note.clone(),
        }
    }

    fn rows(&self) -> usize {
        if self.active.is_some() { 5 } else { 4 }
    }

    fn save(&mut self) {
        let panel = self.panel.as_mut().expect("checked open");
        let draft = &panel.draft;
        let checked = Setup::new(
            draft.harness,
            draft.model.clone(),
            draft.thinking.clone(),
            draft.calls,
        );
        panel.note = match (checked, &self.path) {
            _ if self.agent => AGENT.into(),
            (Err(why), _) => why,
            (Ok(_), None) => "Saving needs an absolute HOME; nothing was written.".into(),
            (Ok(setup), Some(path)) => match super::save(path, &setup) {
                Err(why) => why,
                Ok(()) => {
                    let next = if setup.calls > 0 {
                        "A new launch drafts with it."
                    } else {
                        "A new launch is practice."
                    };
                    self.saved = Ok(Some(setup));
                    format!("Saved. {next}")
                }
            },
        };
    }
}

/// The value after `now` in `options`, wrapping; the first when `now` isn't one.
fn next<T: PartialEq + Copy>(options: &[T], now: &T) -> T {
    let at = options.iter().position(|o| o == now);
    options[at.map_or(0, |i| (i + 1) % options.len())]
}
