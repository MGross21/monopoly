//! Shared UI helpers (centered rects, list cursor, Yes/No prompt) plus the
//! screens that render the game: the board map, dice, menu, and setup.

pub mod dice;
pub mod map;
pub mod menu;
pub mod setup;

use crossterm::event::KeyCode;
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

/// A bounded selection index for vertical menus. `up`/`down` clamp at the ends.
pub struct Cursor {
    pub selected: usize,
    len: usize,
}

impl Cursor {
    pub fn new(len: usize) -> Self {
        Self { selected: 0, len }
    }

    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn down(&mut self) {
        self.selected = (self.selected + 1).min(self.len.saturating_sub(1));
    }

    /// Resize the list (e.g. after the player count changes), keeping the
    /// selection in range.
    pub fn set_len(&mut self, len: usize) {
        self.len = len;
        self.selected = self.selected.min(len.saturating_sub(1));
    }
}

/// One centered `Line` per option, with `selected` drawn reversed.
pub fn selectable_lines<S: AsRef<str>>(options: &[S], selected: usize) -> Vec<Line<'static>> {
    options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let line = Line::from(opt.as_ref().to_string()).centered();
            if i == selected { line.reversed() } else { line }
        })
        .collect()
}

/// Outcome of feeding a key to a [`Confirm`] prompt.
pub enum ConfirmResult {
    Pending,
    Yes,
    No,
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

    /// Drive the prompt from a key press: arrows toggle, Enter resolves, Esc
    /// cancels (treated as No).
    pub fn handle_key(&mut self, key: KeyCode) -> ConfirmResult {
        match key {
            KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
                self.toggle();
                ConfirmResult::Pending
            }
            KeyCode::Enter => {
                if self.is_yes() {
                    ConfirmResult::Yes
                } else {
                    ConfirmResult::No
                }
            }
            KeyCode::Esc => ConfirmResult::No,
            _ => ConfirmResult::Pending,
        }
    }

    pub fn render(&self, frame: &mut Frame, title: &str) {
        choice_popup(frame, title, &["Yes", "No"], self.selected);
    }
}

/// Centered info popup listing static `lines` (no selection). For lists too
/// large for a toast (e.g. owned properties).
pub fn info_popup(frame: &mut Frame, title: &str, lines: &[String]) {
    let width = 48;
    let height = (lines.len() as u16 + 2).clamp(3, frame.area().height);
    let area = centered_rect(frame.area(), width, height);
    let block = Block::bordered()
        .title_top(Line::from(title).centered())
        .style(Style::new().bg(Color::Black).fg(Color::White).bold());
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let body: Vec<Line> = if lines.is_empty() {
        vec![Line::from("(empty)").centered()]
    } else {
        lines.iter().map(|l| Line::from(format!("  {l}"))).collect()
    };
    frame.render_widget(Paragraph::new(body), inner);
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
    frame.render_widget(Paragraph::new(selectable_lines(options, selected)), inner);
}
