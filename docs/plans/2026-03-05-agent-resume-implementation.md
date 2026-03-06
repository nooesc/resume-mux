# agent-resume Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a minimal Rust CLI tool (`ar`) for searching and resuming Claude Code and Codex sessions with a custom fuzzy search and ratatui TUI.

**Architecture:** Adapters scan JSONL session files into memory on startup. Custom fuzzy scorer ranks results as user types. Ratatui renders a two-pane TUI (session list + conversation preview). Resume execs into the agent CLI.

**Tech Stack:** Rust, clap, ratatui, crossterm, serde/serde_json, rayon, chrono

---

### Task 1: Project Scaffold + Core Types

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/adapters/mod.rs`

**Step 1: Initialize cargo project**

Run: `cargo init --name agent-resume`
Expected: Creates `Cargo.toml` and `src/main.rs`

**Step 2: Add dependencies to Cargo.toml**

```toml
[package]
name = "agent-resume"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "ar"
path = "src/main.rs"

[dependencies]
clap = { version = "4", features = ["derive"] }
ratatui = "0.29"
crossterm = "0.28"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rayon = "1"
chrono = { version = "0.4", features = ["serde"] }
dirs = "6"
```

**Step 3: Create core types in `src/adapters/mod.rs`**

```rust
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
}

pub fn load_all_sessions() -> Vec<Session> {
    use rayon::prelude::*;

    let mut sessions: Vec<Session> = Vec::new();

    let claude_sessions = claude::scan_sessions();
    let codex_sessions = codex::scan_sessions();

    sessions.extend(claude_sessions);
    sessions.extend(codex_sessions);

    sessions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    sessions
}
```

**Step 4: Create stub adapter files**

Create `src/adapters/claude.rs`:
```rust
use super::Session;

pub fn scan_sessions() -> Vec<Session> {
    Vec::new()
}
```

Create `src/adapters/codex.rs` (identical stub).

**Step 5: Create minimal main.rs**

```rust
mod adapters;

fn main() {
    let sessions = adapters::load_all_sessions();
    println!("Found {} sessions", sessions.len());
}
```

**Step 6: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully with 0 sessions found

**Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/
git commit -m "scaffold: project structure with core types and stub adapters"
```

---

### Task 2: Claude Code Adapter

**Files:**
- Modify: `src/adapters/claude.rs`

**Step 1: Write tests for Claude adapter**

Create `src/adapters/claude.rs` with tests at bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn make_session_file(dir: &std::path::Path, id: &str, lines: &[&str]) {
        let path = dir.join(format!("{}.jsonl", id));
        let mut f = std::fs::File::create(path).unwrap();
        for line in lines {
            writeln!(f, "{}", line).unwrap();
        }
    }

    #[test]
    fn test_parse_user_and_assistant_messages() {
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("project1");
        std::fs::create_dir_all(&project_dir).unwrap();

        make_session_file(&project_dir, "sess-1", &[
            r#"{"type":"user","message":{"role":"user","content":"Fix the auth bug"},"cwd":"/home/user/myapp","sessionId":"sess-1"}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"I'll look at the auth flow."}]},"sessionId":"sess-1"}"#,
        ]);

        let sessions = scan_sessions_in(dir.path());
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].title, "Fix the auth bug");
        assert_eq!(sessions[0].id, "sess-1");
        assert!(sessions[0].content.contains("Fix the auth bug"));
        assert!(sessions[0].content.contains("I'll look at the auth flow."));
        assert_eq!(sessions[0].message_count, 2);
    }

    #[test]
    fn test_skips_agent_files() {
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("project1");
        std::fs::create_dir_all(&project_dir).unwrap();

        make_session_file(&project_dir, "agent-subprocess", &[
            r#"{"type":"user","message":{"role":"user","content":"hello"},"cwd":"/tmp","sessionId":"agent-sub"}"#,
        ]);

        let sessions = scan_sessions_in(dir.path());
        assert_eq!(sessions.len(), 0);
    }

    #[test]
    fn test_skips_meta_and_tool_messages() {
        let dir = TempDir::new().unwrap();
        let project_dir = dir.path().join("project1");
        std::fs::create_dir_all(&project_dir).unwrap();

        make_session_file(&project_dir, "sess-2", &[
            r#"{"type":"system","subtype":"local_command","content":"<command-name>/usage</command-name>"}"#,
            r#"{"type":"user","message":{"role":"user","content":"Refactor the database layer"},"cwd":"/home/user/core","sessionId":"sess-2"}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"tool1","name":"Read","input":{}}]},"sessionId":"sess-2"}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Done refactoring."}]},"sessionId":"sess-2"}"#,
        ]);

        let sessions = scan_sessions_in(dir.path());
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].title, "Refactor the database layer");
        // tool_use messages should not count as message_count
        assert_eq!(sessions[0].message_count, 2); // 1 user + 1 text assistant
    }
}
```

Add `tempfile` as a dev dependency in Cargo.toml:
```toml
[dev-dependencies]
tempfile = "3"
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -- claude`
Expected: Fails — `scan_sessions_in` doesn't exist yet

**Step 3: Implement Claude adapter**

```rust
use super::{Agent, Session};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Scan sessions from default Claude directory (~/.claude/projects)
pub fn scan_sessions() -> Vec<Session> {
    let claude_dir = dirs::home_dir()
        .map(|h| h.join(".claude").join("projects"))
        .unwrap_or_default();

    if !claude_dir.exists() {
        return Vec::new();
    }

    scan_sessions_in(&claude_dir)
}

