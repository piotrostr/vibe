#![allow(dead_code)]

use anyhow::Result;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AssistantCli {
    #[default]
    Claude,
    Codex,
}

/// Open a new tmux pane running zellij with claude
/// This creates a vertical split in tmux and runs the zellij session there
pub fn open_tmux_pane_with_zellij_claude(session_name: &str, cwd: &Path) -> Result<()> {
    // Build the zellij command that will run claude
    let zellij_cmd = format!(
        "zellij attach {} 2>/dev/null || zellij -s {} -- claude --dangerously-skip-permissions",
        session_name, session_name,
    );

    // Create a new tmux pane (vertical split) and run zellij in it
    // -h = horizontal split (creates pane on right)
    // -c = start directory
    Command::new("tmux")
        .arg("split-window")
        .arg("-h")
        .arg("-c")
        .arg(cwd)
        .arg(&zellij_cmd)
        .spawn()?;

    Ok(())
}

/// Attach to existing zellij session in a new tmux pane
pub fn open_tmux_pane_attach_zellij(session_name: &str) -> Result<()> {
    let zellij_cmd = format!("zellij attach {}", session_name);

    Command::new("tmux")
        .arg("split-window")
        .arg("-h")
        .arg(&zellij_cmd)
        .spawn()?;

    Ok(())
}

// Keep the old functions for reference but rename them
/// Open a new Ghostty terminal window running zellij with claude (legacy)
#[allow(dead_code)]
pub fn open_ghostty_with_zellij_claude(session_name: &str, cwd: &Path) -> Result<()> {
    let zellij_cmd = format!(
        "zellij -s {} --cwd {} -- claude --dangerously-skip-permissions",
        session_name,
        cwd.to_string_lossy()
    );

    let script = format!(
        r#"tell application "Ghostty"
            activate
            tell application "System Events"
                keystroke "n" using command down
            end tell
            delay 0.3
            tell application "System Events"
                keystroke "{}"
                keystroke return
            end tell
        end tell"#,
        zellij_cmd.replace('"', "\\\"")
    );

    Command::new("osascript").arg("-e").arg(&script).spawn()?;

    Ok(())
}

/// Open a new Ghostty terminal window and attach to existing zellij session
pub fn open_ghostty_attach_zellij(session_name: &str) -> Result<()> {
    let zellij_cmd = format!("zellij attach {}", shell_escape(session_name));

    // Wrap in /bin/zsh -c "..." as a single string for Ghostty's -e flag
    let full_cmd = format!("/bin/zsh -c \"{}\"", zellij_cmd);

    Command::new("open")
        .arg("-na")
        .arg("Ghostty")
        .arg("--args")
        .arg("-e")
        .arg(&full_cmd)
        .spawn()?;

    Ok(())
}

/// Open a new Ghostty terminal window with a custom command
pub fn open_ghostty_with_command(command: &str, cwd: Option<&Path>) -> Result<()> {
    let mut cmd = Command::new("open");
    cmd.arg("-na").arg("Ghostty").arg("--args").arg("-e");

    if let Some(dir) = cwd {
        // Wrap command to cd first
        let full_cmd = format!("cd {} && {}", shell_escape(&dir.to_string_lossy()), command);
        cmd.arg(&full_cmd);
    } else {
        cmd.arg(command);
    }

    cmd.spawn()?;
    Ok(())
}

