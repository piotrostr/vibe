# vibe

Terminal-based kanban board for managing Claude Code sessions with git worktrees.

## Features

- **Kanban board** - organize tasks across Backlog, In Progress, In Review, and Done columns
- **Git worktree integration** - each task gets its own worktree via [worktrunk](https://github.com/piotrostr/worktrunk)
- **Zellij sessions** - launch and manage Claude Code sessions per task
- **Linear sync** - import issues from Linear backlog
- **PR status tracking** - see PR state, merge conflicts, and CI status inline
- **Claude activity monitoring** - see when Claude is thinking, idle, or waiting for input

## Installation

```bash
cargo install --git https://github.com/piotrostr/vibe
```

Or build from source:

```bash
git clone https://github.com/piotrostr/vibe
cd vibe
cargo install --path .
```

## Dependencies

- [worktrunk](https://github.com/piotrostr/worktrunk) (`wt`) - git worktree manager
- [zellij](https://zellij.dev/) - terminal multiplexer for Claude sessions
- [gh](https://cli.github.com/) (optional) - GitHub CLI for PR status

## Usage

Run `vibe` in any git repository:

```bash
cd your-project
vibe
```

### Key Bindings

| Key | Action |
|-----|--------|
| `j/k` | Navigate up/down |
| `h/l` | Switch columns |
| `J/K` | Move task between columns |
| `g` | Launch Claude session for task |
| `p` | Launch with plan mode |
| `Enter` | View task details |
| `c` | Create new task |
| `e` | Edit task |
| `d` | Delete task |
| `v` | Open PR in browser |
| `w` | View worktrees |
| `S` | View sessions |
| `/` | Search tasks |
| `?` | Help |
| `q` | Quit |

## Task Storage

Tasks are stored as markdown files with YAML frontmatter:

```
~/.vibe/projects/{project-name}/tasks/
```

Example task file:

```markdown
---
id: 550e8400-e29b-41d4-a716-446655440000
linear_id: TEAM-123
created: 2024-01-15
---

# Implement user authentication

Add OAuth2 login flow with Google and GitHub providers.
```

## Configuration

Logs are written to `~/.vibe/vibe.log`. Set `RUST_LOG=info` for verbose logging.

For Linear integration, set `LINEAR_API_KEY` environment variable.

## License

MIT
