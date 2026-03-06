use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{append_search_text, scan_with_cache, Agent, Session, SEARCH_TEXT_LIMIT};

/// Scans the default ~/.claude/projects directory for Claude Code sessions.
pub fn scan_sessions() -> Vec<Session> {
    let base = match dirs::home_dir() {
        Some(home) => home.join(".claude").join("projects"),
        None => return Vec::new(),
    };
    scan_session_summaries_in(&base)
}

/// Scans a given base directory for Claude Code JSONL session files.
/// Each subdirectory under `base` represents a project, containing `*.jsonl` files.
#[cfg_attr(not(test), allow(dead_code))]
pub fn scan_sessions_in(base: &Path) -> Vec<Session> {
    collect_sessions(base, parse_session_file)
}

pub fn load_session_content(path: &Path) -> Option<String> {
    parse_session_file(path).map(|session| session.content)
}

fn scan_session_summaries_in(base: &Path) -> Vec<Session> {
    scan_with_cache(
        "claude-summaries.json",
        collect_session_paths(base),
        parse_session_summary_file,
    )
}

#[cfg_attr(not(test), allow(dead_code))]
fn collect_sessions(base: &Path, parse: fn(&Path) -> Option<Session>) -> Vec<Session> {
    let mut sessions = Vec::new();
    for file_path in collect_session_paths(base) {
        if let Some(session) = parse(&file_path) {
            sessions.push(session);
        }
    }
    sessions
}

fn collect_session_paths(base: &Path) -> Vec<PathBuf> {
    let mut file_paths = Vec::new();
    let project_dirs = match fs::read_dir(base) {
        Ok(entries) => entries,
        Err(_) => return file_paths,
    };

    for project_entry in project_dirs.flatten() {
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }

        let files = match fs::read_dir(&project_path) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for file_entry in files.flatten() {
            let file_path = file_entry.path();

            if file_path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }

            // Skip agent subprocess files
            if let Some(name) = file_path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("agent-") {
                    continue;
                }
            }

            file_paths.push(file_path);
        }
    }

    file_paths
}

/// Extracts text content from user message content (string or array of content blocks).
/// Returns None if the content should be skipped (tool_result, command messages, isMeta, etc.).
fn extract_user_text(message: &Value) -> Option<String> {
    let content = message.get("content")?;

    match content {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.starts_with("<command") || trimmed.starts_with("<local-command") {
                return None;
            }
            Some(s.clone())
        }
        Value::Array(parts) => {
            let mut texts = Vec::new();
            for part in parts {
                let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match part_type {
                    "text" => {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            let trimmed = text.trim();
                            if trimmed.starts_with("<command")
                                || trimmed.starts_with("<local-command")
                            {
                                continue;
                            }
                            texts.push(text.to_string());
                        }
                    }
                    // Skip tool_result and other non-text types
                    _ => continue,
                }
            }
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n"))
            }
        }
        _ => None,
    }
}

/// Extracts text content from assistant message content (array of content blocks).
/// Only extracts "type": "text" parts, skipping tool_use, thinking, etc.
fn extract_assistant_text(message: &Value) -> Option<String> {
    let content = message.get("content")?;

    match content {
        Value::String(s) => Some(s.clone()),
        Value::Array(parts) => {
            let mut texts = Vec::new();
            for part in parts {
                let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if part_type == "text" {
                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                        texts.push(text.to_string());
                    }
                }
            }
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n"))
            }
        }
        _ => None,
    }
}