/// Simple shell escape for command arguments
fn shell_escape(s: &str) -> String {
    // If string contains no special chars, return as-is
    if s.chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.')
    {
        return s.to_string();
    }
    // Otherwise, wrap in single quotes and escape any single quotes
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Create a launcher script for zellij session
/// fresh_cmd: command for new sessions (with prompt)
/// continue_cmd: command for resuming EXITED sessions (with --continue)
/// plan_mode: if true, kill running sessions to restart in plan mode
fn create_launcher_script(
    session_name: &str,
    fresh_cmd: &str,
    continue_cmd: &str,
    plan_mode: bool,
) -> Result<std::path::PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let script_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("vibe-scripts");
    std::fs::create_dir_all(&script_dir)?;

    // Create wrapper scripts for fresh and continue commands
    // This is more reliable than passing complex commands via -- flag
    let fresh_script_path = script_dir.join(format!("{}-fresh.sh", session_name));
    let fresh_script = format!("#!/bin/zsh\nexec {}\n", fresh_cmd);
    let mut file = std::fs::File::create(&fresh_script_path)?;
    file.write_all(fresh_script.as_bytes())?;
    drop(file);
    std::fs::set_permissions(&fresh_script_path, std::fs::Permissions::from_mode(0o755))?;

    let continue_script_path = script_dir.join(format!("{}-continue.sh", session_name));
    let continue_script = format!("#!/bin/zsh\nexec {}\n", continue_cmd);
    let mut file = std::fs::File::create(&continue_script_path)?;
    file.write_all(continue_script.as_bytes())?;
    drop(file);
    std::fs::set_permissions(
        &continue_script_path,
        std::fs::Permissions::from_mode(0o755),
    )?;

    // Track plan mode state in a marker file
    let plan_marker = script_dir.join(format!("{}-plan.marker", session_name));

    // Launcher script that wt switch -x will execute
    // Check session state and handle accordingly:
    // - Running + plan mode requested but session not in plan mode: kill and restart
    // - Running: attach to it
    // - EXITED: delete and create with --continue (resume conversation)
    // - Not found: create new with prompt
    // Use SHELL=/path/to/script zellij -s session to run script as the shell
    let launcher_path = script_dir.join(format!("{}-launch.sh", session_name));
    let launcher_script = if plan_mode {
        format!(
            r#"#!/bin/zsh
# Strip ANSI color codes for reliable grep
SESSION_LINE=$(zellij list-sessions 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | grep "^{session}")
# Reset terminal state - pipes above can corrupt it, sleep lets terminal settle
stty sane 2>/dev/null
sleep 0.1
if [[ -n "$SESSION_LINE" ]]; then
  if echo "$SESSION_LINE" | grep -q "EXITED"; then
    zellij delete-session {session} 2>/dev/null
    touch {plan_marker}
    SHELL={continue_script} exec zellij -s {session}
  else
    # Plan mode requested - check if session was started in plan mode
    if [[ ! -f {plan_marker} ]]; then
      # Session running but not in plan mode - kill and restart in plan mode
      zellij kill-session {session} 2>/dev/null
      sleep 0.2
      touch {plan_marker}
      SHELL={fresh_script} exec zellij -s {session}
    else
      exec zellij attach {session}
    fi
  fi
fi
touch {plan_marker}
SHELL={fresh_script} exec zellij -s {session}
"#,
            session = session_name,
            fresh_script = fresh_script_path.display(),
            continue_script = continue_script_path.display(),
            plan_marker = plan_marker.display(),
        )
    } else {
        format!(
            r#"#!/bin/zsh
# Strip ANSI color codes for reliable grep
SESSION_LINE=$(zellij list-sessions 2>/dev/null | sed 's/\x1b\[[0-9;]*m//g' | grep "^{session}")
# Reset terminal state - pipes above can corrupt it, sleep lets terminal settle
stty sane 2>/dev/null
sleep 0.1
if [[ -n "$SESSION_LINE" ]]; then
  if echo "$SESSION_LINE" | grep -q "EXITED"; then
    zellij delete-session {session} 2>/dev/null
    rm -f {plan_marker}
    SHELL={continue_script} exec zellij -s {session}
  else
    exec zellij attach {session}
  fi
fi
rm -f {plan_marker}
SHELL={fresh_script} exec zellij -s {session}
"#,
            session = session_name,
            fresh_script = fresh_script_path.display(),
            continue_script = continue_script_path.display(),
            plan_marker = plan_marker.display(),
        )
    };
    let mut file = std::fs::File::create(&launcher_path)?;
    file.write_all(launcher_script.as_bytes())?;
    drop(file);
    std::fs::set_permissions(&launcher_path, std::fs::Permissions::from_mode(0o755))?;

    Ok(launcher_path)
}

