use super::app::{App, Pane};
use crate::adapters::Agent;
use chrono::{DateTime, Local};
use ratatui::{prelude::*, widgets::*};
use std::sync::OnceLock;

fn home_dir() -> &'static std::path::PathBuf {
    static HOME: OnceLock<std::path::PathBuf> = OnceLock::new();
    HOME.get_or_init(|| dirs::home_dir().unwrap_or_default())
}

pub fn render(frame: &mut Frame, app: &mut App) {
    let total = app.sessions.len();
    let count = app.result_count();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // search bar
            Constraint::Min(5),    // main content
            Constraint::Length(1), // status bar
        ])
        .split(frame.area());

    render_search_bar(frame, &app.query, chunks[0]);

    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    render_session_list(frame, app, panes[0]);
    render_preview(frame, app, panes[1]);
    render_status_bar(frame, app, count, total, chunks[2]);

    if let Some(msg) = &app.popup_message {
        render_popup(frame, msg);
    }
}

fn render_search_bar(frame: &mut Frame, query: &str, area: Rect) {
    let input = Paragraph::new(format!("> {}_", query))
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title(" search "));
    frame.render_widget(input, area);
}

fn render_session_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let home = home_dir();
    let home_str = home.to_string_lossy();
    let inner_width = (area.width as usize).saturating_sub(2); // borders

    let items: Vec<ListItem> = app
        .filtered_indices
        .iter()
        .filter_map(|&index| app.sessions.get(index))
        .map(|session| {
            let agent_style = match session.agent {
                Agent::Claude => Style::default().fg(Color::Rgb(180, 160, 220)),
                Agent::Codex => Style::default().fg(Color::Rgb(160, 210, 180)),
            };

            let max_dir = (inner_width / 2).min(35);
            let dir_short = shorten_path(&session.directory, &home_str, max_dir);
            let time_ago = format_time_ago(session.timestamp);
            let right = format!("{:>3}  {:>4}", time_ago, session.message_count);

            // Line 1: agent  dir  ...right-aligned...  time_ago  N turns
            let left_len = 6 + 1 + dir_short.chars().count();
            let padding = inner_width.saturating_sub(left_len + right.chars().count());

            let header = Line::from(vec![
                Span::styled(format!("{:6}", session.agent.label()), agent_style),
                Span::styled(format!(" {}", dir_short), Style::default().fg(Color::Rgb(100, 140, 180))),
                Span::raw(" ".repeat(padding.max(1))),
                Span::styled(right, Style::default().fg(Color::DarkGray)),
            ]);

            ListItem::new(vec![header])
        })
        .collect();

    let border_style = if app.focused_pane == Pane::Sessions {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(" sessions ")
                .title_top(
                    Line::from(Span::styled(" turns ", Style::default().fg(Color::DarkGray)))
                        .alignment(Alignment::Right),
                ),
        )
        .highlight_style(Style::default().bg(Color::Rgb(40, 40, 50)));

    frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn render_preview(frame: &mut Frame, app: &App, area: Rect) {
    let content = if let Some(content) = app.selected_preview_content() {
        format_preview_lines(content, app.preview_scroll)
    } else if app.selected_session().is_some() {
        vec![Line::from(Span::styled(
            "Preview unavailable",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        vec![Line::from(Span::styled(
            "No session selected",
            Style::default().fg(Color::DarkGray),
        ))]
    };

    let border_style = if app.focused_pane == Pane::Preview {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let preview = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(" preview "),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(preview, area);
}

fn format_preview_lines(content: &str, scroll: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut prev_role: Option<&str> = None;

    for raw_line in content.lines().skip(scroll) {
        if let Some(text) = raw_line.strip_prefix("You: ") {
            // Add separator between different speakers
            if prev_role.is_some() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                " You",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                format!(" {}", text),
                Style::default().fg(Color::White),
            )));
            prev_role = Some("user");
        } else if let Some(text) = raw_line.strip_prefix("Assistant: ") {
            if prev_role.is_some() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                " Assistant",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                format!(" {}", text),
                Style::default().fg(Color::Rgb(190, 190, 190)),
            )));
            prev_role = Some("assistant");
        } else {
            // Continuation line — style based on current speaker
            let color = match prev_role {
                Some("user") => Color::White,
                Some("assistant") => Color::Rgb(190, 190, 190),
                _ => Color::DarkGray,
            };
            lines.push(Line::from(Span::styled(
                format!(" {}", raw_line),
                Style::default().fg(color),
            )));
        }
    }

    lines
}

fn render_status_bar(frame: &mut Frame, app: &App, count: usize, total: usize, area: Rect) {
    let mut spans = vec![
        Span::styled(" \u{2191}\u{2193}", Style::default().fg(Color::Yellow)),
        Span::styled(" navigate  ", Style::default().fg(Color::DarkGray)),
    ];

    match app.focused_pane {
        Pane::Sessions => {
            spans.extend([
                Span::styled("\u{2192}/tab", Style::default().fg(Color::Yellow)),
                Span::styled(" preview  ", Style::default().fg(Color::DarkGray)),
            ]);
        }
        Pane::Preview => {
            spans.extend([
                Span::styled("\u{2190}/esc", Style::default().fg(Color::Yellow)),
                Span::styled(" back  ", Style::default().fg(Color::DarkGray)),
            ]);
        }
    }

    spans.extend([
        Span::styled("enter", Style::default().fg(Color::Yellow)),
        Span::styled(" resume  ", Style::default().fg(Color::DarkGray)),
        Span::styled("esc", Style::default().fg(Color::Yellow)),
        Span::styled(" quit  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{}/{} sessions", count, total),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn format_time_ago(time: std::time::SystemTime) -> String {
    let dt: DateTime<Local> = time.into();
    let now = Local::now();
    let duration = now.signed_duration_since(dt);

    let secs = duration.num_seconds();
    if secs < 60 {
        return "now".to_string();
    }
    if secs < 3600 {
        return format!("{}m", secs / 60);
    }
    if secs < 86400 {
        return format!("{}h", secs / 3600);
    }
    if secs < 604800 {
        return format!("{}d", secs / 86400);
    }
    format!("{}w", secs / 604800)
}

fn shorten_path(path: &std::path::Path, home_str: &str, max_len: usize) -> String {
    let display = path.to_string_lossy();
    let short = if display.starts_with(home_str) {
        format!("~{}", &display[home_str.len()..])
    } else {
        display.to_string()
    };
    let char_count = short.chars().count();
    if char_count <= max_len {
        short
    } else {
        let tail: String = short.chars().skip(char_count - (max_len - 3)).collect();
        format!("...{}", tail)
    }
}

fn render_popup(frame: &mut Frame, message: &str) {
    let area = frame.area();
    let width = 40u16.min(area.width.saturating_sub(4));
    let height = 5u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup_area);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            message,
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "press enter to dismiss",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let popup = Paragraph::new(text)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        );

    frame.render_widget(popup, popup_area);
}
