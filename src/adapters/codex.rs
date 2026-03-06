use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{
    append_search_text, metadata_signature, scan_with_cache, Agent, Session, SEARCH_TEXT_LIMIT,
};

/// Scans the default ~/.codex/sessions directory for Codex sessions.
pub fn scan_sessions() -> Vec<Session> {
    let base = match dirs::home_dir() {
        Some(home) => home.join(".codex").join("sessions"),
        None => return Vec::new(),
    };
    scan_session_summaries_in(&base)
}

/// Scans a given base directory for Codex JSONL session files.
/// Sessions are nested as YYYY/MM/DD/*.jsonl under `base`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn scan_sessions_in(base: &Path) -> Vec<Session> {
    let mut sessions = Vec::new();
    collect_sessions_recursive(base, &mut sessions, parse_session_file);
    sessions
}

pub fn load_session_content(path: &Path) -> Option<String> {
    parse_session_file(path).map(|session| session.content)
}

fn scan_session_summaries_in(base: &Path) -> Vec<Session> {
    scan_with_cache(
        "codex-summaries.json",
        dedup_forked_paths(collect_session_paths(base)),
        parse_session_summary_file,
    )
}

/// Recursively traverse directories to find .jsonl files.
#[cfg_attr(not(test), allow(dead_code))]
fn collect_sessions_recursive(
    dir: &Path,
    sessions: &mut Vec<Session>,
    parse: fn(&Path) -> Option<Session>,
) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sessions_recursive(&path, sessions, parse);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            if let Some(session) = parse(&path) {
                sessions.push(session);
            }
        }
    }
}

/// Read the first few lines of a JSONL file to find `forked_from_id`.
/// Returns `Some(parent_id)` if this is a forked session, `None` if original.
fn read_forked_from_id(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    for line in reader.lines().take(3) {
        let line = line.ok()?;
        let entry: Value = match serde_json::from_str(line.trim()) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if entry.get("type").and_then(|t| t.as_str()) == Some("session_meta") {
            if let Some(payload) = entry.get("payload") {
                if let Some(forked_id) = payload.get("forked_from_id").and_then(|v| v.as_str()) {
                    return Some(forked_id.to_string());
                }
            }
        }
    }
    None
}

/// Deduplicate forked session files. For each group of files sharing the same
/// `forked_from_id`, keep only the most recently modified one. Also removes
/// the original parent file if a newer fork exists.
fn dedup_forked_paths(file_paths: Vec<PathBuf>) -> Vec<PathBuf> {
    // Map: root_session_id -> (best_path, best_mtime)
    let mut groups: HashMap<String, (PathBuf, u64)> = HashMap::new();
    let mut standalone: Vec<PathBuf> = Vec::new();

    for path in &file_paths {
        if let Some(forked_from) = read_forked_from_id(path) {
            let mtime = metadata_signature(path).map(|(_, m)| m).unwrap_or(0);
            let entry = groups.entry(forked_from).or_insert_with(|| (path.clone(), mtime));
            if mtime > entry.1 {
                *entry = (path.clone(), mtime);
            }
        } else {
            standalone.push(path.clone());
        }
    }

    // Filter out original parent files that have been superseded by forks
    let forked_roots: std::collections::HashSet<&str> =
        groups.keys().map(|s| s.as_str()).collect();

    let mut result: Vec<PathBuf> = standalone
        .into_iter()
        .filter(|path| {
            // Check if this standalone session's id matches a forked_from_id
            // If so, the fork supersedes it — skip the original
            let id = extract_session_id_from_path(path);
            match id {
                Some(ref id) if forked_roots.contains(id.as_str()) => false,
                _ => true,
            }
        })
        .collect();

    result.extend(groups.into_values().map(|(path, _)| path));
    result
}

/// Extract session ID from filename (UUID portion at end).
fn extract_session_id_from_path(path: &Path) -> Option<String> {
    let name = path.file_stem()?.to_str()?;
    let segments: Vec<&str> = name.split('-').collect();
    if segments.len() >= 5 {
        let uuid_parts = &segments[segments.len() - 5..];
        Some(uuid_parts.join("-"))
    } else {
        None
    }
}

fn collect_session_paths(base: &Path) -> Vec<PathBuf> {
    let mut file_paths = Vec::new();
    collect_session_paths_recursive(base, &mut file_paths);
    file_paths
}

fn collect_session_paths_recursive(dir: &Path, file_paths: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_session_paths_recursive(&path, file_paths);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            file_paths.push(path);
        }
    }
}