/// Get the wt binary path - check WORKTRUNK_BIN env or fall back to cargo bin
fn wt_binary() -> String {
    std::env::var("WORKTRUNK_BIN").unwrap_or_else(|_| {
        dirs::home_dir()
            .map(|h| h.join(".cargo/bin/wt").to_string_lossy().to_string())
            .unwrap_or_else(|| "wt".to_string())
    })
}

fn codex_flags(plan_mode: bool) -> &'static str {
    if plan_mode {
        "--ask-for-approval on-request --sandbox read-only"
    } else {
        "--dangerously-bypass-approvals-and-sandbox"
    }
}

fn commands_for_existing_worktree(assistant: AssistantCli, plan_mode: bool) -> (String, String) {
    match assistant {
        AssistantCli::Claude => {
            // For Claude we can safely use --continue for both fresh and resumed sessions.
            let cmd = if plan_mode {
                "claude --continue --permission-mode plan".to_string()
            } else {
                "claude --continue --dangerously-skip-permissions".to_string()
            };
            (cmd.clone(), cmd)
        }
        AssistantCli::Codex => {
            let flags = codex_flags(plan_mode);
            (
                format!("codex {flags}"),
                format!("codex resume --last {flags}"),
            )
        }
    }
}

fn commands_with_context(
    assistant: AssistantCli,
    plan_mode: bool,
    context_file: &std::path::Path,
) -> (String, String) {
    match assistant {
        AssistantCli::Claude => {
            if plan_mode {
                (
                    format!(
                        "claude --permission-mode plan \"$(cat {})\"",
                        context_file.display()
                    ),
                    "claude --continue --permission-mode plan".to_string(),
                )
            } else {
                (
                    format!(
                        "claude --dangerously-skip-permissions \"$(cat {})\"",
                        context_file.display()
                    ),
                    "claude --continue --dangerously-skip-permissions".to_string(),
                )
            }
        }
        AssistantCli::Codex => {
            let flags = codex_flags(plan_mode);
            (
                format!("codex {flags} \"$(cat {})\"", context_file.display()),
                format!("codex resume --last {flags}"),
            )
        }
    }
}

/// Launch assistant CLI in a zellij session for a worktree
/// Uses `wt switch -x` to switch/create worktree AND launch zellij in one step
/// The -x script inherits TTY from wt, which inherits from us (via .status())
/// project_dir: The project's git repo root directory (wt must run from within repo)
pub fn launch_zellij_claude_in_worktree(
    branch: &str,
    assistant: AssistantCli,
    plan_mode: bool,
    project_dir: &std::path::Path,
) -> Result<()> {
    let session_name = super::session_name_for_branch(branch);
    let wt = wt_binary();

    // Verify paths exist
    if !std::path::Path::new(&wt).exists() {
        anyhow::bail!("wt binary not found at: {}", wt);
    }
    if !project_dir.exists() {
        anyhow::bail!("project_dir does not exist: {:?}", project_dir);
    }

    let (fresh_cmd, continue_cmd) = commands_for_existing_worktree(assistant, plan_mode);
    let launcher = create_launcher_script(&session_name, &fresh_cmd, &continue_cmd, plan_mode)?;
    let launcher_path = launcher.to_str().unwrap();

    // Use .status() to inherit TTY - this is critical for zellij to work!
    // Try existing branch first, then --create if not found
    let status = Command::new(&wt)
        .current_dir(project_dir)
        .args(["switch", branch, "-y", "-x", launcher_path])
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => {
            // Try with --create for new branches
            let status = Command::new(&wt)
                .current_dir(project_dir)
                .args(["switch", "--create", branch, "-y", "-x", launcher_path])
                .status()?;

            if status.success() {
                Ok(())
            } else {
                anyhow::bail!("wt switch --create failed");
            }
        }
        Err(e) => anyhow::bail!("wt command error: {}", e),
    }
}

