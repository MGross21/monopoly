//! Board space data types, no rendering here.

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

/// The eight color groups for street properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// Fields shared by every buyable space: a name, a purchase price, and an
/// optional owner (`None` = bank, `Some(i)` = player `i`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ownable {
    pub name: String,
    pub price: u32,
    pub owner: Option<usize>,
}

impl Ownable {
    fn new(name: &str, price: u32) -> Self {
        Self {
            name: name.to_string(),
            price,
            owner: None,
        }
    }
}

/// A street you can buy, build on, and charge rent for.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Property {
    pub base: Ownable,
    pub group: ColorGroup,
    pub rent: u32, // base rent, no houses
}

/// Every square is exactly one of these; each variant carries only its own data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Space {
    Go,
    Property(Property),
    Railroad(Ownable),
    Utility(Ownable),
    Tax(u32), // amount owed
    Chance,
    CommunityChest,
    Jail, // also "just visiting"
    FreeParking,
    GoToJail,
}

impl Space {
    pub fn street(name: &str, group: ColorGroup, price: u32, rent: u32) -> Self {
        Space::Property(Property {
            base: Ownable::new(name, price),
            group,
            rent,
        })
    }

    pub fn railroad(name: &str, price: u32) -> Self {
        Space::Railroad(Ownable::new(name, price))
    }

    pub fn utility(name: &str, price: u32) -> Self {
        Space::Utility(Ownable::new(name, price))
    }

    /// The shared ownable data (name/price/owner) for buyable spaces.
    fn ownable(&self) -> Option<&Ownable> {
        match self {
            Space::Property(p) => Some(&p.base),
            Space::Railroad(o) | Space::Utility(o) => Some(o),
            _ => None,
        }
    }

    fn ownable_mut(&mut self) -> Option<&mut Ownable> {
        match self {
            Space::Property(p) => Some(&mut p.base),
            Space::Railroad(o) | Space::Utility(o) => Some(o),
            _ => None,
        }
    }

    /// Short label for the board cell.
    pub fn name(&self) -> &str {
        match self {
            Space::Go => "GO",
            Space::Tax(_) => "Tax",
            Space::Chance => "Chance",
            Space::CommunityChest => "Comm Chest",
            Space::Jail => "Jail",
            Space::FreeParking => "Free Parking",
            Space::GoToJail => "Go To Jail",
            _ => &self.ownable().unwrap().name,
        }
    }

    /// Can this space be bought (street, railroad, utility)?
    pub fn is_ownable(&self) -> bool {
        self.ownable().is_some()
    }

    /// Printed purchase price, if any.
    pub fn price(&self) -> Option<u32> {
        self.ownable().map(|o| o.price)
    }

    /// Current owner's player index, if owned.
    pub fn owner(&self) -> Option<usize> {
        self.ownable().and_then(|o| o.owner)
    }

    pub fn set_owner(&mut self, who: Option<usize>) {
        if let Some(o) = self.ownable_mut() {
            o.owner = who;
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
