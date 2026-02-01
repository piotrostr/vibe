use serde::{Deserialize, Serialize};

/// Convert task title to a branch name slug.
/// If linear_id is provided, prefixes the branch name with it (e.g., "AMB-67/add-feature").
pub fn task_title_to_branch(title: &str, linear_id: Option<&str>) -> String {
    let slug = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    match linear_id {
        Some(id) => format!("{}/{}", id, slug),
        None => slug,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Backlog,
    Todo,
    Inprogress,
    Inreview,
    Done,
    Cancelled,
}

impl TaskStatus {
    pub const VISIBLE: [TaskStatus; 4] = [
        TaskStatus::Backlog,
        TaskStatus::Inprogress,
        TaskStatus::Inreview,
        TaskStatus::Done,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            TaskStatus::Backlog => "Backlog",
            TaskStatus::Todo => "To Do",
            TaskStatus::Inprogress => "In Progress",
            TaskStatus::Inreview => "In Review",
            TaskStatus::Done => "Done",
            TaskStatus::Cancelled => "Cancelled",
        }
    }

    pub fn column_index(&self) -> usize {
        match self {
            TaskStatus::Backlog => 0,
            TaskStatus::Todo => 0,
            TaskStatus::Inprogress => 1,
            TaskStatus::Inreview => 2,
            TaskStatus::Done => 3,
            TaskStatus::Cancelled => 3,
        }
    }

    pub fn from_column_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(TaskStatus::Backlog),
            1 => Some(TaskStatus::Inprogress),
            2 => Some(TaskStatus::Inreview),
            3 => Some(TaskStatus::Done),
            _ => None,
        }
    }

    /// Convert Linear state type to TaskStatus
    pub fn from_linear_state_type(state_type: &str) -> Self {
        match state_type {
            "backlog" => TaskStatus::Backlog,
            "unstarted" => TaskStatus::Todo,
            "started" => TaskStatus::Inprogress,
            "completed" => TaskStatus::Done,
            "canceled" | "cancelled" => TaskStatus::Cancelled,
            _ => TaskStatus::Backlog,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub parent_workspace_id: Option<String>,
    pub shared_task_id: Option<String>,
    pub linear_issue_id: Option<String>,
    pub linear_url: Option<String>,
    pub linear_labels: Option<String>,
    pub created_at: String,
    pub updated_at: String,

    #[serde(default)]
    pub has_in_progress_attempt: bool,
    #[serde(default)]
    pub last_attempt_failed: bool,
    #[serde(default)]
    pub executor: String,
    pub pr_url: Option<String>,
    pub pr_status: Option<String>,
    pub pr_is_draft: Option<bool>,
    pub pr_review_decision: Option<String>,
    pub pr_checks_status: Option<String>,
    pub pr_has_conflicts: Option<bool>,
}

use crate::external::{BranchPrInfo, LinearIssueStatus};

impl Task {
    pub fn effective_status(&self) -> TaskStatus {
        if let Some(ref pr_status) = self.pr_status {
            match pr_status.as_str() {
                "merged" => return TaskStatus::Done,
                "closed" => return TaskStatus::Cancelled,
                "open" => {
                    if self.pr_is_draft != Some(true) {
                        return TaskStatus::Inreview;
                    }
                }
                _ => {}
            }
        }
        self.status
    }

    pub fn effective_status_with_pr(
        &self,
        branch_pr: Option<&BranchPrInfo>,
        has_worktree: bool,
        linear_status: Option<&LinearIssueStatus>,
    ) -> TaskStatus {
        // Priority 1: Live fetched PR status (most accurate, up-to-date)
        if let Some(pr) = branch_pr {
            match pr.state.as_str() {
                "MERGED" => return TaskStatus::Done,
                "CLOSED" => return TaskStatus::Cancelled,
                "OPEN" => {
                    if !pr.is_draft {
                        return TaskStatus::Inreview;
                    }
                    if has_worktree {
                        return TaskStatus::Inprogress;
                    }
                }
                _ => {}
            }
        }

        // Priority 2: Stored PR status (fallback if no live data)
        if self.pr_status.is_some() {
            return self.effective_status();
        }

        // Priority 3: Linear terminal states (completed/cancelled) override worktree
        if let Some(linear) = linear_status {
            match linear.state_type.as_str() {
                "completed" => return TaskStatus::Done,
                "canceled" => return TaskStatus::Cancelled,
                _ => {}
            }
        }

        // Priority 4: Worktree presence upgrades backlog/unstarted to in-progress
        if has_worktree {
            return TaskStatus::Inprogress;
        }

        // Priority 5: Linear non-terminal status
        if let Some(linear) = linear_status {
            return TaskStatus::from_linear_state_type(&linear.state_type);
        }

        // Priority 6: Local stored status - fallback
        self.status
    }
}

const NUM_VISIBLE_COLUMNS: usize = 4;

pub struct TasksState {
    pub tasks: Vec<Task>,
    pub selected_column: usize,
    pub selected_card_per_column: [usize; NUM_VISIBLE_COLUMNS],
    pub search_filter: String,
}

impl TasksState {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            selected_column: 0,
            selected_card_per_column: [0; NUM_VISIBLE_COLUMNS],
            search_filter: String::new(),
        }
    }

    pub fn set_tasks(&mut self, tasks: Vec<Task>) {
        self.tasks = tasks;
        self.selected_card_per_column = [0; NUM_VISIBLE_COLUMNS];
    }

    pub fn tasks_in_column_with_prs(
        &self,
        status: TaskStatus,
        branch_prs: &std::collections::HashMap<String, BranchPrInfo>,
        worktrees: &[crate::external::WorktreeInfo],
        linear_statuses: &std::collections::HashMap<String, LinearIssueStatus>,
    ) -> Vec<&Task> {
        let column_index = status.column_index();
        self.tasks
            .iter()
            .filter(|t| {
                // Use the same branch derivation as session launch
                let expected_branch = task_title_to_branch(&t.title, t.linear_issue_id.as_deref());

                // Try to find matching worktree
                let matching_branch = worktrees.iter().find(|w| {
                    w.branch == expected_branch
                        || w.branch
                            .to_lowercase()
                            .contains(&expected_branch.to_lowercase())
                        || expected_branch
                            .to_lowercase()
                            .contains(&w.branch.to_lowercase())
                });

                let has_worktree = matching_branch.is_some();

                // Try to find PR info:
                // 1. First via worktree branch name
                // 2. Then via expected branch name (for merged PRs where worktree is deleted)
                // 3. Then search branch_prs for any branch containing the task slug
                let branch_pr = matching_branch
                    .and_then(|wt| branch_prs.get(&wt.branch))
                    .or_else(|| branch_prs.get(&expected_branch))
                    .or_else(|| {
                        // Fallback: search for any PR branch that matches the task slug
                        let task_slug = t.title.to_lowercase().replace(' ', "-");
                        branch_prs.iter().find_map(|(branch, pr)| {
                            let branch_lower = branch.to_lowercase();
                            if branch_lower.contains(&task_slug)
                                || task_slug.contains(&branch_lower)
                            {
                                Some(pr)
                            } else {
                                None
                            }
                        })
                    });

                let linear_status = t
                    .linear_issue_id
                    .as_ref()
                    .and_then(|id| linear_statuses.get(id));
                t.effective_status_with_pr(branch_pr, has_worktree, linear_status)
                    .column_index()
                    == column_index
            })
            .filter(|t| {
                if self.search_filter.is_empty() {
                    return true;
                }
                let query = self.search_filter.to_lowercase();
                t.title.to_lowercase().contains(&query)
                    || t.description
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&query))
            })
            .collect()
    }

    pub fn selected_task_with_prs(
        &self,
        branch_prs: &std::collections::HashMap<String, BranchPrInfo>,
        worktrees: &[crate::external::WorktreeInfo],
        linear_statuses: &std::collections::HashMap<String, LinearIssueStatus>,
    ) -> Option<&Task> {
        let status = TaskStatus::from_column_index(self.selected_column)?;
        let tasks = self.tasks_in_column_with_prs(status, branch_prs, worktrees, linear_statuses);
        let card_index = self.selected_card_per_column[self.selected_column];
        tasks.get(card_index).copied()
    }

    pub fn select_next_card_with_prs(
        &mut self,
        branch_prs: &std::collections::HashMap<String, BranchPrInfo>,
        worktrees: &[crate::external::WorktreeInfo],
        linear_statuses: &std::collections::HashMap<String, LinearIssueStatus>,
    ) {
        if let Some(status) = TaskStatus::from_column_index(self.selected_column) {
            let count = self
                .tasks_in_column_with_prs(status, branch_prs, worktrees, linear_statuses)
                .len();
            if count > 0 {
                let current = self.selected_card_per_column[self.selected_column];
                if current + 1 >= count {
                    // At the last card - move to next row
                    self.select_next_column();
                } else {
                    self.selected_card_per_column[self.selected_column] = current + 1;
                }
            } else {
                // Empty row - move to next row
                self.select_next_column();
            }
        }
    }

    pub fn select_prev_card_with_prs(
        &mut self,
        branch_prs: &std::collections::HashMap<String, BranchPrInfo>,
        worktrees: &[crate::external::WorktreeInfo],
        linear_statuses: &std::collections::HashMap<String, LinearIssueStatus>,
    ) {
        if let Some(status) = TaskStatus::from_column_index(self.selected_column) {
            let count = self
                .tasks_in_column_with_prs(status, branch_prs, worktrees, linear_statuses)
                .len();
            if count > 0 {
                let current = self.selected_card_per_column[self.selected_column];
                if current == 0 {
                    // At the first card - move to previous row and select last card
                    self.select_prev_column();
                    // Select last card in new row
                    if let Some(new_status) = TaskStatus::from_column_index(self.selected_column) {
                        let new_count = self
                            .tasks_in_column_with_prs(
                                new_status,
                                branch_prs,
                                worktrees,
                                linear_statuses,
                            )
                            .len();
                        if new_count > 0 {
                            self.selected_card_per_column[self.selected_column] = new_count - 1;
                        }
                    }
                } else {
                    self.selected_card_per_column[self.selected_column] = current - 1;
                }
            } else {
                // Empty row - move to previous row
                self.select_prev_column();
            }
        }
    }

    pub fn select_next_column(&mut self) {
        self.selected_column = (self.selected_column + 1) % NUM_VISIBLE_COLUMNS;
    }

    pub fn select_prev_column(&mut self) {
        self.selected_column = if self.selected_column == 0 {
            NUM_VISIBLE_COLUMNS - 1
        } else {
            self.selected_column - 1
        };
    }
}

