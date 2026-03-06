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

    match tui::run(sessions, cli.query) {
        Ok(Some(action)) => {
            if let Err(e) =
                resume::tmux_resume(action.agent, &action.session_id, &action.directory)
            {
                eprintln!("Failed to open tmux window: {}", e);
                std::process::exit(1);
            }
        }
        Ok(None) => {}
        Err(e) => {
            eprintln!("TUI error: {}", e);
            std::process::exit(1);
        }
    }
}
