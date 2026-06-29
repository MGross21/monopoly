//! The standard 40-space US Monopoly board, clockwise from GO.
//!
//! Index 0 = GO, index 39 = Boardwalk.

use crate::space::{ColorGroup, Space};

const RAILROAD_PRICE: u32 = 200;
const UTILITY_PRICE: u32 = 150;

/// The 40 spaces in order, index 0 = GO.
pub fn board() -> Vec<Space> {
    use ColorGroup::*;
    let street = Space::street;
    let railroad = |name| Space::railroad(name, RAILROAD_PRICE);
    let utility = |name| Space::utility(name, UTILITY_PRICE);
    // Each street: name, group, price, rent table [base, 1h, 2h, 3h, 4h, hotel],
    // house cost. Values are the standard US edition.
    vec![
        Space::Go,
        street("Mediterranean Ave", Brown, 60, [2, 10, 30, 90, 160, 250], 50),
        Space::CommunityChest,
        street("Baltic Ave", Brown, 60, [4, 20, 60, 180, 320, 450], 50),
        Space::Tax(200),
        railroad("Reading RR"),
        street("Oriental Ave", LightBlue, 100, [6, 30, 90, 270, 400, 550], 50),
        Space::Chance,
        street("Vermont Ave", LightBlue, 100, [6, 30, 90, 270, 400, 550], 50),
        street("Connecticut Ave", LightBlue, 120, [8, 40, 100, 300, 450, 600], 50),
        Space::Jail,
        street("St. Charles Pl", Pink, 140, [10, 50, 150, 450, 625, 750], 100),
        utility("Electric Co"),
        street("States Ave", Pink, 140, [10, 50, 150, 450, 625, 750], 100),
        street("Virginia Ave", Pink, 160, [12, 60, 180, 500, 700, 900], 100),
        railroad("Pennsylvania RR"),
        street("St. James Pl", Orange, 180, [14, 70, 200, 550, 750, 950], 100),
        Space::CommunityChest,
        street("Tennessee Ave", Orange, 180, [14, 70, 200, 550, 750, 950], 100),
        street("New York Ave", Orange, 200, [16, 80, 220, 600, 800, 1000], 100),
        Space::FreeParking,
        street("Kentucky Ave", Red, 220, [18, 90, 250, 700, 875, 1050], 150),
        Space::Chance,
        street("Indiana Ave", Red, 220, [18, 90, 250, 700, 875, 1050], 150),
        street("Illinois Ave", Red, 240, [20, 100, 300, 750, 925, 1100], 150),
        railroad("B&O RR"),
        street("Atlantic Ave", Yellow, 260, [22, 110, 330, 800, 975, 1150], 150),
        street("Ventnor Ave", Yellow, 260, [22, 110, 330, 800, 975, 1150], 150),
        utility("Water Works"),
        street("Marvin Gardens", Yellow, 280, [24, 120, 360, 850, 1025, 1200], 150),
        Space::GoToJail,
        street("Pacific Ave", Green, 300, [26, 130, 390, 900, 1100, 1275], 200),
        street("N Carolina Ave", Green, 300, [26, 130, 390, 900, 1100, 1275], 200),
        Space::CommunityChest,
        street("Pennsylvania Ave", Green, 320, [28, 150, 450, 1000, 1200, 1400], 200),
        railroad("Short Line"),
        Space::Chance,
        street("Park Place", DarkBlue, 350, [35, 175, 500, 1100, 1300, 1500], 200),
        Space::Tax(100),
        street("Boardwalk", DarkBlue, 400, [50, 200, 600, 1400, 1700, 2000], 200),
    ]
}
