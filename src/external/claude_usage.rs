use std::process::Command;

/// Count running Claude processes via ps
pub fn count_claude_processes() -> usize {
    let output = Command::new("sh")
        .arg("-c")
        .arg("ps aux | grep -i claude | grep -v grep | wc -l")
        .output();

    match output {
        Ok(out) => {
            let count_str = String::from_utf8_lossy(&out.stdout);
            count_str.trim().parse().unwrap_or(0)
        }
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_claude_processes() {
        // Just verify it doesn't panic and returns a valid count
        let _count = count_claude_processes();
    }
}
