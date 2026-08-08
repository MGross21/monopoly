//! The per-turn action menu: the list of things a player can do on their turn,
//! their labels/hotkeys, and the popup that renders them.

use ratatui::{
    Frame,
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Clear, Paragraph},
};

use crate::ui::{Cursor, centered_rect};

#[derive(Clone, Copy)]
pub enum TurnAction {
    RollDice,
    BuyProperty,
    BuildHouses,
    Trade,
    ViewInventory,
    Mortgages,
    SaveGame,
    EndTurn,
}

/// The actions in menu order, each with its label and hotkey. Single source of
/// truth: add an action by adding a row.
const ACTIONS: [(TurnAction, &str, char); 8] = [
    (TurnAction::RollDice, "Roll Dice", 'r'),
    (TurnAction::BuyProperty, "Buy Property", 'b'),
    (TurnAction::BuildHouses, "Build Houses", 'h'),
    (TurnAction::Trade, "Trade", 't'),
    (TurnAction::ViewInventory, "View Inventory", 'i'),
    (TurnAction::Mortgages, "Mortgages", 'g'),
    (TurnAction::SaveGame, "Save Game", 'v'),
    (TurnAction::EndTurn, "End Turn", 'e'),
];

/// The action bound to keyboard key `c`, if any.
pub fn action_for_hotkey(c: char) -> Option<TurnAction> {
    ACTIONS
        .iter()
        .find(|(_, _, hotkey)| *hotkey == c)
        .map(|(action, _, _)| *action)
}

pub struct ActionMenu {
    cursor: Cursor,
}

impl ActionMenu {
    pub fn new() -> Self {
        Self {
            cursor: Cursor::new(ACTIONS.len()),
        }
    }

    pub fn prev(&mut self) {
        self.cursor.up();
    }

    pub fn next(&mut self) {
        self.cursor.down();
    }

    pub fn selected(&self) -> TurnAction {
        ACTIONS[self.cursor.selected].0
    }

    pub fn render(&self, frame: &mut Frame, current: usize, money: u32) {
        // A blank row separates "End Turn" (the last action) from the rest.
        let gap = 1u16;
        let area = centered_rect(frame.area(), 28, ACTIONS.len() as u16 + gap + 2);
        let block = Block::bordered()
            .title_top(Line::from(format!(" Player {} — ${money} ", current + 1)).centered())
            .style(Style::new().bg(Color::Black).fg(Color::White).bold());
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        let last = ACTIONS.len() - 1;
        let mut lines: Vec<Line> = Vec::new();
        for (i, (_, label, hotkey)) in ACTIONS.iter().enumerate() {
            if i == last {
                lines.push(Line::from("")); // gap before End Turn
            }
            let line = Line::from(format!("{label} ({hotkey})")).centered();
            lines.push(if i == self.cursor.selected { line.reversed() } else { line });
        }
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_has_a_unique_hotkey() {
        let mut keys: Vec<char> = ACTIONS.iter().map(|(_, _, key)| *key).collect();
        keys.sort_unstable();
        let count = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), count);
    }

    #[test]
    fn every_hotkey_resolves_to_its_own_row() {
        for (i, (_, _, key)) in ACTIONS.iter().enumerate() {
            let action = action_for_hotkey(*key).expect("a bound hotkey");
            assert_eq!(action as usize, ACTIONS[i].0 as usize);
        }
    }

    #[test]
    fn an_unbound_key_resolves_to_nothing() {
        assert!(action_for_hotkey('z').is_none());
    }

    #[test]
    fn the_menu_cursor_is_bounded_at_both_ends() {
        let mut menu = ActionMenu::new();
        menu.prev();
        assert_eq!(menu.cursor.selected, 0);

        for _ in 0..ACTIONS.len() * 2 {
            menu.next();
        }
        assert_eq!(menu.cursor.selected, ACTIONS.len() - 1);
    }

    #[test]
    fn the_cursor_selects_the_matching_row() {
        let mut menu = ActionMenu::new();
        menu.next();
        assert_eq!(menu.selected() as usize, ACTIONS[1].0 as usize);
    }
}
