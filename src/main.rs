use anyhow::Result;
use clap::{Parser, Subcommand};
use std::fs::OpenOptions;
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
        /// Task title
        #[arg(short, long)]
        title: String,

        /// Task description
        #[arg(short, long)]
        description: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Create { title, description }) => {
            let storage = TaskStorage::from_cwd()?;
            let task = storage.create_task(&title, description.as_deref())?;
            println!("Created task: {}", task.title);
            Ok(())
        }
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
