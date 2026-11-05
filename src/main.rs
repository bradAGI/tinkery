//! The `tinkery` command: reads options, picks the real or practice host, and runs the terminal
//! loop (or prints one `--snapshot`). Everything the screens do lives in the library; this file
//! owns only the terminal: raw mode, events, the clipboard, and restoring it on the way out.

#[path = "main/input.rs"]
mod input;

use std::io::{self, IsTerminal};
use std::{path::PathBuf, sync::Arc};

use crossterm::{
    clipboard::CopyToClipboard,
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEvent, KeyModifiers, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    style::Print,
    terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate},
};
use tinkery::{
    Palette, ThemeSwitch,
    menu::{self, Home},
    model_setup::Controls,
    think::{
        brain_dump::{self, BrainDump, Exit},
        drafting::ModelHost,
    },
};

fn seed_read(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    use tinkery::seed_me::ledger;
    let refused = |reason: String| format!("Tinkery refused: {reason}");
    let value = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["status", "--json", directory] => {
            ledger::status(std::path::Path::new(directory)).map_err(refused)?
        }
        ["checks", directory] => {
            let directory = std::path::Path::new(directory);
            ledger::load(directory).map_err(refused)?;
            ledger::read_checks(directory).map_err(refused)?
        }
        _ => return Err("usage: tinkery seed status --json DIR | tinkery seed checks DIR".into()),
    };
    print!("{}", value.pretty().map_err(refused)?);
    Ok(())
}

