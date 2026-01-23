#![allow(dead_code)]

use anyhow::Result;
use std::path::Path;
use std::process::Command;

/// Strip ANSI escape sequences from a string
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip escape sequence: ESC [ ... letter
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // Skip until we hit a letter (the terminator)
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClaudeActivityState {
    #[default]
    Unknown, // No statusline data available
    Idle,           // Claude not running (stale data)
    Thinking,       // Actively processing (tokens changing)
    WaitingForUser, // Stopped, awaiting input (tokens stable)
}

#[derive(Debug, Clone)]
pub struct ZellijSession {
    pub name: String,
    pub is_current: bool,
    pub is_dead: bool,
    pub needs_attention: bool,
    pub claude_activity: ClaudeActivityState,
    pub context_percentage: Option<f64>,
}

pub fn list_sessions() -> Result<Vec<ZellijSession>> {
    let output = Command::new("zellij").args(["list-sessions"]).output()?;

    if !output.status.success() {
        // zellij returns error if no sessions exist
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("No active sessions") || stderr.is_empty() {
            return Ok(Vec::new());
        }
        anyhow::bail!("zellij list-sessions failed: {}", stderr);
    }

    let stdout = String::from_utf8(output.stdout)?;
    let sessions: Vec<ZellijSession> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            // Strip ANSI color codes first
            let clean_line = strip_ansi(line);

            // Format: "session-name [Created 3m 5s ago] (current)"
            // Or dead: "session-name [Created 3m 5s ago] (EXITED -9attach to resurrect)"
            let is_current = clean_line.contains("(current)");
            let is_dead = clean_line.contains("EXITED");

            // Extract session name: everything before first '[' or space with metadata
            let name = clean_line.split('[').next().unwrap_or("").trim().to_string();

            ZellijSession {
                name,
                is_current,
                is_dead,
                needs_attention: false,
                claude_activity: ClaudeActivityState::Unknown,
                context_percentage: None,
            }
        })
        .collect();

    Ok(sessions)
}

/// Check if a session is waiting for user input by dumping screen content
pub fn check_session_needs_attention(session_name: &str) -> bool {
    // Dump the last few lines of the session screen
    let output = Command::new("zellij")
        .args([
            "action",
            "--session",
            session_name,
            "dump-screen",
            "/dev/stdout",
        ])
        .output();

    let Ok(output) = output else {
        return false;
    };

    if !output.status.success() {
        return false;
    }

    let screen = String::from_utf8_lossy(&output.stdout);
    let last_lines: String = screen.lines().rev().take(10).collect::<Vec<_>>().join("\n");

    // Patterns that indicate Claude is waiting for input
    let attention_patterns = [
        "? ",             // Interactive prompt
        "[y/n]",          // Yes/no prompt
        "(y/N)",          // Yes/no with default
        "(Y/n)",          // Yes/no with default
        "Continue?",      // Confirmation
        "Press Enter",    // Waiting for enter
        "Proceed?",       // Confirmation
        "Do you want to", // Confirmation question
        ">",              // Generic prompt at end of line
        "waiting for",    // Waiting state
        "permission",     // Permission request
    ];

    attention_patterns
        .iter()
        .any(|pattern| last_lines.to_lowercase().contains(&pattern.to_lowercase()))
}

/// List sessions with attention status (slower, checks each session)
pub fn list_sessions_with_status() -> Result<Vec<ZellijSession>> {
    let mut sessions = list_sessions()?;
    for session in &mut sessions {
        session.needs_attention = check_session_needs_attention(&session.name);
    }
    Ok(sessions)
}

pub fn session_exists(name: &str) -> bool {
    list_sessions()
        .map(|sessions| sessions.iter().any(|s| s.name == name))
        .unwrap_or(false)
}

/// Check if a session exists and whether it's dead (needs resurrection)
/// Returns None if session doesn't exist, Some(is_dead) if it does
pub fn get_session_status(name: &str) -> Option<bool> {
    list_sessions()
        .ok()
        .and_then(|sessions| sessions.iter().find(|s| s.name == name).map(|s| s.is_dead))
}

pub fn create_session_with_command(name: &str, cwd: &Path, command: &str) -> Result<()> {
    // Create a zellij session that runs the specified command
    // We use `zellij -s <name> --cwd <path> -- <command>`
    let status = Command::new("zellij")
        .arg("-s")
        .arg(name)
        .arg("--cwd")
        .arg(cwd)
        .arg("--")
        .arg(command)
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to create zellij session: {}", name);
    }
    Ok(())
}

pub fn attach_session(name: &str) -> Result<()> {
    attach_session_with_resurrect(name, false)
}

/// Attach to a session, optionally forcing resurrection of dead sessions
pub fn attach_session_with_resurrect(name: &str, force_resurrect: bool) -> Result<()> {
    let mut args = vec!["attach"];
    if force_resurrect {
        args.push("-f"); // Force resurrection of dead session
    }
    args.push(name);

    let status = Command::new("zellij").args(&args).status()?;

    if !status.success() {
        anyhow::bail!("Failed to attach to zellij session: {}", name);
    }
    Ok(())
}

pub fn kill_session(name: &str) -> Result<()> {
    let status = Command::new("zellij")
        .args(["kill-session", name])
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to kill zellij session: {}", name);
    }
    Ok(())
}

pub fn sanitize_session_name(branch: &str) -> String {
    // Convert branch name to valid zellij session name
    // Replace slashes and special chars with dashes
    // Truncate to 36 chars - longer names cause zellij to hang when started via wt -x
    let sanitized: String = branch
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    if sanitized.len() > 36 {
        sanitized[..36].trim_end_matches('-').to_string()
    } else {
        sanitized
    }
}

pub fn session_name_for_branch(branch: &str) -> String {
    sanitize_session_name(branch)
}

pub fn is_zellij_installed() -> bool {
    Command::new("zellij")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi() {
        // Typical zellij output: ^[[32;1msession-name^[[m [Created ...]
        let input = "\x1b[32;1mtmux-detach-test\x1b[m [Created \x1b[35;1m2days\x1b[m ago] (\x1b[31;1mEXITED\x1b[m)";
        let stripped = strip_ansi(input);
        assert_eq!(stripped, "tmux-detach-test [Created 2days ago] (EXITED)");
    }

    #[test]
    fn test_strip_ansi_no_codes() {
        let input = "plain-session [Created 5m ago]";
        assert_eq!(strip_ansi(input), input);
    }

    #[test]
    fn test_session_name_parsing() {
        let line = "\x1b[32;1mmy-feature-branch\x1b[m [Created \x1b[35;1m1h\x1b[m ago] (current)";
        let clean = strip_ansi(line);
        let name = clean.split('[').next().unwrap_or("").trim();
        assert_eq!(name, "my-feature-branch");
    }

    #[test]
    fn test_sanitize_session_name_truncation() {
        let branch = "close-a-claude-code-session-or-zellij-session";
        let sanitized = sanitize_session_name(branch);
        assert!(sanitized.len() <= 36);
        assert_eq!(sanitized, "close-a-claude-code-session-or-zelli");
    }
}
