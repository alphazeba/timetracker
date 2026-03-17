#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;
use tauri::State;
use time_tracker_lib::Database;
use time_tracker_tauri::{do_list, do_note, do_start, do_stop, AppState, FilterOptions, SessionDto, db_path};

#[tauri::command]
fn cmd_start(title: String, state: State<AppState>) -> Result<String, String> {
    do_start(&state.0.lock().unwrap(), &title)
}

#[tauri::command]
fn cmd_stop(state: State<AppState>) -> Result<String, String> {
    do_stop(&state.0.lock().unwrap())
}

#[tauri::command]
fn cmd_note(text: String, state: State<AppState>) -> Result<String, String> {
    do_note(&state.0.lock().unwrap(), &text)
}

#[tauri::command]
fn cmd_list(filter: FilterOptions, state: State<AppState>) -> Result<Vec<SessionDto>, String> {
    do_list(&state.0.lock().unwrap(), filter)
}

fn main() {
    let db = Database::open(&db_path()).expect("failed to open database");
    tauri::Builder::default()
        .manage(AppState(Mutex::new(db)))
        .invoke_handler(tauri::generate_handler![cmd_start, cmd_stop, cmd_note, cmd_list])
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}
