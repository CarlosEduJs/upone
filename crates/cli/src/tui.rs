//! Interactive TUI (ratatui): shows the plan, asks for confirmation,
//! shows real-time progress and returns the final report.

use std::collections::HashMap;
use std::io;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{backend::CrosstermBackend, Frame, Terminal};

use upone_core::plan::{Plan, Task};
use upone_core::{Event as CoreEvent, Report, StepStatus};

const SPINNER: [&str; 4] = ["\u{2588}", "\u{2592}", "\u{2591}", "\u{2588}"];
const CHECK: &str = "\u{2713}";
const CROSS: &str = "\u{2717}";
const ARROW: &str = "\u{2192}";

enum TaskUi {
    Pending,
    Running,
    Done,
    Failed,
}

struct App {
    tasks: Vec<Task>,
    state: HashMap<String, TaskUi>,
    confirmed: bool,
    finished: bool,
    tick: u64,
}

/// Runs the TUI. When confirmed (Enter or `yes`), starts the engine in
/// a thread and draws the progress. Returns the final report.
pub fn run(
    plan: &Arc<Plan>,
    rx: &Receiver<CoreEvent>,
    yes: bool,
    start_engine: impl FnOnce() -> anyhow::Result<Report> + Send + 'static,
) -> anyhow::Result<Report> {
    let mut tasks: Vec<Task> = plan.tasks().cloned().collect();
    // `Plan` stores tasks in a HashMap; sort by id so the drawn list is
    // stable across runs (execution order itself is deterministic via levels).
    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    let mut app = App {
        tasks,
        state: HashMap::new(),
        confirmed: yes,
        finished: false,
        tick: 0,
    };
    for id in plan.ids() {
        app.state.insert(id.clone(), TaskUi::Pending);
    }

    enable_raw_mode()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;

    let result = run_loop(&mut terminal, &mut app, rx, start_engine);

    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    rx: &Receiver<CoreEvent>,
    start_engine: impl FnOnce() -> anyhow::Result<Report> + Send + 'static,
) -> anyhow::Result<Report> {
    let mut engine: Option<thread::JoinHandle<anyhow::Result<Report>>> = None;
    let mut pending_start = Some(start_engine);
    let mut final_report: Option<Report> = None;

    loop {
        if app.confirmed && !app.finished && engine.is_none() {
            if let Some(start) = pending_start.take() {
                engine = Some(thread::spawn(start));
            }
        }

        if engine.is_some() {
            loop {
                match rx.try_recv() {
                    Ok(ev) => apply_event(&mut app.state, ev),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        if let Some(handle) = engine.take() {
                            final_report = Some(
                                handle
                                    .join()
                                    .map_err(|_| anyhow::anyhow!("engine thread panicked"))??,
                            );
                        }
                        app.finished = true;
                        break;
                    }
                }
                if app.finished {
                    break;
                }
            }
        }

        terminal.draw(|f| ui(f, app))?;

        if app.finished {
            // wait for the user to see the result
            if let Ok(enabled) = event::poll(Duration::from_millis(100)) {
                if enabled
                    && matches!(event::read()?, Event::Key(k) if matches!(k.code, KeyCode::Char('q')))
                {
                    break;
                }
            }
        } else if !app.confirmed {
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(k) = event::read()? {
                    match k.code {
                        KeyCode::Enter => app.confirmed = true,
                        KeyCode::Char('q') => break,
                        _ => {}
                    }
                }
            }
        } else {
            thread::sleep(Duration::from_millis(50));
        }

        app.tick += 1;
    }

    Ok(final_report.unwrap_or_default())
}

fn apply_event(state: &mut HashMap<String, TaskUi>, ev: CoreEvent) {
    match ev {
        CoreEvent::StepDone(step) => {
            let ui = match &step.status {
                StepStatus::Done(_) => TaskUi::Done,
                StepStatus::Error(_) => TaskUi::Failed,
                StepStatus::Running => TaskUi::Pending,
            };
            state.insert(step.task_id, ui);
        }
        CoreEvent::StepStarting(id, _) => {
            state.insert(id, TaskUi::Running);
        }
    }
}

fn ui(f: &mut Frame<'_>, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(area);

    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            "upone",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  preparing environment"),
    ]))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    let mut lines: Vec<Line<'_>> = Vec::new();
    for task in &app.tasks {
        let state = app.state.get(&task.id).unwrap_or(&TaskUi::Pending);
        let (icon, color) = match state {
            TaskUi::Pending => ("\u{00b7}", Color::DarkGray),
            TaskUi::Running => (
                SPINNER[(app.tick / 4) as usize % SPINNER.len()],
                Color::Yellow,
            ),
            TaskUi::Done => (CHECK, Color::Green),
            TaskUi::Failed => (CROSS, Color::Red),
        };
        let risk = task.risk.label();
        lines.push(Line::from(vec![
            Span::styled(format!("{icon} {ARROW} "), Style::default().fg(color)),
            Span::styled(
                task.label.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" (risk: {risk})"),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    let body = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" plan "))
        .wrap(Wrap { trim: true });
    f.render_widget(body, chunks[1]);

    let footer_text = if !app.confirmed {
        "Enter to run the plan · q to quit"
    } else if app.finished {
        "done · q to quit"
    } else {
        "running... (q to abort)"
    };
    let footer = Paragraph::new(Line::from(Span::styled(
        footer_text,
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(Alignment::Center)
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[2]);
}
