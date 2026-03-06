use crate::adapters::Session;
use crate::search::{self, SearchResult};

pub struct App {
    pub sessions: Vec<Session>,
    pub query: String,
    pub selected: usize,
    pub yolo_default: bool,
    pub should_quit: bool,
    pub resume_action: Option<ResumeAction>,
    pub preview_scroll: usize,
}

pub struct ResumeAction {
    pub session_index: usize,
    pub yolo: bool,
}

impl App {
    pub fn new(sessions: Vec<Session>, initial_query: Option<String>, yolo: bool) -> Self {
        Self {
            sessions,
            query: initial_query.unwrap_or_default(),
            selected: 0,
            yolo_default: yolo,
            should_quit: false,
            resume_action: None,
            preview_scroll: 0,
        }
    }

    pub fn filtered_results(&self) -> Vec<SearchResult<'_>> {
        search::search(&self.query, &self.sessions)
    }

    pub fn move_selection(&mut self, delta: isize) {
        let count = self.filtered_results().len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        if delta < 0 {
            self.selected = self.selected.saturating_sub((-delta) as usize);
        } else {
            self.selected = (self.selected + delta as usize).min(count - 1);
        }
        self.preview_scroll = 0;
    }

    pub fn type_char(&mut self, c: char) {
        self.query.push(c);
        self.selected = 0;
        self.preview_scroll = 0;
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
        self.preview_scroll = 0;
    }

    pub fn resume_selected(&mut self, yolo: bool) {
        let results = self.filtered_results();
        if !results.is_empty() && self.selected < results.len() {
            self.resume_action = Some(ResumeAction {
                session_index: self.selected,
                yolo: yolo || self.yolo_default,
            });
        }
    }
}
