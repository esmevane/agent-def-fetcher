use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, LoadingState};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let is_syncing = app.loading == LoadingState::Syncing;

    // Determine popup size based on content
    let (popup_height, popup_width) = if is_syncing {
        (5u16, 30u16)
    } else if let Some(result) = &app.sync_result {
        let warning_count = result.warnings.len();
        let content_height = if warning_count == 0 {
            3 // Just the message
        } else {
            4 + warning_count.min(10) as u16 // Message + header + warnings (max 10 visible)
        };
        (content_height + 2, 60u16.min(area.width.saturating_sub(4)))
    } else {
        (5u16, 30u16)
    };

    let popup_area = centered_rect(popup_width, popup_height, area);

    // Clear background under the popup.
    frame.render_widget(Clear, popup_area);

    let title = if is_syncing { " Syncing... " } else { " Sync Complete " };
    let title_color = if is_syncing { Color::Yellow } else { Color::Green };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_style(Style::default().fg(title_color).add_modifier(Modifier::BOLD));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if is_syncing {
        render_syncing(frame, inner);
    } else if let Some(result) = &app.sync_result {
        render_result(frame, inner, result, app.sync_result_scroll);
    }
}

fn render_syncing(frame: &mut Frame, area: Rect) {
    let style = Style::default().fg(Color::Yellow);
    let text = "Fetching definitions from sources...";
    let paragraph = Paragraph::new(text)
        .style(style)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn render_result(frame: &mut Frame, area: Rect, result: &crate::SyncResult, scroll: usize) {
    let mut lines: Vec<Line> = Vec::new();

    // Summary message
    let msg_style = Style::default().fg(Color::Green);
    lines.push(Line::from(Span::styled(&result.message, msg_style)));

    if !result.warnings.is_empty() {
        lines.push(Line::from("")); // blank line

        let header_style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        lines.push(Line::from(Span::styled(
            format!("Warnings ({}):", result.warnings.len()),
            header_style,
        )));

        let warning_style = Style::default().fg(Color::DarkGray);
        for warning in result.warnings.iter().skip(scroll).take(10) {
            // Truncate long warnings
            let display = if warning.len() > 55 {
                format!("  {}...", &warning[..52])
            } else {
                format!("  {}", warning)
            };
            lines.push(Line::from(Span::styled(display, warning_style)));
        }

        if result.warnings.len() > 10 {
            let more = result.warnings.len().saturating_sub(scroll + 10);
            if more > 0 {
                lines.push(Line::from(Span::styled(
                    format!("  ... and {} more (j/k to scroll)", more),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
    }

    lines.push(Line::from("")); // blank line
    let hint_style = Style::default().fg(Color::DarkGray);
    lines.push(Line::from(Span::styled("Press Enter to dismiss", hint_style)));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Length(width)]).flex(Flex::Center);

    let [vertical_area] = vertical.areas(area);
    let [centered] = horizontal.areas(vertical_area);
    centered
}