// Shape starts Pi in another folder, and a relative program path follows the child's folder.
// Bare names stay bare so PATH still finds them.
fn launch_relative(program: PathBuf) -> io::Result<PathBuf> {
    if program.is_relative() && program.components().count() > 1 {
        return Ok(std::env::current_dir()?.join(program));
    }
    Ok(program)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut no_color = std::env::var_os("NO_COLOR").is_some();
    let mut snapshot = false;
    let mut full_redraw = false;
    let mut shape_pi = false;
    let mut harness = None;
    let mut model = None;
    let mut thinking = None;
    let mut seed_session_root = None;
    let mut record_root = None;
    let mut link_root = None;
    let mut pi_command = None;
    let mut shape_budget = None;
    let mut home_menu = false;
    let mut operator = None;
    let mut sdlc_root = None;
    let mut codex_report = None;
    let mut worker_model = None;
    let mut worker_responses = 0;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "seed" => return seed_read(args.collect()),
            "--no-color" => no_color = true,
            "--home" => home_menu = true,
            "--operator" => {
                if operator.is_some() {
                    return Err("--operator may be declared only once".into());
                }
                let value = args.next().ok_or("--operator needs human or agent")?;
                tinkery::think_record::GoalConfirmation::terminal(Some(&value))?;
                operator = Some(value);
            }
            "--snapshot" => snapshot = true,
            "--sdlc-root" => {
                sdlc_root = Some(PathBuf::from(
                    args.next()
                        .filter(|root| !root.is_empty())
                        .ok_or("--sdlc-root needs a tink-sdlc checkout path")?,
                ))
            }
            "--codex-report" => {
                if codex_report.is_some() {
                    return Err("--codex-report may be declared only once".into());
                }
                codex_report = Some(tinkery::watch::CodexReport::parse(
                    &args
                        .next()
                        .ok_or("--codex-report needs RUN_UUID:CHAT_ID:PATH")?,
                )?);
            }
            "--worker-model" => {
                worker_model = Some(
                    args.next()
                        .ok_or("--worker-model needs provider/model:thinking")?,
                );
            }
            "--worker-responses" => {
                worker_responses = args
                    .next()
                    .and_then(|s| s.parse::<usize>().ok())
                    .ok_or("--worker-responses needs a positive response cap")?;
            }
            "--full-redraw" => full_redraw = true,
            "--shape-pi" => shape_pi = true,
            "--harness" => {
                harness = Some(tinkery::pi::Harness::parse(
                    &args.next().ok_or("--harness needs pi or claude-code")?,
                )?)
            }
            "--model" => model = Some(args.next().ok_or("--model needs a model id")?),
            "--thinking" => thinking = Some(args.next().ok_or("--thinking needs a level")?),
            "--seed-session-root" => {
                seed_session_root = Some(PathBuf::from(
                    args.next()
                        .filter(|root| !root.is_empty())
                        .ok_or("--seed-session-root needs a location outside a repository")?,
                ));
            }
            "--link-root" => {
                link_root = Some(PathBuf::from(
                    args.next()
                        .filter(|root| !root.is_empty())
                        .ok_or("--link-root needs a folder")?,
                ));
            }
            "--record-root" => {
                record_root = Some(PathBuf::from(
                    args.next()
                        .filter(|root| !root.is_empty())
                        .ok_or("--record-root needs a folder")?,
                ));
            }
            "--shape-budget" => {
                shape_budget = Some(
                    args.next()
                        .and_then(|calls| calls.parse::<usize>().ok())
                        .ok_or("--shape-budget needs a number of model requests")?,
                );
            }
            "--pi-command" => {
                pi_command = Some(launch_relative(PathBuf::from(
                    args.next().ok_or("--pi-command needs an executable path")?,
                ))?)
            }
            "--help" | "-h" => {
                println!(
                    "Tinkery: a calm terminal front door for agentic work.\n\nUsage: tinkery [--home] [--no-color] [--snapshot]\n       tinkery --shape-pi [--harness pi|claude-code] --model MODEL\n       tinkery seed status --json DIR\n       tinkery seed checks DIR\n\nWithout a model, think is practice: replies are simulated and nothing leaves your machine.\nWith one, your words go to the model you chose and may cost money. Choose it with m on the menu\n(saved in ~/.config/tinkery/model.json for later launches) or with --shape-pi for one launch.\nNothing runs on its own. Only typing confirm saves a goal, and saving starts no work.\nInside, F2 runs the action the key bar names, F1 lists the rest by number, and Esc goes back.\n\n--home       Open the home menu (the tinkery launcher passes it)\n--operator human|agent  Who is typing, not verified identity (default: not declared); agent saves are the agent's\n--shape-pi   Draft with a real model for this launch\n--harness    pi|claude-code: who drafts (default: saved, else pi); Claude Code uses its own login\n--model      Pi: provider/model-id; Claude Code: a claude- model id (required with --shape-pi unless one is saved)\n--thinking   Reasoning level: Pi off|minimal|low|medium|high|xhigh|max, Claude Code low..max (default: saved, else low)\n--seed-session-root  Saved goals to list/save (default needs absolute HOME: ~/.local/share/tinkery/sessions)\n--record-root  Where Think history is saved/read (default: ~/.local/share/tinkery/think)\n--link-root PATH  Where tend's links go (default: ~/.local/share/tinkery/tend)\n--pi-command Pi executable (default: pi; Claude Code is always claude on PATH)\n--shape-budget CALLS  Drafting runs shared by Think and Shape this launch (default: saved calls, else 0)\n--sdlc-root PATH  The selected tink-sdlc checkout; reads and explicit worker actions stay here\n--worker-model PIN  Exact Pi provider/model:thinking for an explicit worker start\n--worker-responses N  Worker response cap, separate from drafting (default 0); one response may be in flight\n--codex-report RUN_UUID:CHAT_ID:PATH  One saved Codex observation for Watch; requires --home and --sdlc-root\n--full-redraw  Repaint every frame, for terminals that draw badly\n--no-color   Turn off colors (also respects NO_COLOR)\n--snapshot   Print a 100 x 30 view and exit; no terminal mode, no model requests\nTINKERY_NODE Node for local voice (otherwise found on PATH). Voice stays on your machine; installing it asks first."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}; use --help").into()),
        }
    }
    if !shape_pi
        && (harness.is_some()
            || model.is_some()
            || thinking.is_some()
            || pi_command.is_some()
            || shape_budget.is_some_and(|calls| calls > 0))
    {
        return Err(
            "--harness, --model, --thinking, --pi-command and --shape-budget require --shape-pi"
                .into(),
        );
    }
    // Watch, Approve and Taste live in the home menu; Think alone would ignore their inputs.
    if !home_menu && sdlc_root.is_some() {
        return Err("--sdlc-root requires --home".into());
    }
    if codex_report.is_some() && (!home_menu || sdlc_root.is_none()) {
        return Err("--codex-report requires --home and --sdlc-root".into());
    }
    if (worker_model.is_some() || worker_responses > 0) && (!home_menu || sdlc_root.is_none()) {
        return Err("worker options require --home and --sdlc-root".into());
    }
    let sdlc_root = match sdlc_root {
        Some(root) if root.is_relative() => Some(std::env::current_dir()?.join(root)),
        root => root,
    };
    let confirmation = tinkery::think_record::GoalConfirmation::terminal(operator.as_deref())?;
    let record_root = match record_root {
        Some(root) if root.is_absolute() => Ok(root),
        Some(root) => Ok(std::env::current_dir()?.join(root)),
        None => tinkery::think_record::default_root(std::env::var_os("HOME").as_deref()),
    };
    let setup_path = tinkery::model_setup::path(std::env::var_os("HOME").as_deref());
    let saved = setup_path
        .as_deref()
        .map_or(Ok(None), tinkery::model_setup::load);
    let agent = operator.as_deref() == Some("agent");
    // An agent never inherits the operator's saved calls; its allowance is --shape-budget alone.
    let usable =
        saved
            .as_ref()
            .ok()
            .and_then(Option::as_ref)
            .map(|setup| tinkery::model_setup::Setup {
                calls: if agent { 0 } else { setup.calls },
                ..setup.clone()
            });
    let launch = tinkery::model_setup::Setup::launch(
        shape_pi,
        harness,
        model,
        thinking,
        shape_budget,
        usable.as_ref(),
    )?;
    let live = launch.is_some();
    let harness = launch.as_ref().map(|l| l.harness).unwrap_or_default();
    let program = match pi_command.clone() {
        Some(_) if harness == tinkery::pi::Harness::ClaudeCode => {
            return Err("--pi-command applies to Pi, not Claude Code".into());
        }
        Some(command) => command,
        None => PathBuf::from(harness.program()),
    };
    if live {
        record_root.as_ref().map_err(Clone::clone)?;
    }
    let call_budget = tinkery::pi::CallBudget::new(launch.as_ref().map_or(0, |l| l.calls));
    let controls = tinkery::model_setup::Controls::new(
        setup_path,
        saved,
        launch.clone().map(|setup| (setup, call_budget.clone())),
    );
    let controls = controls.listing_pi(
        pi_command
            .clone()
            .unwrap_or_else(|| tinkery::pi::Harness::Pi.program().into()),
    );
    let controls = if agent {
        controls.for_agent()
    } else {
        controls
    };
    let host = match &launch {
        Some(launch) => Some(Arc::new(
            ModelHost::new(launch.harness, program.clone(), launch.model.clone())?
                .with_thinking(&launch.thinking)?
                .with_budget(call_budget.clone()),
        )),
        None => None,
    };
    let default_root_requested = seed_session_root.is_none();
    let link_root = tinkery::tend::needs::resolve_root(link_root);
    let sessions_root = match seed_session_root {
        Some(root) => Ok(tinkery::seed_me::resolve_root(Some(root))?),
        None => tinkery::seed_me::default_root(std::env::var_os("HOME").as_deref()),
    };
    if live && !snapshot {
        sessions_root.as_ref().map_err(Clone::clone)?;
    }
    if !snapshot && (!io::stdin().is_terminal() || !io::stdout().is_terminal()) {
        return Err("interactive mode needs a terminal; use --snapshot or --help".into());
    }
    let migration = if default_root_requested {
        tinkery::seed_me::migration::migrate(std::env::var_os("HOME").as_deref(), !snapshot)?
    } else {
        None
    };
    // Where Tinkery was started; found by looking for `.git`, never by running git.
    let repo = std::env::current_dir()
        .ok()
        .and_then(|dir| tinkery::repo::Repo::enclosing(&dir));
    // Shape's agent step reads files where the goal was made, else in the named SDLC checkout's
    // repository, else where Tinkery started.
    let shape_agent = match &launch {
        Some(launch) => Some(Arc::new(
            tinkery::shape::Agent::new(
                launch.harness,
                program,
                launch.model.clone(),
                launch.thinking.clone(),
                sdlc_root
                    .as_deref()
                    .and_then(tinkery::repo::Repo::enclosing)
                    .or_else(|| repo.clone())
                    .map(|r| r.root().to_path_buf())
                    .or_else(|| std::env::current_dir().ok()),
                call_budget,
                record_root.clone().ok(),
            )?
            .with_confirmation(Some(confirmation.clone())),
        )),
        None => None,
    };
    if home_menu {
        let make_dump: Box<dyn Fn() -> BrainDump> = if let Some(host) = host.clone() {
            let root = sessions_root.clone().ok();
            let records = record_root.clone()?;
            let repo = repo.clone();
            let confirmation = confirmation.clone();
            Box::new(move || {
                let mut dump = BrainDump::with_host(host.clone())
                    .with_record_root(records.clone())
                    .with_repo(repo.clone());
                dump.set_goal_confirmation(confirmation.clone())
                    .expect("validated launch declaration on fresh Think");
                match &root {
                    Some(root) => dump.with_sessions(Some(root.clone())),
                    None => dump,
                }
            })
        } else {
            let confirmation = confirmation.clone();
            Box::new(move || {
                let mut dump = BrainDump::default();
                dump.set_goal_confirmation(confirmation.clone())
                    .expect("validated launch declaration on fresh Think");
                dump
            })
        };
        let make_dump: Box<dyn Fn() -> BrainDump> = if snapshot {
            make_dump
        } else {
            Box::new(move || make_dump().with_voice(Box::new(tinkery::voice::ProcessHost::new())))
        };
        let mut menu = Home::new(make_dump, sessions_root, link_root.clone())
            .with_repo(repo)
            .with_shape_agent(shape_agent)
            .with_shape_held(agent && !live)
            .with_agent_operator(agent)
            .with_sdlc_root(sdlc_root)
            .with_codex_report(codex_report)
            .with_worker(worker_model, worker_responses)?;
        if let Ok(root) = &record_root {
            menu = menu.with_record_root(root.clone());
        }
        if snapshot {
            menu.discover_for_snapshot(std::time::Duration::from_secs(5));
            let mut app = Intake::Home(Box::new(menu));
            println!("{}", screen_text(&mut app, &controls, no_color)?);
            return Ok(());
        }
        let mut terminal = ratatui::try_init()?;
        let mut app = Intake::Home(Box::new(menu));
        let result = run_shape(
            &mut terminal,
            Header {
                theme: ThemeSwitch::new(no_color),
                model: controls,
            },
            &mut app,
            full_redraw,
            link_root,
            None,
            migration,
        );
        ratatui::restore();
        if let Some(receipt) = result? {
            println!("{receipt}");
        }
        return Ok(());
    }
    let mut app = if let Some(host) = host {
        let dump = BrainDump::with_host(host)
            .with_record_root(record_root?)
            .with_repo(repo);
        Intake::Brain(Box::new(match sessions_root {
            Ok(root) => dump.with_sessions(Some(root)),
            Err(_) => dump,
        }))
    } else {
        Intake::Brain(Box::default())
    };
    if let Intake::Brain(dump) = &mut app {
        dump.set_goal_confirmation(confirmation)?;
    }
    if snapshot {
        println!("{}", screen_text(&mut app, &controls, no_color)?);
        return Ok(());
    }
    app = match app {
        Intake::Brain(dump) => Intake::Brain(Box::new(
            (*dump).with_voice(Box::new(tinkery::voice::ProcessHost::new())),
        )),
        other => other,
    };
    let mut terminal = ratatui::try_init()?;
    let result = run_shape(
        &mut terminal,
        Header {
            theme: ThemeSwitch::new(no_color),
            model: controls,
        },
        &mut app,
        full_redraw,
        link_root,
        shape_agent,
        migration,
    );
    ratatui::restore();
    if let Some(receipt) = result? {
        println!("{receipt}");
    }
    Ok(())
}

