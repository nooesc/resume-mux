pub mod app;
mod ui;

use app::App;
use crate::adapters::Session;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
};
use ratatui::prelude::*;
use std::io;

pub fn run(sessions: Vec<Session>, initial_query: Option<String>, yolo: bool) -> io::Result<Option<(String, crate::adapters::Agent, std::path::PathBuf, bool)>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(sessions, initial_query, yolo);

    loop {
        terminal.draw(|frame| ui::render(frame, &app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Esc => {
                    app.should_quit = true;
                    break;
                }
                KeyCode::Enter => {
                    app.resume_selected(false);
                    break;
                }
                KeyCode::Char('q') if app.query.is_empty() => {
                    app.should_quit = true;
                    break;
                }
                KeyCode::Char('y') if app.query.is_empty() => {
                    app.resume_selected(true);
                    break;
                }
                KeyCode::Char('j') if app.query.is_empty() => app.move_selection(1),
                KeyCode::Char('k') if app.query.is_empty() => app.move_selection(-1),
                KeyCode::Char('J') => {
                    app.preview_scroll += 3;
                }
                KeyCode::Char('K') => {
                    app.preview_scroll = app.preview_scroll.saturating_sub(3);
                }
                KeyCode::Up | KeyCode::BackTab => app.move_selection(-1),
                KeyCode::Down | KeyCode::Tab => app.move_selection(1),
                KeyCode::Char(c) => app.type_char(c),
                KeyCode::Backspace => app.backspace(),
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    if app.should_quit {
        return Ok(None);
    }

    if let Some(ref action) = app.resume_action {
        let results = app.filtered_results();
        if let Some(result) = results.get(action.session_index) {
            return Ok(Some((
                result.session.id.clone(),
                result.session.agent.clone(),
                result.session.directory.clone(),
                action.yolo,
            )));
        }
    }

    Ok(None)
}
