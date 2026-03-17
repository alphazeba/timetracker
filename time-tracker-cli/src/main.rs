use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use std::path::PathBuf;
use time_tracker_lib::{
    add_note, list_sessions, start_timer, stop_timer, Database, Error, ListOptions, Note, Session,
};

// ── CLI definition ────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "tt", about = "A simple command-line time tracker")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start a new timer (stops any running timer first)
    #[command(alias = "s")]
    Start {
        #[arg(num_args = 1.., value_delimiter = None)]
        words: Vec<String>,
    },
    /// Stop the currently running timer
    #[command(alias = "x")]
    Stop,
    /// Add a note to the running timer
    #[command(alias = "n")]
    Note {
        #[arg(num_args = 1.., value_delimiter = None)]
        words: Vec<String>,
    },
    /// List timer sessions
    #[command(alias = "l")]
    List {
        /// Show sessions from the last N days (default: 1)
        #[arg(long, short)]
        days: Option<u32>,
        /// Filter by title substring
        #[arg(long, short)]
        title: Option<String>,
        /// Show sessions for a specific local date (YYYY-MM-DD)
        #[arg(long)]
        date: Option<String>,
        /// Show all sessions (no time filter)
        #[arg(long, short)]
        all: bool,
    },
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".time-tracker").join("db.sqlite")
}

/// `[HH:MM:SS(dimmed) | HH:MM:SS(color)]` — `duration_color` lets callers pick pink vs yellow.
fn fmt_time_range(
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    duration_color: fn(&str) -> String,
) -> String {
    let from_str = from.with_timezone(&Local).format("%H:%M:%S").to_string();
    let secs = (to - from).num_seconds().abs();
    let dur_str = format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    );
    format!("[{} | {}]", from_str.dimmed(), duration_color(&dur_str))
}

fn timer_range(from: DateTime<Utc>, to: DateTime<Utc>) -> String {
    fmt_time_range(from, to, |s| s.bright_magenta().to_string())
}

fn note_range(from: DateTime<Utc>, to: DateTime<Utc>) -> String {
    fmt_time_range(from, to, |s| s.yellow().to_string())
}

fn print_notes(session_start: DateTime<Utc>, notes: &[Note]) {
    for note in notes {
        println!(
            "  {} {}",
            note_range(session_start, note.created_at),
            note.text.yellow()
        );
    }
}

fn print_session(session: &Session, now: DateTime<Utc>) {
    let end = session.end_time.unwrap_or(now);
    let running = if session.end_time.is_none() {
        " [running]".green().to_string()
    } else {
        String::new()
    };
    println!(
        "{} {}{}",
        timer_range(session.start_time, end),
        session.title.cyan(),
        running
    );
    print_notes(session.start_time, &session.notes);
}

fn handle_error(err: Error) -> ! {
    match err {
        Error::NoActiveTimer => eprintln!("{}", "No timer is currently running.".yellow()),
        Error::DatabaseIntegrityError(msg) => {
            eprintln!("{} {}", "Database integrity error:".red(), msg);
            eprintln!("You may need to inspect the database manually.");
        }
        Error::ExternalError(msg) => eprintln!("{} {}", "Error:".red(), msg),
    }
    std::process::exit(1);
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();
    let db = Database::open(&db_path()).unwrap_or_else(|e| handle_error(e));
    let now = Utc::now();

    match cli.command {
        Command::Start { words } => {
            let title = words.join(" ");
            match start_timer(&db, &title, now) {
                Ok(result) => {
                    if let Some(s) = result.stopped_session {
                        println!(
                            "{} \"{}\" (started {})",
                            "Stopped".yellow(),
                            s.title.cyan(),
                            s.start_time
                                .with_timezone(&Local)
                                .format("%Y-%m-%d %H:%M:%S")
                        );
                    }
                    println!(
                        "{} \"{}\" at {}",
                        "Started".green(),
                        result.new_session.title.cyan(),
                        result
                            .new_session
                            .start_time
                            .with_timezone(&Local)
                            .format("%Y-%m-%d %H:%M:%S")
                    );
                }
                Err(e) => handle_error(e),
            }
        }

        Command::Stop => match stop_timer(&db, now) {
            Ok(session) => {
                let end = session.end_time.unwrap_or(now);
                println!(
                    "{} \"{}\" {}",
                    "Stopped".yellow(),
                    session.title.cyan(),
                    timer_range(session.start_time, end)
                );
                print_notes(session.start_time, &session.notes);
            }
            Err(e) => handle_error(e),
        },

        Command::Note { words } => {
            let text = words.join(" ");
            match add_note(&db, &text, now) {
                Ok(note) => {
                    // Re-query active session to get start time for the offset display
                    let start = list_sessions(&db, ListOptions::default())
                        .unwrap_or_default()
                        .into_iter()
                        .find(|s| s.end_time.is_none())
                        .map(|s| s.start_time);
                    if let Some(start) = start {
                        println!(
                            "{} {} {}",
                            "Note saved:".green(),
                            note_range(start, note.created_at),
                            note.text.yellow()
                        );
                    } else {
                        println!("{} {}", "Note saved:".green(), note.text.yellow());
                    }
                }
                Err(e) => handle_error(e),
            }
        }

        Command::List {
            days,
            title,
            date,
            all,
        } => {
            let (since, latest) = if all {
                (None, None)
            } else if let Some(date_str) = date {
                match NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") {
                    Ok(d) => {
                        let since = Local
                            .from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
                            .single()
                            .map(|dt| dt.with_timezone(&Utc));
                        let latest = Local
                            .from_local_datetime(&d.and_hms_opt(23, 59, 59).unwrap())
                            .single()
                            .map(|dt| dt.with_timezone(&Utc));
                        (since, latest)
                    }
                    Err(_) => {
                        eprintln!("Invalid date format. Use YYYY-MM-DD.");
                        std::process::exit(1);
                    }
                }
            } else {
                (
                    Some(now - chrono::Duration::hours(days.unwrap_or(1) as i64 * 24)),
                    None,
                )
            };

            match list_sessions(
                &db,
                ListOptions {
                    title_filter: title,
                    since,
                    latest,
                },
            ) {
                Ok(sessions) if sessions.is_empty() => println!("No sessions found."),
                Ok(sessions) => sessions.iter().for_each(|s| print_session(s, now)),
                Err(e) => handle_error(e),
            }
        }
    }
}