/// Extracts text content from a response_item's content field.
/// Content can be a plain string or an array of content blocks
/// with `type` "output_text" or "text" and a `text` field.
fn extract_response_text(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Array(parts) => {
            let mut texts = Vec::new();
            for part in parts {
                let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if part_type == "output_text" || part_type == "text" {
                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            texts.push(trimmed.to_string());
                        }
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

/// Parses a single Codex JSONL session file into a Session.
///
/// Codex format: event_msg entries don't have event_type in newer versions.
/// We use turn_context as a turn boundary marker — the first event_msg with
/// a message after each turn_context is the user's input. Subsequent ones
/// are assistant reasoning (which we skip since response_item captures that).
fn parse_session_file(path: &Path) -> Option<Session> {
    let file_content = fs::read_to_string(path).ok()?;
    let timestamp = fs::metadata(path).ok()?.modified().ok()?;

    let mut session_id: Option<String> = None;
    let mut directory: Option<PathBuf> = None;
    let mut user_prompts: Vec<String> = Vec::new();
    let mut content_parts: Vec<String> = Vec::new();
    let mut message_count: usize = 0;
    let mut awaiting_user_input = false;

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

        match entry_type {
            "session_meta" => {
                let payload = match entry.get("payload") {
                    Some(p) => p,
                    None => continue,
                };
                if session_id.is_none() {
                    session_id = payload.get("id").and_then(|v| v.as_str()).map(String::from);
                }
                if directory.is_none() {
                    directory = payload
                        .get("cwd")
                        .and_then(|v| v.as_str())
                        .map(PathBuf::from);
                }
            }
            "turn_context" => {
                // New turn — next event_msg with a message is the user's input
                awaiting_user_input = true;
            }
            "event_msg" => {
                let payload = match entry.get("payload") {
                    Some(p) => p,
                    None => continue,
                };

                // Check for explicit event_type (older Codex format)
                let event_type = payload.get("event_type").and_then(|v| v.as_str());

                if event_type == Some("user_message") || awaiting_user_input {
                    if let Some(msg) = payload.get("message").and_then(|v| v.as_str()) {
                        let trimmed = msg.trim();
                        if !trimmed.is_empty() {
                            user_prompts.push(trimmed.to_string());
                            content_parts.push(format!("You: {}", trimmed));
                            message_count += 1;
                            awaiting_user_input = false;
                        }
                    }
                }
                // Skip assistant reasoning event_msgs — response_item captures those
            }
            "response_item" => {
                let payload = match entry.get("payload") {
                    Some(p) => p,
                    None => continue,
                };
                let role = payload.get("role").and_then(|v| v.as_str());
                if role == Some("assistant") {
                    if let Some(content) = payload.get("content") {
                        if let Some(text) = extract_response_text(content) {
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

    let title: String = user_prompts[0].chars().take(100).collect();

    if session_id.is_none() {
        session_id = extract_session_id_from_path(path);
    }

    Some(Session::new(
        session_id.unwrap_or_default(),
        Agent::Codex,
        title,
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
    let mut awaiting_user_input = false;
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

        match entry_type {
            "session_meta" => {
                let payload = match entry.get("payload") {
                    Some(p) => p,
                    None => continue,
                };
                if session_id.is_none() {
                    session_id = payload.get("id").and_then(|v| v.as_str()).map(String::from);
                }
                if directory.is_none() {
                    directory = payload
                        .get("cwd")
                        .and_then(|v| v.as_str())
                        .map(PathBuf::from);
                }
            }
            "turn_context" => {
                awaiting_user_input = true;
            }
            "event_msg" => {
                let payload = match entry.get("payload") {
                    Some(p) => p,
                    None => continue,
                };
                let event_type = payload.get("event_type").and_then(|v| v.as_str());

                if event_type == Some("user_message") || awaiting_user_input {
                    if let Some(msg) = payload.get("message").and_then(|v| v.as_str()) {
                        let trimmed = msg.trim();
                        if !trimmed.is_empty() {
                            if title.is_none() {
                                title = Some(trimmed.chars().take(100).collect());
                            }
                            append_search_text(
                                &mut search_text,
                                &mut remaining_search_chars,
                                "You: ",
                                trimmed,
                            );
                            message_count += 1;
                            awaiting_user_input = false;
                        }
                    }
                }
            }
            "response_item" => {
                let payload = match entry.get("payload") {
                    Some(p) => p,
                    None => continue,
                };
                let role = payload.get("role").and_then(|v| v.as_str());
                if role == Some("assistant") {
                    if let Some(content) = payload.get("content") {
                        if let Some(text) = extract_response_text(content) {
                            append_search_text(
                                &mut search_text,
                                &mut remaining_search_chars,
                                "Assistant: ",
                                text.trim(),
                            );
                            message_count += 1;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if title.is_none() {
        return None;
    }

    if session_id.is_none() {
        session_id = extract_session_id_from_path(path);
    }

    Some(Session::from_summary(
        session_id.unwrap_or_default(),
        Agent::Codex,
        title.unwrap_or_default(),
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

    /// Helper to create a JSONL session file nested under YYYY/MM/DD/.
    fn make_codex_session_file(base: &Path, filename: &str, lines: &[&str]) -> PathBuf {
        let nested_dir = base.join("2025").join("06").join("15");
        fs::create_dir_all(&nested_dir).unwrap();
        let file_path = nested_dir.join(format!("{}.jsonl", filename));
        let content = lines.join("\n");
        fs::write(&file_path, content).unwrap();
        file_path
    }

    #[test]
    fn test_parse_codex_session() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        let lines = vec![
            // session_meta line
            r#"{"type":"session_meta","payload":{"id":"codex-sess-001","cwd":"/home/dev/myproject"}}"#,
            // turn_context line (not extracted, but should not break parsing)
            r#"{"type":"turn_context","payload":{"approval_policy":"never","sandbox_policy":{"mode":"danger-full-access"}}}"#,
            // event_msg with user_message
            r#"{"type":"event_msg","payload":{"event_type":"user_message","message":"Refactor the database layer to use connection pooling"}}"#,
            // event_msg with agent_reasoning (should not count as a message)
            r#"{"type":"event_msg","payload":{"event_type":"agent_reasoning","text":"I need to look at the database module first."}}"#,
            // response_item from assistant with array content
            r#"{"type":"response_item","payload":{"role":"assistant","content":[{"type":"output_text","text":"I'll refactor the database layer to use connection pooling. Let me start by examining the current code."}]}}"#,
            // response_item from user with plain string content
            r#"{"type":"response_item","payload":{"role":"user","content":"Now add error handling too"}}"#,
            // response_item from assistant with plain string content
            r#"{"type":"response_item","payload":{"role":"assistant","content":"Done! I've added comprehensive error handling with proper Result types."}}"#,
        ];

        make_codex_session_file(base, "codex-sess-001", &lines);

        let sessions = scan_sessions_in(base);
        assert_eq!(sessions.len(), 1, "Expected exactly one session");

        let session = &sessions[0];
        assert_eq!(session.id, "codex-sess-001");
        assert_eq!(session.agent, Agent::Codex);
        assert_eq!(
            session.title,
            "Refactor the database layer to use connection pooling"
        );
        assert_eq!(session.directory, PathBuf::from("/home/dev/myproject"));

        // 1 user event_msg + 2 assistant response_items = 3
        // (response_item role="user" is skipped — those are system context)
        assert_eq!(session.message_count, 3);

        // Verify content ordering and formatting
        assert!(session
            .content
            .contains("You: Refactor the database layer to use connection pooling"));
        assert!(session
            .content
            .contains("Assistant: I'll refactor the database layer to use connection pooling."));
        // response_item role="user" is no longer captured (system context)
        // Only event_msg user messages are captured
        assert!(session
            .content
            .contains("Assistant: Done! I've added comprehensive error handling"));

        // agent_reasoning should NOT appear in content
        assert!(
            !session
                .content
                .contains("I need to look at the database module first"),
            "agent_reasoning text should not be included in content"
        );
    }

    #[test]
    fn test_skips_empty_sessions() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        // A session file with only session_meta — no user prompts at all
        let lines = vec![
            r#"{"type":"session_meta","payload":{"id":"codex-empty-001","cwd":"/home/dev/empty"}}"#,
            r#"{"type":"turn_context","payload":{"approval_policy":"on-failure","sandbox_policy":{"mode":"sandbox"}}}"#,
        ];

        make_codex_session_file(base, "codex-empty-001", &lines);

        let sessions = scan_sessions_in(base);
        assert!(
            sessions.is_empty(),
            "Sessions with no user prompts should be skipped, but got {} sessions",
            sessions.len()
        );
    }
}
