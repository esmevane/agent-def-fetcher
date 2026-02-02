use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let line = if let Some(msg) = &app.status_message {
        let style = if msg.is_error {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::Green)
        };
        Line::from(Span::styled(format!(" {}", msg.text), style))
    } else {
        let hint_style = Style::default().fg(Color::DarkGray);
        Line::from(vec![
            Span::styled(" \u{2191}\u{2193}", hint_style),
            Span::styled(" navigate  ", hint_style),
            Span::styled("/", hint_style),
            Span::styled(" search  ", hint_style),
            Span::styled("f", hint_style),
            Span::styled(" kind  ", hint_style),
            Span::styled("p", hint_style),
            Span::styled(" source  ", hint_style),
            Span::styled("\u{23ce}", hint_style), // ⏎ Enter symbol
            Span::styled(" install  ", hint_style),
            Span::styled("s", hint_style),
            Span::styled(" sync  ", hint_style),
            Span::styled("c", hint_style),
            Span::styled(" copy  ", hint_style),
            Span::styled("q", hint_style),
            Span::styled(" quit", hint_style),
        ])
    };

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}
