# agent-resume (ar) — Design Doc

Rust CLI tool for searching and resuming Claude Code and Codex sessions. Minimal, fast, single binary.

## Architecture

```
agent-resume (binary: ar)
├── main.rs          — CLI entry, arg parsing (clap)
├── adapters/
│   ├── mod.rs       — Session struct + common types
│   ├── claude.rs    — Parse ~/.claude/projects/*/*.jsonl
│   └── codex.rs     — Parse ~/.codex/sessions/**/*.jsonl
├── search.rs        — Custom fuzzy scorer (no deps)
├── tui/
│   ├── mod.rs       — App state, event loop
│   ├── ui.rs        — Ratatui layout/rendering
│   └── preview.rs   — Conversation preview pane
└── resume.rs        — exec into agent CLI (with yolo flags)
```

### Data Flow

1. Startup: adapters scan session files in parallel (rayon), parse into `Vec<Session>`
2. User types: fuzzy scorer ranks sessions, TUI renders sorted results
3. User selects: `os::unix::process::exec()` replaces process with agent CLI

### Core Types

```rust
enum Agent { Claude, Codex }

struct Session {
    id: String,
    agent: Agent,
    title: String,         // First user message, truncated
    directory: PathBuf,
    timestamp: SystemTime,
    content: String,       // Full conversation text
    message_count: usize,
}
```

### Dependencies

- `clap` — CLI args
- `ratatui` + `crossterm` — TUI
- `serde` + `serde_json` — JSONL parsing
- `rayon` — parallel file scanning
- `chrono` — timestamp display

No search library, no async runtime.

## Fuzzy Search (Custom)

All in-memory, no persistent index. Re-parses on every launch (fast enough in Rust for <1000 sessions).

### Algorithm

1. Tokenize query into lowercase words
2. Score each session against three fields:
   - `title` — 3x weight
   - `directory` — 2x weight
   - `content` — 1x weight
3. Scoring per field:
   - Exact substring match: high score
   - Consecutive character match (fzf-style): medium score, bonus for contiguous runs
   - Levenshtein distance <= 1 per token: low score (typo tolerance)
4. Tiebreaker: recency (newer sessions rank higher)
5. Debounce: ~50ms on keystroke before re-scoring

## TUI Layout

```
┌─────────────────────────────────────────────────┐
│ > search query_                                 │
├──────────────────────────┬──────────────────────┤
│ Sessions (ranked)        │ Preview              │
│                          │                      │
│ * claude  fix auth bug   │ You: Can you fix the │
│   ~/dev/myapp   2h ago   │ auth bug in login?   │
│                          │                      │
│   codex   add api tests  │ Claude: I'll look at │
│   ~/dev/api     1d ago   │ the authentication   │
│                          │ flow...              │
│   claude  refactor db    │                      │
│   ~/dev/core    3d ago   │                      │
│                          │                      │
├──────────────────────────┴──────────────────────┤
│ up/down navigate  enter resume  y yolo  q quit  │
└─────────────────────────────────────────────────┘
```

### Key Bindings

- Type to search (live results)
- `up`/`down` or `j`/`k` — navigate
- `Enter` — resume session
- `y` — resume with yolo mode
- `q` / `Esc` — quit

### Styling

- Dim metadata (directory, time ago)
- Highlight matched characters in results
- Agent name colored per-agent
- No icons/images — terminal-native

## Resume Commands

### Normal

- Claude: `claude --resume <id>`
- Codex: `codex --resume <id>`

### Yolo

- Claude: `claude --resume <id> --dangerously-skip-permissions`
- Codex: `codex --resume <id> --full-auto`

Resume works via `os::unix::process::exec()` — replaces the ar process with the agent CLI, inheriting the session's working directory.

## CLI Interface

```bash
ar                    # Open TUI with all sessions
ar "api error"        # Pre-filled search query
ar -a claude          # Filter to claude only
ar -a codex           # Filter to codex only
ar --yolo             # Default to yolo mode on resume
```

No stats dashboard, no rebuild command, no elaborate filter syntax.