enum Intake {
    Home(Box<Home>),
    Brain(Box<BrainDump>),
}
impl Intake {
    fn quit(&self) -> bool {
        match self {
            Self::Home(a) => a.quit,
            Self::Brain(a) => *a.exit() == Exit::Menu && a.saved().is_none(),
        }
    }
    fn tick(
        &mut self,
        link_root: &Result<PathBuf, String>,
        shape_agent: &Option<Arc<tinkery::shape::Agent>>,
    ) {
        match self {
            Self::Home(a) => a.tick(),
            Self::Brain(a) => a.tick(),
        }
        if let Self::Brain(dump) = self {
            let session = match dump.exit() {
                Exit::Stay => None,
                Exit::Menu => dump.saved().map(|r| (r.session.clone(), false)),
                Exit::Tend(path) => Some((path.clone(), true)),
            };
            if let Some((session, open_tend)) = session {
                let Self::Brain(dump) = std::mem::replace(self, Self::Brain(Box::default())) else {
                    unreachable!()
                };
                *self = Self::Home(Box::new(Home::from_receipt(
                    *dump,
                    &session,
                    open_tend,
                    link_root.clone(),
                    shape_agent.clone(),
                )));
            }
        }
    }
    fn render(&mut self, f: &mut ratatui::Frame, p: Palette) {
        match self {
            Self::Home(a) => menu::render(f, a, p),
            Self::Brain(a) => brain_dump::render(f, a, p),
        }
    }
    fn handle_key(&mut self, k: crossterm::event::KeyEvent) {
        match self {
            Self::Home(a) => a.handle_key(k),
            Self::Brain(a) => a.handle_key(k),
        }
    }
    fn paste(&mut self, text: &str) {
        match self {
            Self::Home(a) => a.paste(text),
            Self::Brain(a) => a.paste(text),
        }
    }
    fn handle_mouse(&mut self, e: crossterm::event::MouseEvent, area: ratatui::layout::Rect) {
        match self {
            Self::Home(a) => a.handle_mouse(e, area),
            Self::Brain(a) => a.handle_mouse(e, area),
        }
    }
    fn take_copy_request(&mut self) -> Option<String> {
        match self {
            Self::Home(a) => a.take_copy_request(),
            Self::Brain(a) => a.take_copy_request(),
        }
    }
    fn copy_result(&mut self, sent: bool) {
        match self {
            Self::Home(a) => a.copy_result(sent),
            Self::Brain(a) => a.copy_result(sent),
        }
    }
    fn presentation(&self) -> Option<brain_dump::Presentation> {
        match self {
            Self::Home(a) => a.presentation(),
            Self::Brain(a) => a.presentation(),
        }
    }
    fn presented(&mut self, presentation: brain_dump::Presentation) {
        match self {
            Self::Home(a) => a.presented(presentation),
            Self::Brain(a) => a.presented(presentation),
        }
    }
    fn take_model_request(&mut self) -> bool {
        match self {
            Self::Home(a) => a.take_model_request(),
            Self::Brain(a) => a.take_model_request(),
        }
    }
}

