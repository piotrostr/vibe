use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use serde::Deserialize;

/// Reads Claude Code plans from session files.
///
/// Claude Code stores plans in `~/.claude/plans/{slug}.md` where `slug` is the session slug
/// from session JSONL files at `~/.claude/projects/{sanitized-path}/`.
pub struct ClaudePlanReader {
    projects_dir: PathBuf,
    plans_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
struct SessionEntry {
    #[serde(rename = "gitBranch")]
    git_branch: Option<String>,
    /// Session slug used to derive plan file path
    slug: Option<String>,
    /// Direct plan file path (legacy or agent plans)
    #[serde(rename = "planFilePath")]
    plan_file_path: Option<String>,
}

impl ClaudePlanReader {
    pub fn new() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let projects_dir = home.join(".claude").join("projects");
        let plans_dir = home.join(".claude").join("plans");

        Self {
            projects_dir,
            plans_dir,
        }
    }

    /// Find the plan for a specific branch in a project.
    pub fn find_plan_for_branch(&self, project_path: &str, branch: &str) -> Option<String> {
        let plan_path = self.find_plan_path_for_branch(project_path, branch)?;
        self.read_plan_file(&plan_path)
    }

    /// Check if a plan exists for a specific branch without reading its content.
    pub fn has_plan_for_branch(&self, project_path: &str, branch: &str) -> bool {
        self.find_plan_path_for_branch(project_path, branch)
            .map(|p| PathBuf::from(&p).exists())
            .unwrap_or(false)
    }

    /// Find the plan file path for a specific branch in a project.
    pub fn find_plan_path_for_branch(&self, project_path: &str, branch: &str) -> Option<String> {
        let sanitized = sanitize_project_path(project_path);
        let project_dir = self.projects_dir.join(&sanitized);

        if !project_dir.exists() {
            return None;
        }

        let Ok(entries) = fs::read_dir(&project_dir) else {
            return None;
        };

        // Collect session files with their modification times for sorting
        let mut session_files: Vec<_> = entries
            .flatten()
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "jsonl")
                    .unwrap_or(false)
            })
            .filter_map(|e| {
                let metadata = e.metadata().ok()?;
                let modified = metadata.modified().ok()?;
                Some((e.path(), modified))
            })
            .collect();

        // Sort by modification time, newest first
        session_files.sort_by(|a, b| b.1.cmp(&a.1));

        // Check sessions from newest to oldest
        for (path, _) in session_files {
            if let Some((session_branch, plan_path)) = self.extract_plan_from_session(&path)
                && session_branch == branch
            {
                return Some(plan_path);
            }
        }

        None
    }

    /// Extract branch and plan path from a session JSONL file.
    /// Returns the last entry that has both a branch and a plan path (either explicit or derived from slug).
    fn extract_plan_from_session(&self, path: &PathBuf) -> Option<(String, String)> {
        let file = fs::File::open(path).ok()?;
        let reader = BufReader::new(file);

        let mut result: Option<(String, String)> = None;
        let mut session_slug: Option<String> = None;
        let mut session_branch: Option<String> = None;

        for line in reader.lines().map_while(Result::ok) {
            if let Ok(entry) = serde_json::from_str::<SessionEntry>(&line) {
                // Track branch
                if let Some(branch) = entry.git_branch
                    && !branch.is_empty()
                {
                    session_branch = Some(branch);
                }

                // Track slug for deriving plan path
                if let Some(slug) = entry.slug
                    && !slug.is_empty()
                {
                    session_slug = Some(slug);
                }

                // Direct plan file path takes precedence
                if let Some(plan_path) = entry.plan_file_path
                    && !plan_path.is_empty()
                    && let Some(ref branch) = session_branch
                {
                    result = Some((branch.clone(), plan_path));
                }
            }
        }

        // If no explicit plan path but we have a slug, derive the plan path
        if result.is_none()
            && let (Some(branch), Some(slug)) = (session_branch, session_slug)
        {
            let plan_path = self.plans_dir.join(format!("{}.md", slug));
            if plan_path.exists() {
                return Some((branch, plan_path.to_string_lossy().to_string()));
            }
        }

        result
    }

    /// Read the content of a plan file.
    fn read_plan_file(&self, path: &str) -> Option<String> {
        let plan_path = PathBuf::from(path);
        if plan_path.exists() {
            fs::read_to_string(&plan_path).ok()
        } else {
            None
        }
    }
}

impl Default for ClaudePlanReader {
    fn default() -> Self {
        Self::new()
    }
}

/// Sanitize a project path to match Claude Code's directory naming.
/// Claude replaces path separators and dots with dashes.
fn sanitize_project_path(path: &str) -> String {
    path.replace(['/', '.'], "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_project_path() {
        assert_eq!(
            sanitize_project_path("/Users/test/my-project"),
            "-Users-test-my-project"
        );
        assert_eq!(
            sanitize_project_path("/home/user/code/app"),
            "-home-user-code-app"
        );
        // Dots are also replaced with dashes (worktree paths like vibe.branch-name)
        assert_eq!(
            sanitize_project_path("/Users/piotrostr/vibe.some-branch"),
            "-Users-piotrostr-vibe-some-branch"
        );
    }

    #[test]
    fn test_reader_creation() {
        let reader = ClaudePlanReader::new();
        assert!(reader.projects_dir.to_string_lossy().contains(".claude"));
    }

    /// Manual test to verify plan loading works on local machine.
    /// Run with: cargo test test_find_plan_for_current_session -- --ignored --nocapture
    #[test]
    #[ignore]
    fn test_find_plan_for_current_session() {
        let reader = ClaudePlanReader::new();

        // This test uses the worktree path for this branch
        let worktree_path =
            "/Users/piotrostr/vibe.take-the-plan-from-claude-session-and-display-in-task-view";
        let branch = "take-the-plan-from-claude-session-and-display-in-task-view";

        println!("Looking for plan...");
        println!("  Worktree path: {}", worktree_path);
        println!("  Branch: {}", branch);

        // Check the sanitized project dir exists
        let sanitized = sanitize_project_path(worktree_path);
        let project_dir = reader.projects_dir.join(&sanitized);
        println!("  Project dir: {:?}", project_dir);
        println!("  Project dir exists: {}", project_dir.exists());

        // List session files
        if project_dir.exists() {
            println!("\nSession files:");
            for entry in std::fs::read_dir(&project_dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                    println!("  {:?}", path.file_name().unwrap());

                    // Try to extract plan info from this session
                    if let Some((session_branch, plan_path)) =
                        reader.extract_plan_from_session(&path)
                    {
                        println!("    Branch: {}", session_branch);
                        println!("    Plan path: {}", plan_path);
                        println!("    Plan exists: {}", PathBuf::from(&plan_path).exists());
                    }
                }
            }
        }

        // Try to find the plan
        let plan = reader.find_plan_for_branch(worktree_path, branch);
        println!("\nResult:");
        println!("  Plan found: {}", plan.is_some());
        if let Some(content) = &plan {
            println!("  Plan length: {} chars", content.len());
            println!("  First 200 chars:\n{}", &content[..content.len().min(200)]);
        }

        assert!(plan.is_some(), "Should find a plan for the current session");
    }
}
