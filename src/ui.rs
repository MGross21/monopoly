//! Shared UI helpers: centered rects and the reusable Yes/No prompt.

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Clear, Paragraph},
};

/// Center a `width` x `height` rect inside `area`.
pub fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    area
}

/// A Yes/No prompt tracking the highlighted row (defaults to No).
pub struct Confirm {
    selected: usize, // 0 = Yes, 1 = No
}

impl Confirm {
    pub fn new() -> Self {
        Self { selected: 1 }
    }

    pub fn toggle(&mut self) {
        self.selected = 1 - self.selected;
    }

    pub fn is_yes(&self) -> bool {
        self.selected == 0
    }

    pub fn render(&self, frame: &mut Frame, title: &str) {
        choice_popup(frame, title, &["Yes", "No"], self.selected);
    }
}

/// Centered black popup listing `options` with `selected` highlighted.
pub fn choice_popup(frame: &mut Frame, title: &str, options: &[&str], selected: usize) {
    let area = centered_rect(frame.area(), 28, options.len() as u16 + 2);
    let block = Block::bordered()
        .title_top(Line::from(title).centered())
        .style(Style::new().bg(Color::Black).fg(Color::White).bold());
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let line = Line::from(*opt).centered();
            if i == selected { line.reversed() } else { line }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}
