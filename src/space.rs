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

/// Fields shared by every buyable space: a name, a purchase price, an optional
/// owner (`None` = bank, `Some(i)` = player `i`), and whether it's mortgaged.
/// Railroads, utilities, and streets can all be mortgaged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ownable {
    pub name: String,
    pub price: u32,
    pub owner: Option<usize>,
    pub mortgaged: bool,
}

impl Ownable {
    fn new(name: &str, price: u32) -> Self {
        Self {
            name: name.to_string(),
            price,
            owner: None,
            mortgaged: false,
        }
    }
}

/// A street you can buy, build on, and charge rent for.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Property {
    pub base: Ownable,
    pub group: ColorGroup,
    /// Rent by development level: `[base, 1 house, 2, 3, 4, hotel]`.
    pub rents: [u32; 6],
    /// Cost of one house (a hotel costs four houses + this again).
    pub house_cost: u32,
    /// Houses built: 0–4, where 5 means a hotel.
    pub houses: u8,
}

impl Property {
    /// Rent owed right now, before monopoly doubling. With houses, the matching
    /// tier; with none, the base rent.
    pub fn current_rent(&self) -> u32 {
        self.rents[self.houses.min(5) as usize]
    }
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
    pub fn street(
        name: &str,
        group: ColorGroup,
        price: u32,
        rents: [u32; 6],
        house_cost: u32,
    ) -> Self {
        Space::Property(Property {
            base: Ownable::new(name, price),
            group,
            rents,
            house_cost,
            houses: 0,
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

    /// True for spaces that can be bought: streets, railroads, utilities.
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

    /// True while this space is mortgaged.
    pub fn is_mortgaged(&self) -> bool {
        self.ownable().is_some_and(|o| o.mortgaged)
    }

    pub fn set_mortgaged(&mut self, value: bool) {
        if let Some(o) = self.ownable_mut() {
            o.mortgaged = value;
        }
    }

    /// Houses built on a street (0–4, 5 = hotel); 0 for anything else.
    pub fn houses(&self) -> u8 {
        match self {
            Space::Property(p) => p.houses,
            _ => 0,
        }
    }

    /// Cost of one house on this street, or 0 if it can't be built on.
    pub fn house_cost(&self) -> u32 {
        match self {
            Space::Property(p) => p.house_cost,
            _ => 0,
        }
    }

    /// Return a space to the bank's stock: clear any houses and the mortgage so
    /// the next buyer gets it undeveloped.
    pub fn reset_buildings(&mut self) {
        if let Space::Property(p) = self {
            p.houses = 0;
        }
        if let Some(o) = self.ownable_mut() {
            o.mortgaged = false;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn street() -> Space {
        Space::street("Boardwalk", ColorGroup::DarkBlue, 400, [50, 200, 600, 1400, 1700, 2000], 200)
    }

    #[test]
    fn only_buyable_spaces_are_ownable() {
        assert!(street().is_ownable());
        assert!(Space::railroad("Reading RR", 200).is_ownable());
        assert!(Space::utility("Electric Co", 150).is_ownable());
        assert!(!Space::Go.is_ownable());
        assert!(!Space::FreeParking.is_ownable());
        assert!(!Space::Tax(200).is_ownable());
    }

    #[test]
    fn corners_and_decks_report_their_own_names() {
        assert_eq!(Space::Go.name(), "GO");
        assert_eq!(Space::Jail.name(), "Jail");
        assert_eq!(Space::FreeParking.name(), "Free Parking");
        assert_eq!(Space::GoToJail.name(), "Go To Jail");
        assert_eq!(Space::Chance.name(), "Chance");
        assert_eq!(Space::CommunityChest.name(), "Comm Chest");
        assert_eq!(Space::Tax(200).name(), "Tax");
        assert_eq!(street().name(), "Boardwalk");
    }

    #[test]
    fn ownership_round_trips_through_the_accessors() {
        let mut space = street();
        assert_eq!(space.owner(), None);
        space.set_owner(Some(2));
        assert_eq!(space.owner(), Some(2));
        space.set_owner(None);
        assert_eq!(space.owner(), None);
    }

    #[test]
    fn setting_an_owner_on_an_unbuyable_space_is_ignored() {
        let mut space = Space::FreeParking;
        space.set_owner(Some(1));
        assert_eq!(space.owner(), None);
    }

    #[test]
    fn the_mortgage_flag_toggles() {
        let mut space = street();
        assert!(!space.is_mortgaged());
        space.set_mortgaged(true);
        assert!(space.is_mortgaged());
        space.set_mortgaged(false);
        assert!(!space.is_mortgaged());
    }

    #[test]
    fn only_streets_report_houses_and_a_build_cost() {
        assert_eq!(street().house_cost(), 200);
        assert_eq!(Space::railroad("Reading RR", 200).houses(), 0);
        assert_eq!(Space::railroad("Reading RR", 200).house_cost(), 0);
        assert_eq!(Space::Go.house_cost(), 0);
    }

    #[test]
    fn price_is_none_for_unbuyable_spaces() {
        assert_eq!(street().price(), Some(400));
        assert_eq!(Space::Go.price(), None);
        assert_eq!(Space::Tax(200).price(), None);
    }

    #[test]
    fn current_rent_follows_the_development_level() {
        let Space::Property(mut p) = street() else { panic!("a street") };
        assert_eq!(p.current_rent(), 50);
        p.houses = 1;
        assert_eq!(p.current_rent(), 200);
        p.houses = 5;
        assert_eq!(p.current_rent(), 2000);
    }

    #[test]
    fn current_rent_clamps_above_a_hotel() {
        let Space::Property(mut p) = street() else { panic!("a street") };
        p.houses = 9;
        assert_eq!(p.current_rent(), 2000, "never indexes past the table");
    }

    #[test]
    fn returning_a_space_to_the_bank_strips_it_bare() {
        let Space::Property(mut p) = street() else { panic!("a street") };
        p.houses = 4;
        let mut space = Space::Property(p);
        space.set_mortgaged(true);
        space.reset_buildings();
        assert_eq!(space.houses(), 0);
        assert!(!space.is_mortgaged());
    }

    #[test]
    fn only_the_flagged_spaces_carry_an_icon() {
        assert!(Space::Chance.icon().is_some());
        assert!(Space::CommunityChest.icon().is_some());
        assert!(Space::Tax(200).icon().is_some());
        assert!(Space::railroad("Reading RR", 200).icon().is_some());
        assert!(street().icon().is_none());
        assert!(Space::Go.icon().is_none());
    }

    #[test]
    fn every_color_group_has_a_distinct_swatch() {
        use ColorGroup::*;
        let groups = [Brown, LightBlue, Pink, Orange, Red, Yellow, Green, DarkBlue];
        for (i, a) in groups.iter().enumerate() {
            for b in &groups[i + 1..] {
                assert_ne!(a.color(), b.color(), "{a:?} and {b:?} share a color");
            }
        }
    }
}
