use crate::adapters::{self, Agent, Session};
use crate::search;
use ratatui::widgets::ListState;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq)]
pub enum Pane {
    Sessions,
    Preview,
}

pub struct App {
    pub sessions: Vec<Session>,
    pub filtered_indices: Vec<usize>,
    pub query: String,
    pub should_quit: bool,
    pub resume_action: Option<ResumeAction>,
    pub preview_scroll: usize,
    pub preview_cache: HashMap<PathBuf, String>,
    pub list_state: ListState,
    pub focused_pane: Pane,
    pub popup_message: Option<String>,
}

pub struct ResumeAction {
    pub session_id: String,
    pub agent: Agent,
    pub directory: PathBuf,
    pub tmux: bool,
}

impl App {
    pub fn new(sessions: Vec<Session>, initial_query: Option<String>) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        let mut app = Self {
            sessions,
            filtered_indices: Vec::new(),
            query: initial_query.unwrap_or_default(),
            should_quit: false,
            resume_action: None,
            preview_scroll: 0,
            preview_cache: HashMap::new(),
            list_state,
            focused_pane: Pane::Sessions,
            popup_message: None,
        };
        app.refresh_results();
        app
    }

    pub fn selected(&self) -> usize {
        self.list_state.selected().unwrap_or(0)
    }

    pub fn result_count(&self) -> usize {
        self.filtered_indices.len()
    }

    pub fn selected_session(&self) -> Option<&Session> {
        let index = *self.filtered_indices.get(self.selected())?;
        self.sessions.get(index)
    }

    pub fn selected_preview_content(&self) -> Option<&str> {
        let session = self.selected_session()?;
        self.preview_cache
            .get(&session.source_path)
            .map(String::as_str)
    }

    pub fn move_selection(&mut self, delta: isize) {
        let count = self.result_count();
        if count == 0 {
            self.list_state.select(None);
            return;
        }
        let current = self.selected();
        let new = if delta < 0 {
            current.saturating_sub((-delta) as usize)
        } else {
            (current + delta as usize).min(count - 1)
        };
        self.list_state.select(Some(new));
        self.preview_scroll = 0;
        self.ensure_preview_loaded();
    }

    pub fn type_char(&mut self, c: char) {
        self.query.push(c);
        self.refresh_results();
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.refresh_results();
    }

    fn refresh_results(&mut self) {
        self.filtered_indices = search::search_indices(&self.query, &self.sessions);
        if self.filtered_indices.is_empty() {
            self.list_state.select(None);
        } else {
            self.list_state.select(Some(0));
            self.ensure_preview_loaded();
        }
        self.preview_scroll = 0;
    }

    fn ensure_preview_loaded(&mut self) {
        let Some((agent, source_path)) = self
            .selected_session()
            .map(|session| (session.agent.clone(), session.source_path.clone()))
        else {
            return;
        };

        if self.preview_cache.contains_key(&source_path) {
            return;
        }

        if let Some(content) = adapters::load_session_content(&agent, &source_path) {
            self.preview_cache.insert(source_path, content);
        }
    }
}
