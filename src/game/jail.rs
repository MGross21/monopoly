//! Jail: sending a player there, the start-of-turn choice (pay / roll / card),
//! and resolving a roll made from jail.

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui_notifications::Level;

use super::{Game, Modal, Payee};
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
                self.charge_then(who, BAIL, Payee::Bank, Some(total));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testkit::*;

    fn jailed(cash: u32) -> Game {
        let mut g = game(2, cash);
        g.send_to_jail();
        g
    }

    #[test]
    fn being_jailed_parks_you_on_the_jail_square() {
        let mut g = game(2, 1500);
        g.doubles = 2;
        place(&mut g, 0, 25);
        g.send_to_jail();
        assert!(g.players[0].in_jail);
        assert_eq!(g.players[0].position, JAIL);
        assert_eq!(g.players[0].jail_turns, 0);
        assert_eq!(g.doubles, 0, "the doubles streak is broken");
        assert_eq!(g.players[0].money, 1500, "no salary on the way");
    }

    #[test]
    fn rolling_while_jailed_opens_the_choices_instead() {
        let mut g = jailed(1500);
        g.start_roll();
        assert!(matches!(g.modal, Modal::Jail(_)));
    }

    #[test]
    fn the_menu_offers_only_what_is_available() {
        let mut g = jailed(1500);
        g.open_jail();
        let Modal::Jail(menu) = &g.modal else { panic!("expected the jail menu") };
        assert_eq!(menu.labels(), vec!["Pay $50 bail", "Roll for doubles"]);

        let mut g = jailed(10);
        g.players[0].get_out_free = 1;
        g.open_jail();
        let Modal::Jail(menu) = &g.modal else { panic!("expected the jail menu") };
        assert_eq!(
            menu.labels(),
            vec!["Roll for doubles", "Use Get Out of Jail Free"],
            "bail is hidden when it is unaffordable"
        );
    }

    #[test]
    fn paying_bail_frees_you_and_rolls() {
        let mut g = jailed(1500);
        g.open_jail();
        g.handle_key(KeyCode::Enter); // "Pay $50 bail" is first
        assert!(!g.players[0].in_jail);
        assert_eq!(g.players[0].money, 1450);
        assert!(matches!(g.modal, Modal::Roll(_)));
    }

    #[test]
    fn a_card_frees_you_without_paying() {
        let mut g = jailed(1500);
        g.players[0].get_out_free = 1;
        g.open_jail();
        g.handle_key(KeyCode::Down);
        g.handle_key(KeyCode::Down); // Pay, Roll, Card
        g.handle_key(KeyCode::Enter);
        assert!(!g.players[0].in_jail);
        assert_eq!(g.players[0].money, 1500);
        assert_eq!(g.players[0].get_out_free, 0);
    }

    #[test]
    fn doubles_escape_and_move_you() {
        let mut g = jailed(1500);
        g.apply_roll(3, 3);
        assert!(!g.players[0].in_jail);
        assert_eq!(g.players[0].position, JAIL + 6);
        assert!(!g.can_roll, "leaving jail never grants a bonus roll");
        assert_eq!(g.doubles, 0);
    }

    #[test]
    fn a_failed_attempt_keeps_you_in_and_is_counted() {
        let mut g = jailed(1500);
        g.apply_roll(2, 5);
        assert!(g.players[0].in_jail);
        assert_eq!(g.players[0].jail_turns, 1);
        assert_eq!(g.players[0].position, JAIL);
        assert!(!g.can_roll);
    }

    #[test]
    fn the_third_failure_forces_the_bail_and_the_move() {
        let mut g = jailed(1500);
        g.apply_roll(2, 5);
        g.apply_roll(2, 5);
        g.apply_roll(2, 5);
        assert!(!g.players[0].in_jail);
        assert_eq!(g.players[0].money, 1450);
        assert_eq!(g.players[0].position, JAIL + 7);
        assert_eq!(g.players[0].jail_turns, 0);
    }

    #[test]
    fn a_jailed_player_still_collects_rent() {
        let mut g = game(2, 1500);
        own(&mut g, BOARDWALK, 1);
        g.current = 1;
        g.send_to_jail();
        g.current = 0;
        place(&mut g, 0, BOARDWALK - 5);
        g.apply_roll(2, 3);
        assert_eq!(g.players[1].money, 1550, "jail does not suspend rent");
    }

    #[test]
    fn escaping_leaves_the_menu_closed() {
        let mut g = jailed(1500);
        g.open_jail();
        g.handle_key(KeyCode::Esc);
        assert!(matches!(g.modal, Modal::None));
        assert!(g.players[0].in_jail);
    }
}
