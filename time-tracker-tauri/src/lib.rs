use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;
use time_tracker_lib::{
    add_note, list_sessions, start_timer, stop_timer, Database, ListOptions,
};

fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".time-tracker").join("db.sqlite")
}

pub struct AppState(pub Mutex<Database>);

// ── serialisable DTOs ─────────────────────────────────────────────────────────

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

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn cmd_start(title: String, state: State<AppState>) -> Result<String, String> {
    let db = state.0.lock().unwrap();
    let now = Utc::now();
    start_timer(&db, &title, now)
        .map(|r| format!("Started \"{}\"", r.new_session.title))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cmd_stop(state: State<AppState>) -> Result<String, String> {
    let db = state.0.lock().unwrap();
    stop_timer(&db, Utc::now())
        .map(|s| format!("Stopped \"{}\"", s.title))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cmd_note(text: String, state: State<AppState>) -> Result<String, String> {
    let db = state.0.lock().unwrap();
    add_note(&db, &text, Utc::now())
        .map(|_| "Note saved".to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cmd_list(state: State<AppState>) -> Result<Vec<SessionDto>, String> {
    let db = state.0.lock().unwrap();
    list_sessions(&db, ListOptions::default())
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

pub fn run() {
    let db = Database::open(&db_path()).expect("failed to open database");
    tauri::Builder::default()
        .manage(AppState(Mutex::new(db)))
        .invoke_handler(tauri::generate_handler![cmd_start, cmd_stop, cmd_note, cmd_list])
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}
