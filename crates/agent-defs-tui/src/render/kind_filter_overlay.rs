use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let kinds = app.available_kinds();

    // Each kind is one line, plus "All" at the top.
    let item_count = 1 + kinds.len();
    let popup_height = (item_count as u16) + 2; // +2 for borders
    let popup_width = 30u16.min(area.width.saturating_sub(4));

    let popup_area = centered_rect(popup_width, popup_height, area);

    // Clear background under the popup.
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Filter by Kind ")
        .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let normal_style = Style::default().fg(Color::White);
    let selected_style = Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let count_style = Style::default().fg(Color::DarkGray);

    let mut lines: Vec<Line> = Vec::with_capacity(item_count);

    // "All" option.
    let all_style = if app.kind_filter_cursor == 0 {
        selected_style
    } else {
        normal_style
    };
    lines.push(Line::from(Span::styled("  All", all_style)));

    // Kind options with counts.
    let kind_counts = compute_kind_counts(app);
    for (i, kind) in kinds.iter().enumerate() {
        let cursor_idx = i + 1;
        let style = if app.kind_filter_cursor == cursor_idx {
            selected_style
        } else {
            normal_style
        };

        let count = kind_counts
            .iter()
            .find(|(k, _)| k == kind)
            .map(|(_, c)| *c)
            .unwrap_or(0);

        let label = format!("  {}", kind.display_label());
        let count_text = format!(" ({count})");

        lines.push(Line::from(vec![
            Span::styled(label, style),
            Span::styled(count_text, count_style),
        ]));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

fn compute_kind_counts(app: &App) -> Vec<(agent_defs::DefinitionKind, usize)> {
    let mut counts: Vec<(agent_defs::DefinitionKind, usize)> = Vec::new();
    for s in &app.summaries {
        if let Some(entry) = counts.iter_mut().find(|(k, _)| k == &s.kind) {
            entry.1 += 1;
        } else {
            counts.push((s.kind.clone(), 1));
        }
    }
    counts
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Length(width)]).flex(Flex::Center);

    let [vertical_area] = vertical.areas(area);
    let [centered] = horizontal.areas(vertical_area);
    centered
}
