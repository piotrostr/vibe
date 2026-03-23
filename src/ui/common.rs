use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::state::{AppState, linear_env_var_name};

const LOGO: &str = r#"
 __   _(_) |__   ___
 \ \ / / | '_ \ / _ \
  \ V /| | |_) |  __/
   \_/ |_|_.__/ \___|"#;

pub fn render_header(frame: &mut Frame, area: Rect, state: &AppState) {
    if area.height >= 5 {
        render_header_with_logo(frame, area, state);
    } else {
        render_header_compact(frame, area, state);
    }
}

fn render_header_with_logo(frame: &mut Frame, area: Rect, state: &AppState) {
    let (project_info, project_name) = match &state.selected_project_id {
        Some(id) => {
            let name = state
                .projects
                .projects
                .iter()
                .find(|p| &p.id == id)
                .map(|p| p.name.as_str())
                .unwrap_or(id.as_str());
            (format!("Project: {}", name), Some(name.to_string()))
        }
        None => (String::new(), None),
    };

    // Linear API key status
    let linear_info = if let Some(ref name) = project_name {
        let env_var = linear_env_var_name(name);
        if state.linear_api_key_available {
            Some((env_var, "set", Color::Green))
        } else {
            Some((env_var, "not set", Color::DarkGray))
        }
    } else {
        None
    };

    // Process counts (cached, polled in background)
    let claude_count = state.claude_process_count;
    let zellij_count = state.sessions.sessions.len();

    // Build lines: logo on left, status on right
    let logo_lines: Vec<&str> = LOGO.lines().skip(1).collect();
    let mut lines: Vec<Line> = Vec::new();

    for (i, logo_line) in logo_lines.iter().enumerate() {
        let mut spans = vec![Span::styled(
            *logo_line,
            Style::default()
                .fg(super::ACCENT)
                .add_modifier(Modifier::BOLD),
        )];

        // Add status info on the right side of the first few lines
        if i == 0 {
            // Line 1: Process counts + loading indicator
            spans.push(Span::raw("  "));
            if claude_count > 0 {
                spans.push(Span::styled("Claude: ", Style::default().fg(Color::White)));
                spans.push(Span::styled(
                    format!("{}", claude_count),
                    Style::default().fg(super::ACCENT),
                ));
            } else {
                spans.push(Span::styled(
                    "Claude: 0",
                    Style::default().fg(Color::DarkGray),
                ));
            }
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            if zellij_count > 0 {
                spans.push(Span::styled("Zellij: ", Style::default().fg(Color::White)));
                spans.push(Span::styled(
                    format!("{}", zellij_count),
                    Style::default().fg(Color::Green),
                ));
            } else {
                spans.push(Span::styled(
                    "Zellij: 0",
                    Style::default().fg(Color::DarkGray),
                ));
            }
            // Show loading indicator when refreshing
            if state.pr_loading || state.worktrees.loading || state.sessions.loading {
                spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
                spans.push(Span::styled(
                    format!("{} syncing", state.spinner_char()),
                    Style::default().fg(Color::Yellow),
                ));
            }
        } else if i == 1 && !project_info.is_empty() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled("Project: ", Style::default().fg(Color::White)));
            let project_name_display = project_info
                .strip_prefix("Project: ")
                .unwrap_or(&project_info);
            spans.push(Span::styled(
                project_name_display.to_string(),
                Style::default().fg(super::ACCENT),
            ));
        } else if i == 2 {
            if let Some((ref env_var, status, label_color)) = linear_info {
                spans.push(Span::raw("  "));
                spans.push(Span::styled("Linear: ", Style::default().fg(Color::White)));
                spans.push(Span::styled(
                    format!("{} {}", env_var, status),
                    Style::default().fg(label_color),
                ));
            }
            // Prime session indicator
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            if state.prime_session_active {
                spans.push(Span::styled("Prime: ", Style::default().fg(Color::White)));
                spans.push(Span::styled("active", Style::default().fg(super::ACCENT)));
            } else {
                spans.push(Span::styled(
                    "Prime: P to launch",
                    Style::default().fg(Color::DarkGray),
                ));
            }
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(
                env!("GIT_HASH"),
                Style::default().fg(Color::DarkGray),
            ));
        }

        lines.push(Line::from(spans));
    }

    let header = Paragraph::new(lines).block(Block::default().borders(Borders::BOTTOM));

    frame.render_widget(header, area);
}