/// Parses a single JSONL session file into a Session.
fn parse_session_file(path: &Path) -> Option<Session> {
    let file_content = fs::read_to_string(path).ok()?;
    let timestamp = fs::metadata(path).ok()?.modified().ok()?;

    let mut session_id: Option<String> = None;
    let mut directory: Option<PathBuf> = None;
    let mut title: Option<String> = None;
    let mut content_parts: Vec<String> = Vec::new();
    let mut message_count: usize = 0;

    for line in file_content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let entry: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let entry_type = match entry.get("type").and_then(|t| t.as_str()) {
            Some(t) => t,
            None => continue,
        };

        // Skip non-conversation types
        if entry_type != "user" && entry_type != "assistant" {
            continue;
        }

        // Skip isMeta messages
        if entry
            .get("isMeta")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }

        let message = match entry.get("message") {
            Some(m) => m,
            None => continue,
        };

        match entry_type {
            "user" => {
                // Extract session metadata from first user message
                if session_id.is_none() {
                    session_id = entry
                        .get("sessionId")
                        .and_then(|s| s.as_str())
                        .map(String::from);
                    directory = entry.get("cwd").and_then(|s| s.as_str()).map(PathBuf::from);
                }

                if let Some(text) = extract_user_text(message) {
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        // Set title from first substantial user message (>10 chars)
                        if title.is_none() && text.len() > 10 {
                            let t: String = text.chars().take(100).collect();
                            title = Some(t);
                        }

                        content_parts.push(format!("You: {}", text));
                        message_count += 1;
                    }
                }
            }
            "assistant" => {
                if let Some(text) = extract_assistant_text(message) {
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        content_parts.push(format!("Assistant: {}", text));
                        message_count += 1;
                    }
                }
            }
            _ => {}
        }
    }

    // Skip sessions with no real content
    if message_count == 0 || content_parts.is_empty() {
        return None;
    }

    Some(Session::new(
        session_id.unwrap_or_default(),
        Agent::Claude,
        title.unwrap_or_else(|| "Untitled session".to_string()),
        directory.unwrap_or_default(),
        timestamp,
        path.to_path_buf(),
        content_parts.join("\n"),
        message_count,
    ))
}

