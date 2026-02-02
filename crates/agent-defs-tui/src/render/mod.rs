mod detail_pane;
mod install_prompt;
mod kind_filter_overlay;
mod list_pane;
mod search_bar;
mod source_filter_overlay;
mod status_bar;
mod sync_overlay;

use ratatui::layout::{Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{App, LayoutGeometry, LoadingState, Mode};

pub fn render(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // Outer layout: title bar (1), main content, bottom bar (1).
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(size);

    // Title bar.
    render_title_bar(frame, outer[0], app);

    // Main content: two horizontal panes.
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(outer[1]);

    list_pane::render(frame, panes[0], app);
    detail_pane::render(frame, panes[1], app);

    // Bottom bar: depends on mode.
    match app.mode {
        Mode::Search => search_bar::render(frame, outer[2], app),
        Mode::Normal
        | Mode::KindFilter
        | Mode::SourceFilter
        | Mode::SyncProgress
        | Mode::InstallPrompt
        | Mode::InstallConfirm => status_bar::render(frame, outer[2], app),
    }

    // Overlays (rendered on top).
    match app.mode {
        Mode::KindFilter => kind_filter_overlay::render(frame, size, app),
        Mode::SourceFilter => source_filter_overlay::render(frame, size, app),
        Mode::SyncProgress => sync_overlay::render(frame, size, app),
        Mode::InstallPrompt => install_prompt::render_explorer(frame, size, app),
        Mode::InstallConfirm => install_prompt::render_confirm(frame, size, app),
        _ => {}
    }
}

fn render_title_bar(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let title_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(Color::DarkGray);
    let filter_style = Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD);

    let mut spans = vec![
        Span::styled(" Agent Definitions", title_style),
        Span::raw("  "),
        Span::styled(format!("[{}]", app.source_label), label_style),
    ];

    if let Some(ref kind) = app.kind_filter {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("{{kind:{}}}", kind.display_label()),
            filter_style,
        ));
    }

    if let Some(ref source) = app.source_filter {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(format!("{{source:{}}}", source), filter_style));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

/// Compute layout geometry for mouse hit testing.
/// This mirrors the layout calculations in render() but returns Rect values.
pub fn compute_layout(frame_size: Rect, app: &App) -> LayoutGeometry {
    // Outer layout: title bar (1), main content, bottom bar (1).
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(frame_size);

    // Main content: two horizontal panes.
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(outer[1]);

    // Compute inner areas (excluding borders).
    let list_block = Block::default().borders(Borders::ALL);
    let list_inner = list_block.inner(panes[0]);

    let detail_block = Block::default().borders(Borders::ALL);
    let detail_inner = detail_block.inner(panes[1]);

    // Compute overlay area if one is displayed.
    let overlay = compute_overlay_rect(frame_size, app);

    // Compute file explorer list inner area for InstallPrompt mode.
    let explorer_list_inner = if app.mode == Mode::InstallPrompt {
        compute_explorer_list_inner(frame_size)
    } else {
        None
    };

    LayoutGeometry {
        list_inner,
        detail_inner,
        overlay,
        explorer_list_inner,
    }
}

/// Compute the overlay Rect based on current mode.
fn compute_overlay_rect(area: Rect, app: &App) -> Option<Rect> {
    match app.mode {
        Mode::KindFilter => {
            let kinds = app.available_kinds();
            let item_count = 1 + kinds.len();
            let popup_height = (item_count as u16) + 2;
            let popup_width = 30u16.min(area.width.saturating_sub(4));
            Some(centered_rect_fixed(popup_width, popup_height, area))
        }
        Mode::SourceFilter => {
            let sources = app.available_sources();
            let item_count = 1 + sources.len();
            let popup_height = (item_count as u16).min(15) + 2;
            let popup_width = 40u16.min(area.width.saturating_sub(4));
            Some(centered_rect_fixed(popup_width, popup_height, area))
        }
        Mode::SyncProgress => {
            let is_syncing = app.loading == LoadingState::Syncing;
            let (popup_height, popup_width) = if is_syncing {
                (5u16, 30u16)
            } else if let Some(result) = &app.sync_result {
                let warning_count = result.warnings.len();
                let content_height = if warning_count == 0 {
                    3
                } else {
                    4 + warning_count.min(10) as u16
                };
                (content_height + 2, 60u16.min(area.width.saturating_sub(4)))
            } else {
                (5u16, 30u16)
            };
            Some(centered_rect_fixed(popup_width, popup_height, area))
        }
        Mode::InstallPrompt => {
            // 60% width, 70% height
            Some(centered_rect_percent(60, 70, area))
        }
        Mode::InstallConfirm => {
            // 50% width, 30% height
            Some(centered_rect_percent(50, 30, area))
        }
        Mode::Normal | Mode::Search => None,
    }
}

/// Helper to create a centered rectangle with fixed dimensions.
fn centered_rect_fixed(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Length(width)]).flex(Flex::Center);

    let [vertical_area] = vertical.areas(area);
    let [centered] = horizontal.areas(vertical_area);
    centered
}

/// Compute the inner area of the file explorer list in InstallPrompt mode.
/// This mirrors the layout in install_prompt::render_explorer.
fn compute_explorer_list_inner(area: Rect) -> Option<Rect> {
    // Center the overlay (60% width, 70% height)
    let popup_area = centered_rect_percent(60, 70, area);

    // Split into explorer area, preview, and hint bar
    let chunks = Layout::default()
        .constraints([
            Constraint::Min(3),
            Constraint::Length(2),
            Constraint::Length(2),
        ])
        .split(popup_area);

    // The outer block with borders (rendered by install_prompt)
    let outer_block = Block::default().borders(Borders::ALL);
    let outer_inner = outer_block.inner(chunks[0]);

    // The file explorer widget has its own internal block with borders
    // (from Theme::default() which uses Borders::ALL)
    let explorer_block = Block::default().borders(Borders::ALL);
    let inner = explorer_block.inner(outer_inner);

    Some(inner)
}

/// Helper to create a centered rectangle with percentage dimensions.
fn centered_rect_percent(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
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
