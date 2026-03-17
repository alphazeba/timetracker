use chrono::{Local, Utc};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use std::{io, path::PathBuf};
use time_tracker_lib::{
    add_note, list_sessions, start_timer, stop_timer, Database, ListOptions, Session,
};

fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".time-tracker").join("db.sqlite")
}

enum Mode {
    Normal,
    Input(InputAction),
}

enum InputAction {
    Start,
    Note,
}

struct App {
    db: Database,
    sessions: Vec<Session>,
    input: String,
    mode: Mode,
    status: String,
}

impl App {
    fn new(db: Database) -> Self {
        let mut app = Self {
            db,
            sessions: vec![],
            input: String::new(),
            mode: Mode::Normal,
            status: String::new(),
        };
        app.refresh();
        app
    }

    fn refresh(&mut self) {
        self.sessions = list_sessions(&self.db, ListOptions::default()).unwrap_or_default();
    }

    fn active_session(&self) -> Option<&Session> {
        self.sessions.iter().find(|s| s.end_time.is_none())
    }

    fn handle_start(&mut self, title: &str) {
        let now = Utc::now();
        match start_timer(&self.db, title, now) {
            Ok(r) => {
                self.status = format!("Started \"{}\"", r.new_session.title);
                self.refresh();
            }
            Err(e) => self.status = format!("Error: {e}"),
        }
    }

    fn handle_stop(&mut self) {
        match stop_timer(&self.db, Utc::now()) {
            Ok(s) => {
                self.status = format!("Stopped \"{}\"", s.title);
                self.refresh();
            }
            Err(e) => self.status = format!("Error: {e}"),
        }
    }

    fn handle_note(&mut self, text: &str) {
        match add_note(&self.db, text, Utc::now()) {
            Ok(_) => {
                self.status = "Note saved".to_string();
                self.refresh();
            }
            Err(e) => self.status = format!("Error: {e}"),
        }
    }
}

fn fmt_duration(secs: i64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

fn render(f: &mut ratatui::Frame, app: &App) {
    let now = Utc::now();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    // ── session list — oldest first so newest is at the bottom ────────────────
    let mut all_lines: Vec<Line> = Vec::new();
    for s in app.sessions.iter().rev() {
        let end = s.end_time.unwrap_or(now);
        let secs = (end - s.start_time).num_seconds().abs();
        let running = s.end_time.is_none();
        let start_str = s
            .start_time
            .with_timezone(&Local)
            .format("%H:%M:%S")
            .to_string();
        all_lines.push(Line::from(vec![
            Span::styled(
                format!("[{} | {}] ", start_str, fmt_duration(secs)),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                s.title.clone(),
                Style::default().fg(Color::Cyan).add_modifier(if running {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
            ),
            if running {
                Span::styled(" [running]", Style::default().fg(Color::Green))
            } else {
                Span::raw("")
            },
        ]));
        for note in &s.notes {
            let offset = (note.created_at - s.start_time).num_seconds().abs();
            all_lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("[{} | {}] ", start_str, fmt_duration(offset)),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(note.text.clone(), Style::default().fg(Color::Yellow)),
            ]));
        }
    }

    // Scroll so the last line sits at the bottom of the inner area (height - 2 for borders)
    let inner_height = chunks[0].height.saturating_sub(2) as usize;
    let total_lines = all_lines.len();
    let scroll = total_lines.saturating_sub(inner_height) as u16;

    let para = Paragraph::new(all_lines)
        .block(Block::default().borders(Borders::ALL).title(" Sessions "))
        .scroll((scroll, 0));
    f.render_widget(para, chunks[0]);

    // ── input / help bar ──────────────────────────────────────────────────────
    let (title, content) = match &app.mode {
        Mode::Input(InputAction::Start) => (" Start timer — title ", app.input.as_str()),
        Mode::Input(InputAction::Note) => (" Add note ", app.input.as_str()),
        Mode::Normal => (" Keys ", "s=start  x=stop  n=note  q=quit"),
    };
    let para = Paragraph::new(content).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(para, chunks[1]);

    // ── status bar ────────────────────────────────────────────────────────────
    let status = Paragraph::new(app.status.as_str()).style(Style::default().fg(Color::DarkGray));
    f.render_widget(status, chunks[2]);
}

fn main() -> io::Result<()> {
    let db = Database::open(&db_path()).expect("failed to open database");
    let mut app = App::new(db);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| render(f, &app))?;

        if event::poll(std::time::Duration::from_millis(500))? {
            if let Event::Key(key) = event::read()? {
                match &app.mode {
                    Mode::Normal => match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('s') => app.mode = Mode::Input(InputAction::Start),
                        KeyCode::Char('x') => app.handle_stop(),
                        KeyCode::Char('n') => {
                            if app.active_session().is_some() {
                                app.mode = Mode::Input(InputAction::Note);
                            } else {
                                app.status = "No active timer".to_string();
                            }
                        }
                        _ => {}
                    },
                    Mode::Input(_) => match key.code {
                        KeyCode::Enter => {
                            let text = app.input.trim().to_string();
                            if !text.is_empty() {
                                match &app.mode {
                                    Mode::Input(InputAction::Start) => app.handle_start(&text),
                                    Mode::Input(InputAction::Note) => app.handle_note(&text),
                                    _ => {}
                                }
                            }
                            app.input.clear();
                            app.mode = Mode::Normal;
                        }
                        KeyCode::Esc => {
                            app.input.clear();
                            app.mode = Mode::Normal;
                        }
                        KeyCode::Backspace => {
                            app.input.pop();
                        }
                        KeyCode::Char(c) if key.modifiers != KeyModifiers::CONTROL => {
                            app.input.push(c);
                        }
                        _ => {}
                    },
                }
            }
        }

        // Refresh every tick so running timer durations update
        if matches!(app.mode, Mode::Normal) {
            app.refresh();
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
