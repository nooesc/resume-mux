use std::path::Path;

use crate::adapters::Agent;

pub fn resume_command(agent: Agent, session_id: &str, yolo: bool) -> Vec<String> {
    let mut cmd = vec![
        agent.label().to_string(),
        "--resume".to_string(),
        session_id.to_string(),
    ];

    if yolo {
        match agent {
            Agent::Claude => cmd.push("--dangerously-skip-permissions".to_string()),
            Agent::Codex => cmd.push("--full-auto".to_string()),
        }
    }

    cmd
}

pub fn exec_resume(agent: Agent, session_id: &str, directory: &Path, yolo: bool) -> std::io::Error {
    use std::os::unix::process::CommandExt;

    let parts = resume_command(agent, session_id, yolo);
    let mut command = std::process::Command::new(&parts[0]);
    command.args(&parts[1..]);
    command.current_dir(directory);

    command.exec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_resume_normal() {
        let result = resume_command(Agent::Claude, "abc-123", false);
        assert_eq!(result, vec!["claude", "--resume", "abc-123"]);
    }

    #[test]
    fn test_claude_resume_yolo() {
        let result = resume_command(Agent::Claude, "abc-123", true);
        assert_eq!(
            result,
            vec!["claude", "--resume", "abc-123", "--dangerously-skip-permissions"]
        );
    }

    #[test]
    fn test_codex_resume_normal() {
        let result = resume_command(Agent::Codex, "xyz-456", false);
        assert_eq!(result, vec!["codex", "--resume", "xyz-456"]);
    }

    #[test]
    fn test_codex_resume_yolo() {
        let result = resume_command(Agent::Codex, "xyz-456", true);
        assert_eq!(result, vec!["codex", "--resume", "xyz-456", "--full-auto"]);
    }
}
