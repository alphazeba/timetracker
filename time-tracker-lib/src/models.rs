use chrono::{DateTime, Utc};

/// A tracked work session with an optional end time and associated notes.
pub struct Session {
    /// Human-readable label for this session.
    pub title: String,
    /// UTC timestamp when the session was started.
    pub start_time: DateTime<Utc>,
    /// UTC timestamp when the session was stopped; `None` while active.
    pub end_time: Option<DateTime<Utc>>,
    /// Notes attached to this session, ordered by creation time.
    pub notes: Vec<Note>,
}

/// A timestamped note attached to a session.
pub struct Note {
    /// The `start_time` of the parent session (foreign-key reference).
    pub session_start: DateTime<Utc>,
    /// The note body text.
    pub text: String,
    /// UTC timestamp when the note was created.
    pub created_at: DateTime<Utc>,
}

/// Returned by `start_timer`: the newly created session and any session that
/// was automatically stopped to make room for it.
pub struct StartResult {
    /// The session that was just started.
    pub new_session: Session,
    /// The previously active session that was stopped, if any.
    pub stopped_session: Option<Session>,
}

/// Filters for `list_sessions`.
#[derive(Default)]
pub struct ListOptions {
    /// If set, only sessions whose title contains this string are returned.
    pub title_filter: Option<String>,
    /// Lower bound on `start_time` (inclusive). `None` means no lower bound.
    pub since: Option<DateTime<Utc>>,
    /// Upper bound on `start_time` (inclusive). `None` means no upper bound.
    pub latest: Option<DateTime<Utc>>,
}
