use chrono::{DateTime, Utc};

use crate::db::Database;
use crate::error::{Error, Result};
use crate::models::{ListOptions, Note, Session, StartResult};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Load all notes for a session identified by its start_time (ms).
fn load_notes(db: &Database, session_start_ms: i64) -> Result<Vec<Note>> {
    let mut stmt = db.conn.prepare(
        "SELECT created_at, session_start, text FROM notes WHERE session_start = ? ORDER BY created_at ASC",
    )?;
    let notes = stmt
        .query_map([session_start_ms], |row| {
            let created_at_ms: i64 = row.get(0)?;
            let session_start_ms: i64 = row.get(1)?;
            let text: String = row.get(2)?;
            Ok((created_at_ms, session_start_ms, text))
        })?
        .map(|r| {
            let (created_at_ms, ss_ms, text) = r?;
            Ok(Note {
                session_start: DateTime::from_timestamp_millis(ss_ms).ok_or_else(|| {
                    Error::ExternalError("invalid session_start timestamp".into())
                })?,
                text,
                created_at: DateTime::from_timestamp_millis(created_at_ms)
                    .ok_or_else(|| Error::ExternalError("invalid created_at timestamp".into()))?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(notes)
}

/// Load a full Session (with notes) from a row of (start_time_ms, title, end_time_ms_opt).
fn row_to_session(
    db: &Database,
    start_ms: i64,
    title: String,
    end_ms: Option<i64>,
) -> Result<Session> {
    let start_time = DateTime::from_timestamp_millis(start_ms)
        .ok_or_else(|| Error::ExternalError("invalid start_time timestamp".into()))?;
    let end_time = end_ms
        .map(|ms| {
            DateTime::from_timestamp_millis(ms)
                .ok_or_else(|| Error::ExternalError("invalid end_time timestamp".into()))
        })
        .transpose()?;
    let notes = load_notes(db, start_ms)?;
    Ok(Session {
        title,
        start_time,
        end_time,
        notes,
    })
}

/// Query for the active session (end_time IS NULL).
/// Returns `Err(DatabaseIntegrityError)` if more than one is found.
fn query_active(db: &Database) -> Result<Option<(i64, String)>> {
    let mut stmt = db
        .conn
        .prepare("SELECT start_time, title FROM sessions WHERE end_time IS NULL")?;
    let rows: Vec<(i64, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    match rows.len() {
        0 => Ok(None),
        1 => Ok(Some(rows.into_iter().next().unwrap())),
        _ => Err(Error::DatabaseIntegrityError(
            "more than one active session found".into(),
        )),
    }
}

// ── public operations ─────────────────────────────────────────────────────────

/// Start a new timer. If another timer is active it is stopped first.
pub fn start_timer(db: &Database, title: &str, now: DateTime<Utc>) -> Result<StartResult> {
    let now_ms = now.timestamp_millis();

    // Check for an existing active session.
    let stopped_session = if let Some((active_start_ms, active_title)) = query_active(db)? {
        // Stop it.
        db.conn.execute(
            "UPDATE sessions SET end_time = ? WHERE end_time IS NULL",
            [now_ms],
        )?;
        // Build the stopped session (end_time = now).
        let notes = load_notes(db, active_start_ms)?;
        let start_time = DateTime::from_timestamp_millis(active_start_ms)
            .ok_or_else(|| Error::ExternalError("invalid start_time timestamp".into()))?;
        Some(Session {
            title: active_title,
            start_time,
            end_time: Some(now),
            notes,
        })
    } else {
        None
    };

    // Insert the new session.
    db.conn.execute(
        "INSERT INTO sessions (start_time, title) VALUES (?, ?)",
        rusqlite::params![now_ms, title],
    )?;

    let new_session = Session {
        title: title.to_string(),
        start_time: now,
        end_time: None,
        notes: vec![],
    };

    Ok(StartResult {
        new_session,
        stopped_session,
    })
}

/// Stop the currently active timer.
pub fn stop_timer(db: &Database, now: DateTime<Utc>) -> Result<Session> {
    let now_ms = now.timestamp_millis();

    let (active_start_ms, active_title) = query_active(db)?.ok_or(Error::NoActiveTimer)?;

    db.conn.execute(
        "UPDATE sessions SET end_time = ? WHERE end_time IS NULL",
        [now_ms],
    )?;

    row_to_session(db, active_start_ms, active_title, Some(now_ms))
}

/// Add a note to the currently active timer.
pub fn add_note(db: &Database, text: &str, now: DateTime<Utc>) -> Result<Note> {
    let now_ms = now.timestamp_millis();

    let (active_start_ms, _) = query_active(db)?.ok_or(Error::NoActiveTimer)?;

    db.conn.execute(
        "INSERT INTO notes (created_at, session_start, text) VALUES (?, ?, ?)",
        rusqlite::params![now_ms, active_start_ms, text],
    )?;

    let session_start = DateTime::from_timestamp_millis(active_start_ms)
        .ok_or_else(|| Error::ExternalError("invalid session_start timestamp".into()))?;

    Ok(Note {
        session_start,
        text: text.to_string(),
        created_at: now,
    })
}

/// List sessions with optional filters.
pub fn list_sessions(db: &Database, opts: ListOptions) -> Result<Vec<Session>> {
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    // Text filter: match session title OR any associated note text
    if let Some(ref filter) = opts.text_filter {
        let pattern = format!("%{}%", filter);
        conditions.push(
            "(title LIKE ? OR start_time IN (SELECT session_start FROM notes WHERE text LIKE ?))"
                .to_string(),
        );
        params.push(Box::new(pattern.clone()));
        params.push(Box::new(pattern));
    }

    // Time bounds: active session always included regardless of since/latest
    let mut time_conditions: Vec<String> = Vec::new();
    if let Some(since) = opts.since {
        time_conditions.push("start_time >= ?".to_string());
        params.push(Box::new(since.timestamp_millis()));
    }
    if let Some(latest) = opts.latest {
        time_conditions.push("start_time <= ?".to_string());
        params.push(Box::new(latest.timestamp_millis()));
    }
    if !time_conditions.is_empty() {
        conditions.push(format!("(end_time IS NULL OR ({}))", time_conditions.join(" AND ")));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!(
        "SELECT start_time, title, end_time FROM sessions {} ORDER BY start_time DESC",
        where_clause
    );

    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = db.conn.prepare(&sql)?;
    let rows: Vec<(i64, String, Option<i64>)> = stmt
        .query_map(param_refs.as_slice(), |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<std::result::Result<_, _>>()?;

    rows.into_iter()
        .map(|(start_ms, title, end_ms)| row_to_session(db, start_ms, title, end_ms))
        .collect()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn ts(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(secs, 0).unwrap()
    }

    // ── integration test: start → note → stop → list ──────────────────────

    #[test]
    fn test_start_note_stop_list() {
        let db = Database::open_in_memory().unwrap();

        let t0 = ts(1_000_000);
        let t1 = ts(1_000_030);
        let t2 = ts(1_000_060);

        // Start timer
        let result = start_timer(&db, "my task", t0).unwrap();
        assert_eq!(result.new_session.title, "my task");
        assert_eq!(result.new_session.start_time, t0);
        assert!(result.stopped_session.is_none());

        // Add note
        let note = add_note(&db, "halfway there", t1).unwrap();
        assert_eq!(note.text, "halfway there");
        assert_eq!(note.created_at, t1);

        // Stop timer
        let session = stop_timer(&db, t2).unwrap();
        assert_eq!(session.title, "my task");
        assert_eq!(session.start_time, t0);
        assert_eq!(session.end_time, Some(t2));
        assert_eq!(session.notes.len(), 1);
        assert_eq!(session.notes[0].text, "halfway there");

        // List sessions
        let sessions = list_sessions(
            &db,
            ListOptions {
                text_filter: None,
                since: None,
                latest: None,
            },
        )
        .unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].title, "my task");
        assert_eq!(sessions[0].start_time, t0);
        assert_eq!(sessions[0].end_time, Some(t2));
        assert_eq!(sessions[0].notes.len(), 1);
        assert_eq!(sessions[0].notes[0].text, "halfway there");
    }

    #[test]
    fn test_stop_no_active_timer() {
        let db = Database::open_in_memory().unwrap();
        let result = stop_timer(&db, ts(1_000_000));
        assert!(matches!(result, Err(Error::NoActiveTimer)));
    }

    #[test]
    fn test_add_note_no_active_timer() {
        let db = Database::open_in_memory().unwrap();
        let result = add_note(&db, "note", ts(1_000_000));
        assert!(matches!(result, Err(Error::NoActiveTimer)));
    }

    #[test]
    fn test_auto_stop_on_second_start() {
        let db = Database::open_in_memory().unwrap();
        let t0 = ts(1_000_000);
        let t1 = ts(1_000_100);

        start_timer(&db, "first", t0).unwrap();
        let result = start_timer(&db, "second", t1).unwrap();

        assert!(result.stopped_session.is_some());
        let stopped = result.stopped_session.unwrap();
        assert_eq!(stopped.title, "first");
        assert_eq!(stopped.end_time, Some(t1));
        assert_eq!(result.new_session.title, "second");
    }

    #[test]
    fn test_list_title_filter() {
        let db = Database::open_in_memory().unwrap();
        start_timer(&db, "alpha task", ts(1_000_000)).unwrap();
        stop_timer(&db, ts(1_000_100)).unwrap();
        start_timer(&db, "beta task", ts(1_000_200)).unwrap();
        stop_timer(&db, ts(1_000_300)).unwrap();

        let sessions = list_sessions(
            &db,
            ListOptions {
                text_filter: Some("alpha".to_string()),
                since: None,
                latest: None,
            },
        )
        .unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].title, "alpha task");
    }

    #[test]
    fn test_list_note_text_filter() {
        let db = Database::open_in_memory().unwrap();
        start_timer(&db, "task one", ts(1_000_000)).unwrap();
        add_note(&db, "important note here", ts(1_000_050)).unwrap();
        stop_timer(&db, ts(1_000_100)).unwrap();
        start_timer(&db, "task two", ts(1_000_200)).unwrap();
        stop_timer(&db, ts(1_000_300)).unwrap();

        // Filter by note text — should return "task one" even though title doesn't match
        let sessions = list_sessions(
            &db,
            ListOptions {
                text_filter: Some("important".to_string()),
                since: None,
                latest: None,
            },
        )
        .unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].title, "task one");
    }

    #[test]
    fn test_list_since_filter_includes_active() {
        let db = Database::open_in_memory().unwrap();
        // Old completed session
        start_timer(&db, "old", ts(1_000)).unwrap();
        stop_timer(&db, ts(2_000)).unwrap();
        // Active session started before since
        start_timer(&db, "active old", ts(3_000)).unwrap();

        let since = ts(5_000); // both sessions are before this
        let sessions = list_sessions(
            &db,
            ListOptions {
                text_filter: None,
                since: Some(since),
                latest: None,
            },
        )
        .unwrap();

        // Active session should always be included
        assert!(sessions.iter().any(|s| s.title == "active old"));
        // Old completed session should be excluded
        assert!(!sessions.iter().any(|s| s.title == "old"));
    }

    #[test]
    fn test_list_ordered_desc() {
        let db = Database::open_in_memory().unwrap();
        start_timer(&db, "first", ts(1_000_000)).unwrap();
        stop_timer(&db, ts(1_001_000)).unwrap();
        start_timer(&db, "second", ts(1_002_000)).unwrap();
        stop_timer(&db, ts(1_003_000)).unwrap();

        let sessions = list_sessions(
            &db,
            ListOptions {
                text_filter: None,
                since: None,
                latest: None,
            },
        )
        .unwrap();
        assert_eq!(sessions.len(), 2);
        assert!(sessions[0].start_time >= sessions[1].start_time);
    }
}
