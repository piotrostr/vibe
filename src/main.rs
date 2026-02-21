use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::Mutex;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod app;
mod external;
mod input;
mod state;
mod storage;
mod terminal;
mod ui;

use app::App;
use external::{AssistantCli, LinearClient, launch_headless_in_worktree, rapporting_instructions};
use state::task_title_to_branch;
use storage::TaskStorage;
use terminal::Terminal;

#[derive(Parser)]
#[command(name = "vibe")]
#[command(about = "Terminal-based kanban board for managing Claude Code or Codex sessions")]
struct Cli {
    /// Launch task sessions with Codex instead of Claude Code
    #[arg(long, global = true)]
    codex: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new task in the backlog
    Create {
        /// Task title (keep short and unambiguous)
        #[arg(short, long)]
        title: String,

        /// Task description (context, schedule, details)
        #[arg(short, long)]
        description: Option<String>,

        /// Immediately spawn a Claude session for the task
        #[arg(long)]
        gas_it: bool,
    },
    /// Import a task from a markdown file
    Import {
        /// Path to markdown file (filename becomes title, contents become description)
        file: PathBuf,

        /// Override task title (default: derived from filename)
        #[arg(short, long)]
        title: Option<String>,

        /// Immediately spawn a Claude session for the task
        #[arg(long)]
        gas_it: bool,
    },
    /// Tear down finished sessions (launchd + zellij + worktree)
    Cleanup {
        /// Specific session or ticket ID to clean up (e.g. VIB-21). Omit for all dead sessions.
        target: Option<String>,
    },
    /// Spawn a Claude session for an existing task
    Gas {
        /// Task identifier: Linear ID (VIB-23), task title substring, or UUID
        target: String,
    },
    /// Show Linear board state grouped by column
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Create {
            title,
            description,
            gas_it,
        }) => {
            let storage = TaskStorage::from_cwd()?;
            let project_name = storage.project_name().to_string();

            // Check for project-specific Linear API key
            let project = project_name.to_uppercase().replace('-', "_");
            let env_var = format!("{}_LINEAR_API_KEY", project);

            let (linear_id, task_desc) = if let Ok(api_key) = std::env::var(&env_var) {
                let client = LinearClient::new(api_key);
                let created = client
                    .create_issue(&title, description.as_deref())
                    .await
                    .map_err(|e| anyhow::anyhow!("Linear: {}", e))?;

                let linear_issue = external::LinearIssue {
                    identifier: created.identifier.clone(),
                    title: title.clone(),
                    description: description.clone(),
                    url: created.url.clone(),
                    labels: vec![],
                };
                let task = storage.create_task_from_linear(&linear_issue)?;
                println!("Created: {} [{}]", task.title, created.identifier);
                println!("  {}", created.url);
                (Some(created.identifier), task.description)
            } else {
                let task = storage.create_task(&title, description.as_deref())?;
                println!("Created: {}", task.title);
                (None, task.description)
            };

            if gas_it {
                let project_dir = std::env::current_dir()?;
                let branch = task_title_to_branch(&title, linear_id.as_deref());

                let mut context = format!("Task: {}", title);
                if let Some(desc) = &task_desc
                    && !desc.is_empty()
                {
                    context.push_str(&format!("\n\nDescription:\n{}", desc));
                }
                context.push_str(&format!("\n\nBranch: {}", branch));
                context.push_str("\n\nRun `just setup` if available to initialize the worktree environment.");
                context.push_str(&rapporting_instructions(&project_name));

                let assistant = if cli.codex {
                    AssistantCli::Codex
                } else {
                    AssistantCli::Claude
                };

                println!("Launching session...");
                launch_headless_in_worktree(
                    &branch,
                    &context,
                    assistant,
                    &project_dir,
                )?;
                println!("Session spawned headlessly. Attach with: zellij attach {}",
                    external::session_name_for_branch(&branch));
            }

            Ok(())
        }
        Some(Command::Import { file, title: title_override, gas_it }) => {
            let storage = TaskStorage::from_cwd()?;
            let project_name = storage.project_name().to_string();

            let content = std::fs::read_to_string(&file)
                .with_context(|| format!("Failed to read file: {:?}", file))?;
            let title = title_override.unwrap_or_else(|| {
                file.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("untitled")
                    .replace(['-', '_'], " ")
            });
            let description = if content.trim().is_empty() {
                None
            } else {
                Some(content)
            };

            // Route through Linear if API key is set
            let project = project_name.to_uppercase().replace('-', "_");
            let env_var = format!("{}_LINEAR_API_KEY", project);

            let (linear_id, task_desc) = if let Ok(api_key) = std::env::var(&env_var) {
                let client = LinearClient::new(api_key);
                let created = client
                    .create_issue(&title, description.as_deref())
                    .await
                    .map_err(|e| anyhow::anyhow!("Linear: {}", e))?;

                let linear_issue = external::LinearIssue {
                    identifier: created.identifier.clone(),
                    title: title.clone(),
                    description: description.clone(),
                    url: created.url.clone(),
                    labels: vec![],
                };
                let task = storage.create_task_from_linear(&linear_issue)?;
                println!("Created: {} [{}]", task.title, created.identifier);
                println!("  {}", created.url);
                (Some(created.identifier), task.description)
            } else {
                let task = storage.create_task(&title, description.as_deref())?;
                println!("Created: {}", task.title);
                (None, task.description)
            };

            if gas_it {
                let project_dir = std::env::current_dir()?;
                let branch = task_title_to_branch(&title, linear_id.as_deref());

                let mut context = format!("Task: {}", title);
                if let Some(desc) = &task_desc
                    && !desc.is_empty()
                {
                    context.push_str(&format!("\n\nDescription:\n{}", desc));
                }
                context.push_str(&format!("\n\nBranch: {}", branch));
                context.push_str("\n\nRun `just setup` if available to initialize the worktree environment.");
                context.push_str(&rapporting_instructions(&project_name));

                let assistant = if cli.codex {
                    AssistantCli::Codex
                } else {
                    AssistantCli::Claude
                };

                println!("Launching session...");
                launch_headless_in_worktree(
                    &branch,
                    &context,
                    assistant,
                    &project_dir,
                )?;
                println!("Session spawned headlessly. Attach with: zellij attach {}",
                    external::session_name_for_branch(&branch));
            }

            Ok(())
        }
        Some(Command::Cleanup { target }) => {
            cmd_cleanup(target.as_deref())?;
            Ok(())
        }
        Some(Command::Gas { target }) => {
            let storage = TaskStorage::from_cwd()?;
            let project_name = storage.project_name().to_string();
            let tasks = storage.list_tasks()?;

            // Match by linear ID, title substring, or UUID
            let upper = target.to_uppercase();
            let task = tasks
                .iter()
                .find(|t| {
                    t.linear_issue_id
                        .as_ref()
                        .is_some_and(|id| id.to_uppercase() == upper)
                })
                .or_else(|| {
                    tasks.iter().find(|t| t.id == target)
                })
                .or_else(|| {
                    tasks
                        .iter()
                        .find(|t| t.title.to_uppercase().contains(&upper))
                })
                .ok_or_else(|| anyhow::anyhow!("no task matching '{}'", target))?;

            let branch = task_title_to_branch(&task.title, task.linear_issue_id.as_deref());

            let mut context = format!("Task: {}", task.title);
            if let Some(desc) = &task.description
                && !desc.is_empty()
            {
                context.push_str(&format!("\n\nDescription:\n{}", desc));
            }
            context.push_str(&format!("\n\nBranch: {}", branch));
            context.push_str(
                "\n\nRun `just setup` if available to initialize the worktree environment.",
            );
            context.push_str(&rapporting_instructions(&project_name));

            let assistant = if cli.codex {
                AssistantCli::Codex
            } else {
                AssistantCli::Claude
            };

            println!("Gassing: {} {}",
                task.linear_issue_id.as_deref().unwrap_or(""),
                task.title);
            launch_headless_in_worktree(&branch, &context, assistant, &std::env::current_dir()?)?;
            println!(
                "Session spawned. Attach with: zellij attach {}",
                external::session_name_for_branch(&branch)
            );

            Ok(())
        }
        Some(Command::Status) => {
            cmd_status().await?;
            Ok(())
        }
        None => {
            init_tracing()?;

            let mut terminal = Terminal::new()?;
            let assistant = if cli.codex {
                AssistantCli::Codex
            } else {
                AssistantCli::Claude
            };
            let mut app = App::new(assistant)?;

            let result = app.run(&mut terminal).await;

            terminal.restore()?;

            result
        }
    }
}

