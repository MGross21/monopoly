//! Test-only helpers for building deterministic games.
//!
//! `Game::new` picks the first player with a random roll-off, so every helper
//! here pins the turn state afterwards.

use super::Game;
use crate::player::{Piece, Player};
use crate::space::{ColorGroup, Space};

// Board indices used by the tests, by name.
pub(super) const MEDITERRANEAN: usize = 1;
pub(super) const BALTIC: usize = 3;
pub(super) const INCOME_TAX: usize = 4;
pub(super) const READING_RR: usize = 5;
pub(super) const ORIENTAL: usize = 6;
pub(super) const CHANCE_LOW: usize = 7;
pub(super) const JAIL: usize = 10;
pub(super) const ELECTRIC_CO: usize = 12;
pub(super) const PENNSYLVANIA_RR: usize = 15;
pub(super) const FREE_PARKING: usize = 20;
pub(super) const ILLINOIS: usize = 24;
pub(super) const B_AND_O_RR: usize = 25;
pub(super) const WATER_WORKS: usize = 28;
pub(super) const GO_TO_JAIL: usize = 30;
pub(super) const SHORT_LINE: usize = 35;
pub(super) const LUXURY_TAX: usize = 38;
pub(super) const BOARDWALK: usize = 39;

/// A game of `count` players holding `cash` each, with player 0 to move.
pub(super) fn game(count: usize, cash: u32) -> Game {
    let players = Piece::all().take(count).map(|p| Player::new(p, cash)).collect();
    let mut g = Game::new(players);
    g.current = 0;
    g.doubles = 0;
    g.can_roll = true;
    g.has_rolled = false;
    g
}

/// Hand `idx` to player `who`.
pub(super) fn own(g: &mut Game, idx: usize, who: usize) {
    g.board[idx].set_owner(Some(who));
}

/// Hand every street in `group` to player `who`.
pub(super) fn own_group(g: &mut Game, group: ColorGroup, who: usize) {
    for space in &mut g.board {
        if matches!(space, Space::Property(p) if p.group == group) {
            space.set_owner(Some(who));
        }
    }
}

/// Put `houses` on `idx` directly, bypassing the build rules and their cost.
pub(super) fn set_houses(g: &mut Game, idx: usize, houses: u8) {
    if let Space::Property(p) = &mut g.board[idx] {
        p.houses = houses;
    }
}

/// Drop player `who` on `idx` without moving them through the board.
pub(super) fn place(g: &mut Game, who: usize, idx: usize) {
    g.players[who].position = idx;
}
