use crate::app::App;
use ansi_to_tui::IntoText;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

pub fn render(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    render_header(f, app, chunks[0]);

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // Step list
            Constraint::Percentage(75), // Logs
        ])
        .split(chunks[1]);

    render_steps(f, app, main_chunks[0]);
    render_logs(f, app, main_chunks[1]);

    render_footer(f, chunks[2]);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let title = if let Some(run) = &app.run {
        let status = &run.status_humanized;
        let duration = run.duration_humanized.as_deref().unwrap_or("00:00:00");
        let job_id = run.job_definition_id.unwrap_or(0);
        let env_id = run.environment_id.unwrap_or(0);
        format!(
            " Run ID: {} | Job: {} | Env: {} | Status: {} | Duration: {} ",
            run.id, job_id, env_id, status, duration
        )
    } else {
        " Loading... ".to_string()
    };

    let header_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Cyan));

    f.render_widget(header_block, area);
}

fn render_steps(f: &mut Frame, app: &App, area: Rect) {
    let mut items = Vec::new();
    if let Some(run) = &app.run
        && let Some(steps) = &run.run_steps
    {
        for (i, step) in steps.iter().enumerate() {
            let prefix = match step.status {
                10 => "[✓]", // Success
                20 => "[✗]", // Error
                3 => "[⟳]",  // Running
                _ => "[ ]",  // Other/Queued
            };
            let style = if i == app.selected_step_idx {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let content = format!("{} {}", prefix, step.name);
            items.push(ListItem::new(Line::from(Span::styled(content, style))));
        }
    }

    let steps_block = Block::default().title(" Steps ").borders(Borders::ALL);
    let list = List::new(items).block(steps_block);

    let mut state = ListState::default();
    state.select(Some(app.selected_step_idx));

    f.render_stateful_widget(list, area, &mut state);
}

fn render_logs(f: &mut Frame, app: &mut App, area: Rect) {
    let mut log_text = Text::raw("No logs available.");
    let mut max_scroll = 0;

    if let Some(run) = &app.run
        && let Some(steps) = &run.run_steps
        && let Some(step) = steps.get(app.selected_step_idx)
    {
        let logs_str = step.logs.as_deref().unwrap_or("");
        if !logs_str.is_empty() {
            let logs_bytes = logs_str.as_bytes();
            if let Ok(parsed_text) = logs_bytes.into_text() {
                log_text = parsed_text;
            } else {
                log_text = Text::raw(logs_str);
            }
        } else {
            log_text = Text::raw("Waiting for logs...");
        }
    }

    let inner_height = area.height.saturating_sub(2);
    let text_lines = log_text.height() as u16;

    if text_lines > inner_height {
        max_scroll = text_lines - inner_height;
    }

    if app.auto_follow || app.scroll_offset > max_scroll {
        app.scroll_offset = max_scroll;
    }

    let block = Block::default().title(" Logs ").borders(Borders::ALL);
    let paragraph = Paragraph::new(log_text)
        .block(block)
        .scroll((app.scroll_offset, app.h_scroll_offset));

    f.render_widget(paragraph, area);
}

fn render_footer(f: &mut Frame, area: Rect) {
    let footer_text = " q/Esc: Quit | Tab: Switch Step | hjkl/Arrows: Scroll | H: Jump to Top | G: Jump to Bottom | r: Refresh Now ";
    let block = Block::default().borders(Borders::ALL);
    let paragraph = Paragraph::new(footer_text)
        .block(block)
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(paragraph, area);
}