fn parse_session_summary_file(path: &Path) -> Option<Session> {
    let file = File::open(path).ok()?;
    let timestamp = fs::metadata(path).ok()?.modified().ok()?;
    let reader = BufReader::new(file);

    let mut session_id: Option<String> = None;
    let mut directory: Option<PathBuf> = None;
    let mut title: Option<String> = None;
    let mut message_count = 0usize;
    let mut search_text = String::new();
    let mut remaining_search_chars = SEARCH_TEXT_LIMIT;

    for line in reader.lines() {
        let line = line.ok()?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let entry: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let entry_type = match entry.get("type").and_then(|t| t.as_str()) {
            Some(t) => t,
            None => continue,
        };

        if entry_type != "user" && entry_type != "assistant" {
            continue;
        }

        if entry
            .get("isMeta")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }

        let message = match entry.get("message") {
            Some(m) => m,
            None => continue,
        };

        match entry_type {
            "user" => {
                if session_id.is_none() {
                    session_id = entry
                        .get("sessionId")
                        .and_then(|s| s.as_str())
                        .map(String::from);
                    directory = entry.get("cwd").and_then(|s| s.as_str()).map(PathBuf::from);
                }

                if let Some(text) = extract_user_text(message) {
                    let text = text.trim();
                    if !text.is_empty() {
                        if title.is_none() && text.len() > 10 {
                            title = Some(text.chars().take(100).collect());
                        }
                        append_search_text(
                            &mut search_text,
                            &mut remaining_search_chars,
                            "You: ",
                            text,
                        );
                        message_count += 1;
                    }
                }
            }
            "assistant" => {
                if let Some(text) = extract_assistant_text(message) {
                    let text = text.trim();
                    if !text.is_empty() {
                        append_search_text(
                            &mut search_text,
                            &mut remaining_search_chars,
                            "Assistant: ",
                            text,
                        );
                        message_count += 1;
                    }
                }
            }
            _ => {}
        }
    }

    if message_count == 0 {
        return None;
    }

    Some(Session::from_summary(
        session_id.unwrap_or_default(),
        Agent::Claude,
        title.unwrap_or_else(|| "Untitled session".to_string()),
        directory.unwrap_or_default(),
        timestamp,
        path.to_path_buf(),
        search_text,
        message_count,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper to create a JSONL session file inside a project subdirectory.
    /// `dir` is the base directory (e.g., TempDir path).
    /// Creates `dir/project/filename.jsonl` with the given lines joined by newlines.
    fn make_session_file(dir: &Path, filename: &str, lines: &[&str]) -> PathBuf {
        let project_dir = dir.join("project");
        fs::create_dir_all(&project_dir).unwrap();
        let file_path = project_dir.join(format!("{}.jsonl", filename));
        let content = lines.join("\n");
        fs::write(&file_path, content).unwrap();
        file_path
    }

    #[test]
    fn test_parse_user_and_assistant_messages() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        let session_id = "abc-123-def";
        let lines: Vec<String> = vec![
            // A user message with plain string content
            format!(
                r#"{{"type":"user","sessionId":"{}","cwd":"/home/user/project","message":{{"role":"user","content":"Hello, can you help me refactor this code?"}}}}"#,
                session_id
            ),
            // An assistant message with text content in array form
            format!(
                r#"{{"type":"assistant","sessionId":"{}","message":{{"role":"assistant","content":[{{"type":"text","text":"Sure! I'd be happy to help you refactor. What code would you like to work on?"}}]}}}}"#,
                session_id
            ),
            // A second user message
            format!(
                r#"{{"type":"user","sessionId":"{}","cwd":"/home/user/project","message":{{"role":"user","content":"The main.rs file needs cleanup"}}}}"#,
                session_id
            ),
        ];

        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        make_session_file(base, session_id, &line_refs);

        let sessions = scan_sessions_in(base);
        assert_eq!(sessions.len(), 1);

        let session = &sessions[0];
        assert_eq!(session.id, session_id);
        assert_eq!(session.agent, Agent::Claude);
        assert_eq!(session.title, "Hello, can you help me refactor this code?");
        assert_eq!(session.directory, PathBuf::from("/home/user/project"));
        // 2 user messages + 1 assistant text response = 3
        assert_eq!(session.message_count, 3);

        assert!(session
            .content
            .contains("You: Hello, can you help me refactor this code?"));
        assert!(session
            .content
            .contains("Assistant: Sure! I'd be happy to help you refactor."));
        assert!(session
            .content
            .contains("You: The main.rs file needs cleanup"));
    }

    #[test]
    fn test_skips_agent_files() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();
        let project_dir = base.join("project");
        fs::create_dir_all(&project_dir).unwrap();

        let agent_file = project_dir.join("agent-subprocess-123.jsonl");
        let content = r#"{"type":"user","sessionId":"agent-sub","cwd":"/tmp","message":{"role":"user","content":"This is an agent subprocess message that should be ignored"}}"#;
        fs::write(&agent_file, content).unwrap();

        let sessions = scan_sessions_in(base);
        assert!(
            sessions.is_empty(),
            "Agent files should be skipped, but got {} sessions",
            sessions.len()
        );
    }

    #[test]
    fn test_skips_meta_and_tool_messages() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        let session_id = "meta-test-456";
        let lines = vec![
            // Normal user message
            format!(
                r#"{{"type":"user","sessionId":"{}","cwd":"/tmp/proj","message":{{"role":"user","content":"Please fix the build errors"}}}}"#,
                session_id
            ),
            // System/progress message — should be completely skipped
            format!(
                r#"{{"type":"progress","sessionId":"{}","data":{{"type":"hook_progress"}}}}"#,
                session_id
            ),
            // isMeta user message — should be skipped
            format!(
                r#"{{"type":"user","sessionId":"{}","cwd":"/tmp/proj","isMeta":true,"message":{{"role":"user","content":[{{"type":"text","text":"Skill instructions that are metadata"}}]}}}}"#,
                session_id
            ),
            // Assistant message with only tool_use — should NOT count toward message_count
            format!(
                r#"{{"type":"assistant","sessionId":"{}","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"toolu_123","name":"Read","input":{{"path":"/tmp/file"}}}}]}}}}"#,
                session_id
            ),
            // User message with tool_result content — should be skipped (no text parts)
            format!(
                r#"{{"type":"user","sessionId":"{}","cwd":"/tmp/proj","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"toolu_123","content":"file contents here"}}]}}}}"#,
                session_id
            ),
            // Assistant message with text — this SHOULD count
            format!(
                r#"{{"type":"assistant","sessionId":"{}","message":{{"role":"assistant","content":[{{"type":"text","text":"I found the issue. The import is missing."}}]}}}}"#,
                session_id
            ),
            // User message starting with <command — should be skipped
            format!(
                r#"{{"type":"user","sessionId":"{}","cwd":"/tmp/proj","message":{{"role":"user","content":"<command>cargo build</command>"}}}}"#,
                session_id
            ),
        ];

        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        make_session_file(base, session_id, &line_refs);

        let sessions = scan_sessions_in(base);
        assert_eq!(sessions.len(), 1);

        let session = &sessions[0];
        // Only 1 real user message + 1 assistant text = 2
        // (progress: skipped by type filter, isMeta: skipped, tool_use-only assistant: skipped,
        //  tool_result user: skipped, <command user: skipped)
        assert_eq!(session.message_count, 2);
        assert_eq!(session.title, "Please fix the build errors");

        // Verify content doesn't include skipped messages
        assert!(session.content.contains("You: Please fix the build errors"));
        assert!(session.content.contains("Assistant: I found the issue."));
        assert!(!session.content.contains("Skill instructions"));
        assert!(!session.content.contains("cargo build"));
        assert!(!session.content.contains("file contents here"));
    }
}