// The header's two switches the loop owns over every screen: light or dark, and the model.
struct Header {
    theme: ThemeSwitch,
    model: Controls,
}

// The screen, then the model label and panel over it; the loop and --snapshot draw the same.
fn draw(frame: &mut ratatui::Frame, app: &mut Intake, controls: &Controls, palette: Palette) {
    app.render(frame, palette);
    controls.render(frame, palette);
}

// The 100 x 30 view as text: no terminal mode, no model requests. Home lines are trimmed.
fn screen_text(
    app: &mut Intake,
    controls: &Controls,
    no_color: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let (width, height) = (100, 30);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))?;
    terminal.draw(|frame| draw(frame, app, controls, Palette::new(no_color)))?;
    let trim = matches!(app, Intake::Home(_));
    let buffer = terminal.backend().buffer();
    Ok((0..height)
        .map(|y| {
            let line: String = (0..width).map(|x| buffer[(x, y)].symbol()).collect();
            if trim {
                line.trim_end().to_owned()
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

struct InputGuard;

// The renderer may resize before a Resize event or input-size check observes it.
fn frame_painted(
    area: ratatui::layout::Rect,
    previous: &mut Option<ratatui::layout::Rect>,
    discard_mouse: &mut bool,
) {
    if previous.replace(area).is_some_and(|old| old != area) {
        *discard_mouse = true;
    }
}

// Keys and paste survive repaint; mouse coordinates from the old layout do not.
fn input_for_frame(
    input: Event,
    drawn: ratatui::layout::Rect,
    current: ratatui::layout::Rect,
    pending: &mut Option<Event>,
    discard_mouse: &mut bool,
) -> Option<Event> {
    if matches!(input, Event::Resize(..)) || current != drawn {
        *discard_mouse = true;
        if matches!(input, Event::Key(_) | Event::Paste(_)) {
            *pending = Some(input);
        }
        return None;
    }
    Some(input)
}

impl Drop for InputGuard {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            EndSynchronizedUpdate,
            PopKeyboardEnhancementFlags,
            Print("\x1b[>0s"),
            DisableMouseCapture,
            DisableBracketedPaste
        );
    }
}

// F2 runs the named action, but hosts such as herdr or macOS may keep F2. Cmd+Enter (in
// terminals that report it) and Ctrl+Alt+S are the same key; every screen sees only F2.
fn send_alias(input: Event) -> Event {
    let Event::Key(key) = input else {
        return input;
    };
    let alias = match key.code {
        KeyCode::Enter => key.modifiers == KeyModifiers::SUPER,
        KeyCode::Char('s' | 'S') => key.modifiers == KeyModifiers::CONTROL | KeyModifiers::ALT,
        _ => false,
    };
    if !alias {
        return input;
    }
    Event::Key(KeyEvent {
        code: KeyCode::F(2),
        modifiers: KeyModifiers::NONE,
        ..key
    })
}

// A terminal receipt is truthful only when both frame drawing and ending synchronized
// output succeeded. Keep this single boundary shared by the real loop and failure tests.
fn finish_frame<T>(
    drawn: io::Result<T>,
    synchronized: io::Result<()>,
    acknowledge: impl FnOnce(),
) -> io::Result<T> {
    synchronized?;
    let drawn = drawn?;
    acknowledge();
    Ok(drawn)
}

fn run_shape(
    terminal: &mut ratatui::DefaultTerminal,
    header: Header,
    app: &mut Intake,
    full_redraw: bool,
    link_root: Result<PathBuf, String>,
    shape_agent: Option<Arc<tinkery::shape::Agent>>,
    mut migration: Option<String>,
) -> io::Result<Option<String>> {
    let _input_guard = InputGuard;
    execute!(
        io::stdout(),
        EnableBracketedPaste,
        EnableMouseCapture,
        Print("\x1b[>1s"),
        // Terminals that know this report Cmd+Enter; the rest ignore it.
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )?;
    let mut terminal_input = input::Input::new()?;
    let mut repaint = full_redraw;
    let mut pending_input = None;
    let mut previous_area = None;
    let Header {
        mut theme,
        model: mut controls,
    } = header;
    while !app.quit() {
        app.tick(&link_root, &shape_agent);
        execute!(io::stdout(), BeginSynchronizedUpdate)?;
        let mut presentation = None;
        let drawn = (|| {
            let drawn = terminal.draw(|frame| {
                if let Some(notice) = &migration {
                    frame.render_widget(
                        ratatui::widgets::Paragraph::new(format!(
                            "{notice}\n\nPress any key to continue."
                        ))
                        .wrap(ratatui::widgets::Wrap { trim: false }),
                        frame.area(),
                    );
                } else {
                    draw(frame, app, &controls, theme.palette());
                    if !controls.is_open() {
                        presentation = app.presentation();
                    }
                }
                if repaint || full_redraw {
                    for cell in &mut frame.buffer_mut().content {
                        cell.set_diff_option(ratatui::buffer::CellDiffOption::AlwaysUpdate);
                    }
                }
            })?;
            io::Result::Ok(drawn.area)
        })();
        let drawn_area =
            finish_frame(drawn, execute!(io::stdout(), EndSynchronizedUpdate), || {
                if let Some(presentation) = presentation {
                    app.presented(presentation);
                }
            })?;
        let mut resized = false;
        frame_painted(drawn_area, &mut previous_area, &mut resized);
        if resized {
            terminal_input.invalidate_mouse();
        }
        repaint = false;
        let ready = pending_input.is_some() || terminal_input.poll();
        if !ready {
            std::thread::park_timeout(std::time::Duration::from_millis(100));
        }
        if ready || terminal_input.poll() {
            for _ in 0..64 {
                let input = match pending_input.take() {
                    Some(input) => input,
                    None => terminal_input.read()?,
                };
                let area = terminal.size()?.into();
                let Some(input) =
                    input_for_frame(input, drawn_area, area, &mut pending_input, &mut resized)
                else {
                    terminal_input.invalidate_mouse();
                    break;
                };
                match send_alias(input) {
                    // Only the source can acknowledge that old-layout input has drained.
                    Event::Mouse(_) if !terminal_input.mouse_ready() => {}
                    Event::Key(_) if migration.is_some() => {
                        migration = None;
                    }
                    Event::Paste(_) | Event::Mouse(_) if migration.is_some() => {}
                    Event::Key(key)
                        if key.code == KeyCode::Char('l')
                            && key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        repaint = true
                    }
                    Event::Key(key) if theme.key(key) => {}
                    Event::Mouse(event) if theme.click(event, area) => {}
                    Event::Key(key) if controls.key(key) => {}
                    Event::Paste(text) if controls.paste(&text) => {}
                    Event::Mouse(event) if controls.click(event, area) => {}
                    Event::Key(key) => app.handle_key(key),
                    Event::Paste(text) => app.paste(&text),
                    Event::Mouse(event) => app.handle_mouse(event, area),
                    _ => {}
                }
                if app.take_model_request() {
                    controls.open();
                }
                if app.quit() || !terminal_input.poll() {
                    break;
                }
            }
            if let Some(markdown) = app.take_copy_request() {
                app.copy_result(
                    execute!(io::stdout(), CopyToClipboard::to_clipboard_from(markdown)).is_ok(),
                );
            }
        }
    }
    Ok(match app {
        Intake::Home(a) => a.finish(),
        Intake::Brain(a) => a.finish_handoff().map(|receipt| receipt.text()),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn presentation_receipt_requires_successful_draw_and_synchronized_output() {
        for (draw_ok, sync_ok) in [(true, true), (false, true), (true, false), (false, false)] {
            let mut acknowledged = false;
            let drawn = if draw_ok {
                Ok(())
            } else {
                Err(std::io::Error::other("TEST draw failed"))
            };
            let sync = if sync_ok {
                Ok(())
            } else {
                Err(std::io::Error::other("TEST synchronization failed"))
            };
            let result = super::finish_frame(drawn, sync, || acknowledged = true);
            assert_eq!(result.is_ok(), draw_ok && sync_ok);
            assert_eq!(acknowledged, draw_ok && sync_ok);
        }
    }

    use super::*;
    use crossterm::event::KeyEvent;
    use ratatui::layout::Rect;

    #[test]
    fn cmd_enter_and_ctrl_alt_s_are_f2_and_nothing_else_is() {
        let key = |code, modifiers| Event::Key(KeyEvent::new(code, modifiers));
        let f2 = key(KeyCode::F(2), KeyModifiers::NONE);
        let ctrl_alt = KeyModifiers::CONTROL | KeyModifiers::ALT;
        assert_eq!(send_alias(key(KeyCode::Enter, KeyModifiers::SUPER)), f2);
        assert_eq!(send_alias(key(KeyCode::Char('s'), ctrl_alt)), f2);
        for kept in [
            key(KeyCode::Enter, KeyModifiers::NONE),
            key(KeyCode::Enter, KeyModifiers::SHIFT),
            key(KeyCode::Enter, KeyModifiers::SUPER | KeyModifiers::SHIFT),
            key(KeyCode::Char('s'), KeyModifiers::NONE),
            key(KeyCode::Char('s'), KeyModifiers::CONTROL),
            key(KeyCode::Char('s'), KeyModifiers::ALT),
            Event::Paste("s".into()),
        ] {
            assert_eq!(send_alias(kept.clone()), kept);
        }
    }

    #[test]
    fn resized_input_waits_for_paint_and_is_delivered_once_unchanged() {
        let large = Rect::new(0, 0, 100, 30);
        let small = Rect::new(0, 0, 79, 24);
        for (drawn, current) in [(large, small), (small, large)] {
            for input in [
                Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                Event::Paste("confirm café 👩‍💻".into()),
            ] {
                let mut pending = None;
                let mut discard_mouse = false;
                // Size changed before any Resize event arrived.
                assert!(
                    input_for_frame(
                        input.clone(),
                        drawn,
                        current,
                        &mut pending,
                        &mut discard_mouse
                    )
                    .is_none()
                );
                let retained = pending.take().expect("input lost during resize");
                assert_eq!(
                    input_for_frame(retained, current, current, &mut pending, &mut discard_mouse),
                    Some(input)
                );
                assert!(pending.is_none());
            }
        }
    }

    #[test]
    fn relative_pi_paths_keep_meaning_when_shape_starts_pi_elsewhere() {
        let launch = std::env::current_dir().unwrap();
        assert_eq!(
            launch_relative("./TEST-bin/pi".into()).unwrap(),
            launch.join("./TEST-bin/pi")
        );
        assert_eq!(
            launch_relative("TEST-bin/pi".into()).unwrap(),
            launch.join("TEST-bin/pi")
        );
        assert_eq!(launch_relative("pi".into()).unwrap(), PathBuf::from("pi"));
        assert_eq!(
            launch_relative("/TEST/pi".into()).unwrap(),
            PathBuf::from("/TEST/pi")
        );
    }

    #[test]
    fn resize_requests_a_paint_without_creating_input() {
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        let mut pending = None;
        let mut discard_mouse = false;
        assert!(
            input_for_frame(
                Event::Resize(80, 24),
                area,
                area,
                &mut pending,
                &mut discard_mouse
            )
            .is_none()
        );
        assert!(pending.is_none());
    }

    #[test]
    fn resize_marks_queued_mouse_coordinates_stale() {
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        let large = Rect::new(0, 0, 100, 30);
        let small = Rect::new(0, 0, 80, 24);
        // Outside Back in the old review, directly on Back in the new review.
        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 40,
            row: 21,
            modifiers: KeyModifiers::NONE,
        });
        for resize_first in [false, true] {
            let mut pending = None;
            let mut discard_mouse = false;
            let painted = if resize_first {
                assert!(
                    input_for_frame(
                        Event::Resize(80, 24),
                        large,
                        small,
                        &mut pending,
                        &mut discard_mouse
                    )
                    .is_none()
                );
                small
            } else {
                large
            };
            let delivered = input_for_frame(
                click.clone(),
                painted,
                small,
                &mut pending,
                &mut discard_mouse,
            );
            assert!(discard_mouse);
            // The dispatch match ignores mouse while draining the old layout's queue.
            assert!(delivered.is_none() || matches!(delivered, Some(Event::Mouse(_))));
            assert!(pending.is_none(), "old-layout click was retained");
        }
    }

    #[test]
    fn automatic_draw_resize_marks_mouse_stale_without_a_resize_event() {
        let large = Rect::new(0, 0, 100, 30);
        let small = Rect::new(0, 0, 80, 24);
        let mut previous = None;
        let mut discard_mouse = false;
        frame_painted(large, &mut previous, &mut discard_mouse);
        assert!(!discard_mouse);
        frame_painted(small, &mut previous, &mut discard_mouse);
        // The next input-size check sees equal sizes, so paint must notice the change.
        let enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let mut pending = None;
        assert_eq!(
            input_for_frame(
                enter.clone(),
                small,
                small,
                &mut pending,
                &mut discard_mouse
            ),
            Some(enter)
        );
        assert!(
            discard_mouse,
            "auto-resize admitted old-layout mouse coordinates"
        );
        frame_painted(small, &mut previous, &mut discard_mouse);
        assert!(
            discard_mouse,
            "a later paint cleared protection before input drained"
        );
    }
}
