pub mod claude;
pub mod codex;

use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub fn label(&self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
        }
    }
}

impl std::fmt::Display for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub agent: Agent,
    pub title: String,
    pub directory: PathBuf,
    pub timestamp: SystemTime,
    pub content: String,
    pub message_count: usize,
    // Pre-computed lowercase for search performance
    pub title_lower: String,
    pub dir_lower: String,
    pub content_lower: String,
}

impl Session {
    pub fn new(
        id: String,
        agent: Agent,
        title: String,
        directory: PathBuf,
        timestamp: SystemTime,
        content: String,
        message_count: usize,
    ) -> Self {
        let title_lower = title.to_lowercase();
        let dir_lower = directory.to_string_lossy().to_lowercase();
        // Only index first 2000 chars of content for search — keeps search fast
        let content_lower = content.chars().take(2000).collect::<String>().to_lowercase();
        Self {
            id,
            agent,
            title,
            directory,
            timestamp,
            content,
            message_count,
            title_lower,
            dir_lower,
            content_lower,
        }
    }
}

pub fn load_all_sessions() -> Vec<Session> {
    let mut sessions: Vec<Session> = Vec::new();

    let claude_sessions = claude::scan_sessions();
    let codex_sessions = codex::scan_sessions();

    sessions.extend(claude_sessions);
    sessions.extend(codex_sessions);

    sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    sessions
}
