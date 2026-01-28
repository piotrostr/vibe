use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;

/// PR review from a user
#[derive(Debug, Clone, Deserialize)]
pub struct Review {
    pub state: String, // APPROVED, CHANGES_REQUESTED, COMMENTED, etc.
    pub author: ReviewAuthor,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewAuthor {
    pub login: String,
}

/// PR info fetched from `gh pr view`
#[derive(Debug, Clone, Deserialize)]
pub struct BranchPrInfo {
    #[serde(rename = "number")]
    pub _number: i64,
    pub url: String,
    pub state: String, // OPEN, CLOSED, MERGED
    #[serde(rename = "isDraft")]
    pub is_draft: bool,
    #[serde(rename = "reviewDecision")]
    pub review_decision: Option<String>, // APPROVED, CHANGES_REQUESTED, REVIEW_REQUIRED
    #[serde(rename = "statusCheckRollup")]
    pub status_check_rollup: Option<Vec<StatusCheck>>,
    #[serde(rename = "mergeable")]
    pub mergeable: Option<String>, // MERGEABLE, CONFLICTING, UNKNOWN
    #[serde(default)]
    pub reviews: Vec<Review>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StatusCheck {
    #[serde(rename = "__typename")]
    pub _typename: String,
    pub conclusion: Option<String>, // SUCCESS, FAILURE, etc.
    pub status: Option<String>,     // COMPLETED, IN_PROGRESS, etc.
}

impl BranchPrInfo {
    /// Get overall checks status: SUCCESS, FAILURE, PENDING, or None
    pub fn checks_status(&self) -> Option<String> {
        let checks = self.status_check_rollup.as_ref()?;
        if checks.is_empty() {
            return None;
        }

        let mut has_failure = false;
        let mut has_pending = false;

        for check in checks {
            match check.conclusion.as_deref() {
                Some("FAILURE") | Some("ERROR") | Some("TIMED_OUT") => has_failure = true,
                Some("SUCCESS") | Some("NEUTRAL") | Some("SKIPPED") => {}
                _ => {
                    if check.status.as_deref() != Some("COMPLETED") {
                        has_pending = true;
                    }
                }
            }
        }

        if has_failure {
            Some("FAILURE".to_string())
        } else if has_pending {
            Some("PENDING".to_string())
        } else {
            Some("SUCCESS".to_string())
        }
    }

    /// Check if PR has merge conflicts
    pub fn has_conflicts(&self) -> bool {
        self.mergeable.as_deref() == Some("CONFLICTING")
    }

