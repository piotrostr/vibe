# vibe

Terminal-based kanban board for managing Claude Code sessions with git worktrees.

## Features

- **Kanban board** - organize tasks across Backlog, In Progress, In Review, and Done columns
- **Git worktree integration** - each task gets its own worktree via [worktrunk](https://github.com/piotrostr/worktrunk)
- **Zellij sessions** - launch and manage Claude Code sessions per task
- **Linear sync** - import issues from Linear backlog
- **PR status tracking** - see PR state, merge conflicts, and CI status inline
- **Claude activity monitoring** - see when Claude is thinking, idle, or waiting for input

## Quick Setup with Claude

If you have [Claude Code](https://github.com/anthropics/claude-code) installed, run:

```bash
claude --dangerously-skip-permissions \
  "$(curl -fsSL https://raw.githubusercontent.com/piotrostr/vibe/main/setup-prompt.md)"
```

This will detect your OS, install dependencies, configure Zellij, set up the Claude statusline, and install vibe.

## Manual Installation

### Prerequisites

- [Claude Code](https://github.com/anthropics/claude-code) - the AI coding assistant this tool orchestrates
- [Rust](https://rustup.rs/) - for building vibe and worktrunk

### Install vibe

```bash
cargo install --git https://github.com/piotrostr/vibe
```

Or build from source:

```bash
git clone https://github.com/piotrostr/vibe
cd vibe
cargo install --path .
```

### Dependencies

- [worktrunk](https://github.com/max-sixty/worktrunk) (`wt`) - git worktree manager
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

### Zellij Configuration

Vibe works best with a minimal Zellij config. Example `~/.config/zellij/config.kdl`:

```kdl
default_mode "locked"
default_layout "vibe"
pane_frames false
simplified_ui true
show_startup_tips false
show_release_notes false
copy_command "pbcopy"
copy_on_select false
scrollback_editor "nvim"

theme "gruvbox-dark"

themes {
    gruvbox-dark {
        fg "#ebdbb2"
        bg "#282828"
        black "#282828"
        red "#cc241d"
        green "#98971a"
        yellow "#d79921"
        blue "#458588"
        magenta "#b16286"
        cyan "#689d6a"
        white "#a89984"
        orange "#d65d0e"
    }
}

keybinds clear-defaults=true {
    locked {
        bind "Ctrl b" { SwitchToMode "Normal"; }
        bind "Ctrl d" { Detach; }
    }

    normal {
        bind "Ctrl b" { SwitchToMode "Locked"; }
        bind "Esc" { SwitchToMode "Locked"; }
        bind "d" { Detach; }
        bind "[" { SwitchToMode "Scroll"; }
        bind "s" { NewPane "Down"; SwitchToMode "Locked"; }
        bind "v" { NewPane "Right"; SwitchToMode "Locked"; }
        bind "c" { NewTab; SwitchToMode "Locked"; }
        bind "n" { GoToNextTab; SwitchToMode "Locked"; }
        bind "p" { GoToPreviousTab; SwitchToMode "Locked"; }
        bind "h" { MoveFocus "Left"; SwitchToMode "Locked"; }
        bind "j" { MoveFocus "Down"; SwitchToMode "Locked"; }
        bind "k" { MoveFocus "Up"; SwitchToMode "Locked"; }
        bind "l" { MoveFocus "Right"; SwitchToMode "Locked"; }
        bind "x" { CloseFocus; SwitchToMode "Locked"; }
        bind "z" { ToggleFocusFullscreen; SwitchToMode "Locked"; }
    }

    scroll {
        bind "j" "Down" { ScrollDown; }
        bind "k" "Up" { ScrollUp; }
        bind "d" "Ctrl d" { HalfPageScrollDown; }
        bind "u" "Ctrl u" { HalfPageScrollUp; }
        bind "g" { ScrollToTop; }
        bind "G" { ScrollToBottom; }
        bind "/" { SwitchToMode "EnterSearch"; SearchInput 0; }
        bind "Esc" "q" { SwitchToMode "Locked"; }
        bind "e" { EditScrollback; SwitchToMode "Locked"; }
    }

    search {
        bind "n" { Search "down"; }
        bind "N" { Search "up"; }
        bind "Esc" { SwitchToMode "Scroll"; }
    }

    entersearch {
        bind "Enter" { SwitchToMode "Search"; }
        bind "Esc" { SwitchToMode "Scroll"; }
    }
}
```

Create a minimal layout at `~/.config/zellij/layouts/vibe.kdl`:

```kdl
layout {
    default_tab_template {
        pane size=1 borderless=true {
            plugin location="compact-bar"
        }
        children
    }
}
```

Key bindings (tmux-like with `Ctrl+b` prefix):
- `Ctrl+b` - enter command mode
- `Ctrl+d` - detach session
- `Ctrl+b [` - scroll mode (vim keys, `/` to search)
- `Ctrl+b s/v` - split horizontal/vertical
- `Ctrl+b h/j/k/l` - navigate panes
- `Ctrl+b c/n/p` - new tab / next tab / previous tab

### Claude Activity Indication

For real-time Claude session status indicators (thinking/waiting/idle), configure Claude Code's statusline:

1. Create `~/.vibe/claude-statusline.sh`:

```bash
#!/bin/bash
STATE_DIR="$HOME/.vibe/claude-activity"
mkdir -p "$STATE_DIR"

input=$(cat)
working_dir=$(echo "$input" | jq -r '.workspace.current_dir // empty')
input_tokens=$(echo "$input" | jq -r '.context_window.current_usage.input_tokens // "null"')
output_tokens=$(echo "$input" | jq -r '.context_window.current_usage.output_tokens // "null"')

if [ -n "$working_dir" ]; then
    dir_hash=$(echo -n "$working_dir" | md5 | cut -c1-16)
    cat > "$STATE_DIR/$dir_hash.json" << EOF
{"working_dir":"$working_dir","input_tokens":$input_tokens,"output_tokens":$output_tokens,"timestamp":$(date +%s)}
EOF
fi

# Optional: display git branch
cd "$working_dir" 2>/dev/null || true
branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null)
[ -n "$branch" ] && printf '\033[33mgit:\033[31m%s\033[0m' "$branch"
```

2. Make executable: `chmod +x ~/.vibe/claude-statusline.sh`

3. Add to `~/.claude/settings.json`:

```json
{
  "statusLine": {
    "type": "command",
    "command": "~/.vibe/claude-statusline.sh"
  }
}
```

Activity indicators in vibe:
- Spinner (yellow) - Claude is thinking
- `[!]` (red) - Claude is waiting for input
- `[-]` (gray) - Session idle/stale

## License

MIT
