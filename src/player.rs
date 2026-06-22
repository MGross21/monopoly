//! Players and their board tokens.
#![allow(dead_code)] // money/position read once game logic lands

/// The eight classic tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub const ALL: [Piece; 8] = [
        Piece::TopHat,
        Piece::Car,
        Piece::Dog,
        Piece::Cat,
        Piece::Ship,
        Piece::Boot,
        Piece::Thimble,
        Piece::Wheelbarrow,
    ];

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

    fn index(self) -> usize {
        Self::ALL.iter().position(|&p| p == self).unwrap()
    }

    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone)]
pub struct Player {
    pub piece: Piece,
    pub money: u32,
    pub position: usize, // board index, starts on GO
}

impl Player {
    pub fn new(piece: Piece, money: u32) -> Self {
        Self {
            piece,
            money,
            position: 0,
        }
    }
}
