use chrono::Local;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use chrono::Utc;

use crate::app::{App, InputAction, Mode, TimeFilter};
use crate::highlight::highlight_spans;

fn fmt_duration(secs: i64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

pub struct RenderOutput {
    pub max_scroll: u16,
}

/// Render the full UI
pub fn render(f: &mut ratatui::Frame, app: &App) -> RenderOutput {
    let now = Utc::now();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // sessions
            Constraint::Length(1), // filter bar
            Constraint::Length(3), // input / help
            Constraint::Length(1), // status
        ])
        .split(f.area());

    // ── session list ──────────────────────────────────────────────────────────
    let mut date_totals: std::collections::HashMap<chrono::NaiveDate, i64> =
        std::collections::HashMap::new();
    for s in &app.sessions {
        let date = s.start_time.with_timezone(&Local).date_naive();
        let end = s.end_time.unwrap_or(now);
        let secs = (end - s.start_time).num_seconds().abs();
        *date_totals.entry(date).or_insert(0) += secs;
    }

    let mut all_lines: Vec<Line> = Vec::new();
    let mut last_date: Option<chrono::NaiveDate> = None;
    for s in app.sessions.iter().rev() {
        let date = s.start_time.with_timezone(&Local).date_naive();
        if last_date != Some(date) {
            let total = date_totals.get(&date).copied().unwrap_or(0);
            all_lines.push(Line::from(vec![
                Span::styled(
                    format!("── {} ", date.format("%Y-%m-%d")),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(fmt_duration(total), Style::default().fg(Color::Magenta)),
                Span::styled(" ──", Style::default().fg(Color::DarkGray)),
            ]));
            last_date = Some(date);
        }
        let end = s.end_time.unwrap_or(now);
        let secs = (end - s.start_time).num_seconds().abs();
        let running = s.end_time.is_none();
        let start_str = s.start_time.with_timezone(&Local).format("%H:%M:%S").to_string();
        let title_style = Style::default().fg(Color::Cyan).add_modifier(if running {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });
        let mut session_spans = vec![
            Span::styled(format!("[{} ", start_str), Style::default().fg(Color::White)),
            Span::styled(fmt_duration(secs), Style::default().fg(Color::Green)),
            Span::styled("] ", Style::default().fg(Color::White)),
        ];
        session_spans.extend(highlight_spans(&s.title, &app.text_filter, title_style));
        if running {
            session_spans.push(Span::styled(" [running]", Style::default().fg(Color::Green)));
        }
        all_lines.push(Line::from(session_spans));
        for note in &s.notes {
            let offset = (note.created_at - s.start_time).num_seconds().abs();
            let mut note_spans = vec![
                Span::raw("  "),
                Span::styled(
                    format!("[{} {}] ", start_str, fmt_duration(offset)),
                    Style::default().fg(Color::DarkGray),
                ),
            ];
            note_spans.extend(highlight_spans(
                &note.text,
                &app.text_filter,
                Style::default().fg(Color::Yellow),
            ));
            all_lines.push(Line::from(note_spans));
        }
    }

    let inner_height = chunks[0].height.saturating_sub(2) as usize;
    let total_lines = all_lines.len();
    let auto_scroll = total_lines.saturating_sub(inner_height) as u16;
    let clamped_offset = app.scroll_offset.min(auto_scroll);
    let scroll = if app.user_scrolled {
        auto_scroll.saturating_sub(clamped_offset)
    } else {
        auto_scroll
    };

    f.render_widget(
        Paragraph::new(all_lines)
            .block(Block::default().borders(Borders::ALL).title(" Sessions "))
            .scroll((scroll, 0)),
        chunks[0],
    );

    // ── filter bar ────────────────────────────────────────────────────────────
    let text_label = if app.text_filter.is_empty() {
        Span::styled("filter: —", Style::default().fg(Color::DarkGray))
    } else {
        Span::styled(
            format!("filter: \"{}\"", app.text_filter),
            Style::default().fg(Color::Yellow),
        )
    };
    let time_label = Span::styled(
        format!("  time: [{}]", app.time_filter.label()),
        Style::default().fg(Color::Cyan),
    );
    let hint = if matches!(app.time_filter, TimeFilter::Days(_)) {
        Span::styled("  f=filter  -/+=days  q=quit", Style::default().fg(Color::DarkGray))
    } else {
        Span::styled("  f=filter  +=days  q=quit", Style::default().fg(Color::DarkGray))
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![text_label, time_label, hint])),
        chunks[1],
    );

    // ── input / help bar ──────────────────────────────────────────────────────
    let (bar_title, content) = match &app.mode {
        Mode::Input(InputAction::Start) => (" Start timer — title ", app.input.as_str()),
        Mode::Input(InputAction::Note) => (" Add note ", app.input.as_str()),
        Mode::Input(InputAction::TextFilter) => (" Text filter ", app.input.as_str()),
        Mode::Normal => (" Keys ", "s=start  x=stop  n=note  ↑/k=up  ↓/j=down"),
    };
    f.render_widget(
        Paragraph::new(content).block(Block::default().borders(Borders::ALL).title(bar_title)),
        chunks[2],
    );

    // ── status bar ────────────────────────────────────────────────────────────
    f.render_widget(
        Paragraph::new(app.status.as_str()).style(Style::default().fg(Color::DarkGray)),
        chunks[3],
    );

    RenderOutput { max_scroll: auto_scroll }
}