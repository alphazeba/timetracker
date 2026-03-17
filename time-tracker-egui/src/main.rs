use chrono::{Local, Utc};
use eframe::egui;
use std::path::PathBuf;
use time_tracker_lib::{
    add_note, list_sessions, start_timer, stop_timer, Database, ListOptions, Session,
};

fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".time-tracker").join("db.sqlite")
}

fn fmt_duration(secs: i64) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
}

struct App {
    db: Database,
    sessions: Vec<Session>,
    title_input: String,
    note_input: String,
    status: String,
}

impl App {
    fn new(db: Database) -> Self {
        let mut app = Self {
            db,
            sessions: vec![],
            title_input: String::new(),
            note_input: String::new(),
            status: String::new(),
        };
        app.refresh();
        app
    }

    fn refresh(&mut self) {
        self.sessions = list_sessions(&self.db, ListOptions::default()).unwrap_or_default();
    }

    fn active_session(&self) -> Option<&Session> {
        self.sessions.iter().find(|s| s.end_time.is_none())
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Utc::now();
        // Repaint every second so running timers tick
        ctx.request_repaint_after(std::time::Duration::from_secs(1));

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Title:");
                ui.text_edit_singleline(&mut self.title_input);
                if ui.button("▶ Start").clicked() {
                    let title = self.title_input.trim().to_string();
                    if !title.is_empty() {
                        match start_timer(&self.db, &title, now) {
                            Ok(r) => {
                                self.status = format!("Started \"{}\"", r.new_session.title);
                                self.title_input.clear();
                                self.refresh();
                            }
                            Err(e) => self.status = format!("Error: {e}"),
                        }
                    }
                }
                if ui.button("■ Stop").clicked() {
                    match stop_timer(&self.db, now) {
                        Ok(s) => {
                            self.status = format!("Stopped \"{}\"", s.title);
                            self.refresh();
                        }
                        Err(e) => self.status = format!("Error: {e}"),
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Note:");
                ui.text_edit_singleline(&mut self.note_input);
                let has_active = self.active_session().is_some();
                if ui.add_enabled(has_active, egui::Button::new("+ Note")).clicked() {
                    let text = self.note_input.trim().to_string();
                    if !text.is_empty() {
                        match add_note(&self.db, &text, now) {
                            Ok(_) => {
                                self.status = "Note saved".to_string();
                                self.note_input.clear();
                                self.refresh();
                            }
                            Err(e) => self.status = format!("Error: {e}"),
                        }
                    }
                }
            });
            if !self.status.is_empty() {
                ui.label(egui::RichText::new(&self.status).color(egui::Color32::GRAY));
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for session in &self.sessions {
                    let end = session.end_time.unwrap_or(now);
                    let secs = (end - session.start_time).num_seconds().abs();
                    let start_str = session.start_time.with_timezone(&Local).format("%H:%M:%S").to_string();
                    let running = session.end_time.is_none();

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(
                            format!("[{} | {}]", start_str, fmt_duration(secs))
                        ).color(if running { egui::Color32::GREEN } else { egui::Color32::WHITE }));
                        ui.label(egui::RichText::new(&session.title)
                            .color(egui::Color32::from_rgb(0, 200, 220))
                            .strong());
                        if running {
                            ui.label(egui::RichText::new("[running]").color(egui::Color32::GREEN));
                        }
                    });

                    for note in &session.notes {
                        let offset = (note.created_at - session.start_time).num_seconds().abs();
                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            ui.label(egui::RichText::new(
                                format!("[{} | {}]", start_str, fmt_duration(offset))
                            ).color(egui::Color32::DARK_GRAY));
                            ui.label(egui::RichText::new(&note.text).color(egui::Color32::YELLOW));
                        });
                    }
                    ui.separator();
                }
            });
        });
    }
}

fn main() -> eframe::Result {
    let db = Database::open(&db_path()).expect("failed to open database");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([600.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native("Time Tracker", options, Box::new(|_cc| Ok(Box::new(App::new(db)))))
}
