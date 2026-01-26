# Vibe Setup

Set up vibe - a terminal kanban board for managing Claude Code sessions with git worktrees.

Reference documentation: https://github.com/piotrostr/vibe/blob/main/README.md

## Tasks

1. **Detect the operating system** (macOS/Linux/WSL) and package manager (brew/apt/dnf/pacman)

2. **Install system dependencies**:
   - `zellij` - terminal multiplexer
   - `neovim` - used as Zellij's scrollback editor
   - `jq` - JSON processor (for statusline)
   - `gh` - GitHub CLI (optional but recommended)

   Use the appropriate package manager:
   - macOS: `brew install zellij neovim jq gh`
   - Debian/Ubuntu: `sudo apt install zellij neovim jq gh`
   - Fedora: `sudo dnf install zellij neovim jq gh`
   - Arch: `sudo pacman -S zellij neovim jq github-cli`

3. **Install Rust** if not present: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y`

4. **Install worktrunk** (git worktree manager):
   ```bash
   cargo install --git https://github.com/max-sixty/worktrunk
   ```

5. **Install vibe**:
   ```bash
   cargo install --git https://github.com/piotrostr/vibe
   ```

6. **Create Zellij config directory**: `mkdir -p ~/.config/zellij/layouts`

7. **Write Zellij config** to `~/.config/zellij/config.kdl`:
   ```kdl
   default_mode "locked"
   default_layout "vibe"
   pane_frames false
   simplified_ui true
   show_startup_tips false
   show_release_notes false
   copy_command "pbcopy"  # Use "xclip -selection clipboard" on Linux
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

8. **Write Zellij layout** to `~/.config/zellij/layouts/vibe.kdl`:
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

9. **Create vibe directory**: `mkdir -p ~/.vibe`

10. **Write Claude statusline script** to `~/.vibe/claude-statusline.sh`:
    ```bash
    #!/bin/bash
    STATE_DIR="$HOME/.vibe/claude-activity"
    mkdir -p "$STATE_DIR"

    input=$(cat)
    working_dir=$(echo "$input" | jq -r '.workspace.current_dir // empty')
    session_id=$(echo "$input" | jq -r '.session_id // empty')
    input_tokens=$(echo "$input" | jq -r '.context_window.current_usage.input_tokens // "null"')
    output_tokens=$(echo "$input" | jq -r '.context_window.current_usage.output_tokens // "null"')
    used_pct=$(echo "$input" | jq -r '.context_window.used_percentage // "null"')
    api_duration_ms=$(echo "$input" | jq -r '.cost.total_api_duration_ms // "null"')

    if [ -n "$working_dir" ]; then
        dir_hash=$(echo -n "$working_dir" | md5sum 2>/dev/null | cut -c1-16 || echo -n "$working_dir" | md5 | cut -c1-16)
        cat > "$STATE_DIR/$dir_hash.json" << EOF
    {"working_dir":"$working_dir","session_id":"$session_id","input_tokens":$input_tokens,"output_tokens":$output_tokens,"used_percentage":$used_pct,"api_duration_ms":$api_duration_ms,"timestamp":$(date +%s)}
    EOF
    fi

    # Display git branch
    cd "$working_dir" 2>/dev/null || true
    branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null)
    [ -n "$branch" ] && printf '\033[33mgit:\033[31m%s\033[0m' "$branch"
    ```

11. **Write Claude thinking-start hook** to `~/.vibe/claude-thinking-start.sh`:
    ```bash
    #!/bin/bash
    STATE_DIR="$HOME/.vibe/claude-activity"
    input=$(cat)
    working_dir=$(echo "$input" | jq -r '.cwd // empty')

    if [ -n "$working_dir" ]; then
        dir_hash=$(echo -n "$working_dir" | md5sum 2>/dev/null | cut -c1-16 || echo -n "$working_dir" | md5 | cut -c1-16)
        mkdir -p "$STATE_DIR"
        touch "$STATE_DIR/$dir_hash.thinking"
    fi
    ```

12. **Write Claude thinking-stop hook** to `~/.vibe/claude-thinking-stop.sh`:
    ```bash
    #!/bin/bash
    STATE_DIR="$HOME/.vibe/claude-activity"
    input=$(cat)
    working_dir=$(echo "$input" | jq -r '.cwd // empty')

    if [ -n "$working_dir" ]; then
        dir_hash=$(echo -n "$working_dir" | md5sum 2>/dev/null | cut -c1-16 || echo -n "$working_dir" | md5 | cut -c1-16)
        rm -f "$STATE_DIR/$dir_hash.thinking"
    fi
    ```

13. **Make all scripts executable**:
    ```bash
    chmod +x ~/.vibe/claude-statusline.sh
    chmod +x ~/.vibe/claude-thinking-start.sh
    chmod +x ~/.vibe/claude-thinking-stop.sh
    ```

14. **Configure Claude Code** - update `~/.claude/settings.json` to include:
    ```json
    {
      "statusLine": {
        "type": "command",
        "command": "~/.vibe/claude-statusline.sh"
      },
      "hooks": {
        "UserPromptSubmit": [
          {
            "hooks": [
              {
                "type": "command",
                "command": "~/.vibe/claude-thinking-start.sh"
              }
            ]
          }
        ],
        "Stop": [
          {
            "hooks": [
              {
                "type": "command",
                "command": "~/.vibe/claude-thinking-stop.sh"
              }
            ]
          }
        ]
      }
    }
    ```
    If the file exists, merge the statusLine and hooks config. If it doesn't exist, create it.

15. **Adjust for Linux** if detected:
    - Change `copy_command` in Zellij config from `"pbcopy"` to `"xclip -selection clipboard"` or `"wl-copy"` (Wayland)
    - The scripts above already handle `md5sum` vs `md5` differences

16. **Verify installation** by running:
    - `which vibe` - should show the installed binary
    - `which wt` - should show worktrunk
    - `which zellij` - should show zellij
    - `which nvim` - should show neovim
    - `zellij --version` - should work

17. **Print success message** with quick start instructions:
    - Run `vibe` in any git repository to start
    - Press `c` to create a task, `g` to launch a Claude session
    - Press `?` for help
    - Activity indicators: spinning star = thinking, [?] = waiting for input
