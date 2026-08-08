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
    text::{Line, Span},
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

/// How a selectable list sits in its popup.
#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    /// Short, uniform options (the main menu).
    Center,
    /// Data rows of differing length, which read as ragged when centered.
    Left,
}

/// One `Line` per option, with `selected` drawn reversed. `Left` rows are
/// indented and padded to `width` so the highlight is a full-width bar.
pub fn selectable_lines<S: AsRef<str>>(
    options: &[S],
    selected: usize,
    align: Align,
    width: u16,
) -> Vec<Line<'static>> {
    options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let line = match align {
                Align::Center => Line::from(opt.as_ref().to_string()).centered(),
                Align::Left => Line::from(format!("  {:<w$}", opt.as_ref(), w = width as usize)),
            };
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
        choice_popup(frame, title, &["Yes", "No"], self.selected, CONFIRM_KEYS);
    }
}

/// Every popup is this wide, so they don't resize as the turn moves between
/// them. Wide enough for the longest deed line and the longest key hint.
const POPUP_W: u16 = 54;

/// The hint every scrolling list shares; `keys!` builds anything else.
pub const LIST_KEYS: &str = "↑↓ move · enter select · esc back";

/// A yes/no prompt: every arrow toggles, and Esc means no.
pub const CONFIRM_KEYS: &str = "↑↓ choose · enter confirm · esc cancel";

/// Build a key-hint footer: `keys!("enter" => "build", "s" => "sell")` renders
/// as `enter build · s sell`. The one place hint syntax is decided.
#[macro_export]
macro_rules! keys {
    ($($key:expr => $action:expr),+ $(,)?) => {
        [$(concat!($key, " ", $action)),+].join(" · ")
    };
}

/// Split `label` at its hotkey letter so the key reads *inside* the word:
/// `Build Houses` + `h` → `Build ` + **H** + `ouses`. Falls back to appending
/// the key in brackets when the label doesn't contain it (e.g. Space).
pub fn mnemonic(
    label: &'static str,
    key: char,
    key_style: Style,
    rest_style: Style,
) -> Vec<Span<'static>> {
    // Prefer the start of a word — the letter a player actually associates with
    // the label. "View Inventory" + 'i' is Inventory, not the i in View.
    let at_word_start = |i: usize| i == 0 || label[..i].ends_with(' ');
    let hit = label
        .char_indices()
        .find(|&(i, c)| c.eq_ignore_ascii_case(&key) && at_word_start(i))
        .or_else(|| label.char_indices().find(|(_, c)| c.eq_ignore_ascii_case(&key)));
    match hit {
        Some((i, c)) => vec![
            Span::styled(&label[..i], rest_style),
            Span::styled(c.to_uppercase().to_string(), key_style),
            Span::styled(&label[i + c.len_utf8()..], rest_style),
        ],
        None => vec![Span::styled(label, rest_style), Span::styled(format!(" {key}"), key_style)],
    }
}

/// Draw a centered, bordered black panel and return the area inside it. `keys`
/// is the key-hint footer, drawn into the bottom border so it always sits in
/// the same place and never costs a content row.
pub fn popup_frame(frame: &mut Frame, title: &str, keys: &str, height: u16) -> Rect {
    let area = centered_rect(frame.area(), POPUP_W, height);
    let block = Block::bordered()
        .title_top(Line::from(format!(" {} ", title.trim())).centered())
        .title_bottom(Line::from(format!(" {} ", keys.trim())).centered().dim())
        .style(Style::new().bg(Color::Black).fg(Color::White).bold());
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    inner
}

/// Centered info popup listing static `lines` (no selection). For lists too
/// large for a toast (e.g. owned properties).
pub fn info_popup(frame: &mut Frame, title: &str, lines: &[String], keys: &str) {
    let height = (lines.len() as u16 + 2).clamp(3, frame.area().height);
    let inner = popup_frame(frame, title, keys, height);
    let body: Vec<Line> = lines.iter().map(|l| Line::from(format!("  {l}"))).collect();
    frame.render_widget(Paragraph::new(body), inner);
}

