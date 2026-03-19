mod app;
mod highlight;
mod render;

use app::{App, InputAction, Mode, TimeFilter};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::Terminal;
use render::RenderOutput;
use std::{io, path::PathBuf};
use time_tracker_lib::Database;

fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".time-tracker").join("db.sqlite")
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
        let mut output = RenderOutput { max_scroll: 0 };
        terminal.draw(|f| { output = render::render(f, &app); })?;
        app.max_scroll = output.max_scroll;
        app.scroll_offset = app.scroll_offset.min(app.max_scroll);

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
                            app.input = String::new();
                            app.mode = Mode::Input(InputAction::TextFilter);
                        }
                        KeyCode::Char('-') => {
                            app.time_filter = match app.time_filter {
                                TimeFilter::Days(1) => TimeFilter::All,
                                TimeFilter::Days(n) => TimeFilter::Days(n - 1),
                                TimeFilter::All => TimeFilter::All,
                            };
                        }
                        KeyCode::Char('=') | KeyCode::Char('+') => {
                            app.time_filter = match app.time_filter {
                                TimeFilter::All => TimeFilter::Days(1),
                                TimeFilter::Days(n) => TimeFilter::Days(n + 1),
                            };
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
                    Mode::Input(action) => match key.code {
                        KeyCode::Enter => {
                            let text = app.input.trim().to_string();
                            match action {
                                InputAction::Start if !text.is_empty() => {
                                    app.handle_start(&text);
                                }
                                InputAction::Note if !text.is_empty() => {
                                    app.handle_note(&text);
                                }
                                InputAction::TextFilter => {
                                    app.text_filter = text;
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

        // Refresh every tick — reloads sessions and updates running durations.
        app.refresh();
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
