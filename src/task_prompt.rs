use crate::external::rapporting_instructions;

pub struct PullRequestContext<'a> {
    pub url: &'a str,
    pub state: &'a str,
}

pub struct TaskPromptOptions<'a> {
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub branch: &'a str,
    pub pull_request: Option<PullRequestContext<'a>>,
    pub project_name: &'a str,
    pub with_prime: bool,
}

pub fn build_task_prompt(options: TaskPromptOptions<'_>) -> String {
    let mut prompt = format!("Task: {}", options.title);

    if let Some(description) = options
        .description
        .filter(|description| !description.is_empty())
    {
        prompt.push_str(&format!("\n\nDescription:\n{description}"));
    }

    prompt.push_str(&format!("\n\nBranch: {}", options.branch));

    if let Some(pr) = options.pull_request {
        prompt.push_str(&format!("\nPR: {} ({})", pr.url, pr.state));
    }

    prompt.push_str("\n\nRun `just setup` if available to initialize the worktree environment.");

    if options.with_prime {
        prompt.push_str(&rapporting_instructions(options.project_name));
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::{PullRequestContext, TaskPromptOptions, build_task_prompt};

    #[test]
    fn omits_prime_instructions_by_default() {
        let prompt = build_task_prompt(TaskPromptOptions {
            title: "Ship opt-in prompts",
            description: Some("Remove the hidden vibe prime appendix."),
            branch: "vib-123-ship-opt-in-prompts",
            pull_request: None,
            project_name: "vibe",
            with_prime: false,
        });

        assert!(prompt.contains("Task: Ship opt-in prompts"));
        assert!(prompt.contains("Description:\nRemove the hidden vibe prime appendix."));
        assert!(prompt.contains("Branch: vib-123-ship-opt-in-prompts"));
        assert!(prompt.contains("Run `just setup` if available"));
        assert!(!prompt.contains("cousin prime"));
    }

    #[test]
    fn appends_prime_instructions_when_requested() {
        let prompt = build_task_prompt(TaskPromptOptions {
            title: "Ship opt-in prompts",
            description: None,
            branch: "vib-123-ship-opt-in-prompts",
            pull_request: Some(PullRequestContext {
                url: "https://github.com/piotrostr/vibe/pull/123",
                state: "OPEN",
            }),
            project_name: "vibe",
            with_prime: true,
        });

        assert!(prompt.contains("PR: https://github.com/piotrostr/vibe/pull/123 (OPEN)"));
        assert!(prompt.contains("cousin prime"));
        assert!(prompt.contains("Full protocol: cat ~/.claude/skills/vibe/SKILL.md"));
    }
}
