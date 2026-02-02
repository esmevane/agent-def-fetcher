use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap};
use ratatui::Frame;

use crate::app::{App, LoadingState};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Detail ");

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.loading == LoadingState::Fetching && app.selected_definition.is_none() {
        let loading = Paragraph::new("Loading...")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(loading, inner);
        return;
    }

    let Some(def) = &app.selected_definition else {
        let hint = Paragraph::new("Select a definition to view details")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint, inner);
        return;
    };

    let label_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let value_style = Style::default().fg(Color::White);
    let dim_style = Style::default().fg(Color::DarkGray);

    let mut lines: Vec<Line> = Vec::new();

    // Name
    lines.push(Line::from(vec![
        Span::styled("Name:     ", label_style),
        Span::styled(def.name.clone(), value_style),
    ]));

    // Kind
    lines.push(Line::from(vec![
        Span::styled("Kind:     ", label_style),
        Span::styled(def.kind.to_string(), value_style),
    ]));

    // Category
    if let Some(cat) = &def.category {
        lines.push(Line::from(vec![
            Span::styled("Category: ", label_style),
            Span::styled(cat.clone(), value_style),
        ]));
    }

    // Model
    if let Some(model) = &def.model {
        lines.push(Line::from(vec![
            Span::styled("Model:    ", label_style),
            Span::styled(model.clone(), value_style),
        ]));
    }

    // Tools
    if !def.tools.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("Tools:    ", label_style),
            Span::styled(def.tools.join(", "), value_style),
        ]));
    }

    // Source
    lines.push(Line::from(vec![
        Span::styled("Source:   ", label_style),
        Span::styled(def.source_label.clone(), value_style),
    ]));

    // ID
    lines.push(Line::from(vec![
        Span::styled("ID:       ", label_style),
        Span::styled(def.id.to_string(), dim_style),
    ]));

    // Separator
    lines.push(Line::from(""));
    let separator_width = inner.width as usize;
    lines.push(Line::from(Span::styled(
        "\u{2500}".repeat(separator_width),
        dim_style,
    )));
    lines.push(Line::from(""));

    // Body
    for line in def.body.lines() {
        lines.push(Line::from(line.to_owned()));
    }

    let content_length = lines.len();
    let visible_height = inner.height as usize;

    let paragraph = Paragraph::new(lines)
        .scroll((app.detail_scroll, 0))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, inner);

    // Render scrollbar if content exceeds visible height.
    if content_length > visible_height {
        let mut scrollbar_state = ScrollbarState::new(content_length)
            .position(app.detail_scroll as usize)
            .viewport_content_length(visible_height);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .thumb_symbol("█");

        frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
    }
}
