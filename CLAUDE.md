# vibe

Multi-Claude orchestration system. Manages autonomous Claude Code sessions ("cousins") in isolated git worktrees, coordinated by a prime session. Includes a TUI kanban board, CLI tools, and inter-session communication.

## Version Control

Merging requires making a PR and merging into main. Use `gh` CLI.

You can safely create PR and merge the work ONLY when the TDD turns green OR user asks you to OR user gives feedback regarding testing done manually.

If you don't have tests - write them.

If you need user to test - prompt them.

`gh pr merge <pr-number> --squash`

## Build and Run Commands

You have a `justfile`, check it out.

Quick commands:

```bash
# Run the TUI
cargo run --bin vibe  # or `just vibe`

# Run with logging
RUST_LOG=info cargo run --bin vibe

# Run tests
cargo test

# Run a single test
cargo test test_slugify

# Check/lint
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

Logs are written to `~/.vibe/vibe.log`.

## CLI Commands

```bash
vibe                          # open the TUI kanban board
vibe create --title "..." --description "..." [--gas-it]  # create ticket (+ spawn cousin)
vibe gas VIB-23               # spawn cousin for existing task (by Linear ID, title, or UUID)
vibe import plan.md --title "..." [--gas-it]  # import markdown plan as task (+ spawn cousin)
vibe status                   # show Linear board state grouped by column
vibe cleanup [target]         # tear down finished sessions (launchd + zellij)
cousin list                   # list active cousins for this project
cousin <target> <message>     # send message to a cousin (prime, ticket ID, or session name)
cousin --urgent <target> <msg> # Ctrl+C interrupt then send
```

## Orchestration Model

- **Prime**: coordinator session. Plans with Piotr, spawns cousins, reviews PRs, tracks progress.
- **Cousin**: worker session. One ticket = one branch = one worktree = one session. Reports [DONE]/[BLOCKED]/[PROGRESS] to prime via `cousin` CLI.
- **Atomic model**: cousins never reuse branches or take on new tickets. When done, they stop.

Sessions are spawned headlessly via launchd (escapes Claude Code sandbox). Prime attaches interactively via `zellij attach`. Full protocol in `~/.claude/skills/vibe/SKILL.md`.

## Architecture

### TUI (`app.rs`)

The `App` struct owns all state and runs the main loop:
1. Poll background channels for worktree/session/PR updates
2. Render current view
3. Handle keyboard input via action dispatch

Background loading uses `tokio::task::spawn_blocking` with mpsc channels to avoid blocking the UI thread.

### Module Structure

- **state/** - View state for each screen (kanban, tasks, worktrees, sessions, search, logs). `AppState` in `app_state.rs` aggregates all view states.
- **input/** - `keybindings.rs` maps keys to `Action` enum based on current view.
- **ui/** - Ratatui rendering functions. One file per view (kanban.rs, worktrees.rs, etc.).
- **storage/** - File-based task storage. Tasks are markdown files in `~/.vibe/projects/{project}/tasks/` with YAML frontmatter.
- **external/** - Shell-out wrappers:
  - `zellij.rs` - Session listing, attach, kill, attention detection
  - `worktrunk.rs` - `wt` CLI wrapper for worktree management
  - `terminal_spawn.rs` - Session launch logic, launchd headless spawn, prime launch
  - `linear.rs` - Linear GraphQL API (create issues, fetch board state)
  - `gh.rs` - GitHub CLI for PR info
  - `editor.rs` - External editor invocation
- **`src/bin/cousin_mail.rs`** - `cousin` CLI binary for inter-session IPC via zellij write-chars
- **`src/bin/headless-zellij.py`** - Python PTY holder that keeps headless sessions attached

### Headless Session Spawn Flow

1. Create launcher shell script in `~/.vibe/cache/vibe-scripts/`
2. Write launchd plist to `~/Library/Caches/vibe-launchd/`
3. `launchctl load` spawns headless-zellij.py as child of PID 1 (outside Claude Code sandbox)
4. headless-zellij.py creates a PTY and holds it so zellij write-chars can reach the session

### Key Bindings

View-specific bindings in `input/keybindings.rs`. Global: `q` quit, `?` help, `/` search, `Esc` back.

Kanban: `j/k` navigate, `J/K` change columns, `g` launch session, `p` launch with plan mode, `e` edit, `c` create, `d` delete, `v` view PR, `w` worktrees, `S` sessions.

### Task Storage Format

Markdown files with YAML frontmatter:
```markdown
---
id: uuid
linear_id: TEAM-123  # optional
created: 2024-01-15
---

# Task Title

Description here...
```

Tasks stored at `~/.vibe/projects/{cwd_dirname}/tasks/`.

## Environment Variables

- `{PROJECT}_LINEAR_API_KEY` - Linear API key (e.g. `VIBE_LINEAR_API_KEY`, `MYPROJECT_LINEAR_API_KEY`)

## Dependencies

- `wt` CLI (worktrunk) - must be installed at `~/.cargo/bin/wt` or set `WORKTRUNK_BIN`
- `zellij` - terminal multiplexer for Claude sessions
- `gh` CLI - optional, for PR status
- `python3` - for headless-zellij.py PTY holder
