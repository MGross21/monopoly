//! Board space data types, no rendering here.
//!
//! `allow(dead_code)` covers fields not yet read (e.g. `rent`) until game logic
//! lands.
#![allow(dead_code)]

use ratatui::style::Color;

/// The eight color groups for street properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorGroup {
    Brown,
    LightBlue,
    Pink,
    Orange,
    Red,
    Yellow,
    Green,
    DarkBlue,
}

impl ColorGroup {
    /// Border/banner color used when drawing a property.
    pub fn color(self) -> Color {
        match self {
            ColorGroup::Brown => Color::Rgb(149, 84, 54),
            ColorGroup::LightBlue => Color::Rgb(170, 224, 250),
            ColorGroup::Pink => Color::Rgb(217, 58, 150),
            ColorGroup::Orange => Color::Rgb(247, 148, 29),
            ColorGroup::Red => Color::Rgb(237, 27, 36),
            ColorGroup::Yellow => Color::Rgb(254, 242, 0),
            ColorGroup::Green => Color::Rgb(31, 158, 75),
            ColorGroup::DarkBlue => Color::Rgb(0, 114, 187),
        }
    }
}

/// A street you can buy, build on, and charge rent for.
#[derive(Debug, Clone)]
pub struct Property {
    pub name: String,
    pub group: ColorGroup,
    pub price: u32,
    pub rent: u32, // base rent, no houses
    pub owner: Option<usize>, // None = bank, Some(i) = player i
}

/// One of the four railroads.
#[derive(Debug, Clone)]
pub struct Railroad {
    pub name: String,
    pub price: u32,
    pub owner: Option<usize>,
}

/// Electric Company or Water Works.
#[derive(Debug, Clone)]
pub struct Utility {
    pub name: String,
    pub price: u32,
    pub owner: Option<usize>,
}

/// Every square is exactly one of these; each variant carries only its own data.
#[derive(Debug, Clone)]
pub enum Space {
    Go,
    Property(Property),
    Railroad(Railroad),
    Utility(Utility),
    Tax(u32), // amount owed
    Chance,
    CommunityChest,
    Jail, // also "just visiting"
    FreeParking,
    GoToJail,
}

impl Space {
    /// Short label for the board cell.
    pub fn name(&self) -> &str {
        match self {
            Space::Go => "GO",
            Space::Property(p) => &p.name,
            Space::Railroad(r) => &r.name,
            Space::Utility(u) => &u.name,
            Space::Tax(_) => "Tax",
            Space::Chance => "Chance",
            Space::CommunityChest => "Comm Chest",
            Space::Jail => "Jail",
            Space::FreeParking => "Free Parking",
            Space::GoToJail => "Go To Jail",
        }
    }

    /// Can this space be bought (street, railroad, utility)?
    pub fn is_ownable(&self) -> bool {
        matches!(
            self,
            Space::Property(_) | Space::Railroad(_) | Space::Utility(_)
        )
    }

    /// Printed purchase price, if any.
    pub fn price(&self) -> Option<u32> {
        match self {
            Space::Property(p) => Some(p.price),
            Space::Railroad(r) => Some(r.price),
            Space::Utility(u) => Some(u.price),
            _ => None,
        }
    }

    /// Current owner's player index, if owned.
    pub fn owner(&self) -> Option<usize> {
        match self {
            Space::Property(p) => p.owner,
            Space::Railroad(r) => r.owner,
            Space::Utility(u) => u.owner,
            _ => None,
        }
    }

    pub fn set_owner(&mut self, who: Option<usize>) {
        match self {
            Space::Property(p) => p.owner = who,
            Space::Railroad(r) => r.owner = who,
            Space::Utility(u) => u.owner = who,
            _ => {}
        }
    }

    /// Banner icon for non-property spaces. `None` = show the name only.
    ///
    /// Nerd Font glyphs (monochrome, honor `fg`/bold) — needs a Nerd Font in the
    /// terminal. Codepoints: `nf-fa-archive` (chest), `nf-fa-train` (railroad).
    pub fn icon(&self) -> Option<&'static str> {
        match self {
            Space::Chance => Some("?"),
            Space::CommunityChest => Some("\u{f187}"),
            Space::Railroad(_) => Some("\u{f238}"),
            Space::Tax(_) => Some("$"),
            _ => None,
        }
    }
}
