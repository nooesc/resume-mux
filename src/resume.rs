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

pub fn exec_resume(agent: Agent, session_id: &str, directory: &Path) -> std::io::Error {
    use std::os::unix::process::CommandExt;

    let parts = resume_command(agent, session_id);
    let mut command = std::process::Command::new(&parts[0]);
    command.args(&parts[1..]);
    command.current_dir(directory);

    command.exec()
}

pub fn tmux_resume(
    agent: Agent,
    session_id: &str,
    directory: &Path,
) -> std::io::Result<()> {
    let parts = resume_command(agent, session_id);

    let status = std::process::Command::new("tmux")
        .arg("new-window")
        .arg("-c")
        .arg(directory)
        .arg("--")
        .args(&parts)
        .status()?;

    if !status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "tmux new-window failed",
        ));
    }

    Ok(())
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
