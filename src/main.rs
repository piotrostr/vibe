use anyhow::Result;
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
use external::LinearClient;
use state::{linear_env_var_name, task_title_to_branch};
use storage::TaskStorage;
use terminal::Terminal;

#[derive(Parser)]
#[command(name = "vibe")]
#[command(about = "Terminal-based kanban board for managing Claude Code sessions")]
struct Cli {
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
    },
    /// Import a task from a markdown file
    Import {
        /// Path to markdown file (filename becomes title, contents become description)
        file: PathBuf,
    },
    /// Gas a Linear ticket - fetch it, create a worktree, and launch Claude on it
    Gas {
        /// Linear issue identifier (e.g. AMB-123, VIB-42)
        identifier: String,

        /// Launch in plan mode (blue mode) instead of dangerous permissions
        #[arg(short, long)]
        plan: bool,
    },
    /// Watch Linear for tickets tagged with ~gasit and auto-gas them
    Watch {
        /// Poll interval in seconds (default: 30)
        #[arg(short, long, default_value = "30")]
        interval: u64,

        /// Launch in plan mode (blue mode) instead of dangerous permissions
        #[arg(short, long)]
        plan: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Create { title, description }) => {
            let storage = TaskStorage::from_cwd()?;

            // Check for project-specific Linear API key
            let project = storage.project_name().to_uppercase().replace('-', "_");
            let env_var = format!("{}_LINEAR_API_KEY", project);

            if let Ok(api_key) = std::env::var(&env_var) {
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
            } else {
                let task = storage.create_task(&title, description.as_deref())?;
                println!("Created: {}", task.title);
            }
            Ok(())
        }
        Some(Command::Import { file }) => {
            let storage = TaskStorage::from_cwd()?;
            let task = storage.create_task_from_file(&file)?;
            println!("Created: {}", task.title);
            Ok(())
        }
        Some(Command::Gas { identifier, plan }) => cmd_gas(&identifier, plan).await,
        Some(Command::Watch { interval, plan }) => cmd_watch(interval, plan).await,
        None => {
            init_tracing()?;

            let mut terminal = Terminal::new()?;
            let mut app = App::new()?;

            let result = app.run(&mut terminal).await;

            terminal.restore()?;

            result
        }
    }
}

/// Gas a single Linear ticket: fetch it, create local task, launch Claude session
async fn cmd_gas(identifier: &str, plan_mode: bool) -> Result<()> {
    let storage = TaskStorage::from_cwd()?;
    let project_name = storage.project_name().to_string();
    let env_var = linear_env_var_name(&project_name);

    let api_key = std::env::var(&env_var)
        .map_err(|_| anyhow::anyhow!("Linear API key not set. Export {}", env_var))?;

    let client = LinearClient::new(api_key);

    println!("Fetching {}...", identifier);
    let issue = client
        .fetch_issue_by_identifier(identifier)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    println!("  {} - {}", issue.identifier, issue.title);

    // Check if task already exists locally
    let existing_tasks = storage.list_tasks()?;
    let already_imported = existing_tasks
        .iter()
        .any(|t| t.linear_issue_id.as_deref() == Some(&issue.identifier));

    if !already_imported {
        storage.create_task_from_linear(&issue)?;
        println!("  Imported to local task storage");
    }

    // Derive branch and build context
    let branch = task_title_to_branch(&issue.title, Some(&issue.identifier));
    let task_context = build_task_context(&issue);

    println!("  Branch: {}", branch);
    println!("  Launching Claude session...");

    let project_dir =
        std::env::current_dir().map_err(|e| anyhow::anyhow!("Failed to get cwd: {}", e))?;

    external::launch_zellij_claude_in_worktree_with_context(
        &branch,
        &task_context,
        plan_mode,
        &project_dir,
    )?;

    Ok(())
}

/// Watch Linear for ~gasit tickets and auto-gas them
async fn cmd_watch(interval_secs: u64, plan_mode: bool) -> Result<()> {
    let storage = TaskStorage::from_cwd()?;
    let project_name = storage.project_name().to_string();
    let env_var = linear_env_var_name(&project_name);

    let api_key = std::env::var(&env_var)
        .map_err(|_| anyhow::anyhow!("Linear API key not set. Export {}", env_var))?;

    let project_dir =
        std::env::current_dir().map_err(|e| anyhow::anyhow!("Failed to get cwd: {}", e))?;

    // Track which issues we've already gassed to avoid re-launching
    let mut gassed: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Pre-populate with existing local tasks that have linear IDs
    for task in storage.list_tasks()? {
        if let Some(linear_id) = &task.linear_issue_id {
            gassed.insert(linear_id.clone());
        }
    }

    println!(
        "Watching Linear for ~gasit tickets (polling every {}s)...",
        interval_secs
    );
    println!("  Project: {}", project_name);
    println!("  Known tickets: {}", gassed.len());
    println!("  Press Ctrl+C to stop\n");

    loop {
        let client = LinearClient::new(api_key.clone());
        match client.fetch_gasit_issues().await {
            Ok(issues) => {
                let new_issues: Vec<_> = issues
                    .into_iter()
                    .filter(|i| !gassed.contains(&i.identifier))
                    .collect();

                for issue in new_issues {
                    println!("New ~gasit ticket: {} - {}", issue.identifier, issue.title);

                    // Import to local storage
                    if let Err(e) = storage.create_task_from_linear(&issue) {
                        eprintln!("  Failed to import: {}", e);
                        continue;
                    }

                    let branch = task_title_to_branch(&issue.title, Some(&issue.identifier));
                    let task_context = build_task_context(&issue);

                    println!("  Branch: {}", branch);
                    println!("  Launching Claude session...");

                    // Launch in background - spawn a new process so we don't block the watcher
                    let branch_clone = branch.clone();
                    let context_clone = task_context.clone();
                    let dir_clone = project_dir.clone();
                    std::thread::spawn(move || {
                        if let Err(e) = external::launch_zellij_claude_in_worktree_with_context(
                            &branch_clone,
                            &context_clone,
                            plan_mode,
                            &dir_clone,
                        ) {
                            eprintln!("  Failed to launch session for {}: {}", branch_clone, e);
                        }
                    });

                    gassed.insert(issue.identifier);
                }
            }
            Err(e) => {
                eprintln!("Linear fetch error: {}", e);
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
    }
}

fn build_task_context(issue: &external::LinearIssue) -> String {
    let mut context = format!("Task: {}", issue.title);
    if let Some(desc) = &issue.description {
        // Strip the ~gasit tag from context sent to Claude
        let clean_desc = desc.replace("~gasit", "").trim().to_string();
        if !clean_desc.is_empty() {
            context.push_str(&format!("\n\nDescription:\n{}", clean_desc));
        }
    }
    if !issue.labels.is_empty() {
        context.push_str(&format!("\n\nLabels: {}", issue.labels.join(", ")));
    }
    context
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
