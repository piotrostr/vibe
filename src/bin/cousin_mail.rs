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

fn resolve_target(target: &str) -> Result<String, String> {
    // "prime" -> {project}.prime
    if target == "prime" {
        let project = project_name().ok_or("could not determine project name from git remote")?;
        return Ok(format!("{}.prime", project));
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

fn send_message(session: &str, message: &str) -> Result<(), String> {
    let status = Command::new("zellij")
        .args(["-s", session, "action", "write-chars", message])
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
    if args.len() < 3 {
        eprintln!("usage: cousin-mail <target> <message...>");
        eprintln!("  target: prime | TICKET-ID | session-name");
        return ExitCode::FAILURE;
    }

    let target = &args[1];
    let message = args[2..].join(" ");

    let session = match resolve_target(target) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::FAILURE;
        }
    };

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
        // depends on being in a git repo, which we are during cargo test
        let result = resolve_target("prime");
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with(".prime"));
    }

    #[test]
    fn test_resolve_raw_session() {
        let result = resolve_target("my-session");
        assert_eq!(result.unwrap(), "my-session");
    }

    #[test]
    fn test_resolve_ticket_no_match() {
        // no zellij session will match a random ticket
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
}
