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

// ── filter state ──────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum TimeFilter {
    Days(u32),
    All,
}

impl TimeFilter {
    fn label(&self) -> String {
        match self {
            TimeFilter::Days(n) => format!("last {}d", n),
            TimeFilter::All => "last ∞".to_string(),
        }
    }
}

// ── mode ──────────────────────────────────────────────────────────────────────

enum Mode {
    Normal,
    Input(InputAction),
}

enum InputAction {
    Start,
    Note,
    TextFilter,
}

// ── app ───────────────────────────────────────────────────────────────────────

struct App {
    db: Database,
    sessions: Vec<Session>,
    input: String,
    mode: Mode,
    status: String,
    scroll_offset: u16,
    user_scrolled: bool,
    text_filter: String,
    time_filter: TimeFilter,
    /// Max lines we can scroll up — updated each render frame.
    max_scroll: u16,
}

impl App {
    fn new(db: Database) -> Self {
        let mut app = Self {
            db,
            sessions: vec![],
            input: String::new(),
            mode: Mode::Normal,
            status: String::new(),
            scroll_offset: 0,
            user_scrolled: false,
            text_filter: String::new(),
            time_filter: TimeFilter::Days(1),
            max_scroll: 0,
        };
        app.refresh();
        app
    }

    fn build_list_opts(&self) -> ListOptions {
        let now = Utc::now();
        let text_filter = if self.text_filter.is_empty() {
            None
        } else {
            Some(self.text_filter.clone())
        };
        let (since, latest) = match &self.time_filter {
            TimeFilter::Days(n) => (
                Some(now - chrono::Duration::hours(*n as i64 * 24)),
                None,
            ),
            TimeFilter::All => (None, None),
        };
        ListOptions { text_filter, since, latest }
    }

    fn refresh(&mut self) {
        let opts = self.build_list_opts();
        self.sessions = list_sessions(&self.db, opts).unwrap_or_default();
        if !self.user_scrolled {
            self.scroll_offset = 0;
        }
    }

    fn refresh_and_jump(&mut self) {
        self.user_scrolled = false;
        self.refresh();
    }

    fn active_session(&self) -> Option<&Session> {
        self.sessions.iter().find(|s| s.end_time.is_none())
    }

    fn exec<T, E: std::fmt::Display>(&mut self, result: Result<T, E>, ok_msg: impl Into<String>) {
        match result {
            Ok(_) => {
                self.status = ok_msg.into();
                self.refresh_and_jump();
            }
            Err(e) => self.status = format!("Error: {e}"),
        }
    }

    fn handle_start(&mut self, title: &str) {
        let result = start_timer(&self.db, title, Utc::now())
            .map(|r| format!("Started \"{}\"", r.new_session.title));
        let msg = result.as_deref().unwrap_or("").to_string();
        self.exec(result.map(|_| ()), msg);
    }

    fn handle_stop(&mut self) {
        let result = stop_timer(&self.db, Utc::now())
            .map(|s| format!("Stopped \"{}\"", s.title));
        let msg = result.as_deref().unwrap_or("").to_string();
        self.exec(result.map(|_| ()), msg);
    }

    fn handle_note(&mut self, text: &str) {
        self.exec(add_note(&self.db, text, Utc::now()), "Note saved");
    }