impl Default for TasksState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(status: TaskStatus) -> Task {
        Task {
            id: "test-id".to_string(),
            project_id: "test-project".to_string(),
            title: "Test Task".to_string(),
            description: None,
            status,
            parent_workspace_id: None,
            shared_task_id: None,
            linear_issue_id: None,
            linear_url: None,
            linear_labels: None,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
            has_in_progress_attempt: false,
            last_attempt_failed: false,
            executor: String::new(),
            pr_url: None,
            pr_status: None,
            pr_is_draft: None,
            pr_review_decision: None,
            pr_checks_status: None,
            pr_has_conflicts: None,
        }
    }

    #[test]
    fn test_effective_status_no_pr() {
        let task = make_task(TaskStatus::Inprogress);
        assert_eq!(task.effective_status(), TaskStatus::Inprogress);
    }

    #[test]
    fn test_effective_status_pr_open() {
        let mut task = make_task(TaskStatus::Inprogress);
        task.pr_url = Some("https://github.com/org/repo/pull/1".to_string());
        task.pr_status = Some("open".to_string());
        task.pr_is_draft = Some(false);
        assert_eq!(task.effective_status(), TaskStatus::Inreview);
    }

    #[test]
    fn test_effective_status_pr_draft() {
        let mut task = make_task(TaskStatus::Inprogress);
        task.pr_url = Some("https://github.com/org/repo/pull/1".to_string());
        task.pr_status = Some("open".to_string());
        task.pr_is_draft = Some(true);
        assert_eq!(task.effective_status(), TaskStatus::Inprogress);
    }

    #[test]
    fn test_effective_status_pr_merged() {
        let mut task = make_task(TaskStatus::Inprogress);
        task.pr_url = Some("https://github.com/org/repo/pull/1".to_string());
        task.pr_status = Some("merged".to_string());
        assert_eq!(task.effective_status(), TaskStatus::Done);
    }

    #[test]
    fn test_effective_status_pr_closed() {
        let mut task = make_task(TaskStatus::Inprogress);
        task.pr_url = Some("https://github.com/org/repo/pull/1".to_string());
        task.pr_status = Some("closed".to_string());
        assert_eq!(task.effective_status(), TaskStatus::Cancelled);
    }

    #[test]
    fn test_tasks_in_column_with_pr_transitions() {
        let mut state = TasksState::new();

        let mut task1 = make_task(TaskStatus::Inprogress);
        task1.id = "task1".to_string();

        let mut task2 = make_task(TaskStatus::Inprogress);
        task2.id = "task2".to_string();
        task2.pr_status = Some("open".to_string());
        task2.pr_is_draft = Some(false);

        let mut task3 = make_task(TaskStatus::Inprogress);
        task3.id = "task3".to_string();
        task3.pr_status = Some("merged".to_string());

        state.set_tasks(vec![task1, task2, task3]);

        let empty_prs = std::collections::HashMap::new();
        let empty_wt: Vec<crate::external::WorktreeInfo> = vec![];
        let empty_linear: std::collections::HashMap<String, LinearIssueStatus> =
            std::collections::HashMap::new();

        let in_progress = state.tasks_in_column_with_prs(
            TaskStatus::Inprogress,
            &empty_prs,
            &empty_wt,
            &empty_linear,
        );
        assert_eq!(in_progress.len(), 1);
        assert_eq!(in_progress[0].id, "task1");

        let in_review = state.tasks_in_column_with_prs(
            TaskStatus::Inreview,
            &empty_prs,
            &empty_wt,
            &empty_linear,
        );
        assert_eq!(in_review.len(), 1);
        assert_eq!(in_review[0].id, "task2");

        let done =
            state.tasks_in_column_with_prs(TaskStatus::Done, &empty_prs, &empty_wt, &empty_linear);
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].id, "task3");
    }

    #[test]
    fn test_from_linear_state_type() {
        assert_eq!(
            TaskStatus::from_linear_state_type("backlog"),
            TaskStatus::Backlog
        );
        assert_eq!(
            TaskStatus::from_linear_state_type("unstarted"),
            TaskStatus::Todo
        );
        assert_eq!(
            TaskStatus::from_linear_state_type("started"),
            TaskStatus::Inprogress
        );
        assert_eq!(
            TaskStatus::from_linear_state_type("completed"),
            TaskStatus::Done
        );
        assert_eq!(
            TaskStatus::from_linear_state_type("canceled"),
            TaskStatus::Cancelled
        );
        assert_eq!(
            TaskStatus::from_linear_state_type("cancelled"),
            TaskStatus::Cancelled
        );
        assert_eq!(
            TaskStatus::from_linear_state_type("unknown"),
            TaskStatus::Backlog
        );
    }

    #[test]
    fn test_effective_status_with_linear() {
        let mut task = make_task(TaskStatus::Backlog);
        task.linear_issue_id = Some("VIB-6".to_string());

        // Without Linear status, should return local status
        assert_eq!(
            task.effective_status_with_pr(None, false, None),
            TaskStatus::Backlog
        );

        // With Linear status "started", should return InProgress
        let linear_status = LinearIssueStatus {
            identifier: "VIB-6".to_string(),
            state_type: "started".to_string(),
            state_name: "In Progress".to_string(),
        };
        assert_eq!(
            task.effective_status_with_pr(None, false, Some(&linear_status)),
            TaskStatus::Inprogress
        );

        // With Linear status "completed", should return Done
        let linear_done = LinearIssueStatus {
            identifier: "VIB-6".to_string(),
            state_type: "completed".to_string(),
            state_name: "Done".to_string(),
        };
        assert_eq!(
            task.effective_status_with_pr(None, false, Some(&linear_done)),
            TaskStatus::Done
        );
    }

    #[test]
    fn test_linear_terminal_overrides_worktree() {
        let mut task = make_task(TaskStatus::Backlog);
        task.linear_issue_id = Some("VIB-6".to_string());

        // Linear says "completed" - should win over worktree presence
        let linear_done = LinearIssueStatus {
            identifier: "VIB-6".to_string(),
            state_type: "completed".to_string(),
            state_name: "Done".to_string(),
        };
        assert_eq!(
            task.effective_status_with_pr(None, true, Some(&linear_done)),
            TaskStatus::Done
        );

        // Linear says "canceled" - should win over worktree presence
        let linear_cancelled = LinearIssueStatus {
            identifier: "VIB-6".to_string(),
            state_type: "canceled".to_string(),
            state_name: "Cancelled".to_string(),
        };
        assert_eq!(
            task.effective_status_with_pr(None, true, Some(&linear_cancelled)),
            TaskStatus::Cancelled
        );
    }

    #[test]
    fn test_worktree_upgrades_backlog_to_inprogress() {
        let mut task = make_task(TaskStatus::Backlog);
        task.linear_issue_id = Some("VIB-6".to_string());

        // Linear says "backlog" but worktree exists - worktree upgrades to in-progress
        let linear_backlog = LinearIssueStatus {
            identifier: "VIB-6".to_string(),
            state_type: "backlog".to_string(),
            state_name: "Backlog".to_string(),
        };
        assert_eq!(
            task.effective_status_with_pr(None, true, Some(&linear_backlog)),
            TaskStatus::Inprogress
        );

        // Linear says "unstarted" but worktree exists - worktree upgrades to in-progress
        let linear_unstarted = LinearIssueStatus {
            identifier: "VIB-6".to_string(),
            state_type: "unstarted".to_string(),
            state_name: "Todo".to_string(),
        };
        assert_eq!(
            task.effective_status_with_pr(None, true, Some(&linear_unstarted)),
            TaskStatus::Inprogress
        );
    }

    #[test]
    fn test_task_title_to_branch_without_linear_id() {
        assert_eq!(task_title_to_branch("Hello World", None), "hello-world");
        assert_eq!(
            task_title_to_branch("Add feature: user auth", None),
            "add-feature-user-auth"
        );
        assert_eq!(task_title_to_branch("Fix bug #123", None), "fix-bug-123");
        assert_eq!(
            task_title_to_branch("  Multiple   Spaces  ", None),
            "multiple-spaces"
        );
    }

    #[test]
    fn test_task_title_to_branch_with_linear_id() {
        assert_eq!(
            task_title_to_branch("Add some feature", Some("AMB-67")),
            "AMB-67/add-some-feature"
        );
        assert_eq!(
            task_title_to_branch("Fix the bug", Some("TEAM-123")),
            "TEAM-123/fix-the-bug"
        );
    }

    #[test]
    fn test_merged_pr_found_without_worktree() {
        use std::collections::HashMap;

        let mut state = TasksState::new();

        // Task with matching branch name
        let mut task = make_task(TaskStatus::Inprogress);
        task.id = "task1".to_string();
        task.title = "batch pr status queries".to_string();

        state.set_tasks(vec![task]);

        // PR exists for the branch (MERGED), but no worktree
        let mut branch_prs = HashMap::new();
        branch_prs.insert(
            "batch-pr-status-queries".to_string(),
            BranchPrInfo {
                _number: 12,
                url: "https://github.com/test/repo/pull/12".to_string(),
                state: "MERGED".to_string(),
                is_draft: false,
                review_decision: None,
                status_check_rollup: None,
                mergeable: None,
                reviews: vec![],
            },
        );

        let empty_wt: Vec<crate::external::WorktreeInfo> = vec![];
        let empty_linear: HashMap<String, LinearIssueStatus> = HashMap::new();

        // Task should appear in Done column (not In Progress) due to merged PR
        let in_progress = state.tasks_in_column_with_prs(
            TaskStatus::Inprogress,
            &branch_prs,
            &empty_wt,
            &empty_linear,
        );
        assert_eq!(
            in_progress.len(),
            0,
            "Merged PR task should not be in In Progress"
        );

        let done =
            state.tasks_in_column_with_prs(TaskStatus::Done, &branch_prs, &empty_wt, &empty_linear);
        assert_eq!(done.len(), 1, "Merged PR task should be in Done");
        assert_eq!(done[0].id, "task1");
    }
}