/// Centered black popup listing `options` with `selected` highlighted.
pub fn choice_popup<S: AsRef<str>>(
    frame: &mut Frame,
    title: &str,
    options: &[S],
    selected: usize,
    keys: &str,
) {
    let inner = popup_frame(frame, title, keys, options.len() as u16 + 2);
    let rows = selectable_lines(options, selected, Align::Left, inner.width);
    frame.render_widget(Paragraph::new(rows), inner);
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
        let lines = selectable_lines(&["one", "two", "three"], 1, Align::Center, 20);
        assert_eq!(lines.len(), 3);
        assert!(lines[1].style.add_modifier.contains(Modifier::REVERSED));
        assert!(!lines[0].style.add_modifier.contains(Modifier::REVERSED));
        assert!(!lines[2].style.add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn a_selection_past_the_end_highlights_nothing() {
        let lines = selectable_lines(&["one", "two"], 9, Align::Center, 20);
        assert!(lines.iter().all(|l| !l.style.add_modifier.contains(Modifier::REVERSED)));
    }
}

#[cfg(test)]
mod chrome_tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    /// Render one popup and return its rows as trimmed strings.
    fn draw(f: impl FnOnce(&mut Frame)) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(70, 12)).expect("backend");
        terminal.draw(f).expect("draw");
        let buf = terminal.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .filter(|row| !row.trim().is_empty())
            .collect()
    }

    #[test]
    fn a_mnemonic_splits_the_label_at_its_key() {
        let text = |spans: Vec<Span>| -> Vec<String> {
            spans.into_iter().map(|s| s.content.into_owned()).collect()
        };
        let plain = Style::new();
        assert_eq!(text(mnemonic("Build Houses", 'h', plain, plain)), ["Build ", "H", "ouses"]);
        assert_eq!(text(mnemonic("Trade", 't', plain, plain)), ["", "T", "rade"]);
    }

    #[test]
    fn a_mnemonic_prefers_the_start_of_a_word() {
        let text = |spans: Vec<Span>| -> Vec<String> {
            spans.into_iter().map(|s| s.content.into_owned()).collect()
        };
        let plain = Style::new();
        // Not the "i" in View.
        assert_eq!(text(mnemonic("View Inventory", 'i', plain, plain)), ["View ", "I", "nventory"]);
    }

    #[test]
    fn a_key_not_in_the_label_is_appended_instead() {
        let spans = mnemonic("Roll", ' ', Style::new(), Style::new());
        let joined: String = spans.into_iter().map(|s| s.content.into_owned()).collect();
        assert_eq!(joined, "Roll  ");
    }

    /// Every hint string the game can show, so the vocabulary stays in step.
    fn all_hints() -> Vec<String> {
        use crate::game::hint_strings;
        let mut hints = vec![LIST_KEYS.to_string(), CONFIRM_KEYS.to_string()];
        hints.extend(hint_strings());
        hints
    }

    #[test]
    fn arrow_hints_always_name_both_directions() {
        for hint in all_hints() {
            for (one, pair) in [('↑', "↑↓"), ('↓', "↑↓"), ('←', "←→"), ('→', "←→")]
            {
                if hint.contains(one) {
                    assert!(hint.contains(pair), "{hint:?} shows one arrow of a pair");
                }
            }
        }
    }

    #[test]
    fn hints_spell_keys_the_same_way_everywhere() {
        for hint in all_hints() {
            assert!(!hint.contains('⏎'), "{hint:?}: spell Enter as `enter`");
            assert!(!hint.contains('␣'), "{hint:?}: spell Space as `space`");
            for word in ["Enter", "Esc", "Space"] {
                assert!(!hint.contains(word), "{hint:?}: popup keys are lowercase");
            }
        }
    }

    #[test]
    fn a_scrolling_list_always_says_how_to_scroll() {
        for hint in all_hints() {
            if hint.contains("select") || hint.contains("mortgage") || hint.contains("build") {
                assert!(hint.contains("↑↓"), "{hint:?} scrolls but doesn't say so");
            }
        }
    }

    #[test]
    fn the_hint_syntax_is_built_in_one_place() {
        assert_eq!(keys!("enter" => "build"), "enter build");
        assert_eq!(keys!("s" => "sell", "esc" => "back"), "s sell · esc back");
    }

    #[test]
    fn a_title_sits_in_the_top_border_and_hints_in_the_bottom() {
        let rows = draw(|f| {
            choice_popup(f, "Build Houses", &["Baltic Ave"], 0, &keys!("s" => "sell"));
        });
        assert!(rows.first().expect("a top border").contains("Build Houses"));
        assert!(rows.last().expect("a bottom border").contains("s sell"));
        assert!(
            !rows[1..rows.len() - 1].iter().any(|r| r.contains("s sell")),
            "hints must not cost a content row"
        );
    }

    #[test]
    fn every_popup_is_the_same_width() {
        let width = |rows: Vec<String>| rows[0].trim().chars().count();
        let narrow = width(draw(|f| choice_popup(f, "A", &["b"], 0, "c")));
        let wide = width(draw(|f| {
            info_popup(
                f,
                "A much longer popup title",
                &["a considerably longer body line than the other".into()],
                &keys!("enter" => "confirm", "esc" => "back"),
            )
        }));
        assert_eq!(narrow, wide, "popups must not resize between turns");
    }

    #[test]
    fn list_rows_are_left_aligned_so_they_do_not_read_as_ragged() {
        let rows = draw(|f| {
            choice_popup(f, "Deeds", &["Mediterranean Ave", "Baltic Ave"], 0, "esc back");
        });
        let indent = |row: &String| row.find(|c: char| c.is_alphanumeric()).expect("text");
        assert_eq!(indent(&rows[1]), indent(&rows[2]));
    }
}