/// Launch assistant CLI in a zellij session with task context for fresh tasks
/// Creates worktree if needed, passes task context as initial prompt
/// project_dir: The project's git repo root directory (wt must run from within repo)
pub fn launch_zellij_claude_in_worktree_with_context(
    branch: &str,
    task_context: &str,
    assistant: AssistantCli,
    plan_mode: bool,
    project_dir: &std::path::Path,
) -> Result<()> {
    let session_name = super::session_name_for_branch(branch);
    let wt = wt_binary();

    // Verify paths exist
    if !std::path::Path::new(&wt).exists() {
        anyhow::bail!("wt binary not found at: {}", wt);
    }
    if !project_dir.exists() {
        anyhow::bail!("project_dir does not exist: {:?}", project_dir);
    }

    // Write task context to file
    let script_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("vibe-scripts");
    std::fs::create_dir_all(&script_dir)?;

    let context_file = script_dir.join(format!("{}-context.txt", session_name));
    std::fs::write(&context_file, task_context)?;

    let (fresh_cmd, continue_cmd) = commands_with_context(assistant, plan_mode, &context_file);

    let launcher = create_launcher_script(&session_name, &fresh_cmd, &continue_cmd, plan_mode)?;
    let launcher_path = launcher.to_str().unwrap();

    // Use .status() to inherit TTY - critical for zellij!
    let status = Command::new(&wt)
        .current_dir(project_dir)
        .args(["switch", branch, "-y", "-x", launcher_path])
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => {
            // Try with --create for new branches
            let status = Command::new(&wt)
                .current_dir(project_dir)
                .args(["switch", "--create", branch, "-y", "-x", launcher_path])
                .status()?;

            if status.success() {
                Ok(())
            } else {
                anyhow::bail!("wt switch --create failed");
            }
        }
        Err(e) => anyhow::bail!("wt command error: {}", e),
    }
}

fn prime_prompt(project_name: &str) -> String {
    let prime_session = super::sanitize_session_name(&format!("{}.prime", project_name));
    format!(
        "This is the prime session for {project}.\n\
         \n\
         You're the coordinator here - not implementing, but steering the work. \
         Your ~/.claude/CLAUDE.md is loaded with everything you need to know.\n\
         \n\
         This session is for:\n\
         - Reviewing what's in flight, what's blocked, what's next\n\
         - Planning new work items and breaking down features\n\
         - Architecture, trade-offs, priorities\n\
         - PR reviews and feedback\n\
         - Any conversation that doesn't belong to a specific task branch\n\
         \n\
         The board (vibe) and Linear track the tasks. Implementation happens in \
         separate worktree sessions. This is the war room.\n\
         \n\
         You are session: {session}\n\
         Workers report to you via zellij write-chars with prefixes: [DONE], [BLOCKED], [PROGRESS].\n\
         You can check on workers with dump-screen and send them input with write-chars.\n\
         Use /prime for the full reference on inter-session communication.",
        project = project_name,
        session = prime_session,
    )
}

/// Rapporting instructions appended to worker task context.
/// Tells the worker how to report back to the prime session.
pub fn rapporting_instructions(project_name: &str) -> String {
    let prime_session = super::sanitize_session_name(&format!("{}.prime", project_name));
    format!(
        "\n---\n\
         Prime coordination: report to the prime session when you complete, get blocked, or hit a milestone.\n\
         Also reply via the same channel when prime checks in on you.\n\
         \n\
         Done:     zellij -s {prime} action write-chars '[DONE] <1-line summary>' && \
         zellij -s {prime} action write 13\n\
         Blocked:  zellij -s {prime} action write-chars '[BLOCKED] <what you need>' && \
         zellij -s {prime} action write 13\n\
         Progress: zellij -s {prime} action write-chars '[PROGRESS] <milestone>' && \
         zellij -s {prime} action write 13\n\
         Reply:    zellij -s {prime} action write-chars '<your response>' && \
         zellij -s {prime} action write 13\n\
         \n\
         When prime messages you (check-ins, questions, feedback), always respond back through \
         write-chars. Prime can't see your screen - the only way to communicate is through this channel.\n\
         \n\
         IMPORTANT: You own ONE ticket on ONE branch. Do not start new tickets or features on this branch.\n\
         When you are done, report [DONE] and stop. New work gets its own worktree and session - \
         prime handles that. This keeps everything atomic: one ticket = one branch = one worktree.",
        prime = prime_session,
    )
}

