//! Jail: sending a player there, the start-of-turn choice (pay / roll / card),
//! and resolving a roll made from jail.

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui_notifications::Level;

use super::{Game, Modal};
use crate::ui::choice_popup;
use crate::ui::dice::Roll;

const JAIL_INDEX: usize = 10;
const BAIL: u32 = 50;

/// What a jailed player may do at the start of their turn. `Pay` and `Card` only
/// appear when available; `Roll` is always present.
#[derive(Clone, Copy)]
enum Choice {
    Pay,
    Roll,
    Card,
}

/// The in-jail action picker.
pub(super) struct JailMenu {
    choices: Vec<Choice>,
    cursor: crate::ui::Cursor,
}

impl JailMenu {
    fn labels(&self) -> Vec<&'static str> {
        self.choices
            .iter()
            .map(|c| match c {
                Choice::Pay => "Pay $50 bail",
                Choice::Roll => "Roll for doubles",
                Choice::Card => "Use Get Out of Jail Free",
            })
            .collect()
    }
}

impl Game {
    /// Send the current player to Jail (no salary, doubles streak reset).
    pub(super) fn send_to_jail(&mut self) {
        let who = self.current;
        self.players[who].position = JAIL_INDEX;
        self.players[who].in_jail = true;
        self.players[who].jail_turns = 0;
        self.doubles = 0;
        self.notify(format!("Player {} was sent to Jail", who + 1), Level::Warn);
    }

    /// Show the jailed player's options at the start of their turn.
    pub(super) fn open_jail(&mut self) {
        let p = &self.players[self.current];
        let mut choices = Vec::new();
        if p.money >= BAIL {
            choices.push(Choice::Pay);
        }
        choices.push(Choice::Roll);
        if p.get_out_free > 0 {
            choices.push(Choice::Card);
        }
        let cursor = crate::ui::Cursor::new(choices.len());
        self.modal = Modal::Jail(JailMenu { choices, cursor });
    }

    /// Handle one jail-menu key press. Returns `true` to keep the popup open.
    pub(super) fn jail_input(&mut self, menu: &mut JailMenu, key: KeyCode) -> bool {
        match key {
            KeyCode::Up => menu.cursor.up(),
            KeyCode::Down => menu.cursor.down(),
            KeyCode::Esc => return false,
            KeyCode::Enter => {
                self.resolve_jail_choice(menu.choices[menu.cursor.selected]);
                return false; // resolve_jail_choice opened the roll popup
            }
            _ => {}
        }
        true
    }

    /// Act on the chosen escape. Pay/Card free the player and then roll normally;
    /// Roll just rolls (jail semantics handled in `apply_jail_roll`).
    fn resolve_jail_choice(&mut self, choice: Choice) {
        let who = self.current;
        match choice {
            Choice::Pay => {
                self.players[who].money -= BAIL; // offered only when affordable
                self.free_from_jail(who);
                self.notify(format!("Player {} paid ${BAIL} bail", who + 1), Level::Warn);
            }
            Choice::Card => {
                self.players[who].get_out_free -= 1;
                self.free_from_jail(who);
                self.notify(format!("Player {} used a Get Out of Jail Free card", who + 1), Level::Info);
            }
            Choice::Roll => {}
        }
        self.modal = Modal::Roll(Roll::new());
    }

    /// Resolve a roll made from jail: doubles escape and move; otherwise count a
    /// failed attempt, and on the third pay the bail and move regardless.
    pub(super) fn apply_jail_roll(&mut self, who: usize, a: u8, b: u8, total: usize) {
        if a == b {
            self.free_from_jail(who);
            self.notify(format!("Doubles! Player {} leaves Jail", who + 1), Level::Info);
            self.advance(who, total);
        } else {
            self.players[who].jail_turns += 1;
            let n = self.players[who].jail_turns;
            if n >= 3 {
                self.free_from_jail(who);
                self.notify(format!("Player {} failed three times — pays ${BAIL} bail", who + 1), Level::Warn);
                if self.players[who].money >= BAIL {
                    self.players[who].money -= BAIL;
                    self.advance(who, total);
                } else {
                    self.pay_bank(BAIL); // can't cover bail — bankrupt
                }
            } else {
                self.notify(format!("Player {} failed to roll doubles ({n}/3)", who + 1), Level::Warn);
            }
        }
        // Leaving jail never grants a bonus roll; the turn ends after this. A
        // landing or the forced bail may also have bankrupted the player.
        self.can_roll = false;
        self.settle_if_bankrupt(who);
    }

    fn free_from_jail(&mut self, who: usize) {
        self.players[who].in_jail = false;
        self.players[who].jail_turns = 0;
    }

    pub(super) fn render_jail(&self, frame: &mut Frame, menu: &JailMenu) {
        choice_popup(frame, " In Jail ", &menu.labels(), menu.cursor.selected);
    }
}
