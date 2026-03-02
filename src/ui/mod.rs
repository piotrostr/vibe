mod common;
mod kanban;
mod logs;
mod search;
mod task_detail;
mod worktrees;

pub use common::*;
pub use kanban::*;
pub use logs::*;
pub use search::*;
pub use task_detail::*;
pub use worktrees::*;

use ratatui::style::Color;

pub const ACCENT: Color = Color::Rgb(232, 145, 58);