    /// Get list of usernames who approved the PR
    pub fn approvers(&self) -> Vec<&str> {
        self.reviews
            .iter()
            .filter(|r| r.state == "APPROVED")
            .map(|r| r.author.login.as_str())
            .collect()
    }
}

// GraphQL response types for batch PR fetching
#[derive(Debug, Deserialize)]
struct GraphQLResponse {
    data: Option<GraphQLData>,
    errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Deserialize)]
struct GraphQLError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct GraphQLData {
    repository: Option<GraphQLRepository>,
}

#[derive(Debug, Deserialize)]
struct GraphQLRepository {
    #[serde(rename = "pullRequests")]
    pull_requests: GraphQLPullRequests,
}

#[derive(Debug, Deserialize)]
struct GraphQLPullRequests {
    nodes: Vec<GraphQLPullRequest>,
}

#[derive(Debug, Deserialize)]
struct GraphQLPullRequest {
    number: i64,
    url: String,
    state: String,
    #[serde(rename = "isDraft")]
    is_draft: bool,
    #[serde(rename = "reviewDecision")]
    review_decision: Option<String>,
    mergeable: Option<String>,
    #[serde(rename = "headRefName")]
    head_ref_name: String,
    reviews: GraphQLReviews,
    #[serde(rename = "statusCheckRollup")]
    status_check_rollup: Option<GraphQLStatusCheckRollup>,
}

#[derive(Debug, Deserialize)]
struct GraphQLReviews {
    nodes: Vec<GraphQLReview>,
}

#[derive(Debug, Deserialize)]
struct GraphQLReview {
    state: String,
    author: Option<GraphQLAuthor>,
}

#[derive(Debug, Deserialize)]
struct GraphQLAuthor {
    login: String,
}

#[derive(Debug, Deserialize)]
struct GraphQLStatusCheckRollup {
    contexts: GraphQLContexts,
}

#[derive(Debug, Deserialize)]
struct GraphQLContexts {
    nodes: Vec<GraphQLContext>,
}

#[derive(Debug, Deserialize)]
struct GraphQLContext {
    #[serde(rename = "__typename")]
    typename: String,
    // CheckRun fields
    conclusion: Option<String>,
    status: Option<String>,
    // StatusContext fields
    state: Option<String>,
}

const BATCH_PR_QUERY: &str = r#"
query($owner: String!, $repo: String!) {
  repository(owner: $owner, name: $repo) {
    pullRequests(states: [OPEN, MERGED, CLOSED], first: 100, orderBy: {field: UPDATED_AT, direction: DESC}) {
      nodes {
        number
        url
        state
        isDraft
        reviewDecision
        mergeable
        headRefName
        reviews(first: 10, states: [APPROVED, CHANGES_REQUESTED, COMMENTED]) {
          nodes {
            state
            author { login }
          }
        }
        statusCheckRollup {
          contexts(first: 50) {
            nodes {
              __typename
              ... on CheckRun {
                conclusion
                status
              }
              ... on StatusContext {
                state
              }
            }
          }
        }
      }
    }
  }
}
"#;

/// Fetch all PRs (open, merged, closed) for the repository in a single GraphQL query.
/// Returns a map from branch name to PR info.
///
/// This is much more efficient than per-branch polling:
/// - 1 API call instead of N calls for N branches
/// - Reduces rate limit usage from N requests/poll to 1 request/poll
///
/// Note: Limited to 100 most recently updated PRs. For repos with more PRs,
/// pagination would be needed (rare for active worktrees).
pub fn get_all_open_prs() -> Result<HashMap<String, BranchPrInfo>> {
    let start = std::time::Instant::now();

    // Get owner and repo from gh CLI
    tracing::trace!("gh api: repo view");
    let repo_output = Command::new("gh")
        .args(["repo", "view", "--json", "owner,name"])
        .output()?;

    if !repo_output.status.success() {
        let stderr = String::from_utf8_lossy(&repo_output.stderr);
        anyhow::bail!("Failed to get repo info: {}", stderr);
    }

    #[derive(Deserialize)]
    struct RepoInfo {
        owner: RepoOwner,
        name: String,
    }
    #[derive(Deserialize)]
    struct RepoOwner {
        login: String,
    }

    let repo_info: RepoInfo = serde_json::from_slice(&repo_output.stdout)?;
    let owner = repo_info.owner.login;
    let repo = repo_info.name;
    tracing::trace!("gh api: repo view done in {:?} - {}/{}", start.elapsed(), owner, repo);

    // Execute batch GraphQL query
    let gql_start = std::time::Instant::now();
    tracing::trace!("gh api: graphql batch PR query for {}/{}", owner, repo);
    let output = Command::new("gh")
        .args([
            "api",
            "graphql",
            "-f",
            &format!("query={}", BATCH_PR_QUERY),
            "-f",
            &format!("owner={}", owner),
            "-f",
            &format!("repo={}", repo),
        ])
        .output()?;
    tracing::trace!("gh api: graphql done in {:?}", gql_start.elapsed());

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("GraphQL query failed: {}", stderr);
    }

    let response: GraphQLResponse = serde_json::from_slice(&output.stdout)?;

    if let Some(errors) = response.errors {
        let messages: Vec<_> = errors.iter().map(|e| e.message.as_str()).collect();
        anyhow::bail!("GraphQL errors: {}", messages.join(", "));
    }

    let Some(data) = response.data else {
        return Ok(HashMap::new());
    };

    let Some(repository) = data.repository else {
        return Ok(HashMap::new());
    };

    // Convert GraphQL response to our format
    let mut result = HashMap::new();
    for pr in repository.pull_requests.nodes {
        let branch = pr.head_ref_name.clone();

        // Convert reviews
        let reviews: Vec<Review> = pr
            .reviews
            .nodes
            .into_iter()
            .filter_map(|r| {
                r.author.map(|a| Review {
                    state: r.state,
                    author: ReviewAuthor { login: a.login },
                })
            })
            .collect();

        // Convert status checks
        let status_check_rollup = pr.status_check_rollup.map(|rollup| {
            rollup
                .contexts
                .nodes
                .into_iter()
                .map(|ctx| {
                    // StatusContext uses 'state' field, CheckRun uses 'conclusion'/'status'
                    let (conclusion, status) = if ctx.typename == "StatusContext" {
                        // Map StatusContext state to conclusion format
                        let conclusion = ctx.state.map(|s| match s.as_str() {
                            "SUCCESS" => "SUCCESS".to_string(),
                            "FAILURE" | "ERROR" => "FAILURE".to_string(),
                            "PENDING" | "EXPECTED" => "PENDING".to_string(),
                            _ => s,
                        });
                        (conclusion, Some("COMPLETED".to_string()))
                    } else {
                        (ctx.conclusion, ctx.status)
                    };
                    StatusCheck {
                        _typename: ctx.typename,
                        conclusion,
                        status,
                    }
                })
                .collect()
        });

        let pr_info = BranchPrInfo {
            _number: pr.number,
            url: pr.url,
            state: pr.state,
            is_draft: pr.is_draft,
            review_decision: pr.review_decision,
            status_check_rollup,
            mergeable: pr.mergeable,
            reviews,
        };

        result.insert(branch, pr_info);
    }

