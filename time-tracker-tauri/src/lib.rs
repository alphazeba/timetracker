use chrono::Utc;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use time_tracker_lib::{add_note, list_sessions, start_timer, stop_timer, Database, ListOptions};

pub fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".time-tracker").join("db.sqlite")
}

pub struct AppState(pub Mutex<Database>);

#[derive(Serialize)]
pub struct NoteDto {
    pub text: String,
    pub created_at_ms: i64,
    pub offset_ms: i64,
}

#[derive(Serialize)]
pub struct SessionDto {
    pub title: String,
    pub start_time_ms: i64,
    pub end_time_ms: Option<i64>,
    pub running: bool,
    pub notes: Vec<NoteDto>,
}

pub fn do_start(db: &Database, title: &str) -> Result<String, String> {
    start_timer(db, title, Utc::now())
        .map(|r| format!("Started \"{}\"", r.new_session.title))
        .map_err(|e| e.to_string())
}

pub fn do_stop(db: &Database) -> Result<String, String> {
    stop_timer(db, Utc::now())
        .map(|s| format!("Stopped \"{}\"", s.title))
        .map_err(|e| e.to_string())
}

pub fn do_note(db: &Database, text: &str) -> Result<String, String> {
    add_note(db, text, Utc::now())
        .map(|_| "Note saved".to_string())
        .map_err(|e| e.to_string())
}

pub fn do_list(db: &Database) -> Result<Vec<SessionDto>, String> {
    list_sessions(db, ListOptions::default())
        .map(|sessions| {
            sessions.into_iter().map(|s| {
                let notes = s.notes.iter().map(|n| NoteDto {
                    text: n.text.clone(),
                    created_at_ms: n.created_at.timestamp_millis(),
                    offset_ms: (n.created_at - s.start_time).num_milliseconds(),
                }).collect();
                SessionDto {
                    running: s.end_time.is_none(),
                    start_time_ms: s.start_time.timestamp_millis(),
                    end_time_ms: s.end_time.map(|t| t.timestamp_millis()),
                    title: s.title,
                    notes,
                }
            }).collect()
        })
        .map_err(|e| e.to_string())
}