fn cmd_cleanup(target: Option<&str>) -> Result<()> {
    use std::process::Command as Cmd;

    let launchd_dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("vibe-launchd");

    // Resolve which sessions to clean up
    let sessions: Vec<String> = if let Some(target) = target {
        // Specific target: resolve like cousin does (ticket ID or raw name)
        let output = Cmd::new("zellij")
            .args(["list-sessions", "-s"])
            .output()?;
        let all: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect();
        let upper = target.to_uppercase();
        let matches: Vec<String> = all
            .into_iter()
            .filter(|s| s.to_uppercase().contains(&upper))
            .collect();
        if matches.is_empty() {
            println!("no session matching '{}'", target);
            return Ok(());
        }
        matches
    } else {
        // No target: clean up all EXITED sessions for this project
        let output = Cmd::new("zellij")
            .args(["list-sessions"])
            .output()?;
        let raw = String::from_utf8_lossy(&output.stdout).to_string();
        // Strip ANSI
        let stripped: String = {
            let mut result = String::new();
            let mut chars = raw.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\x1b' {
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next.is_ascii_alphabetic() { break; }
                    }
                } else {
                    result.push(c);
                }
            }
            result
        };
        stripped
            .lines()
            .filter(|l| l.contains("EXITED"))
            .filter_map(|l| l.split_whitespace().next())
            .map(|s| s.to_string())
            .collect()
    };

    if sessions.is_empty() {
        println!("nothing to clean up");
        return Ok(());
    }

    for session in &sessions {
        // 1. Unload launchd plist
        let plist = launchd_dir.join(format!("com.vibe.headless.{}.plist", session));
        if plist.exists() {
            let _ = Cmd::new("launchctl").args(["unload", plist.to_str().unwrap()]).output();
            let _ = std::fs::remove_file(&plist);
            println!("  unloaded launchd: {}", session);
        }

        // 2. Kill + delete zellij session
        let _ = Cmd::new("zellij").args(["kill-session", session]).output();
        let _ = Cmd::new("zellij").args(["delete-session", session]).output();
        println!("  removed session: {}", session);
    }

    println!("cleaned {} session(s)", sessions.len());
    Ok(())
}

