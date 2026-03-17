use rusqlite::Connection;
use std::path::Path;

use crate::error::Result;

/// Opaque wrapper around a `rusqlite::Connection`.
pub struct Database {
    pub(crate) conn: Connection,
}

impl Database {
    /// Opens (or creates) the SQLite file at `path`, creating parent directories
    /// as needed, then initialises the schema.
    pub fn open(path: &Path) -> Result<Database> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        init_schema(&conn)?;
        Ok(Database { conn })
    }

    /// Opens an in-memory SQLite database (useful for tests).
    pub fn open_in_memory() -> Result<Database> {
        let conn = Connection::open_in_memory()?;
        init_schema(&conn)?;
        Ok(Database { conn })
    }
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS sessions (
            start_time INTEGER PRIMARY KEY,
            title      TEXT    NOT NULL,
            end_time   INTEGER
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_one_active
            ON sessions (1)
            WHERE end_time IS NULL;

        CREATE TABLE IF NOT EXISTS notes (
            created_at    INTEGER PRIMARY KEY,
            session_start INTEGER NOT NULL REFERENCES sessions(start_time),
            text          TEXT    NOT NULL
        );
        ",
    )?;
    Ok(())
}
