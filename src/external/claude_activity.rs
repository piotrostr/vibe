use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use super::ClaudeActivityState;

// Thresholds for activity detection
// Claude statusline updates on events, not continuously, so use generous thresholds
const WAITING_THRESHOLD_SECS: u64 = 30; // Recent activity, waiting for user
const IDLE_THRESHOLD_SECS: u64 = 300; // 5 minutes - no updates for this long = idle

#[derive(Debug, Deserialize)]
struct ClaudeStatusFile {
    working_dir: String,
    #[serde(default)]
    #[allow(dead_code)] // Captured for future correlation
    session_id: Option<String>,
    #[allow(dead_code)] // Kept for debugging
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    #[serde(default)]
    used_percentage: Option<f64>,
    #[serde(default)]
    api_duration_ms: Option<u64>,
    timestamp: u64,
}

#[derive(Debug, Clone)]
struct ActivitySnapshot {
    api_duration_ms: Option<u64>,
    output_tokens: Option<u64>,
}

/// Result of activity detection, includes state and context usage
#[derive(Debug, Clone)]
pub struct ActivityResult {
    pub state: ClaudeActivityState,
    pub context_percentage: Option<f64>,
}

pub struct ClaudeActivityTracker {
    state_dir: PathBuf,
    previous_snapshots: HashMap<String, ActivitySnapshot>,
}

impl ClaudeActivityTracker {
    pub fn new() -> Self {
        let state_dir = dirs::home_dir()
            .map(|h| h.join(".vibe").join("claude-activity"))
            .unwrap_or_else(|| PathBuf::from("/tmp/claude-activity"));

        Self {
            state_dir,
            previous_snapshots: HashMap::new(),
        }
    }

    pub fn get_activity_for_session(&mut self, session_name: &str) -> ActivityResult {
        // Try to find a status file that matches this session name
        // The status file is named by MD5 hash of the working directory
        // We need to scan all files and match by session name in the working_dir

        let Ok(entries) = fs::read_dir(&self.state_dir) else {
            return ActivityResult {
                state: ClaudeActivityState::Unknown,
                context_percentage: None,
            };
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false)
                && let Ok(content) = fs::read_to_string(&path)
                && let Ok(status) = serde_json::from_str::<ClaudeStatusFile>(&content)
                && self.session_matches_working_dir(session_name, &status.working_dir)
            {
                return self.determine_state(&status);
            }
        }

        ActivityResult {
            state: ClaudeActivityState::Unknown,
            context_percentage: None,
        }
    }

    fn session_matches_working_dir(&self, session_name: &str, working_dir: &str) -> bool {
        // Session names are sanitized versions of branch names
        // Check if the working directory path contains the session name
        let normalized_session = session_name.to_lowercase();
        let normalized_dir = working_dir.to_lowercase();

        // Check if the directory ends with the session name (worktree scenario)
        if let Some(last_component) = working_dir.split('/').next_back()
            && last_component.to_lowercase() == normalized_session
        {
            return true;
        }

        // Check if the session name is contained in the directory path
        normalized_dir.contains(&normalized_session)
    }

    fn determine_state(&mut self, status: &ClaudeStatusFile) -> ActivityResult {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let age_secs = now.saturating_sub(status.timestamp);

        // Check if status is stale (>30s old) - definitely idle
        if age_secs > IDLE_THRESHOLD_SECS {
            return ActivityResult {
                state: ClaudeActivityState::Idle,
                context_percentage: status.used_percentage,
            };
        }

        let current_snapshot = ActivitySnapshot {
            api_duration_ms: status.api_duration_ms,
            output_tokens: status.output_tokens,
        };

        // Check against previous snapshot
        let state = if let Some(prev) = self.previous_snapshots.get(&status.working_dir) {
            // Primary signal: api_duration_ms increasing means Claude is making API calls
            let api_duration_changed = match (prev.api_duration_ms, current_snapshot.api_duration_ms)
            {
                (Some(prev_dur), Some(curr_dur)) => curr_dur > prev_dur,
                (None, Some(_)) => true, // Started making calls
                _ => false,
            };

            // Secondary signal: output tokens increasing
            let tokens_changed = match (prev.output_tokens, current_snapshot.output_tokens) {
                (Some(prev_tok), Some(curr_tok)) => curr_tok > prev_tok,
                (None, Some(_)) => true,
                _ => false,
            };

            if api_duration_changed || tokens_changed {
                // Actively working
                ClaudeActivityState::Thinking
            } else if age_secs < WAITING_THRESHOLD_SECS {
                // Recent activity but stable - waiting for user
                ClaudeActivityState::WaitingForUser
            } else {
                // Stable for a while, getting stale
                ClaudeActivityState::Idle
            }
        } else {
            // First time seeing this session
            if age_secs < WAITING_THRESHOLD_SECS {
                // Fresh data, assume waiting for user (just started or just finished)
                ClaudeActivityState::WaitingForUser
            } else {
                ClaudeActivityState::Idle
            }
        };

        // Update snapshot
        self.previous_snapshots
            .insert(status.working_dir.clone(), current_snapshot);

        ActivityResult {
            state,
            context_percentage: status.used_percentage,
        }
    }

    pub fn update_sessions(&mut self, sessions: &mut [super::ZellijSession]) {
        for session in sessions.iter_mut() {
            let result = self.get_activity_for_session(&session.name);
            session.claude_activity = result.state;
            session.context_percentage = result.context_percentage;
        }
    }
}

