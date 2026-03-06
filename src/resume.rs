use std::path::Path;

use crate::adapters::Agent;

pub fn resume_command(agent: Agent, session_id: &str) -> Vec<String> {
    let mut cmd = vec![
        agent.label().to_string(),
        "--resume".to_string(),
        session_id.to_string(),
    ];

    match agent {
        Agent::Claude => cmd.push("--dangerously-skip-permissions".to_string()),
        Agent::Codex => {
            cmd.push("--sandbox".to_string());
            cmd.push("danger-full-access".to_string());
        }
    }

    cmd
}

pub fn tmux_resume(
    agent: Agent,
    session_id: &str,
    directory: &Path,
) -> std::io::Result<()> {
    let parts = resume_command(agent, session_id);
    let cmd_str = parts
        .iter()
        .map(|p| shell_escape(p))
        .collect::<Vec<_>>()
        .join(" ");

    let status = std::process::Command::new("tmux")
        .args(["new-window", "-c"])
        .arg(directory)
        .status()?;

    if !status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "tmux new-window failed",
        ));
    }

    let status = std::process::Command::new("tmux")
        .args(["send-keys", &cmd_str, "Enter"])
        .status()?;

    if !status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "tmux send-keys failed",
        ));
    }

    Ok(())
}

fn shell_escape(s: &str) -> String {
    if s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/') {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_resume() {
        let result = resume_command(Agent::Claude, "abc-123");
        assert_eq!(
            result,
            vec![
                "claude",
                "--resume",
                "abc-123",
                "--dangerously-skip-permissions"
            ]
        );
    }

    #[test]
    fn test_codex_resume() {
        let result = resume_command(Agent::Codex, "xyz-456");
        assert_eq!(
            result,
            vec!["codex", "--resume", "xyz-456", "--sandbox", "danger-full-access"]
        );
    }
}
