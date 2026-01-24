use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::state::Task;

pub fn render_task_detail(
    frame: &mut Frame,
    area: Rect,
    task: &Task,
    plan: Option<&str>,
    plan_scroll_offset: usize,
    plan_line_count: usize,
) {
    let has_linear = task.linear_url.is_some() || task.linear_issue_id.is_some();
    let has_pr = task.pr_url.is_some();
    let has_plan = plan.is_some();

    let mut constraints = vec![Constraint::Length(3)]; // Title with status
    if has_linear {
        constraints.push(Constraint::Length(3)); // Linear
    }
    if has_pr {
        constraints.push(Constraint::Length(3)); // PR
    }
    if has_plan {
        // Plan section takes up to 50% of remaining space
        constraints.push(Constraint::Percentage(50));
    }
    constraints.push(Constraint::Min(0)); // Description

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut chunk_idx = 0;

    // Title with status inlined
    let status_color = match task.status {
        crate::state::TaskStatus::Backlog => Color::Gray,
        crate::state::TaskStatus::Todo => Color::Blue,
        crate::state::TaskStatus::Inprogress => Color::Yellow,
        crate::state::TaskStatus::Inreview => Color::Magenta,
        crate::state::TaskStatus::Done => Color::Green,
        crate::state::TaskStatus::Cancelled => Color::Red,
    };

    let mut title_spans = vec![
        Span::styled(
            task.title.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!("[{}]", task.status.label()),
            Style::default().fg(status_color),
        ),
    ];

    // Add attempt status inline
    if task.has_in_progress_attempt {
        title_spans.push(Span::raw(" "));
        title_spans.push(Span::styled("Running", Style::default().fg(Color::Yellow)));
    } else if task.last_attempt_failed {
        title_spans.push(Span::raw(" "));
        title_spans.push(Span::styled("Failed", Style::default().fg(Color::Red)));
    }

    let title = Paragraph::new(Line::from(title_spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(title, chunks[chunk_idx]);
    chunk_idx += 1;

    // Linear URL row
    if has_linear {
        let linear_text = task
            .linear_url
            .as_deref()
            .or(task.linear_issue_id.as_deref())
            .unwrap_or("");

        let linear = Paragraph::new(linear_text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Linear ")
                .border_style(Style::default().fg(Color::Blue)),
        );
        frame.render_widget(linear, chunks[chunk_idx]);
        chunk_idx += 1;
    }

    // PR URL row
    if has_pr {
        let pr_url = task.pr_url.as_deref().unwrap_or("");
        let pr_status_color = match task.pr_status.as_deref() {
            Some("merged") => Color::Magenta,
            Some("closed") => Color::Red,
            _ => Color::Green,
        };

        let pr = Paragraph::new(pr_url).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Pull Request ")
                .border_style(Style::default().fg(pr_status_color)),
        );
        frame.render_widget(pr, chunks[chunk_idx]);
        chunk_idx += 1;
    }

    // Plan section with scrolling
    if let Some(plan_content) = plan {
        // Calculate visible area height (minus borders)
        let plan_area = chunks[chunk_idx];
        let visible_height = plan_area.height.saturating_sub(2) as usize;

        // Get lines with offset for scrolling
        let lines: Vec<&str> = plan_content.lines().collect();
        let visible_lines: Vec<Line> = lines
            .iter()
            .skip(plan_scroll_offset)
            .take(visible_height)
            .map(|l| Line::from(*l))
            .collect();

        // Build title with scroll position
        let title = if plan_line_count > visible_height {
            let current_pos = plan_scroll_offset + 1;
            format!(" Plan [{}/{}] ", current_pos, plan_line_count)
        } else {
            " Plan ".to_string()
        };

        let plan_widget = Paragraph::new(visible_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Magenta)),
        );
        frame.render_widget(plan_widget, plan_area);
        chunk_idx += 1;
    }

    // Description
    let description_text = task.description.as_deref().unwrap_or("No description");

    let description = Paragraph::new(description_text)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Description ")
                .border_style(Style::default().fg(Color::DarkGray)),
        );

    frame.render_widget(description, chunks[chunk_idx]);
}

pub fn render_task_detail_with_actions(
    frame: &mut Frame,
    area: Rect,
    task: &Task,
    plan: Option<&str>,
    plan_scroll_offset: usize,
    plan_line_count: usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // Task detail
            Constraint::Length(3), // Actions bar
        ])
        .split(area);

    render_task_detail(
        frame,
        chunks[0],
        task,
        plan,
        plan_scroll_offset,
        plan_line_count,
    );

    // Actions bar
    let actions = Paragraph::new(Line::from(vec![
        Span::styled("[g]", Style::default().fg(Color::Cyan)),
        Span::raw(" Gas it  "),
        Span::styled("[p]", Style::default().fg(Color::Cyan)),
        Span::raw(" Plan it  "),
        Span::styled("[P]", Style::default().fg(Color::Cyan)),
        Span::raw(" View Plan  "),
        Span::styled("[v]", Style::default().fg(Color::Cyan)),
        Span::raw(" View PR  "),
        Span::styled("[e]", Style::default().fg(Color::Cyan)),
        Span::raw(" Edit  "),
        Span::styled("[d]", Style::default().fg(Color::Cyan)),
        Span::raw(" Delete"),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Actions ")
            .border_style(Style::default().fg(Color::DarkGray)),
    );

    frame.render_widget(actions, chunks[1]);
}