impl Default for ClaudeActivityTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
fn hash_working_dir(working_dir: &str) -> String {
    format!("{:x}", md5::compute(working_dir.as_bytes()))
        .chars()
        .take(16)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_hash_working_dir() {
        let hash = hash_working_dir("/Users/test/project");
        assert_eq!(hash.len(), 16);
    }

    #[test]
    fn test_session_matches_working_dir() {
        let tracker = ClaudeActivityTracker::new();

        // Exact match at end of path
        assert!(
            tracker.session_matches_working_dir(
                "feature-branch",
                "/Users/test/worktrees/feature-branch"
            )
        );

        // Session name contained in path
        assert!(
            tracker.session_matches_working_dir("my-feature", "/Users/test/my-feature-worktree")
        );

        // Case insensitive
        assert!(
            tracker.session_matches_working_dir("Feature-Branch", "/users/test/feature-branch")
        );

        // No match
        assert!(!tracker.session_matches_working_dir("other-branch", "/Users/test/feature-branch"));
    }

    #[test]
    fn test_parse_status_file_new_format() {
        let json = r#"{
            "working_dir": "/Users/test/my-project",
            "session_id": "abc-123-def",
            "input_tokens": 1500,
            "output_tokens": 500,
            "used_percentage": 25.5,
            "api_duration_ms": 45000,
            "timestamp": 1700000000
        }"#;

        let status: ClaudeStatusFile = serde_json::from_str(json).unwrap();
        assert_eq!(status.working_dir, "/Users/test/my-project");
        assert_eq!(status.session_id, Some("abc-123-def".to_string()));
        assert_eq!(status.input_tokens, Some(1500));
        assert_eq!(status.output_tokens, Some(500));
        assert_eq!(status.used_percentage, Some(25.5));
        assert_eq!(status.api_duration_ms, Some(45000));
        assert_eq!(status.timestamp, 1700000000);
    }

    #[test]
    fn test_parse_status_file_old_format() {
        // Old format without new fields - should still parse with defaults
        let json = r#"{
            "working_dir": "/Users/test/my-project",
            "input_tokens": 1500,
            "output_tokens": 500,
            "timestamp": 1700000000
        }"#;

        let status: ClaudeStatusFile = serde_json::from_str(json).unwrap();
        assert_eq!(status.working_dir, "/Users/test/my-project");
        assert_eq!(status.session_id, None);
        assert_eq!(status.used_percentage, None);
        assert_eq!(status.api_duration_ms, None);
    }

    #[test]
    fn test_parse_status_file_null_values() {
        // JSON with explicit null values (as produced by jq when fields missing)
        let json = r#"{
            "working_dir": "/Users/test/my-project",
            "session_id": "",
            "input_tokens": null,
            "output_tokens": null,
            "used_percentage": null,
            "api_duration_ms": null,
            "timestamp": 1700000000
        }"#;

        let status: ClaudeStatusFile = serde_json::from_str(json).unwrap();
        assert_eq!(status.input_tokens, None);
        assert_eq!(status.output_tokens, None);
        assert_eq!(status.used_percentage, None);
        assert_eq!(status.api_duration_ms, None);
    }

    #[test]
    fn test_activity_state_thinking_api_duration_increasing() {
        let mut tracker = ClaudeActivityTracker::new();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // First snapshot
        let status1 = ClaudeStatusFile {
            working_dir: "/test/project".to_string(),
            session_id: Some("test-session".to_string()),
            input_tokens: Some(100),
            output_tokens: Some(50),
            used_percentage: Some(10.0),
            api_duration_ms: Some(1000),
            timestamp: now,
        };
        let result1 = tracker.determine_state(&status1);
        // First time seeing - should be WaitingForUser since fresh
        assert_eq!(result1.state, ClaudeActivityState::WaitingForUser);

        // Second snapshot with increased api_duration_ms
        let status2 = ClaudeStatusFile {
            working_dir: "/test/project".to_string(),
            session_id: Some("test-session".to_string()),
            input_tokens: Some(100),
            output_tokens: Some(50),
            used_percentage: Some(10.0),
            api_duration_ms: Some(2000), // Increased!
            timestamp: now,
        };
        let result2 = tracker.determine_state(&status2);
        assert_eq!(result2.state, ClaudeActivityState::Thinking);
    }

    #[test]
    fn test_activity_state_waiting_stable() {
        let mut tracker = ClaudeActivityTracker::new();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // First snapshot
        let status1 = ClaudeStatusFile {
            working_dir: "/test/project".to_string(),
            session_id: None,
            input_tokens: Some(100),
            output_tokens: Some(50),
            used_percentage: Some(10.0),
            api_duration_ms: Some(1000),
            timestamp: now,
        };
        tracker.determine_state(&status1);

        // Second snapshot - same values, recent timestamp
        let status2 = ClaudeStatusFile {
            working_dir: "/test/project".to_string(),
            session_id: None,
            input_tokens: Some(100),
            output_tokens: Some(50),
            used_percentage: Some(10.0),
            api_duration_ms: Some(1000), // Same
            timestamp: now,
        };
        let result = tracker.determine_state(&status2);
        assert_eq!(result.state, ClaudeActivityState::WaitingForUser);
    }

    #[test]
    fn test_activity_state_idle_stale() {
        let mut tracker = ClaudeActivityTracker::new();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Stale timestamp (>5 minutes old)
        let status = ClaudeStatusFile {
            working_dir: "/test/project".to_string(),
            session_id: None,
            input_tokens: Some(100),
            output_tokens: Some(50),
            used_percentage: Some(10.0),
            api_duration_ms: Some(1000),
            timestamp: now - 600, // 10 minutes ago
        };
        let result = tracker.determine_state(&status);
        assert_eq!(result.state, ClaudeActivityState::Idle);
    }

    #[test]
    fn test_context_percentage_returned() {
        let mut tracker = ClaudeActivityTracker::new();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let status = ClaudeStatusFile {
            working_dir: "/test/project".to_string(),
            session_id: None,
            input_tokens: Some(100),
            output_tokens: Some(50),
            used_percentage: Some(75.5),
            api_duration_ms: Some(1000),
            timestamp: now,
        };
        let result = tracker.determine_state(&status);
        assert_eq!(result.context_percentage, Some(75.5));
    }

    #[test]
    #[ignore] // Run with: cargo test test_read_live_activity -- --ignored
    fn test_read_live_activity_file() {
        // This test reads from the actual activity directory
        // Use the current worktree as a test case
        let working_dir = "/Users/piotrostr/vibe.close-a-claude-code-session-or-zellij-session";
        let expected_hash = "cb7e56a8e4fc216c";

        // Verify hash calculation
        let hash = hash_working_dir(working_dir);
        assert_eq!(hash, expected_hash);

        // Try to read the activity file
        let state_dir = dirs::home_dir()
            .unwrap()
            .join(".vibe")
            .join("claude-activity");
        let file_path = state_dir.join(format!("{}.json", expected_hash));

        if file_path.exists() {
            let content = fs::read_to_string(&file_path).unwrap();
            println!("Activity file content:\n{}", content);

            let status: ClaudeStatusFile = serde_json::from_str(&content).unwrap();
            println!("Parsed status: {:?}", status);

            assert_eq!(status.working_dir, working_dir);
            // New fields should be present if statusline was updated
            println!("session_id: {:?}", status.session_id);
            println!("used_percentage: {:?}", status.used_percentage);
            println!("api_duration_ms: {:?}", status.api_duration_ms);
        } else {
            println!(
                "Activity file not found at {:?} - start a Claude session in this worktree first",
                file_path
            );
        }
    }

    #[test]
    #[ignore] // Run with: cargo test test_tracker_with_live_session -- --ignored
    fn test_tracker_with_live_session() {
        // Test the full tracker flow with a live session
        let mut tracker = ClaudeActivityTracker::new();

        // The session name would be derived from the branch
        let session_name = "close-a-claude-code-session-or-zellij-session";
        let result = tracker.get_activity_for_session(session_name);

        println!("Activity state: {:?}", result.state);
        println!("Context percentage: {:?}", result.context_percentage);

        // If we found a matching session, we should get a known state
        assert!(
            matches!(
                result.state,
                ClaudeActivityState::Thinking
                    | ClaudeActivityState::WaitingForUser
                    | ClaudeActivityState::Idle
                    | ClaudeActivityState::Unknown
            ),
            "Expected a valid activity state"
        );
    }
}
