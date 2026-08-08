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
    let [area] = Layout::horizontal([Constraint::Length(width)]).flex(Flex::Center).areas(area);
    let [area] = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center).areas(area);
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

/// Draw a centered, bordered black panel and return the area inside it. The
/// shared scaffold behind every popup in the game.
pub fn popup_frame(frame: &mut Frame, title: &str, width: u16, height: u16) -> Rect {
    let area = centered_rect(frame.area(), width, height);
    let block = Block::bordered()
        .title_top(Line::from(title).centered())
        .style(Style::new().bg(Color::Black).fg(Color::White).bold());
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    inner
}

/// Centered info popup listing static `lines` (no selection). For lists too
/// large for a toast (e.g. owned properties).
pub fn info_popup(frame: &mut Frame, title: &str, lines: &[String]) {
    let height = (lines.len() as u16 + 2).clamp(3, frame.area().height);
    let inner = popup_frame(frame, title, 48, height);
    let body: Vec<Line> = if lines.is_empty() {
        vec![Line::from("(empty)").centered()]
    } else {
        lines.iter().map(|l| Line::from(format!("  {l}"))).collect()
    };
    frame.render_widget(Paragraph::new(body), inner);
}

/// Centered black popup listing `options` with `selected` highlighted. Width
/// grows to fit the longest option (and the title), with a 28-col floor.
pub fn choice_popup<S: AsRef<str>>(frame: &mut Frame, title: &str, options: &[S], selected: usize) {
    let longest = options.iter().map(|o| o.as_ref().chars().count()).max().unwrap_or(0) as u16;
    let width = (longest + 4).max(title.chars().count() as u16 + 4).max(28);
    let inner = popup_frame(frame, title, width, options.len() as u16 + 2);
    frame.render_widget(Paragraph::new(selectable_lines(options, selected)), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Modifier;

    // --- Cursor -------------------------------------------------------------

    #[test]
    fn the_cursor_starts_at_the_top_and_stops_there() {
        let mut cursor = Cursor::new(3);
        assert_eq!(cursor.selected, 0);
        cursor.up();
        assert_eq!(cursor.selected, 0);
    }

    #[test]
    fn the_cursor_stops_at_the_last_row() {
        let mut cursor = Cursor::new(3);
        for _ in 0..10 {
            cursor.down();
        }
        assert_eq!(cursor.selected, 2);
    }

    #[test]
    fn an_empty_list_leaves_the_cursor_at_zero() {
        let mut cursor = Cursor::new(0);
        cursor.down();
        assert_eq!(cursor.selected, 0);
    }

    #[test]
    fn shrinking_a_list_pulls_the_cursor_back_into_range() {
        let mut cursor = Cursor::new(5);
        for _ in 0..4 {
            cursor.down();
        }
        assert_eq!(cursor.selected, 4);
        cursor.set_len(2);
        assert_eq!(cursor.selected, 1);
    }

    // --- Confirm ------------------------------------------------------------

    #[test]
    fn a_prompt_defaults_to_no() {
        let confirm = Confirm::new();
        assert!(!confirm.is_yes());
        assert!(matches!(Confirm::new().handle_key(KeyCode::Enter), ConfirmResult::No));
    }

    #[test]
    fn any_arrow_toggles_the_answer() {
        for key in [KeyCode::Up, KeyCode::Down, KeyCode::Left, KeyCode::Right] {
            let mut confirm = Confirm::new();
            assert!(matches!(confirm.handle_key(key), ConfirmResult::Pending));
            assert!(confirm.is_yes());
        }
    }

    #[test]
    fn enter_resolves_the_highlighted_answer() {
        let mut confirm = Confirm::new();
        confirm.toggle();
        assert!(matches!(confirm.handle_key(KeyCode::Enter), ConfirmResult::Yes));
    }

    #[test]
    fn escape_always_means_no() {
        let mut confirm = Confirm::new();
        confirm.toggle();
        assert!(matches!(confirm.handle_key(KeyCode::Esc), ConfirmResult::No));
    }

    #[test]
    fn an_unrelated_key_leaves_the_prompt_pending() {
        let mut confirm = Confirm::new();
        assert!(matches!(confirm.handle_key(KeyCode::Char('x')), ConfirmResult::Pending));
        assert!(!confirm.is_yes(), "and does not move the highlight");
    }

    // --- layout and lists ---------------------------------------------------

    #[test]
    fn a_centered_rect_sits_in_the_middle() {
        let area = Rect::new(0, 0, 100, 50);
        let inner = centered_rect(area, 20, 10);
        assert_eq!((inner.width, inner.height), (20, 10));
        assert_eq!(inner.x, 40);
        assert_eq!(inner.y, 20);
    }

    #[test]
    fn a_centered_rect_never_exceeds_its_area() {
        let area = Rect::new(0, 0, 10, 4);
        let inner = centered_rect(area, 40, 20);
        assert!(inner.width <= area.width);
        assert!(inner.height <= area.height);
    }

    #[test]
    fn only_the_selected_line_is_highlighted() {
        let lines = selectable_lines(&["one", "two", "three"], 1);
        assert_eq!(lines.len(), 3);
        assert!(lines[1].style.add_modifier.contains(Modifier::REVERSED));
        assert!(!lines[0].style.add_modifier.contains(Modifier::REVERSED));
        assert!(!lines[2].style.add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn a_selection_past_the_end_highlights_nothing() {
        let lines = selectable_lines(&["one", "two"], 9);
        assert!(lines.iter().all(|l| !l.style.add_modifier.contains(Modifier::REVERSED)));
    }
}
