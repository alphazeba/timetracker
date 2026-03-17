// time-tracker-lib: core business logic for the time-tracker application

pub mod error;
pub use error::{Error, Result};

pub mod models;
pub use models::*;

pub mod db;
pub use db::Database;

pub mod elapsed;
pub use elapsed::{format_elapsed, note_offset};

pub mod ops;
pub use ops::{add_note, list_sessions, start_timer, stop_timer};
