//! Players and their board tokens.

use serde::{Deserialize, Serialize};
use strum::{EnumCount, EnumIter, FromRepr, IntoEnumIterator};

/// The eight classic tokens.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, EnumIter, EnumCount, FromRepr,
)]
#[repr(usize)]
pub enum Piece {
    TopHat,
    Car,
    Dog,
    Cat,
    Ship,
    Boot,
    Thimble,
    Wheelbarrow,
}

impl Piece {
    /// Every piece, in declaration order.
    pub fn all() -> impl Iterator<Item = Piece> {
        Piece::iter()
    }

    /// Token glyph. Emoji so it works without a patched font; centralized here
    /// so you can swap to Nerd Font codepoints in one place.
    pub fn icon(self) -> &'static str {
        match self {
            Piece::TopHat => "🎩",
            Piece::Car => "🚗",
            Piece::Dog => "🐕",
            Piece::Cat => "🐈",
            Piece::Ship => "🚢",
            Piece::Boot => "👢",
            Piece::Thimble => "🧵",
            Piece::Wheelbarrow => "🛒",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Piece::TopHat => "Top Hat",
            Piece::Car => "Car",
            Piece::Dog => "Dog",
            Piece::Cat => "Cat",
            Piece::Ship => "Ship",
            Piece::Boot => "Boot",
            Piece::Thimble => "Thimble",
            Piece::Wheelbarrow => "Wheelbarrow",
        }
    }

    pub fn next(self) -> Self {
        Self::from_repr((self as usize + 1) % Self::COUNT).unwrap()
    }

    pub fn prev(self) -> Self {
        Self::from_repr((self as usize + Self::COUNT - 1) % Self::COUNT).unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub piece: Piece,
    pub money: u32,
    pub position: usize, // board index, starts on GO
    pub in_jail: bool,
    pub jail_turns: u8,    // consecutive turns spent in jail (0..3)
    pub get_out_free: u8,  // unused Get Out of Jail Free cards held
    pub bankrupt: bool,    // eliminated from the game
}

impl Player {
    pub fn new(piece: Piece, money: u32) -> Self {
        Self {
            piece,
            money,
            position: 0,
            in_jail: false,
            jail_turns: 0,
            get_out_free: 0,
            bankrupt: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_eight_tokens() {
        assert_eq!(Piece::all().count(), Piece::COUNT);
        assert_eq!(Piece::COUNT, 8);
    }

    #[test]
    fn stepping_forward_wraps_round_the_tokens() {
        let mut piece = Piece::TopHat;
        for _ in 0..Piece::COUNT {
            piece = piece.next();
        }
        assert_eq!(piece, Piece::TopHat);
    }

    #[test]
    fn stepping_back_wraps_the_other_way() {
        assert_eq!(Piece::TopHat.prev(), Piece::Wheelbarrow);
        assert_eq!(Piece::TopHat.next().prev(), Piece::TopHat);
    }

    #[test]
    fn every_token_has_a_distinct_icon_and_label() {
        let icons: Vec<&str> = Piece::all().map(Piece::icon).collect();
        let labels: Vec<&str> = Piece::all().map(Piece::label).collect();
        for i in 0..icons.len() {
            for j in i + 1..icons.len() {
                assert_ne!(icons[i], icons[j]);
                assert_ne!(labels[i], labels[j]);
            }
        }
    }

    #[test]
    fn a_new_player_starts_on_go_and_out_of_jail() {
        let p = Player::new(Piece::Car, 1500);
        assert_eq!(p.money, 1500);
        assert_eq!(p.position, 0);
        assert!(!p.in_jail);
        assert_eq!(p.jail_turns, 0);
        assert_eq!(p.get_out_free, 0);
        assert!(!p.bankrupt);
    }
}
