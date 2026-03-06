use super::app::App;
use crate::adapters::Agent;
use chrono::{DateTime, Local};
use ratatui::{
    prelude::*,
    widgets::*,
};

pub fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // search bar
            Constraint::Min(5),    // main content
            Constraint::Length(1), // status bar
        ])
        .split(frame.area());

    render_search_bar(frame, app, chunks[0]);
    render_main_panes(frame, app, chunks[1]);
    render_status_bar(frame, app, chunks[2]);
}

fn render_search_bar(frame: &mut Frame, app: &App, area: Rect) {
    let input = Paragraph::new(format!("> {}_", app.query))
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title(" search "));
    frame.render_widget(input, area);
}

fn render_main_panes(frame: &mut Frame, app: &mut App, area: Rect) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    render_session_list(frame, app, panes[0]);
    render_preview(frame, app, panes[1]);
}

fn render_session_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let results = app.filtered_results();

    let items: Vec<ListItem> = results
        .iter()
        .map(|r| {
            let agent_style = match r.session.agent {
                Agent::Claude => Style::default().fg(Color::Rgb(204, 120, 50)),
                Agent::Codex => Style::default().fg(Color::Rgb(100, 200, 100)),
            };

            let time_ago = format_time_ago(r.session.timestamp);
            let dir_short = shorten_path(&r.session.directory);

            let line = Line::from(vec![
                Span::styled(
                    format!("{:6}", r.session.agent.label()),
                    agent_style,
                ),
                Span::raw(" "),
                Span::styled(
                    truncate_str(&r.session.title, (area.width as usize).saturating_sub(25)),
                    Style::default().fg(Color::White),
                ),
            ]);

            let meta = Line::from(vec![
                Span::raw("       "),
                Span::styled(
                    format!("{}  {}  {}msg", dir_short, time_ago, r.session.message_count),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);

            ListItem::new(vec![line, meta])
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" sessions "))
        .highlight_style(Style::default().bg(Color::Rgb(40, 40, 50)));

    frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn render_preview(frame: &mut Frame, app: &App, area: Rect) {
    let results = app.filtered_results();
    let content = if let Some(r) = results.get(app.selected) {
        let lines: Vec<Line> = r.session.content
            .lines()
            .skip(app.preview_scroll)
            .map(|line| {
                if line.starts_with("You:") {
                    Line::from(Span::styled(line, Style::default().fg(Color::Cyan)))
                } else if line.starts_with("Assistant:") {
                    Line::from(Span::styled(line, Style::default().fg(Color::White)))
                } else {
                    Line::from(Span::styled(line, Style::default().fg(Color::DarkGray)))
                }
            })
            .collect();
        lines
    } else {
        vec![Line::from(Span::styled("No session selected", Style::default().fg(Color::DarkGray)))]
    };

    let preview = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title(" preview "))
        .wrap(Wrap { trim: false });

    frame.render_widget(preview, area);
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let results = app.filtered_results();
    let count = results.len();
    let total = app.sessions.len();

    let status = Line::from(vec![
        Span::styled(" \u{2191}\u{2193}", Style::default().fg(Color::Yellow)),
        Span::styled(" navigate  ", Style::default().fg(Color::DarkGray)),
        Span::styled("enter", Style::default().fg(Color::Yellow)),
        Span::styled(" resume  ", Style::default().fg(Color::DarkGray)),
        Span::styled("y", Style::default().fg(Color::Yellow)),
        Span::styled(" yolo  ", Style::default().fg(Color::DarkGray)),
        Span::styled("esc", Style::default().fg(Color::Yellow)),
        Span::styled(" quit  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}/{} sessions", count, total),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    frame.render_widget(Paragraph::new(status), area);
}

fn format_time_ago(time: std::time::SystemTime) -> String {
    let dt: DateTime<Local> = time.into();
    let now = Local::now();
    let duration = now.signed_duration_since(dt);

    let secs = duration.num_seconds();
    if secs < 60 { return "just now".to_string(); }
    if secs < 3600 { return format!("{}m ago", secs / 60); }
    if secs < 86400 { return format!("{}h ago", secs / 3600); }
    if secs < 604800 { return format!("{}d ago", secs / 86400); }
    format!("{}w ago", secs / 604800)
}

fn shorten_path(path: &std::path::Path) -> String {
    let home = dirs::home_dir().unwrap_or_default();
    let display = path.to_string_lossy();
    let home_str = home.to_string_lossy();
    if display.starts_with(home_str.as_ref()) {
        format!("~{}", &display[home_str.len()..])
    } else {
        display.to_string()
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{}...", t)
    }
}