/// Launch a prime session - the project-level coordination session.
/// Uses headless PTY so the session always accepts write-chars (even when detached),
/// then attaches interactively.
pub fn launch_prime_session(
    project_name: &str,
    assistant: AssistantCli,
    project_dir: &std::path::Path,
) -> Result<()> {
    let session_name = super::sanitize_session_name(&format!("{}.prime", project_name));

    if !project_dir.exists() {
        anyhow::bail!("project_dir does not exist: {:?}", project_dir);
    }

    // Check session state: alive, exited, or doesn't exist
    let session_output = Command::new("zellij")
        .args(["list-sessions"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let session_line = session_output
        .lines()
        .find(|l| l.contains(&session_name));
    let session_alive = session_line.is_some_and(|l| !l.contains("EXITED"));
    let session_exited = session_line.is_some_and(|l| l.contains("EXITED"));

    if !session_alive {
        if session_exited {
            // Clean up the dead session so we can recreate it
            let _ = Command::new("zellij")
                .args(["delete-session", &session_name])
                .status();
        }

        let script_dir = dirs::cache_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("vibe-scripts");
        std::fs::create_dir_all(&script_dir)?;

        let context_file = script_dir.join(format!("{}-context.txt", session_name));
        std::fs::write(&context_file, prime_prompt(project_name))?;

        let (fresh_cmd, continue_cmd) = match assistant {
            AssistantCli::Claude => (
                format!(
                    "claude --dangerously-skip-permissions \"$(cat {})\"",
                    context_file.display()
                ),
                "claude --continue --dangerously-skip-permissions".to_string(),
            ),
            AssistantCli::Codex => {
                let flags = codex_flags(false);
                (
                    format!("codex {flags} \"$(cat {})\"", context_file.display()),
                    format!("codex resume --last {flags}"),
                )
            }
        };

        let _launcher =
            create_launcher_script(&session_name, &fresh_cmd, &continue_cmd, false)?;

        // Use continue script if resuming a dead session, fresh if brand new
        let shell_script = if session_exited {
            script_dir.join(format!("{}-continue.sh", session_name))
        } else {
            script_dir.join(format!("{}-fresh.sh", session_name))
        };

        spawn_headless_via_launchd(
            &session_name,
            shell_script.to_str().unwrap(),
            project_dir.to_str().unwrap(),
        )?;
    }

    // Attach interactively (user sees the session in their terminal)
    attach_zellij_foreground(&session_name)?;

    Ok(())
}

/// Get the prime session name for a project
pub fn prime_session_name(project_name: &str) -> String {
    super::sanitize_session_name(&format!("{}.prime", project_name))
}

fn headless_zellij_bin() -> String {
    dirs::home_dir()
        .map(|h| h.join(".vibe/bin/headless-zellij"))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "headless-zellij".to_string())
}

/// Spawn a headless zellij session via launchd so the PTY-holding process
/// lives outside any sandbox/process-group. This is the only reliable way
/// to keep write-chars working when the spawning process (e.g. Claude Code)
/// tears down its process tree.
fn spawn_headless_via_launchd(
    session_name: &str,
    shell_script: &str,
    working_dir: &str,
) -> Result<()> {
    let label = format!("com.vibe.headless.{}", session_name);
    let headless = headless_zellij_bin();

    let plist_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("vibe-launchd");
    std::fs::create_dir_all(&plist_dir)?;
    let plist_path = plist_dir.join(format!("{}.plist", label));

    // Collect current PATH and HOME for the plist
    let path = std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string());
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>python3</string>
        <string>{headless}</string>
        <string>--no-fork</string>
        <string>{session_name}</string>
        <string>{shell_script}</string>
        <string>{working_dir}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>{path}</string>
        <key>HOME</key>
        <string>{home}</string>
        <key>TERM</key>
        <string>xterm-256color</string>
        <key>COLORTERM</key>
        <string>truecolor</string>
    </dict>
</dict>
</plist>"#,
    );

    std::fs::write(&plist_path, plist)?;

    // Unload first in case a stale job exists
    let _ = Command::new("launchctl")
        .args(["unload", plist_path.to_str().unwrap()])
        .output();

    let status = Command::new("launchctl")
        .args(["load", plist_path.to_str().unwrap()])
        .status()?;

    if !status.success() {
        anyhow::bail!("launchctl load failed for: {}", session_name);
    }

    // Wait for zellij to register
    std::thread::sleep(std::time::Duration::from_millis(500));

    Ok(())
}