/// Scan sessions from a given base directory (for testing)
pub fn scan_sessions_in(base: &Path) -> Vec<Session> {
    let mut sessions = Vec::new();

    let project_dirs = match fs::read_dir(base) {
        Ok(entries) => entries,
        Err(_) => return sessions,
    };

    for project_entry in project_dirs.flatten() {
        if !project_entry.path().is_dir() {
            continue;
        }

        let files = match fs::read_dir(project_entry.path()) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for file_entry in files.flatten() {
            let path = file_entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }

            // Skip agent subprocess files
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("agent-"))
                .unwrap_or(false)
            {
                continue;
            }

            if let Some(session) = parse_session_file(&path) {
                sessions.push(session);
            }
        }
    }

    sessions
}

fn parse_session_file(path: &Path) -> Option<Session> {
    let raw = fs::read_to_string(path).ok()?;
    let mtime = fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);

    let mut session_id = String::new();
    let mut directory = PathBuf::new();
    let mut title = String::new();
    let mut content_parts: Vec<String> = Vec::new();
    let mut message_count: usize = 0;

    for line in raw.lines() {
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match msg_type {
            "user" => {
                if session_id.is_empty() {
                    if let Some(sid) = v.get("sessionId").and_then(|s| s.as_str()) {
                        session_id = sid.to_string();
                    }
                }

                if directory.as_os_str().is_empty() {
                    if let Some(cwd) = v.get("cwd").and_then(|c| c.as_str()) {
                        directory = PathBuf::from(cwd);
                    }
                }

                if let Some(msg) = v.get("message") {
                    if let Some(content) = extract_user_content(msg) {
                        if title.is_empty() && content.len() > 10 {
                            title = truncate(&content, 100);
                        }
                        content_parts.push(format!("You: {}", content));
                        message_count += 1;
                    }
                }
            }
            "assistant" => {
                if let Some(msg) = v.get("message") {
                    if let Some(text) = extract_assistant_text(msg) {
                        content_parts.push(format!("Assistant: {}", text));
                        message_count += 1;
                    }
                }
            }
            _ => {}
        }
    }

    if title.is_empty() || message_count == 0 {
        return None;
    }

    Some(Session {
        id: session_id,
        agent: Agent::Claude,
        title,
        directory,
        timestamp: mtime,
        content: content_parts.join("\n\n"),
        message_count,
    })
}

