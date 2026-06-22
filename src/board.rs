//! The standard 40-space US Monopoly board, clockwise from GO.
//!
//! Index 0 = GO, index 39 = Boardwalk.

use crate::space::{ColorGroup, Property, Railroad, Space, Utility};

/// The 40 spaces in order, index 0 = GO.
pub fn board() -> Vec<Space> {
    use ColorGroup::*;
    vec![
        Space::Go,
        street("Mediterranean Ave", Brown, 60, 2),
        Space::CommunityChest,
        street("Baltic Ave", Brown, 60, 4),
        Space::Tax(200),
        railroad("Reading RR"),
        street("Oriental Ave", LightBlue, 100, 6),
        Space::Chance,
        street("Vermont Ave", LightBlue, 100, 6),
        street("Connecticut Ave", LightBlue, 120, 8),
        Space::Jail,
        street("St. Charles Pl", Pink, 140, 10),
        utility("Electric Co"),
        street("States Ave", Pink, 140, 10),
        street("Virginia Ave", Pink, 160, 12),
        railroad("Pennsylvania RR"),
        street("St. James Pl", Orange, 180, 14),
        Space::CommunityChest,
        street("Tennessee Ave", Orange, 180, 14),
        street("New York Ave", Orange, 200, 16),
        Space::FreeParking,
        street("Kentucky Ave", Red, 220, 18),
        Space::Chance,
        street("Indiana Ave", Red, 220, 18),
        street("Illinois Ave", Red, 240, 20),
        railroad("B&O RR"),
        street("Atlantic Ave", Yellow, 260, 22),
        street("Ventnor Ave", Yellow, 260, 22),
        utility("Water Works"),
        street("Marvin Gardens", Yellow, 280, 24),
        Space::GoToJail,
        street("Pacific Ave", Green, 300, 26),
        street("N Carolina Ave", Green, 300, 26),
        Space::CommunityChest,
        street("Pennsylvania Ave", Green, 320, 28),
        railroad("Short Line"),
        Space::Chance,
        street("Park Place", DarkBlue, 350, 35),
        Space::Tax(100),
        street("Boardwalk", DarkBlue, 400, 50),
    ]
}

// Constructors keeping the board list above readable.

fn street(name: &str, group: ColorGroup, price: u32, rent: u32) -> Space {
    Space::Property(Property {
        name: name.to_string(),
        group,
        price,
        rent,
        owner: None,
    })
}

fn railroad(name: &str) -> Space {
    Space::Railroad(Railroad {
        name: name.to_string(),
        price: 200,
        owner: None,
    })
}

fn utility(name: &str) -> Space {
    Space::Utility(Utility {
        name: name.to_string(),
        price: 150,
        owner: None,
    })
}
