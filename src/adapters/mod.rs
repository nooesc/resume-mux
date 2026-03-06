pub mod claude;
pub mod codex;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SESSION_CACHE_VERSION: u32 = 3;
pub(super) const SEARCH_TEXT_LIMIT: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub source_path: PathBuf,
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
        source_path: PathBuf,
        content: String,
        message_count: usize,
    ) -> Self {
        let title_lower = title.to_lowercase();
        let dir_lower = directory.to_string_lossy().to_lowercase();
        let content_lower = content
            .chars()
            .take(SEARCH_TEXT_LIMIT)
            .collect::<String>()
            .to_lowercase();
        Self {
            id,
            agent,
            title,
            directory,
            timestamp,
            source_path,
            content,
            message_count,
            title_lower,
            dir_lower,
            content_lower,
        }
    }

    pub fn from_summary(
        id: String,
        agent: Agent,
        title: String,
        directory: PathBuf,
        timestamp: SystemTime,
        source_path: PathBuf,
        search_text: String,
        message_count: usize,
    ) -> Self {
        let title_lower = title.to_lowercase();
        let dir_lower = directory.to_string_lossy().to_lowercase();
        let content_lower = search_text.to_lowercase();
        Self {
            id,
            agent,
            title,
            directory,
            timestamp,
            source_path,
            content: String::new(),
            message_count,
            title_lower,
            dir_lower,
            content_lower,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCacheEntry {
    pub id: String,
    pub agent: Agent,
    pub title: String,
    pub directory: PathBuf,
    pub source_path: PathBuf,
    pub modified_secs: u64,
    pub file_size: u64,
    pub message_count: usize,
    pub content_lower: String,
}

impl SessionCacheEntry {
    pub fn from_session(session: &Session, file_size: u64, modified_secs: u64) -> Self {
        Self {
            id: session.id.clone(),
            agent: session.agent.clone(),
            title: session.title.clone(),
            directory: session.directory.clone(),
            source_path: session.source_path.clone(),
            modified_secs,
            file_size,
            message_count: session.message_count,
            content_lower: session.content_lower.clone(),
        }
    }

    pub fn to_session(&self) -> Session {
        Session {
            id: self.id.clone(),
            agent: self.agent.clone(),
            title: self.title.clone(),
            directory: self.directory.clone(),
            timestamp: UNIX_EPOCH + Duration::from_secs(self.modified_secs),
            source_path: self.source_path.clone(),
            content: String::new(),
            message_count: self.message_count,
            title_lower: self.title.to_lowercase(),
            dir_lower: self.directory.to_string_lossy().to_lowercase(),
            content_lower: self.content_lower.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionCacheFile {
    version: u32,
    entries: Vec<SessionCacheEntry>,
}

pub fn load_all_sessions() -> Vec<Session> {
    // Scan both adapters in parallel
    let (claude_sessions, codex_sessions) =
        rayon::join(|| claude::scan_sessions(), || codex::scan_sessions());

    let mut sessions = claude_sessions;
    sessions.extend(codex_sessions);
    sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    sessions
}

pub fn load_session_content(agent: &Agent, path: &Path) -> Option<String> {
    match agent {
        Agent::Claude => claude::load_session_content(path),
        Agent::Codex => codex::load_session_content(path),
    }
}

pub fn cache_path(name: &str) -> Option<PathBuf> {
    let base = dirs::cache_dir().or_else(|| dirs::home_dir().map(|home| home.join(".cache")))?;
    Some(base.join("resume-mux").join(name))
}

pub fn load_session_cache(path: &Path) -> HashMap<PathBuf, SessionCacheEntry> {
    let file_content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return HashMap::new(),
    };

    let cache: SessionCacheFile = match serde_json::from_str::<SessionCacheFile>(&file_content) {
        Ok(cache) if cache.version == SESSION_CACHE_VERSION => cache,
        _ => return HashMap::new(),
    };

    cache
        .entries
        .into_iter()
        .map(|entry| (entry.source_path.clone(), entry))
        .collect()
}

pub fn save_session_cache(path: &Path, entries: Vec<SessionCacheEntry>) {
    let Some(parent) = path.parent() else {
        return;
    };

    if fs::create_dir_all(parent).is_err() {
        return;
    }

    let cache = SessionCacheFile {
        version: SESSION_CACHE_VERSION,
        entries,
    };

    let Ok(serialized) = serde_json::to_string(&cache) else {
        return;
    };

    let _ = fs::write(path, serialized);
}

pub(super) fn scan_with_cache(
    cache_name: &str,
    file_paths: Vec<PathBuf>,
    parse: fn(&Path) -> Option<Session>,
) -> Vec<Session> {
    let Some(cache_path) = cache_path(cache_name) else {
        return file_paths
            .into_iter()
            .filter_map(|path| parse(&path))
            .collect();
    };

    let cache = load_session_cache(&cache_path);
    let mut next_cache = Vec::with_capacity(file_paths.len());
    let mut sessions = Vec::with_capacity(file_paths.len());

    for file_path in file_paths {
        let Some((file_size, modified_secs)) = metadata_signature(&file_path) else {
            continue;
        };

        if let Some(cached) = cache.get(&file_path) {
            if cached.file_size == file_size && cached.modified_secs == modified_secs {
                next_cache.push(cached.clone());
                sessions.push(cached.to_session());
                continue;
            }
        }

        if let Some(session) = parse(&file_path) {
            next_cache.push(SessionCacheEntry::from_session(
                &session,
                file_size,
                modified_secs,
            ));
            sessions.push(session);
        }
    }

    save_session_cache(&cache_path, next_cache);
    sessions
}

pub(super) fn append_search_text(
    buffer: &mut String,
    remaining_chars: &mut usize,
    prefix: &str,
    text: &str,
) {
    if *remaining_chars == 0 {
        return;
    }

    if !buffer.is_empty() {
        append_limited(buffer, remaining_chars, "\n");
    }

    append_limited(buffer, remaining_chars, prefix);
    append_limited(buffer, remaining_chars, text);
}

fn append_limited(buffer: &mut String, remaining_chars: &mut usize, text: &str) {
    if *remaining_chars == 0 {
        return;
    }

    let chunk: String = text.chars().take(*remaining_chars).collect();
    *remaining_chars = (*remaining_chars).saturating_sub(chunk.chars().count());
    buffer.push_str(&chunk);
}

pub fn metadata_signature(path: &Path) -> Option<(u64, u64)> {
    let metadata = fs::metadata(path).ok()?;
    let modified_secs = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some((metadata.len(), modified_secs))
}
