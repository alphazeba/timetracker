use chrono::{Local, TimeZone, Utc};
use time_tracker_lib::{add_note, list_sessions, start_timer, stop_timer, Database, ListOptions, Session};

// ── filter ────────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
pub enum TimeFilter {
    Days(u32),
    All,
}

impl TimeFilter {
    pub fn label(&self) -> String {
        match self {
            TimeFilter::Days(n) => format!("last {}d", n),
            TimeFilter::All => "last ∞".to_string(),
        }
    }
}

// ── mode ──────────────────────────────────────────────────────────────────────

pub enum Mode {
    Normal,
    Input(InputAction),
}

pub enum InputAction {
    Start,
    Note,
    TextFilter,
}

// ── app ───────────────────────────────────────────────────────────────────────

pub struct App {
    pub db: Database,
    pub sessions: Vec<Session>,
    pub input: String,
    pub mode: Mode,
    pub status: String,
    pub scroll_offset: u16,
    pub user_scrolled: bool,
    pub text_filter: String,
    pub time_filter: TimeFilter,
    /// Max lines we can scroll up — updated each render frame.
    pub max_scroll: u16,
}

impl App {
    pub fn new(db: Database) -> Self {
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

    pub fn build_list_opts(&self) -> ListOptions {
        let text_filter = if self.text_filter.is_empty() {
            None
        } else {
            Some(self.text_filter.clone())
        };
        let (since, latest) = match &self.time_filter {
            TimeFilter::Days(n) => {
                let target_date =
                    Local::now().date_naive() - chrono::Duration::days(*n as i64 - 1);
                let since = Local
                    .from_local_datetime(&target_date.and_hms_opt(0, 0, 0).unwrap())
                    .earliest()
                    .map(|dt| dt.with_timezone(&Utc));
                (since, None)
            }
            TimeFilter::All => (None, None),
        };
        ListOptions { text_filter, since, latest }
    }

    pub fn refresh(&mut self) {
        let opts = self.build_list_opts();
        self.sessions = list_sessions(&self.db, opts).unwrap_or_default();
        if !self.user_scrolled {
            self.scroll_offset = 0;
        }
    }

    pub fn refresh_and_jump(&mut self) {
        self.user_scrolled = false;
        self.refresh();
    }

    pub fn active_session(&self) -> Option<&Session> {
        self.sessions.iter().find(|s| s.end_time.is_none())
    }

    fn update_status<T: Into<String>, E: std::fmt::Display>(&mut self, result: Result<T, E>) {
        match result {
            Ok(t) => {
                self.status = t.into();
                self.refresh_and_jump();
            }
            Err(e) => self.status = format!("Error: {e}"),
        }
    }

    pub fn handle_start(&mut self, title: &str) {
        let result = start_timer(&self.db, title, Utc::now())
            .map(|r| format!("Started \"{}\"", r.new_session.title));
        self.update_status(result);
    }

    pub fn handle_stop(&mut self) {
        let result = stop_timer(&self.db, Utc::now())
            .map(|s| format!("Stopped \"{}\"", s.title));
        self.update_status(result);
    }

    pub fn handle_note(&mut self, text: &str) {
        let result = add_note(&self.db, text, Utc::now()).map(|_| "Note saved".to_string());
        self.update_status(result);
    }

    pub fn cancel_input(&mut self) {
        self.input.clear();
        self.mode = Mode::Normal;
    }
}
