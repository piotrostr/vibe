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
use external::{
    AssistantCli, LinearClient, launch_zellij_claude_in_worktree_with_context,
    rapporting_instructions,
};
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
    },
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
                context.push_str(&rapporting_instructions(&project_name));

                let assistant = if cli.codex {
                    AssistantCli::Codex
                } else {
                    AssistantCli::Claude
                };

                println!("Launching session...");
                launch_zellij_claude_in_worktree_with_context(
                    &branch,
                    &context,
                    assistant,
                    false,
                    &project_dir,
                )?;
            }

            Ok(())
        }
        Some(Command::Import { file }) => {
            let storage = TaskStorage::from_cwd()?;
            let task = storage.create_task_from_file(&file)?;
            println!("Created: {}", task.title);
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
