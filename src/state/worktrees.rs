use std::collections::HashMap;
use std::time::Instant;

use crate::external::{BranchPrInfo, WorktreeInfo};

/// How long to cache "no PR" results before re-checking
const NO_PR_CACHE_TTL_SECS: u64 = 120;

pub struct WorktreesState {
    pub worktrees: Vec<WorktreeInfo>,
    pub selected_index: usize,
    pub loading: bool,
    pub error: Option<String>,
    pub branch_prs: HashMap<String, BranchPrInfo>,
    /// Branches we've checked that have no PR, with timestamp of last check
    no_pr_cache: HashMap<String, Instant>,
}

impl WorktreesState {
    pub fn new() -> Self {
        Self {
            worktrees: Vec::new(),
            selected_index: 0,
            loading: false,
            error: None,
            branch_prs: HashMap::new(),
            no_pr_cache: HashMap::new(),
        }
    }

    pub fn pr_for_branch(&self, branch: &str) -> Option<&BranchPrInfo> {
        self.branch_prs.get(branch)
    }

    pub fn set_branch_pr(&mut self, branch: String, pr_info: BranchPrInfo) {
        // Clear from no-PR cache if we found a PR
        self.no_pr_cache.remove(&branch);
        self.branch_prs.insert(branch, pr_info);
    }

    pub fn clear_branch_pr(&mut self, branch: &str) {
        self.branch_prs.remove(branch);
    }

    /// Mark a branch as having no PR (cache this to avoid repeated lookups)
    pub fn mark_no_pr(&mut self, branch: String) {
        self.no_pr_cache.insert(branch, Instant::now());
    }

    /// Check if a branch is in the "no PR" cache and still valid
    pub fn is_cached_no_pr(&self, branch: &str) -> bool {
        if let Some(checked_at) = self.no_pr_cache.get(branch) {
            checked_at.elapsed().as_secs() < NO_PR_CACHE_TTL_SECS
        } else {
            false
        }
    }

    /// Get branches that need PR lookup (have worktree, not in PR map, not in no-PR cache)
    pub fn branches_needing_pr_lookup(&self) -> Vec<String> {
        self.worktrees
            .iter()
            .filter(|wt| {
                !self.branch_prs.contains_key(&wt.branch) && !self.is_cached_no_pr(&wt.branch)
            })
            .map(|wt| wt.branch.clone())
            .collect()
    }

    /// Clear expired entries from the no-PR cache
    pub fn cleanup_no_pr_cache(&mut self) {
        self.no_pr_cache
            .retain(|_, checked_at| checked_at.elapsed().as_secs() < NO_PR_CACHE_TTL_SECS);
    }

    /// Clear all no-PR cache entries (for manual refresh)
    pub fn clear_no_pr_cache(&mut self) {
        self.no_pr_cache.clear();
    }

    pub fn set_worktrees(&mut self, worktrees: Vec<WorktreeInfo>) {
        self.worktrees = worktrees;
        self.error = None;
        if let Some(idx) = self.worktrees.iter().position(|wt| wt.is_current) {
            self.selected_index = idx;
        } else if self.selected_index >= self.worktrees.len() {
            self.selected_index = self.worktrees.len().saturating_sub(1);
        }
    }

    pub fn selected(&self) -> Option<&WorktreeInfo> {
        self.worktrees.get(self.selected_index)
    }

    pub fn select_next(&mut self) {
        if !self.worktrees.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.worktrees.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.worktrees.is_empty() {
            self.selected_index = if self.selected_index == 0 {
                self.worktrees.len() - 1
            } else {
                self.selected_index - 1
            };
        }
    }
}

impl Default for WorktreesState {
    fn default() -> Self {
        Self::new()
    }
}
