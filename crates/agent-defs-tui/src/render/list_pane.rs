use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::Frame;

use crate::app::App;
use crate::grouping::ListRow;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Definitions ");

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let visible_height = inner.height as usize;
    if visible_height == 0 || app.flat_items.is_empty() {
        return;
    }

    // Adjust scroll so cursor is always visible.
    let scroll_offset = compute_scroll_offset(app.cursor, app.list_scroll_offset, visible_height);

    let lines: Vec<Line> = app
        .flat_items
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_height)
        .map(|(idx, row)| render_row(row, idx == app.cursor, app))
        .collect();

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);

    // Render scrollbar if content exceeds visible height.
    if app.flat_items.len() > visible_height {
        let mut scrollbar_state = ScrollbarState::new(app.flat_items.len())
            .position(scroll_offset)
            .viewport_content_length(visible_height);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("█");

        frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
    }
}

fn render_row<'a>(row: &ListRow, is_selected: bool, app: &App) -> Line<'a> {
    match row {
        ListRow::Header { label, count } => {
            let style = Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD);
            Line::from(Span::styled(format!("{label} ({count})"), style))
        }
        ListRow::Item { summary_index } => {
            let name = app
                .view_summaries
                .get(*summary_index)
                .map(|s| s.name.as_str())
                .unwrap_or("???");

            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Line::from(Span::styled(format!("  {name}"), style))
        }
    }
}

fn compute_scroll_offset(cursor: usize, current_offset: usize, visible_height: usize) -> usize {
    if cursor < current_offset {
        cursor
    } else if cursor >= current_offset + visible_height {
        cursor.saturating_sub(visible_height - 1)
    } else {
        current_offset
    }
}
