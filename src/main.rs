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
        Ok(None) => {}
        Err(e) => {
            eprintln!("TUI error: {}", e);
            std::process::exit(1);
        }
    }
}