fn render_header_compact(frame: &mut Frame, area: Rect, state: &AppState) {
    let title = match &state.selected_project_id {
        Some(id) => {
            // Try to find the project name in the projects list, otherwise use the id directly
            // (in standalone mode, the projects list is empty and id is the project name)
            let project_name = state
                .projects
                .projects
                .iter()
                .find(|p| &p.id == id)
                .map(|p| p.name.as_str())
                .unwrap_or(id.as_str());
            format!(" vibe - {} ", project_name)
        }
        None => " vibe ".to_string(),
    };

    let status = if state.backend_connected {
        Span::styled(" Connected ", Style::default().fg(Color::Green))
    } else {
        Span::styled(" Disconnected ", Style::default().fg(Color::Red))
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled(&title, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" | "),
        status,
        Span::raw(format!(" | {}", env!("GIT_HASH"))),
    ]))
    .block(Block::default().borders(Borders::BOTTOM));

    frame.render_widget(header, area);
}

pub fn render_footer(frame: &mut Frame, area: Rect, state: &AppState) {
    // Show command bar when active (vim-like ;f)
    if let Some(ref cmd) = state.command_input {
        let cmd_line = Line::from(vec![
            Span::styled(";", Style::default().fg(Color::Yellow)),
            Span::raw(cmd.as_str()),
            Span::styled("_", Style::default().fg(Color::Yellow)), // cursor
        ]);

        let footer = Paragraph::new(cmd_line)
            .style(Style::default())
            .block(Block::default().borders(Borders::TOP));

        frame.render_widget(footer, area);
        return;
    }

    // Show search bar when active
    if state.search_active {
        let search_line = Line::from(vec![
            Span::styled("/", Style::default().fg(Color::Yellow)),
            Span::raw(&state.search_query),
            Span::styled("_", Style::default().fg(Color::Yellow)), // cursor
        ]);

        let footer = Paragraph::new(search_line)
            .style(Style::default())
            .block(Block::default().borders(Borders::TOP));

        frame.render_widget(footer, area);
        return;
    }

    // Show active search filter if present
    let search_indicator = if !state.search_query.is_empty() {
        format!(" [/{}] |", state.search_query)
    } else {
        String::new()
    };

    let hints = match state.view {
        crate::state::View::Projects => {
            format!(
                "{}j/k: navigate | Enter: select | /: search | q: quit | ?: help",
                search_indicator
            )
        }
        crate::state::View::Kanban => {
            format!(
                "{}h/j/k/l: nav | Enter: details | /: search | s: session | Esc: back",
                search_indicator
            )
        }
        crate::state::View::TaskDetail => {
            format!(
                "{}e: edit | r: refresh | s/Enter: session | /: search | Esc: back",
                search_indicator
            )
        }
        crate::state::View::Worktrees => {
            format!(
                "{}j/k: nav | Enter: switch | g: session | /: search | Esc: back",
                search_indicator
            )
        }
        crate::state::View::Logs => "j/k: scroll | r: refresh | Esc: back".to_string(),
        crate::state::View::Search => "j/k/Ctrl-j/k: nav | Enter: select | Esc: cancel".to_string(),
    };

    let footer = Paragraph::new(hints)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::TOP));

    frame.render_widget(footer, area);
}

pub fn render_help_modal(frame: &mut Frame, area: Rect) {
    let help_text = vec![
        Line::from(vec![Span::styled(
            "Keyboard Shortcuts",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Yellow),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Navigation",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  h/j/k/l or arrows  Move around"),
        Line::from("  Enter              Select / Open"),
        Line::from("  Esc / q            Back / Quit"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Tasks",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  c                  Create task"),
        Line::from("  e                  Edit task (nvim)"),
        Line::from("  d                  Delete task"),
        Line::from("  A                  Archive done tasks"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Worktrees",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  w                  Show worktrees"),
        Line::from("  W                  Create worktree"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Sessions",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  g                  Gas it (launch Claude)"),
        Line::from("  G                  Gas it with prime"),
        Line::from("  p                  Plan it (launch in plan mode)"),
        Line::from("  P                  Prime session (war room)"),
        Line::from("  v                  View PR"),
        Line::from("  S                  Show sessions"),
        Line::from("  a / Enter          Attach to session"),
        Line::from("  K                  Kill session"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Linear",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  L                  Sync Linear backlog"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Other",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from("  / or ;f            Search"),
        Line::from("  r                  Refresh"),
        Line::from("  ?                  This help"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Press Esc to close",
            Style::default().fg(Color::DarkGray),
        )]),
    ];

    // Center the modal
    let modal_width = 50;
    let modal_height = help_text.len() as u16 + 2;
    let x = (area.width.saturating_sub(modal_width)) / 2;
    let y = (area.height.saturating_sub(modal_height)) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);

    // Clear the area behind the modal
    let clear = Block::default().style(Style::default().bg(Color::Black));
    frame.render_widget(clear, modal_area);

    let help = Paragraph::new(help_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Help ")
            .border_style(Style::default().fg(super::ACCENT)),
    );

    frame.render_widget(help, modal_area);
}
