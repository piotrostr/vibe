use std::process::{Command, ExitCode};

fn project_name() -> Option<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8(output.stdout).ok()?;
    let url = url.trim();
    // git@github.com:user/repo.git or https://github.com/user/repo.git
    let basename = url.rsplit('/').next()?;
    Some(basename.trim_end_matches(".git").to_string())
}

/// Match sanitize_session_name from src/external/zellij.rs:
/// non-alphanumeric chars (except - and _) become -, truncate to 36 chars.
fn sanitize_session_name(name: &str) -> String {
    let sanitized: String = name
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

fn list_zellij_sessions() -> Vec<String> {
    let Ok(output) = Command::new("zellij").args(["list-sessions", "-s"]).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect()
}

/// Full session info (name, age, alive/exited) from zellij list-sessions
fn list_zellij_sessions_full() -> Vec<(String, String, bool)> {
    let Ok(output) = Command::new("zellij").args(["list-sessions"]).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    // Strip ANSI codes
    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    let stripped = strip_ansi(&raw);
    stripped
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let name = line.split_whitespace().next().unwrap_or("").to_string();
            let exited = line.contains("EXITED");
            // Extract the age from [Created ...ago]
            let age = line
                .find("[Created ")
                .and_then(|start| {
                    let rest = &line[start + 9..];
                    rest.find(']').map(|end| rest[..end].trim().to_string())
                })
                .unwrap_or_default();
            (name, age, !exited)
        })
        .collect()
}

fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // skip until we hit a letter (end of ANSI sequence)
            while let Some(&next) = chars.peek() {
                chars.next();
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn cmd_list() -> ExitCode {
    let project = match project_name() {
        Some(p) => p,
        None => {
            eprintln!("could not determine project name from git remote");
            return ExitCode::FAILURE;
        }
    };
    let prime_name = sanitize_session_name(&format!("{}.prime", &project));
    let sessions = list_zellij_sessions_full();

    // Match sessions by full project name OR Linear ticket prefix (first 3 chars).
    // "vibe" matches "vibe-prime" and "VIB-21-...", "reflex" matches "reflex-prime" and "REF-123-..."
    let project_upper = project.to_uppercase();
    let ticket_prefix = format!("{}-", &project_upper[..project_upper.len().min(3)]);
    let cousins: Vec<_> = sessions
        .iter()
        .filter(|(name, _, alive)| {
            let upper = name.to_uppercase();
            *alive && (upper.starts_with(&project_upper) || upper.starts_with(&ticket_prefix))
        })
        .collect();

    if cousins.is_empty() {
        eprintln!("no active cousins for project '{}'", project);
        return ExitCode::SUCCESS;
    }

    for (name, age, _) in &cousins {
        let role = if *name == prime_name {
            " (prime)"
        } else {
            ""
        };
        println!("  {}{} - {}", name, role, age);
    }

    ExitCode::SUCCESS
}

fn resolve_target(target: &str) -> Result<String, String> {
    // "prime" -> sanitize("{project}.prime") -> e.g. "vibe-prime"
    if target == "prime" {
        let project = project_name().ok_or("could not determine project name from git remote")?;
        return Ok(sanitize_session_name(&format!("{}.prime", project)));
    }

    // ticket ID pattern (letters-digits, e.g. AMB-921, VIB-19)
    let is_ticket_id = {
        let parts: Vec<&str> = target.split('-').collect();
        parts.len() == 2
            && parts[0].chars().all(|c| c.is_ascii_alphabetic())
            && parts[1].chars().all(|c| c.is_ascii_digit())
    };

    if is_ticket_id {
        let upper = target.to_uppercase();
        let sessions = list_zellij_sessions();
        let matches: Vec<&String> = sessions
            .iter()
            .filter(|s| s.to_uppercase().contains(&upper))
            .collect();
        return match matches.len() {
            0 => Err(format!("no zellij session matching ticket '{}'", target)),
            1 => Ok(matches[0].clone()),
            _ => Err(format!(
                "ambiguous ticket '{}', matches: {}",
                target,
                matches
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        };
    }

    // raw session name
    Ok(target.to_string())
}

fn send_interrupt(session: &str) -> Result<(), String> {
    // write 3 = Ctrl+C
    let status = Command::new("zellij")
        .args(["-s", session, "action", "write", "3"])
        .status()
        .map_err(|e| format!("failed to run zellij: {}", e))?;
    if !status.success() {
        return Err(format!("zellij write (ctrl-c) failed for session '{}'", session));
    }
    // small delay so the target processes the interrupt before we type
    std::thread::sleep(std::time::Duration::from_millis(200));
    Ok(())
}

fn send_message(session: &str, message: &str) -> Result<(), String> {
    // Collapse newlines to spaces - multiline write-chars inserts literal newlines
    // which submits partial lines before our Enter key fires
    let flat: String = message
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    let status = Command::new("zellij")
        .args(["-s", session, "action", "write-chars", &flat])
        .status()
        .map_err(|e| format!("failed to run zellij: {}", e))?;
    if !status.success() {
        return Err(format!("zellij write-chars failed for session '{}'", session));
    }

    // write 13 = Enter key
    let status = Command::new("zellij")
        .args(["-s", session, "action", "write", "13"])
        .status()
        .map_err(|e| format!("failed to run zellij: {}", e))?;
    if !status.success() {
        return Err(format!("zellij write (enter) failed for session '{}'", session));
    }

    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    let mut urgent = false;
    let mut positional: Vec<&str> = Vec::new();
    for arg in &args[1..] {
        if arg == "--urgent" || arg == "-u" {
            urgent = true;
        } else {
            positional.push(arg);
        }
    }

    if positional.first().copied() == Some("list") {
        return cmd_list();
    }

    if positional.len() < 2 {
        eprintln!("usage: cousin [--urgent] <target> <message...>");
        eprintln!("       cousin list");
        eprintln!("  target: prime | TICKET-ID | session-name");
        eprintln!("  --urgent: send Ctrl+C before message to interrupt target");
        return ExitCode::FAILURE;
    }

    let target = positional[0];
    let message = positional[1..].join(" ");

    let session = match resolve_target(target) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    if urgent
        && let Err(e) = send_interrupt(&session)
    {
        eprintln!("error: {}", e);
        return ExitCode::FAILURE;
    }

    if let Err(e) = send_message(&session, &message) {
        eprintln!("error: {}", e);
        return ExitCode::FAILURE;
    }

    eprintln!("sent to '{}'", session);
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_prime() {
        let result = resolve_target("prime");
        assert!(result.is_ok());
        // vibe.prime -> sanitized to vibe-prime
        assert_eq!(result.unwrap(), "vibe-prime");
    }

    #[test]
    fn test_resolve_raw_session() {
        let result = resolve_target("my-session");
        assert_eq!(result.unwrap(), "my-session");
    }

    #[test]
    fn test_resolve_ticket_no_match() {
        let result = resolve_target("ZZZ-999");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no zellij session"));
    }

    #[test]
    fn test_project_name() {
        let name = project_name();
        assert!(name.is_some());
        assert_eq!(name.unwrap(), "vibe");
    }

    #[test]
    fn test_sanitize_session_name() {
        assert_eq!(sanitize_session_name("vibe.prime"), "vibe-prime");
        assert_eq!(sanitize_session_name("my/branch"), "my-branch");
        assert_eq!(sanitize_session_name("simple"), "simple");
    }

    #[test]
    fn test_sanitize_truncation() {
        let long = "a".repeat(50);
        let result = sanitize_session_name(&long);
        assert!(result.len() <= 36);
    }

    #[test]
    fn test_arg_parsing_urgent_flag() {
        // simulate: cousin --urgent prime hello
        let args = ["cousin", "--urgent", "prime", "hello"];
        let mut urgent = false;
        let mut positional: Vec<&str> = Vec::new();
        for arg in &args[1..] {
            if *arg == "--urgent" || *arg == "-u" {
                urgent = true;
            } else {
                positional.push(arg);
            }
        }
        assert!(urgent);
        assert_eq!(positional, vec!["prime", "hello"]);
    }

    #[test]
    fn test_arg_parsing_no_flag() {
        let args = ["cousin", "prime", "hello", "world"];
        let mut urgent = false;
        let mut positional: Vec<&str> = Vec::new();
        for arg in &args[1..] {
            if *arg == "--urgent" || *arg == "-u" {
                urgent = true;
            } else {
                positional.push(arg);
            }
        }
        assert!(!urgent);
        assert_eq!(positional, vec!["prime", "hello", "world"]);
    }
}
