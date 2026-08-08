//! What a landing costs: the rent tables, color-group monopolies, and the
//! special rates the "advance to the nearest …" cards charge.

use super::Game;
use crate::space::{ColorGroup, Space};

const RAILROAD_BASE_RENT: u32 = 25;

/// The rate a landing is charged at. The "advance to the nearest …" cards bill
/// more than the printed rate; everything else is `Normal`.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum RentRule {
    Normal,
    /// Twice the railroad's usual rent.
    DoubleRailroad,
    /// Ten times this roll, however many utilities the owner holds.
    TenTimesUtility(usize),
}

/// The two space kinds whose rent scales with how many the owner holds.
#[derive(Clone, Copy)]
enum Kind {
    Railroad,
    Utility,
}

impl Game {
    /// Rent owed for the space at `pos`, owned by `owner`, under `rule`. A
    /// mortgaged space collects nothing.
    pub(super) fn rent(&self, pos: usize, owner: usize, total: usize, rule: RentRule) -> u32 {
        match &self.board[pos] {
            Space::Property(p) => {
                if p.base.mortgaged {
                    return 0;
                }
                // A full color group doubles the base rent, but only while the
                // group is undeveloped; once houses go up the table takes over.
                if p.houses == 0 && self.owns_full_group(owner, p.group) {
                    p.current_rent() * 2
                } else {
                    p.current_rent()
                }
            }
            Space::Railroad(o) | Space::Utility(o) if o.mortgaged => 0,
            Space::Railroad(_) => {
                let rent = RAILROAD_BASE_RENT * self.count_kind(owner, Kind::Railroad);
                if rule == RentRule::DoubleRailroad { rent * 2 } else { rent }
            }
            // The "nearest utility" card bills ten times its own throw, not the
            // roll that brought the player here.
            Space::Utility(_) => match rule {
                RentRule::TenTimesUtility(roll) => roll as u32 * 10,
                _ if self.count_kind(owner, Kind::Utility) == 2 => total as u32 * 10,
                _ => total as u32 * 4,
            },
            _ => 0,
        }
    }

    /// True when `owner` holds every street in `group`, the precondition for
    /// building and for doubled rent.
    pub(super) fn owns_full_group(&self, owner: usize, group: ColorGroup) -> bool {
        let mut total = 0;
        let mut mine = 0;
        for space in &self.board {
            if let Space::Property(p) = space
                && p.group == group
            {
                total += 1;
                if p.base.owner == Some(owner) {
                    mine += 1;
                }
            }
        }
        total > 0 && mine == total
    }

    /// How many railroads/utilities `owner` holds.
    fn count_kind(&self, owner: usize, kind: Kind) -> u32 {
        self.estate(owner)
            .filter(|s| match kind {
                Kind::Railroad => matches!(s, Space::Railroad(_)),
                Kind::Utility => matches!(s, Space::Utility(_)),
            })
            .count() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::HOTEL;
    use crate::game::testkit::*;

    #[test]
    fn rent_is_the_printed_base_for_a_lone_street() {
        let mut g = game(2, 1500);
        own(&mut g, MEDITERRANEAN, 1);
        assert_eq!(g.rent(MEDITERRANEAN, 1, 7, RentRule::Normal), 2);
    }

    #[test]
    fn a_full_group_doubles_undeveloped_rent() {
        let mut g = game(2, 1500);
        own_group(&mut g, ColorGroup::Brown, 1);
        assert_eq!(g.rent(MEDITERRANEAN, 1, 7, RentRule::Normal), 4);
    }

    #[test]
    fn houses_replace_the_doubled_group_rent() {
        let mut g = game(2, 1500);
        own_group(&mut g, ColorGroup::Brown, 1);
        set_houses(&mut g, MEDITERRANEAN, 1);
        assert_eq!(g.rent(MEDITERRANEAN, 1, 7, RentRule::Normal), 10, "the table takes over");
    }

    #[test]
    fn a_hotel_charges_the_top_of_the_table() {
        let mut g = game(2, 1500);
        own_group(&mut g, ColorGroup::Brown, 1);
        set_houses(&mut g, MEDITERRANEAN, HOTEL);
        assert_eq!(g.rent(MEDITERRANEAN, 1, 7, RentRule::Normal), 250);
    }

    #[test]
    fn railroad_rent_scales_with_the_number_held() {
        let mut g = game(2, 1500);
        own(&mut g, READING_RR, 1);
        assert_eq!(g.rent(READING_RR, 1, 7, RentRule::Normal), 25);
        own(&mut g, PENNSYLVANIA_RR, 1);
        assert_eq!(g.rent(READING_RR, 1, 7, RentRule::Normal), 50);
        own(&mut g, B_AND_O_RR, 1);
        own(&mut g, SHORT_LINE, 1);
        assert_eq!(g.rent(READING_RR, 1, 7, RentRule::Normal), 100);
    }

    #[test]
    fn the_railroad_card_doubles_whatever_the_rate_is() {
        let mut g = game(2, 1500);
        own(&mut g, READING_RR, 1);
        own(&mut g, PENNSYLVANIA_RR, 1);
        assert_eq!(g.rent(READING_RR, 1, 7, RentRule::DoubleRailroad), 100);
    }

    #[test]
    fn utility_rent_is_four_or_ten_times_the_roll() {
        let mut g = game(2, 1500);
        own(&mut g, ELECTRIC_CO, 1);
        assert_eq!(g.rent(ELECTRIC_CO, 1, 9, RentRule::Normal), 36);
        own(&mut g, WATER_WORKS, 1);
        assert_eq!(g.rent(ELECTRIC_CO, 1, 9, RentRule::Normal), 90);
    }

    #[test]
    fn the_utility_card_bills_its_own_throw() {
        let mut g = game(2, 1500);
        own(&mut g, ELECTRIC_CO, 1);
        assert_eq!(g.rent(ELECTRIC_CO, 1, 9, RentRule::TenTimesUtility(4)), 40);
    }

    #[test]
    fn a_mortgaged_space_collects_nothing() {
        let mut g = game(2, 1500);
        for idx in [BOARDWALK, READING_RR, ELECTRIC_CO] {
            own(&mut g, idx, 1);
            g.board[idx].set_mortgaged(true);
            assert_eq!(g.rent(idx, 1, 7, RentRule::Normal), 0);
        }
    }

    #[test]
    fn a_mortgaged_space_collects_nothing_under_the_cards_either() {
        let mut g = game(2, 1500);
        own(&mut g, READING_RR, 1);
        g.board[READING_RR].set_mortgaged(true);
        assert_eq!(g.rent(READING_RR, 1, 7, RentRule::DoubleRailroad), 0);
        own(&mut g, ELECTRIC_CO, 1);
        g.board[ELECTRIC_CO].set_mortgaged(true);
        assert_eq!(g.rent(ELECTRIC_CO, 1, 7, RentRule::TenTimesUtility(9)), 0);
    }

    #[test]
    fn a_full_group_is_only_a_monopoly_when_every_street_is_held() {
        let mut g = game(2, 1500);
        own(&mut g, MEDITERRANEAN, 0);
        assert!(!g.owns_full_group(0, ColorGroup::Brown));
        own(&mut g, BALTIC, 0);
        assert!(g.owns_full_group(0, ColorGroup::Brown));
    }
}
