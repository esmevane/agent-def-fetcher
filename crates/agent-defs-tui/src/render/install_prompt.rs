use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

/// Render the file explorer overlay for selecting install directory.
pub fn render_explorer(frame: &mut Frame, area: Rect, app: &App) {
    let Some(explorer) = &app.file_explorer else {
        return;
    };

    // Center the overlay (60% width, 70% height)
    let popup_area = centered_rect(60, 70, area);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    // Split into explorer area, preview, and hint bar
    let chunks = Layout::default()
        .constraints([
            Constraint::Min(3),
            Constraint::Length(2),
            Constraint::Length(2),
        ])
        .split(popup_area);

    // Render the block/border
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .title(" Select Install Directory ");
    let inner = block.inner(chunks[0]);
    frame.render_widget(block, chunks[0]);

    // Render the file explorer inside the block
    frame.render_widget(&explorer.widget(), inner);

    // Render install path preview
    let preview_style = Style::default().fg(Color::DarkGray);
    let path_style = Style::default().fg(Color::Yellow);

    let preview_text = if let Some(def) = &app.selected_definition {
        let target = explorer.cwd();
        let install_path = agent_defs::install::install_path(target, def);
        Line::from(vec![
            Span::styled(" Will install to: ", preview_style),
            Span::styled(install_path.display().to_string(), path_style),
        ])
    } else {
        Line::from(Span::styled(" No definition selected", preview_style))
    };

    frame.render_widget(Paragraph::new(preview_text), chunks[1]);

    // Render hint bar
    let hint_style = Style::default().fg(Color::DarkGray);
    let key_style = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);

    let hints = Line::from(vec![
        Span::styled(" j/k", key_style),
        Span::styled(" nav  ", hint_style),
        Span::styled("Enter", key_style),
        Span::styled(" open  ", hint_style),
        Span::styled("i", key_style),
        Span::styled(" install  ", hint_style),
        Span::styled("I", key_style),
        Span::styled(" quick  ", hint_style),
        Span::styled("Esc", key_style),
        Span::styled(" cancel", hint_style),
    ]);

    frame.render_widget(Paragraph::new(hints), chunks[2]);
}

/// Render the install confirmation dialog.
pub fn render_confirm(frame: &mut Frame, area: Rect, app: &App) {
    // Smaller centered dialog
    let popup_area = centered_rect(50, 30, area);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Confirm Installation ");
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Content layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .margin(1)
        .split(inner);

    // Question
    let question = Paragraph::new(Line::from(vec![
        Span::styled("Install to this location?", Style::default().fg(Color::White)),
    ]));
    frame.render_widget(question, chunks[0]);

    // Path
    let path_text = if let Some(path) = &app.pending_install_path {
        path.display().to_string()
    } else if let (Some(target), Some(def)) = (&app.install_target, &app.selected_definition) {
        agent_defs::install::install_path(target, def)
            .display()
            .to_string()
    } else {
        "(unknown)".to_string()
    };

    let path_para = Paragraph::new(Line::from(Span::styled(
        path_text,
        Style::default().fg(Color::Yellow),
    )))
    .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(path_para, chunks[1]);

    // Hint bar
    let hint_style = Style::default().fg(Color::DarkGray);
    let key_style = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);

    let hints = Line::from(vec![
        Span::styled(" Enter/y", key_style),
        Span::styled(" confirm  ", hint_style),
        Span::styled("Esc/n", key_style),
        Span::styled(" cancel", hint_style),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[2]);
}

/// Helper to create a centered rectangle.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