fn extract_user_content(msg: &Value) -> Option<String> {
    let content = msg.get("content")?;

    // Simple string content
    if let Some(s) = content.as_str() {
        // Skip command messages
        if s.starts_with("<command") || s.starts_with("<local-command") {
            return None;
        }
        return Some(s.to_string());
    }

    // Array content — look for text parts
    if let Some(arr) = content.as_array() {
        let texts: Vec<String> = arr
            .iter()
            .filter_map(|part| {
                if part.get("type")?.as_str()? == "text" {
                    part.get("text").and_then(|t| t.as_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect();
        if !texts.is_empty() {
            return Some(texts.join("\n"));
        }
    }

    None
}

fn extract_assistant_text(msg: &Value) -> Option<String> {
    let content = msg.get("content")?;

    // Simple string
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }

    // Array — collect text parts, skip tool_use
    if let Some(arr) = content.as_array() {
        let texts: Vec<String> = arr
            .iter()
            .filter_map(|part| {
                if part.get("type")?.as_str()? == "text" {
                    part.get("text").and_then(|t| t.as_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect();
        if !texts.is_empty() {
            return Some(texts.join("\n"));
        }
    }

    None
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...", truncated)
    }
}
```

**Step 4: Run tests**

Run: `cargo test -- claude`
Expected: All 3 tests pass

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: implement Claude Code session adapter with JSONL parsing"
```

---

### Task 3: Codex Adapter

**Files:**
- Modify: `src/adapters/codex.rs`

**Step 1: Write tests for Codex adapter**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn make_codex_file(dir: &std::path::Path, subpath: &str, lines: &[&str]) {
        let path = dir.join(subpath);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = std::fs::File::create(path).unwrap();
        for line in lines {
            writeln!(f, "{}", line).unwrap();
        }
    }

    #[test]
    fn test_parse_codex_session() {
        let dir = TempDir::new().unwrap();

        make_codex_file(dir.path(), "2026/03/05/session1.jsonl", &[
            r#"{"type":"session_meta","payload":{"id":"codex-123","cwd":"/home/user/api"}}"#,
            r#"{"type":"event_msg","payload":{"event_type":"user_message","message":"Add API tests"}}"#,
            r#"{"type":"response_item","payload":{"role":"assistant","content":[{"type":"output_text","text":"I'll add tests for the API."}]}}"#,
        ]);

        let sessions = scan_sessions_in(dir.path());
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "codex-123");
        assert_eq!(sessions[0].title, "Add API tests");
        assert!(sessions[0].content.contains("Add API tests"));
        assert!(sessions[0].content.contains("I'll add tests for the API."));
    }

    #[test]
    fn test_skips_empty_sessions() {
        let dir = TempDir::new().unwrap();

        make_codex_file(dir.path(), "2026/03/05/empty.jsonl", &[
            r#"{"type":"session_meta","payload":{"id":"empty-1","cwd":"/tmp"}}"#,
        ]);

        let sessions = scan_sessions_in(dir.path());
        assert_eq!(sessions.len(), 0);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -- codex`
Expected: Fails — `scan_sessions_in` doesn't exist

**Step 3: Implement Codex adapter**

```rust
use super::{Agent, Session};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub fn scan_sessions() -> Vec<Session> {
    let codex_dir = dirs::home_dir()
        .map(|h| h.join(".codex").join("sessions"))
        .unwrap_or_default();

    if !codex_dir.exists() {
        return Vec::new();
    }

    scan_sessions_in(&codex_dir)
}

pub fn scan_sessions_in(base: &Path) -> Vec<Session> {
    let mut sessions = Vec::new();
    collect_jsonl_files(base, &mut sessions);
    sessions
}

fn collect_jsonl_files(dir: &Path, sessions: &mut Vec<Session>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl_files(&path, sessions);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            if let Some(session) = parse_session_file(&path) {
                sessions.push(session);
            }
        }
    }
}

fn parse_session_file(path: &Path) -> Option<Session> {
    let raw = fs::read_to_string(path).ok()?;
    let mtime = fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);

    let mut session_id = String::new();
    let mut directory = PathBuf::new();
    let mut user_prompts: Vec<String> = Vec::new();
    let mut content_parts: Vec<String> = Vec::new();
    let mut message_count: usize = 0;
    let mut yolo = false;

    for line in raw.lines() {
        let v: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let payload = v.get("payload");

        match msg_type {
            "session_meta" => {
                if let Some(p) = payload {
                    if let Some(id) = p.get("id").and_then(|i| i.as_str()) {
                        session_id = id.to_string();
                    }
                    if let Some(cwd) = p.get("cwd").and_then(|c| c.as_str()) {
                        directory = PathBuf::from(cwd);
                    }
                }
            }
            "turn_context" => {
                if let Some(p) = payload {
                    let approval = p.get("approval_policy").and_then(|a| a.as_str());
                    let sandbox_mode = p
                        .pointer("/sandbox_policy/mode")
                        .and_then(|m| m.as_str());

                    if approval == Some("never")
                        || sandbox_mode == Some("danger-full-access")
                    {
                        yolo = true;
                    }
                }
            }
            "event_msg" => {
                if let Some(p) = payload {
                    let event_type = p.get("event_type").and_then(|e| e.as_str());
                    if event_type == Some("user_message") {
                        if let Some(msg) = p.get("message").and_then(|m| m.as_str()) {
                            user_prompts.push(msg.to_string());
                            content_parts.push(format!("You: {}", msg));
                            message_count += 1;
                        }
                    } else if event_type == Some("agent_reasoning") {
                        if let Some(text) = p.get("text").and_then(|t| t.as_str()) {
                            content_parts.push(format!("Assistant: {}", text));
                        }
                    }
                }
            }
            "response_item" => {
                if let Some(p) = payload {
                    let role = p.get("role").and_then(|r| r.as_str()).unwrap_or("");

                    if role == "user" {
                        if let Some(text) = extract_text_content(p) {
                            content_parts.push(format!("You: {}", text));
                            message_count += 1;
                        }
                    } else if role == "assistant" {
                        if let Some(text) = extract_text_content(p) {
                            content_parts.push(format!("Assistant: {}", text));
                            message_count += 1;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if user_prompts.is_empty() {
        return None;
    }

    let title = truncate(&user_prompts[0], 80);

    Some(Session {
        id: session_id,
        agent: Agent::Codex,
        title,
        directory,
        timestamp: mtime,
        content: content_parts.join("\n\n"),
        message_count,
    })
}

fn extract_text_content(payload: &Value) -> Option<String> {
    let content = payload.get("content")?;

    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }

    if let Some(arr) = content.as_array() {
        let texts: Vec<String> = arr
            .iter()
            .filter_map(|part| {
                let part_type = part.get("type")?.as_str()?;
                if part_type == "output_text" || part_type == "text" {
                    part.get("text").and_then(|t| t.as_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect();
        if !texts.is_empty() {
            return Some(texts.join("\n"));
        }
    }

    None
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...", truncated)
    }
}
```

**Step 4: Run tests**

Run: `cargo test -- codex`
Expected: All 2 tests pass

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: implement Codex session adapter with JSONL parsing"
```

---

### Task 4: Custom Fuzzy Search

**Files:**
- Create: `src/search.rs`

**Step 1: Write tests for fuzzy search**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_session(title: &str, dir: &str, content: &str) -> Session {
        Session {
            id: "test".to_string(),
            agent: Agent::Claude,
            title: title.to_string(),
            directory: PathBuf::from(dir),
            timestamp: SystemTime::now(),
            content: content.to_string(),
            message_count: 1,
        }
    }

    #[test]
    fn test_exact_substring_ranks_highest() {
        let sessions = vec![
            make_session("fix auth bug", "/app", "some content"),
            make_session("refactor database", "/app", "auth related stuff"),
        ];
        let results = search("auth", &sessions);
        assert_eq!(results[0].session.title, "fix auth bug"); // title match > content match
    }

    #[test]
    fn test_empty_query_returns_all() {
        let sessions = vec![
            make_session("first", "/a", ""),
            make_session("second", "/b", ""),
        ];
        let results = search("", &sessions);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_fuzzy_character_matching() {
        let sessions = vec![
            make_session("fix authentication bug", "/app", ""),
            make_session("add new feature", "/app", ""),
        ];
        let results = search("fxath", &sessions);
        // "fix authentication" should match fuzzy chars f-x-a-t-h
        assert!(results[0].score > 0.0);
        assert_eq!(results[0].session.title, "fix authentication bug");
    }

    #[test]
    fn test_directory_matching() {
        let sessions = vec![
            make_session("some task", "/home/user/backend", ""),
            make_session("other task", "/home/user/frontend", ""),
        ];
        let results = search("backend", &sessions);
        assert_eq!(results[0].session.title, "some task");
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -- search`
Expected: Fails — module doesn't exist

**Step 3: Implement fuzzy search**

```rust
use crate::adapters::{Agent, Session};
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug)]
pub struct SearchResult<'a> {
    pub session: &'a Session,
    pub score: f64,
}

pub fn search<'a>(query: &str, sessions: &'a [Session]) -> Vec<SearchResult<'a>> {
    let query_lower = query.to_lowercase();
    let tokens: Vec<&str> = query_lower.split_whitespace().collect();

    let mut results: Vec<SearchResult<'a>> = sessions
        .iter()
        .map(|session| {
            let score = if tokens.is_empty() {
                // Empty query: score by recency only
                recency_score(session)
            } else {
                score_session(&tokens, &query_lower, session)
            };
            SearchResult { session, score }
        })
        .filter(|r| tokens.is_empty() || r.score > 0.0)
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results
}

fn score_session(tokens: &[&str], full_query: &str, session: &Session) -> f64 {
    let title_lower = session.title.to_lowercase();
    let dir_lower = session.directory.to_string_lossy().to_lowercase();
    let content_lower = session.content.to_lowercase();

    let title_score = score_field(tokens, full_query, &title_lower) * 3.0;
    let dir_score = score_field(tokens, full_query, &dir_lower) * 2.0;
    let content_score = score_field(tokens, full_query, &content_lower) * 1.0;

    let field_score = title_score + dir_score + content_score;

    if field_score > 0.0 {
        field_score + recency_score(session) * 0.1
    } else {
        0.0
    }
}

fn score_field(tokens: &[&str], full_query: &str, field: &str) -> f64 {
    let mut score = 0.0;

    // Exact full query substring match (highest)
    if !full_query.is_empty() && field.contains(full_query) {
        score += 100.0;
    }

    for token in tokens {
        // Exact token substring match
        if field.contains(token) {
            score += 50.0;
            // Bonus for word boundary match
            if field.split_whitespace().any(|w| w == *token) {
                score += 20.0;
            }
        } else {
            // Fuzzy: consecutive character matching (fzf-style)
            let fuzzy = fuzzy_match_score(token, field);
            if fuzzy > 0.0 {
                score += fuzzy;
            } else {
                // Levenshtein distance <= 1 for any word in the field
                let lev_match = field.split_whitespace().any(|word| {
                    levenshtein(token, word) <= 1
                });
                if lev_match {
                    score += 10.0;
                }
            }
        }
    }

    score
}

/// fzf-style sequential character matching.
/// Returns a score based on how well the pattern chars appear in order in the text.
/// Bonus for contiguous runs of matched characters.
fn fuzzy_match_score(pattern: &str, text: &str) -> f64 {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    if pattern_chars.is_empty() {
        return 0.0;
    }

    let mut p_idx = 0;
    let mut matched = 0;
    let mut contiguous_bonus = 0.0;
    let mut last_match_pos: Option<usize> = None;

    for (t_idx, &tc) in text_chars.iter().enumerate() {
        if p_idx < pattern_chars.len() && tc == pattern_chars[p_idx] {
            matched += 1;
            if let Some(last) = last_match_pos {
                if t_idx == last + 1 {
                    contiguous_bonus += 5.0;
                }
            }
            last_match_pos = Some(t_idx);
            p_idx += 1;
        }
    }

    if matched == pattern_chars.len() {
        let base = (matched as f64 / text_chars.len().max(1) as f64) * 30.0;
        base + contiguous_bonus
    } else {
        0.0
    }
}

/// Simple Levenshtein distance
fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 { return n; }
    if n == 0 { return m; }

    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

fn recency_score(session: &Session) -> f64 {
    let age = session
        .timestamp
        .elapsed()
        .map(|d| d.as_secs_f64())
        .unwrap_or(f64::MAX);

    // Decay: sessions from now score ~1.0, a week ago ~0.5
    1.0 / (1.0 + age / 604800.0)
}
```

**Step 4: Wire into main.rs**

Add `mod search;` to main.rs.

**Step 5: Run tests**

Run: `cargo test -- search`
Expected: All 4 tests pass

**Step 6: Commit**

```bash
git add -A
git commit -m "feat: implement custom fuzzy search with fzf-style matching and levenshtein"
```

---

### Task 5: Resume Logic

**Files:**
- Create: `src/resume.rs`

**Step 1: Write tests for resume command generation**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_resume_normal() {
        let cmd = resume_command(Agent::Claude, "abc-123", false);
        assert_eq!(cmd, vec!["claude", "--resume", "abc-123"]);
    }

    #[test]
    fn test_claude_resume_yolo() {
        let cmd = resume_command(Agent::Claude, "abc-123", true);
        assert_eq!(cmd, vec!["claude", "--resume", "abc-123", "--dangerously-skip-permissions"]);
    }

    #[test]
    fn test_codex_resume_normal() {
        let cmd = resume_command(Agent::Codex, "xyz-456", false);
        assert_eq!(cmd, vec!["codex", "--resume", "xyz-456"]);
    }

    #[test]
    fn test_codex_resume_yolo() {
        let cmd = resume_command(Agent::Codex, "xyz-456", true);
        assert_eq!(cmd, vec!["codex", "--resume", "xyz-456", "--full-auto"]);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -- resume`

**Step 3: Implement resume module**

```rust
use crate::adapters::Agent;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

pub fn resume_command(agent: Agent, session_id: &str, yolo: bool) -> Vec<String> {
    match agent {
        Agent::Claude => {
            let mut cmd = vec![
                "claude".to_string(),
                "--resume".to_string(),
                session_id.to_string(),
            ];
            if yolo {
                cmd.push("--dangerously-skip-permissions".to_string());
            }
            cmd
        }
        Agent::Codex => {
            let mut cmd = vec![
                "codex".to_string(),
                "--resume".to_string(),
                session_id.to_string(),
            ];
            if yolo {
                cmd.push("--full-auto".to_string());
            }
            cmd
        }
    }
}

/// Replace current process with the agent CLI.
/// This function does not return on success.
pub fn exec_resume(agent: Agent, session_id: &str, directory: &Path, yolo: bool) -> std::io::Error {
    let cmd = resume_command(agent, session_id, yolo);

    let err = Command::new(&cmd[0])
        .args(&cmd[1..])
        .current_dir(directory)
        .exec();

    // exec() only returns on error
    err
}
```

**Step 4: Run tests**

Run: `cargo test -- resume`
Expected: All 4 tests pass

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: implement resume logic with yolo mode for Claude and Codex"
```

---

### Task 6: CLI Argument Parsing

**Files:**
- Modify: `src/main.rs`

**Step 1: Implement CLI with clap**

```rust
mod adapters;
mod resume;
mod search;

use clap::Parser;

#[derive(Parser)]
#[command(name = "ar", about = "Search and resume coding agent sessions")]
pub struct Cli {
    /// Pre-fill the search query
    pub query: Option<String>,

    /// Filter by agent (claude, codex)
    #[arg(short, long)]
    pub agent: Option<String>,

    /// Default to yolo mode when resuming
    #[arg(long)]
    pub yolo: bool,
}

fn main() {
    let cli = Cli::parse();

    let mut sessions = adapters::load_all_sessions();

    // Filter by agent if specified
    if let Some(ref agent_filter) = cli.agent {
        let filter_lower = agent_filter.to_lowercase();
        sessions.retain(|s| s.agent.label() == filter_lower);
    }

    if sessions.is_empty() {
        eprintln!("No sessions found.");
        std::process::exit(0);
    }

    // TUI will be wired in next task
    println!("Found {} sessions", sessions.len());
    for s in sessions.iter().take(10) {
        println!("  [{}] {} — {}", s.agent, s.title, s.directory.display());
    }
}
```

**Step 2: Verify it compiles and runs**

Run: `cargo run`
Expected: Lists sessions found (or "No sessions found")

Run: `cargo run -- --help`
Expected: Shows usage help

**Step 3: Commit**

```bash
git add -A
git commit -m "feat: add clap CLI argument parsing"
```

---

### Task 7: TUI — App State and Event Loop

**Files:**
- Create: `src/tui/mod.rs`
- Create: `src/tui/app.rs`

**Step 1: Create TUI app state**

`src/tui/app.rs`:

```rust
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
    pub session_index: usize, // index into filtered results
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
```

`src/tui/mod.rs`:

```rust
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

    if let Some(action) = app.resume_action {
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
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles (UI rendering not yet implemented, will be next task)

**Step 3: Commit**

```bash
git add -A
git commit -m "feat: add TUI app state and event loop"
```

---

### Task 8: TUI — Rendering

**Files:**
- Create: `src/tui/ui.rs`

**Step 1: Implement the two-pane layout rendering**

```rust
use super::app::App;
use crate::adapters::Agent;
use chrono::{DateTime, Local};
use ratatui::{
    prelude::*,
    widgets::*,
};

pub fn render(frame: &mut Frame, app: &App) {
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

fn render_main_panes(frame: &mut Frame, app: &App, area: Rect) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    render_session_list(frame, app, panes[0]);
    render_preview(frame, app, panes[1]);
}

fn render_session_list(frame: &mut Frame, app: &App, area: Rect) {
    let results = app.filtered_results();

    let items: Vec<ListItem> = results
        .iter()
        .enumerate()
        .map(|(i, r)| {
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
                    truncate(&r.session.title, (area.width as usize).saturating_sub(25)),
                    Style::default().fg(Color::White),
                ),
            ]);

            let meta = Line::from(vec![
                Span::raw("       "),
                Span::styled(
                    format!("{}  {}", dir_short, time_ago),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);

            let style = if i == app.selected {
                Style::default().bg(Color::Rgb(40, 40, 50))
            } else {
                Style::default()
            };

            ListItem::new(vec![line, meta]).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" sessions "))
        .highlight_style(Style::default().bg(Color::Rgb(40, 40, 50)));

    frame.render_widget(list, area);
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
        Span::styled(" ↑↓", Style::default().fg(Color::Yellow)),
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

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{}...", t)
    }
}
```

**Step 2: Wire TUI into main.rs**

```rust
mod adapters;
mod resume;
mod search;
mod tui;

use clap::Parser;

#[derive(Parser)]
#[command(name = "ar", about = "Search and resume coding agent sessions")]
pub struct Cli {
    /// Pre-fill the search query
    pub query: Option<String>,

    /// Filter by agent (claude, codex)
    #[arg(short, long)]
    pub agent: Option<String>,

    /// Default to yolo mode when resuming
    #[arg(long)]
    pub yolo: bool,
}

fn main() {
    let cli = Cli::parse();

    let mut sessions = adapters::load_all_sessions();

    if let Some(ref agent_filter) = cli.agent {
        let filter_lower = agent_filter.to_lowercase();
        sessions.retain(|s| s.agent.label() == filter_lower);
    }

    if sessions.is_empty() {
        eprintln!("No sessions found.");
        std::process::exit(0);
    }

    match tui::run(sessions, cli.query, cli.yolo) {
        Ok(Some((id, agent, dir, yolo))) => {
            let err = resume::exec_resume(agent, &id, &dir, yolo);
            eprintln!("Failed to exec: {}", err);
            std::process::exit(1);
        }
        Ok(None) => {} // User quit
        Err(e) => {
            eprintln!("TUI error: {}", e);
            std::process::exit(1);
        }
    }
}
```

**Step 3: Build and manual test**

Run: `cargo build && cargo run --bin ar`
Expected: TUI launches, shows sessions, search works, Esc quits

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: implement TUI rendering with two-pane layout"
```

---

### Task 9: Integration Test + Polish

**Files:**
- Modify: `src/tui/mod.rs` (j/k navigation)
- Modify: `src/tui/ui.rs` (scroll preview)

**Step 1: Add j/k keybindings and preview scrolling**

In `src/tui/mod.rs`, add to the key match:
```rust
KeyCode::Char('j') if app.query.is_empty() => app.move_selection(1),
KeyCode::Char('k') if app.query.is_empty() => app.move_selection(-1),
KeyCode::Char('J') => {
    app.preview_scroll += 3;
}
KeyCode::Char('K') => {
    app.preview_scroll = app.preview_scroll.saturating_sub(3);
}
```

**Step 2: End-to-end manual test**

Run: `cargo run --bin ar`
Test the following:
- [ ] Sessions appear sorted by recency
- [ ] Typing filters results live
- [ ] j/k navigates when search is empty
- [ ] Arrow keys always navigate
- [ ] Preview updates on selection change
- [ ] Enter resumes session
- [ ] Esc quits
- [ ] `--help` shows usage

**Step 3: Commit**

```bash
git add -A
git commit -m "feat: add j/k navigation and preview scrolling"
```

---

### Task 10: Install Script + README (optional)

**Step 1: Add a basic .gitignore**

```
/target
```

**Step 2: Test release build**

Run: `cargo build --release`
Run: `ls -lh target/release/ar`
Expected: Single binary, reasonable size

**Step 3: Install locally**

Run: `cargo install --path .`
Expected: `ar` available in PATH

**Step 4: Commit**

```bash
git add -A
git commit -m "chore: add .gitignore and verify release build"
```