async fn cmd_status() -> Result<()> {
    let storage = TaskStorage::from_cwd()?;
    let project = storage
        .project_name()
        .to_uppercase()
        .replace('-', "_");
    let env_var = format!("{}_LINEAR_API_KEY", project);

    let api_key = std::env::var(&env_var)
        .map_err(|_| anyhow::anyhow!("{} not set", env_var))?;

    let client = LinearClient::new(api_key);
    let issues = client
        .fetch_assigned_issues()
        .await
        .map_err(|e| anyhow::anyhow!("Linear: {}", e))?;

    // Group by state type in board order
    let columns = ["started", "unstarted", "backlog", "completed"];

    for col in &columns {
        let group: Vec<_> = issues.iter().filter(|i| i.state_type == *col).collect();
        if group.is_empty() {
            continue;
        }
        let label = group.first().map(|i| i.state_name.as_str()).unwrap_or(col);
        println!("\n  {} ({})", label, group.len());
        let show = if *col == "completed" { 3 } else { group.len() };
        for issue in group.iter().take(show) {
            println!("    {} {}", issue.identifier, issue.title);
        }
        if group.len() > show {
            println!("    ...");
        }
    }
    println!();

    Ok(())
}

fn init_tracing() -> Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,tui=info"));

    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".vibe");
    std::fs::create_dir_all(&log_dir)?;

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("vibe.log"))?;

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(Mutex::new(log_file)))
        .init();

    Ok(())
}