    fn cancel_input(&mut self) {
        self.input.clear();
        self.mode = Mode::Normal;
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn fmt_duration(secs: i64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

// ── highlight ─────────────────────────────────────────────────────────────────

/// Split `text` into spans, highlighting every case-insensitive match of `term`.
/// Non-matching segments get `base_style`; matches get `base_style` + black-on-yellow.
fn highlight_spans<'a>(text: &'a str, term: &str, base_style: Style) -> Vec<Span<'a>> {
    if term.is_empty() {
        return vec![Span::styled(text.to_string(), base_style)];
    }
    let lower_text = text.to_lowercase();
    let lower_term = term.to_lowercase();
    let match_style = base_style
        .fg(Color::Black)
        .bg(Color::Rgb(255, 182, 193))
        .add_modifier(Modifier::BOLD);

    let mut spans = Vec::new();
    let mut pos = 0;
    while let Some(idx) = lower_text[pos..].find(&lower_term) {
        let abs = pos + idx;
        if abs > pos {
            spans.push(Span::styled(text[pos..abs].to_string(), base_style));
        }
        let end = abs + term.len();
        spans.push(Span::styled(text[abs..end].to_string(), match_style));
        pos = end;
    }
    if pos < text.len() {
        spans.push(Span::styled(text[pos..].to_string(), base_style));
    }
    spans
}

// ── render ────────────────────────────────────────────────────────────────────

fn render(f: &mut ratatui::Frame, app: &mut App) {
    let now = Utc::now();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // sessions
            Constraint::Length(1), // filter bar
            Constraint::Length(3), // input / help
            Constraint::Length(1), // status
        ])
        .split(f.area());

    // ── session list ──────────────────────────────────────────────────────────
    let mut all_lines: Vec<Line> = Vec::new();
    let mut last_date: Option<chrono::NaiveDate> = None;
    for s in app.sessions.iter().rev() {
        let date = s.start_time.with_timezone(&Local).date_naive();
        if last_date != Some(date) {
            let label = format!("── {} ──", date.format("%Y-%m-%d"));
            all_lines.push(Line::from(Span::styled(
                label,
                Style::default().fg(Color::DarkGray),
            )));
            last_date = Some(date);
        }
        let end = s.end_time.unwrap_or(now);
        let secs = (end - s.start_time).num_seconds().abs();
        let running = s.end_time.is_none();
        let start_str = s.start_time.with_timezone(&Local).format("%H:%M:%S").to_string();
        let title_style = Style::default().fg(Color::Cyan).add_modifier(if running {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });
        let mut session_spans = vec![
            Span::styled(
                format!("[{} | ", start_str),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                fmt_duration(secs),
                Style::default().fg(Color::Green),
            ),
            Span::styled("] ", Style::default().fg(Color::White)),
        ];
        session_spans.extend(highlight_spans(&s.title, &app.text_filter, title_style));
        if running {
            session_spans.push(Span::styled(" [running]", Style::default().fg(Color::Green)));
        }
        all_lines.push(Line::from(session_spans));
        for note in &s.notes {
            let offset = (note.created_at - s.start_time).num_seconds().abs();
            let mut note_spans = vec![
                Span::raw(" "),
                Span::styled(
                    format!("[{} | {}] ", start_str, fmt_duration(offset)),
                    Style::default().fg(Color::DarkGray),
                ),
            ];
            note_spans.extend(highlight_spans(&note.text, &app.text_filter, Style::default().fg(Color::Yellow)));
            all_lines.push(Line::from(note_spans));
        }
    }

    let inner_height = chunks[0].height.saturating_sub(2) as usize;
    let total_lines = all_lines.len();
    let auto_scroll = total_lines.saturating_sub(inner_height) as u16;
    // Store max so the event loop can clamp scroll_offset before we get here.
    app.max_scroll = auto_scroll;
    app.scroll_offset = app.scroll_offset.min(auto_scroll);
    let scroll = if app.user_scrolled {
        auto_scroll.saturating_sub(app.scroll_offset)
    } else {
        auto_scroll
    };

    let para = Paragraph::new(all_lines)
        .block(Block::default().borders(Borders::ALL).title(" Sessions "))
        .scroll((scroll, 0));
    f.render_widget(para, chunks[0]);

    // ── filter bar ────────────────────────────────────────────────────────────
    let text_label = if app.text_filter.is_empty() {
        Span::styled("filter: —", Style::default().fg(Color::DarkGray))
    } else {
        Span::styled(
            format!("filter: \"{}\"", app.text_filter),
            Style::default().fg(Color::Yellow),
        )
    };
    let time_label = Span::styled(
        format!("  time: [{}]", app.time_filter.label()),
        Style::default().fg(Color::Cyan),
    );
    let hint = if matches!(app.time_filter, TimeFilter::Days(_)) {
        Span::styled("  f=text  -/+=days  q=quit", Style::default().fg(Color::DarkGray))
    } else {
        Span::styled("  f=text  +=days  q=quit", Style::default().fg(Color::DarkGray))
    };
    let filter_line = Line::from(vec![text_label, time_label, hint]);
    f.render_widget(Paragraph::new(filter_line), chunks[1]);

    // ── input / help bar ──────────────────────────────────────────────────────
    let (bar_title, content) = match &app.mode {
        Mode::Input(InputAction::Start) => (" Start timer — title ", app.input.as_str()),
        Mode::Input(InputAction::Note) => (" Add note ", app.input.as_str()),
        Mode::Input(InputAction::TextFilter) => (" Text filter ", app.input.as_str()),
        Mode::Normal => (" Keys ", "s=start  x=stop  n=note  ↑/k=up  ↓/j=down"),
    };
    let para = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title(bar_title));
    f.render_widget(para, chunks[2]);

    // ── status bar ────────────────────────────────────────────────────────────
    let status = Paragraph::new(app.status.as_str()).style(Style::default().fg(Color::DarkGray));
    f.render_widget(status, chunks[3]);
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() -> io::Result<()> {
    let db = Database::open(&db_path()).expect("failed to open database");
    let mut app = App::new(db);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| render(f, &mut app))?;

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
                        KeyCode::Char('f') => {
                            // Pre-fill input with current filter so user can edit it
                            app.input = String::new();
                            app.mode = Mode::Input(InputAction::TextFilter);
                        }
                        KeyCode::Char('-') => {
                            app.time_filter = match app.time_filter {
                                TimeFilter::Days(1) => TimeFilter::All,
                                TimeFilter::Days(n) => TimeFilter::Days(n - 1),
                                TimeFilter::All => TimeFilter::All,
                            };
                            app.refresh();
                        }
                        KeyCode::Char('=') | KeyCode::Char('+') => {
                            app.time_filter = match app.time_filter {
                                TimeFilter::All => TimeFilter::Days(1),
                                TimeFilter::Days(n) => TimeFilter::Days(n + 1),
                            };
                            app.refresh();
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            app.scroll_offset = (app.scroll_offset + 1).min(app.max_scroll);
                            app.user_scrolled = true;
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if app.scroll_offset > 0 {
                                app.scroll_offset -= 1;
                            }
                            if app.scroll_offset == 0 {
                                app.user_scrolled = false;
                            }
                        }
                        _ => {}
                    },
                    Mode::Input(_) => match key.code {
                        KeyCode::Enter => {
                            let text = app.input.trim().to_string();
                            match &app.mode {
                                Mode::Input(InputAction::Start) if !text.is_empty() => {
                                    app.handle_start(&text);
                                }
                                Mode::Input(InputAction::Note) if !text.is_empty() => {
                                    app.handle_note(&text);
                                }
                                Mode::Input(InputAction::TextFilter) => {
                                    app.text_filter = text;
                                    app.refresh();
                                }
                                _ => {}
                            }
                            app.cancel_input();
                        }
                        KeyCode::Esc => {
                            app.cancel_input();
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
