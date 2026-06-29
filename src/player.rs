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
