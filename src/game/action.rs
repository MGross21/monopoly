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
    EndTurn,
}

/// The actions in menu order, each with its label and hotkey. Single source of
/// truth: add an action by adding a row.
const ACTIONS: [(TurnAction, &str, char); 7] = [
    (TurnAction::RollDice, "Roll Dice", 'r'),
    (TurnAction::BuyProperty, "Buy Property", 'b'),
    (TurnAction::BuildHouses, "Build Houses", 'h'),
    (TurnAction::Trade, "Trade", 't'),
    (TurnAction::ViewInventory, "View Inventory", 'i'),
    (TurnAction::Mortgages, "Mortgages", 'g'),
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
