pub mod app;
mod ui;

use crate::adapters::Session;
use app::{App, Pane, ResumeAction};
use crossterm::{
    event::{
        self, Event, KeyCode, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;

pub fn run(
    sessions: Vec<Session>,
    initial_query: Option<String>,
) -> io::Result<Option<ResumeAction>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let _ = execute!(
        stdout,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(sessions, initial_query);

    loop {
        terminal.draw(|frame| ui::render(frame, &mut app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            // Popup dismissal
            if app.popup_message.is_some() {
                if key.code == KeyCode::Enter || key.code == KeyCode::Esc {
                    app.popup_message = None;
                }
                continue;
            }

            // Global keys (work in any pane)
            match key.code {
                KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                    if std::env::var("TMUX").is_err() {
                        app.popup_message =
                            Some("Not inside a tmux session".to_string());
                        continue;
                    }
                    if let Some((session_id, agent, directory)) = app
                        .selected_session()
                        .map(|r| (r.id.clone(), r.agent.clone(), r.directory.clone()))
                    {
                        app.resume_action = Some(ResumeAction {
                            session_id,
                            agent,
                            directory,
                            tmux: true,
                        });
                    }
                    break;
                }
                KeyCode::Enter => {
                    if let Some((session_id, agent, directory)) = app
                        .selected_session()
                        .map(|r| (r.id.clone(), r.agent.clone(), r.directory.clone()))
                    {
                        app.resume_action = Some(ResumeAction {
                            session_id,
                            agent,
                            directory,
                            tmux: false,
                        });
                    }
                    break;
                }
                KeyCode::Char('q') if app.query.is_empty() => {
                    app.should_quit = true;
                    break;
                }
                KeyCode::Char(c) => {
                    app.focused_pane = Pane::Sessions;
                    app.type_char(c);
                    continue;
                }
                KeyCode::Backspace => {
                    app.backspace();
                    continue;
                }
                _ => {}
            }

            // Pane-specific keys
            match app.focused_pane {
                Pane::Sessions => match key.code {
                    KeyCode::Esc => {
                        app.should_quit = true;
                        break;
                    }
                    KeyCode::Right | KeyCode::Tab => {
                        app.focused_pane = Pane::Preview;
                    }
                    KeyCode::Up | KeyCode::BackTab => app.move_selection(-1),
                    KeyCode::Down => app.move_selection(1),
                    KeyCode::Char('j') if app.query.is_empty() => app.move_selection(1),
                    KeyCode::Char('k') if app.query.is_empty() => app.move_selection(-1),
                    _ => {}
                },
                Pane::Preview => match key.code {
                    KeyCode::Esc | KeyCode::Left => {
                        app.focused_pane = Pane::Sessions;
                    }
                    KeyCode::Up => {
                        app.preview_scroll = app.preview_scroll.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        app.preview_scroll += 1;
                    }
                    KeyCode::Tab | KeyCode::BackTab | KeyCode::Right => {
                        app.focused_pane = Pane::Sessions;
                    }
                    _ => {}
                },
            }
        }
    }

    disable_raw_mode()?;
    let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    if app.should_quit {
        return Ok(None);
    }

    Ok(app.resume_action)
}
