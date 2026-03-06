use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{Agent, Session};

/// Scans the default ~/.codex/sessions directory for Codex sessions.
pub fn scan_sessions() -> Vec<Session> {
    let base = match dirs::home_dir() {
        Some(home) => home.join(".codex").join("sessions"),
        None => return Vec::new(),
    };
    scan_sessions_in(&base)
}

/// Scans a given base directory for Codex JSONL session files.
/// Sessions are nested as YYYY/MM/DD/*.jsonl under `base`.
pub fn scan_sessions_in(base: &Path) -> Vec<Session> {
    let mut sessions = Vec::new();
    collect_sessions_recursive(base, &mut sessions);
    sessions
}

/// Recursively traverse directories to find .jsonl files.
fn collect_sessions_recursive(dir: &Path, sessions: &mut Vec<Session>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sessions_recursive(&path, sessions);
        } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            if let Some(session) = parse_session_file(&path) {
                sessions.push(session);
            }
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

        let payload = match entry.get("payload") {
            Some(p) => p,
            None => continue,
        };

        match entry_type {
            "session_meta" => {
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
            "event_msg" => {
                let event_type = payload.get("event_type").and_then(|v| v.as_str());
                match event_type {
                    Some("user_message") => {
                        if let Some(msg) = payload.get("message").and_then(|v| v.as_str()) {
                            let trimmed = msg.trim();
                            if !trimmed.is_empty() {
                                if title.is_none() {
                                    let mut t = trimmed.to_string();
                                    t.truncate(80);
                                    title = Some(t);
                                }
                                content_parts.push(format!("You: {}", trimmed));
                                message_count += 1;
                            }
                        }
                    }
                    _ => {}
                }
            }
            "response_item" => {
                let role = payload.get("role").and_then(|v| v.as_str());
                if let Some(content) = payload.get("content") {
                    if let Some(text) = extract_response_text(content) {
                        match role {
                            Some("user") => {
                                content_parts.push(format!("You: {}", text));
                                message_count += 1;
                            }
                            Some("assistant") => {
                                content_parts.push(format!("Assistant: {}", text));
                                message_count += 1;
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Skip sessions with no user prompts
    if title.is_none() || message_count == 0 {
        return None;
    }

    Some(Session {
        id: session_id.unwrap_or_default(),
        agent: Agent::Codex,
        title: title.unwrap_or_else(|| "Untitled session".to_string()),
        directory: directory.unwrap_or_default(),
        timestamp,
        content: content_parts.join("\n"),
        message_count,
    })
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

        // 1 event_msg user_message + 1 assistant response_item + 1 user response_item + 1 assistant response_item = 4
        assert_eq!(session.message_count, 4);

        // Verify content ordering and formatting
        assert!(session
            .content
            .contains("You: Refactor the database layer to use connection pooling"));
        assert!(session
            .content
            .contains("Assistant: I'll refactor the database layer to use connection pooling."));
        assert!(session.content.contains("You: Now add error handling too"));
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