    Ok(result)
}

/// Get PR info for a specific branch using `gh pr view`
/// Returns None if no PR exists for the branch
pub fn get_pr_for_branch(branch: &str) -> Result<Option<BranchPrInfo>> {
    let start = std::time::Instant::now();
    tracing::trace!("gh api: pr view {}", branch);

    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            branch,
            "--json",
            "number,url,state,isDraft,reviewDecision,statusCheckRollup,mergeable,reviews",
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no pull requests found") || stderr.contains("no open pull requests") {
            tracing::trace!("gh api: pr view {} - no PR found ({:?})", branch, start.elapsed());
            return Ok(None);
        }
        if stderr.contains("Could not resolve") {
            tracing::trace!("gh api: pr view {} - branch not found ({:?})", branch, start.elapsed());
            return Ok(None);
        }
        anyhow::bail!("gh pr view failed: {}", stderr);
    }

    let stdout = String::from_utf8(output.stdout)?;
    let pr_info: BranchPrInfo = serde_json::from_str(&stdout)?;
    tracing::trace!("gh api: pr view {} - found PR #{} state={} ({:?})", branch, pr_info._number, pr_info.state, start.elapsed());
    Ok(Some(pr_info))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_graphql_pr_response() {
        // Sample GraphQL response from batch PR query
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequests": {
                        "nodes": [
                            {
                                "number": 10,
                                "url": "https://github.com/test/repo/pull/10",
                                "state": "OPEN",
                                "isDraft": false,
                                "reviewDecision": "APPROVED",
                                "mergeable": "MERGEABLE",
                                "headRefName": "feature-branch",
                                "reviews": {
                                    "nodes": [
                                        {
                                            "state": "APPROVED",
                                            "author": { "login": "reviewer1" }
                                        }
                                    ]
                                },
                                "statusCheckRollup": {
                                    "contexts": {
                                        "nodes": [
                                            {
                                                "__typename": "CheckRun",
                                                "conclusion": "SUCCESS",
                                                "status": "COMPLETED"
                                            },
                                            {
                                                "__typename": "StatusContext",
                                                "state": "SUCCESS"
                                            }
                                        ]
                                    }
                                }
                            }
                        ]
                    }
                }
            }
        }"#;

        let response: GraphQLResponse = serde_json::from_str(json).unwrap();
        assert!(response.errors.is_none());
        let data = response.data.unwrap();
        let repo = data.repository.unwrap();
        let pr = &repo.pull_requests.nodes[0];

        assert_eq!(pr.number, 10);
        assert_eq!(pr.head_ref_name, "feature-branch");
        assert_eq!(pr.state, "OPEN");
        assert!(!pr.is_draft);
        assert_eq!(pr.review_decision, Some("APPROVED".to_string()));
        assert_eq!(pr.reviews.nodes.len(), 1);

        let rollup = pr.status_check_rollup.as_ref().unwrap();
        assert_eq!(rollup.contexts.nodes.len(), 2);
    }

    #[test]
    fn test_parse_graphql_empty_response() {
        let json = r#"{
            "data": {
                "repository": {
                    "pullRequests": {
                        "nodes": []
                    }
                }
            }
        }"#;

        let response: GraphQLResponse = serde_json::from_str(json).unwrap();
        let data = response.data.unwrap();
        let repo = data.repository.unwrap();
        assert!(repo.pull_requests.nodes.is_empty());
    }

    #[test]
    fn test_approvers() {
        let pr = BranchPrInfo {
            _number: 1,
            url: "https://github.com/test/repo/pull/1".to_string(),
            state: "OPEN".to_string(),
            is_draft: false,
            review_decision: Some("APPROVED".to_string()),
            status_check_rollup: None,
            mergeable: Some("MERGEABLE".to_string()),
            reviews: vec![
                Review {
                    state: "APPROVED".to_string(),
                    author: ReviewAuthor {
                        login: "alice".to_string(),
                    },
                },
                Review {
                    state: "COMMENTED".to_string(),
                    author: ReviewAuthor {
                        login: "bob".to_string(),
                    },
                },
                Review {
                    state: "APPROVED".to_string(),
                    author: ReviewAuthor {
                        login: "charlie".to_string(),
                    },
                },
            ],
        };

        let approvers = pr.approvers();
        assert_eq!(approvers, vec!["alice", "charlie"]);
    }

    #[test]
    fn test_approvers_empty() {
        let pr = BranchPrInfo {
            _number: 1,
            url: "https://github.com/test/repo/pull/1".to_string(),
            state: "OPEN".to_string(),
            is_draft: false,
            review_decision: None,
            status_check_rollup: None,
            mergeable: None,
            reviews: vec![],
        };

        let approvers = pr.approvers();
        assert!(approvers.is_empty());
    }
}