/// Launch a Claude session headlessly in a worktree (no TTY required).
/// Creates the worktree via `wt`, then spawns zellij in the background with a pseudo-TTY.
pub fn launch_headless_in_worktree(
    branch: &str,
    task_context: &str,
    assistant: AssistantCli,
    project_dir: &std::path::Path,
) -> Result<()> {
    let session_name = super::session_name_for_branch(branch);
    let wt = wt_binary();

    if !std::path::Path::new(&wt).exists() {
        anyhow::bail!("wt binary not found at: {}", wt);
    }
    if !project_dir.exists() {
        anyhow::bail!("project_dir does not exist: {:?}", project_dir);
    }

    // Write task context to file
    let script_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
        .join("vibe-scripts");
    std::fs::create_dir_all(&script_dir)?;

    let context_file = script_dir.join(format!("{}-context.txt", session_name));
    std::fs::write(&context_file, task_context)?;

    let (fresh_cmd, continue_cmd) = commands_with_context(assistant, false, &context_file);
    let _launcher = create_launcher_script(&session_name, &fresh_cmd, &continue_cmd, false)?;

    // Create worktree (wt switch without -x, just ensure worktree exists)
    let status = Command::new(&wt)
        .current_dir(project_dir)
        .args(["switch", branch, "-y"])
        .status();

    // If branch doesn't exist, create it
    if matches!(status, Ok(s) if !s.success()) || status.is_err() {
        let status = Command::new(&wt)
            .current_dir(project_dir)
            .args(["switch", "--create", branch, "-y"])
            .status()?;
        if !status.success() {
            anyhow::bail!("wt switch --create failed for branch: {}", branch);
        }
    }

    // Resolve worktree path via `wt list --format=json`
    let worktree_output = Command::new(&wt)
        .current_dir(project_dir)
        .args(["list", "--format=json"])
        .output()?;
    let worktree_dir = if worktree_output.status.success() {
        serde_json::from_slice::<Vec<serde_json::Value>>(&worktree_output.stdout)
            .ok()
            .and_then(|entries| {
                entries
                    .iter()
                    .find(|e| e.get("branch").and_then(|b| b.as_str()) == Some(branch))
                    .and_then(|e| e.get("path"))
                    .and_then(|p| p.as_str())
                    .map(std::path::PathBuf::from)
            })
            .unwrap_or_else(|| project_dir.to_path_buf())
    } else {
        project_dir.to_path_buf()
    };

    // Fresh script is what headless-zellij uses as SHELL
    let fresh_script = script_dir.join(format!("{}-fresh.sh", session_name));

    spawn_headless_via_launchd(
        &session_name,
        fresh_script.to_str().unwrap(),
        worktree_dir.to_str().unwrap(),
    )?;

    Ok(())
}

/// Attach to existing zellij session in current terminal (blocks)
/// Handles dead sessions by force-resurrecting them
pub fn attach_zellij_foreground(session_name: &str) -> Result<()> {
    use super::zellij::get_session_status;

    // Check if session is dead (None = doesn't exist, Some(is_dead) = exists)
    let is_dead = get_session_status(session_name).unwrap_or(false);

    let mut args = vec!["attach"];
    if is_dead {
        args.push("-f"); // Force resurrection of dead session
    }
    args.push(session_name);

    let status = Command::new("zellij").args(&args).status()?;

    if !status.success() {
        anyhow::bail!("zellij attach exited with error");
    }
    Ok(())
}
