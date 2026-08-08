//! Property management: mortgaging/unmortgaging and building or selling houses,
//! both presented as a list of the current player's holdings.

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui_notifications::Level;

use super::{Game, HOTEL, Modal};
use crate::keys;
use crate::space::{ColorGroup, Space};
use crate::ui::{Cursor, choice_popup};

pub(super) fn mortgage_keys() -> String {
    keys!("↑↓" => "move", "enter" => "toggle", "esc" => "back")
}

pub(super) fn build_keys() -> String {
    keys!("↑↓" => "move", "enter" => "build", "s" => "sell", "esc" => "back")
}

/// Whether the estate popup is mortgaging or building.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Mortgage,
    Build,
}

/// A list of board indices the player can act on. The slots are fixed for the
/// popup's lifetime; their labels are recomputed from live board state.
pub(super) struct EstateMenu {
    mode: Mode,
    slots: Vec<usize>,
    cursor: Cursor,
}

impl EstateMenu {
    fn new(mode: Mode, slots: Vec<usize>) -> Self {
        Self { mode, cursor: Cursor::new(slots.len()), slots }
    }

    fn selected(&self) -> usize {
        self.slots[self.cursor.selected]
    }
}

impl Game {
    /// Open the mortgage list for the current player's holdings.
    pub(super) fn open_mortgages(&mut self) {
        let slots = self.holdings(self.current);
        if slots.is_empty() {
            self.notify("You don't own anything to mortgage", Level::Warn);
            return;
        }
        self.modal = Modal::Estate(EstateMenu::new(Mode::Mortgage, slots));
    }

    /// Open the build list: streets in a fully-owned color group.
    pub(super) fn open_build(&mut self) {
        let me = self.current;
        let slots: Vec<usize> = (0..self.board.len())
            .filter(|&i| match &self.board[i] {
                Space::Property(p) => p.base.owner == Some(me) && self.owns_full_group(me, p.group),
                _ => false,
            })
            .collect();
        if slots.is_empty() {
            self.notify("Own a full color group before building", Level::Warn);
            return;
        }
        self.modal = Modal::Estate(EstateMenu::new(Mode::Build, slots));
    }

    /// Handle one estate key press. Returns `true` to keep the popup open.
    pub(super) fn estate_input(&mut self, menu: &mut EstateMenu, key: KeyCode) -> bool {
        match key {
            KeyCode::Up => menu.cursor.up(),
            KeyCode::Down => menu.cursor.down(),
            KeyCode::Esc => return false,
            KeyCode::Enter => match menu.mode {
                Mode::Mortgage => self.toggle_mortgage(self.current, menu.selected()),
                Mode::Build => self.build_house(menu.selected()),
            },
            // Sell a house back (build mode only).
            KeyCode::Char('s') if menu.mode == Mode::Build => {
                self.sell_house(self.current, menu.selected())
            }
            _ => {}
        }
        true
    }

    fn toggle_mortgage(&mut self, who: usize, idx: usize) {
        if self.board[idx].is_mortgaged() {
            self.unmortgage(who, idx);
        } else {
            self.mortgage(who, idx);
        }
    }

    /// Take a mortgage for half the printed price. Blocked while the color group
    /// still has buildings on it.
    pub(super) fn mortgage(&mut self, who: usize, idx: usize) {
        if self.board[idx].is_mortgaged() {
            self.notify("Already mortgaged", Level::Warn);
            return;
        }
        if self.group_has_houses(idx) {
            self.notify("Sell the group's houses first", Level::Warn);
            return;
        }
        let half = self.board[idx].mortgage_value();
        self.players[who].money += half;
        self.board[idx].set_mortgaged(true);
        let name = self.board[idx].name().to_string();
        self.notify(format!("Player {} mortgaged {name} for ${half}", who + 1), Level::Info);
    }

    /// Lift a mortgage for its value plus 10% interest.
    fn unmortgage(&mut self, who: usize, idx: usize) {
        let lift = self.board[idx].unmortgage_cost();
        let name = self.board[idx].name().to_string();
        if self.players[who].money < lift {
            self.notify(format!("Need ${lift} to unmortgage {name}"), Level::Error);
            return;
        }
        self.players[who].money -= lift;
        self.board[idx].set_mortgaged(false);
        self.notify(format!("Player {} unmortgaged {name} for ${lift}", who + 1), Level::Info);
    }

    /// Build one house (or the hotel) on `idx`, enforcing even building.
    pub(super) fn build_house(&mut self, idx: usize) {
        let me = self.current;
        let Space::Property(p) = &self.board[idx] else {
            return;
        };
        let (group, houses, cost) = (p.group, p.houses, p.house_cost);
        if self.group_any_mortgaged(group) {
            self.notify("Unmortgage the whole group before building", Level::Warn);
            return;
        }
        if houses >= HOTEL {
            self.notify("Already has a hotel", Level::Warn);
            return;
        }
        if houses > self.group_house_bounds(group).0 {
            self.notify("Build evenly across the group first", Level::Warn);
            return;
        }
        let hotel = houses + 1 == HOTEL;
        if hotel && self.hotels_left == 0 {
            self.notify("The bank is out of hotels", Level::Warn);
            return;
        }
        if !hotel && self.houses_left == 0 {
            self.notify("The bank is out of houses", Level::Warn);
            return;
        }
        if self.players[me].money < cost {
            self.notify(format!("Need ${cost} to build here"), Level::Error);
            return;
        }
        self.players[me].money -= cost;
        // A hotel is four houses traded back in, so the bank regains them.
        if hotel {
            self.hotels_left -= 1;
            self.houses_left += HOTEL - 1;
        } else {
            self.houses_left -= 1;
        }
        if let Space::Property(p) = &mut self.board[idx] {
            p.houses += 1;
        }
        let name = self.board[idx].name().to_string();
        let what = if hotel { "a hotel" } else { "a house" };
        self.notify(format!("Player {} built {what} on {name} (-${cost})", me + 1), Level::Info);
    }

    /// Sell one house back to the bank for half its cost, even-build enforced.
    pub(super) fn sell_house(&mut self, who: usize, idx: usize) {
        let Space::Property(p) = &self.board[idx] else {
            return;
        };
        let (group, houses, cost) = (p.group, p.houses, p.house_cost);
        if houses == 0 {
            self.notify("No houses to sell here", Level::Warn);
            return;
        }
        if houses < self.group_house_bounds(group).1 {
            self.notify("Sell evenly across the group first", Level::Warn);
            return;
        }
        // Breaking a hotel takes four houses back out of the bank's stock.
        if houses == HOTEL {
            if self.houses_left < HOTEL - 1 {
                self.notify("The bank has no houses to break the hotel into", Level::Warn);
                return;
            }
            self.houses_left -= HOTEL - 1;
            self.hotels_left += 1;
        } else {
            self.houses_left += 1;
        }
        if let Space::Property(p) = &mut self.board[idx] {
            p.houses -= 1;
        }
        let refund = cost / 2;
        self.players[who].money += refund;
        let name = self.board[idx].name().to_string();
        self.notify(format!("Player {} sold a house on {name} (+${refund})", who + 1), Level::Info);
    }

    /// Lowest and highest house counts across `group` (for even-build rules).
    fn group_house_bounds(&self, group: ColorGroup) -> (u8, u8) {
        let mut min = u8::MAX;
        let mut max = 0;
        for space in &self.board {
            if let Space::Property(p) = space
                && p.group == group
            {
                min = min.min(p.houses);
                max = max.max(p.houses);
            }
        }
        (min, max)
    }

    /// True when any property in `group` is mortgaged, which blocks building.
    fn group_any_mortgaged(&self, group: ColorGroup) -> bool {
        self.board.iter().any(|s| match s {
            Space::Property(p) => p.group == group && p.base.mortgaged,
            _ => false,
        })
    }

    /// True when the color group containing `idx` still has buildings on it,
    /// which blocks mortgaging. Always false for railroads and utilities.
    pub(super) fn group_has_houses(&self, idx: usize) -> bool {
        let Space::Property(p) = &self.board[idx] else {
            return false;
        };
        self.group_house_bounds(p.group).1 > 0
    }

    pub(super) fn render_estate(&self, frame: &mut Frame, menu: &EstateMenu) {
        let (title, keys) = match menu.mode {
            Mode::Mortgage => ("Mortgages", mortgage_keys()),
            Mode::Build => ("Build Houses", build_keys()),
        };
        let lines: Vec<String> = menu
            .slots
            .iter()
            .map(|&i| {
                let s = &self.board[i];
                match menu.mode {
                    Mode::Mortgage => {
                        if s.is_mortgaged() {
                            format!("{}  [unmortgage ${}]", s.name(), s.unmortgage_cost())
                        } else {
                            format!("{}  [mortgage +${}]", s.name(), s.mortgage_value())
                        }
                    }
                    Mode::Build => {
                        let level = match s.houses() {
                            HOTEL => "hotel".to_string(),
                            h => format!("{h} house"),
                        };
                        format!("{}  {level}  (house ${})", s.name(), s.house_cost())
                    }
                }
            })
            .collect();
        choice_popup(frame, title, &lines, menu.cursor.selected, &keys);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testkit::*;
    use crate::game::{TOTAL_HOTELS, TOTAL_HOUSES};

    /// Player 0 owning the brown group outright, with cash to build.
    fn brown_monopoly() -> Game {
        let mut g = game(2, 1500);
        own_group(&mut g, ColorGroup::Brown, 0);
        g
    }

    // --- building -----------------------------------------------------------

    #[test]
    fn building_needs_the_whole_color_group() {
        let mut g = game(2, 1500);
        own(&mut g, MEDITERRANEAN, 0);
        g.open_build();
        assert!(matches!(g.modal, Modal::None), "a partial group offers nothing");
    }

    #[test]
    fn a_house_costs_its_printed_price() {
        let mut g = brown_monopoly();
        g.build_house(MEDITERRANEAN);
        assert_eq!(g.board[MEDITERRANEAN].houses(), 1);
        assert_eq!(g.players[0].money, 1450);
    }

    #[test]
    fn building_must_stay_even_across_the_group() {
        let mut g = brown_monopoly();
        g.build_house(MEDITERRANEAN);
        g.build_house(MEDITERRANEAN);
        assert_eq!(g.board[MEDITERRANEAN].houses(), 1, "the second house must go elsewhere");

        g.build_house(BALTIC);
        g.build_house(MEDITERRANEAN);
        assert_eq!(g.board[MEDITERRANEAN].houses(), 2);
    }

    #[test]
    fn the_fifth_house_is_the_hotel_and_the_last() {
        let mut g = brown_monopoly();
        for _ in 0..HOTEL {
            g.build_house(MEDITERRANEAN);
            g.build_house(BALTIC);
        }
        assert_eq!(g.board[MEDITERRANEAN].houses(), HOTEL);
        g.build_house(MEDITERRANEAN);
        assert_eq!(g.board[MEDITERRANEAN].houses(), HOTEL, "nothing beyond a hotel");
    }

    #[test]
    fn building_is_refused_without_the_cash() {
        let mut g = brown_monopoly();
        g.players[0].money = 10;
        g.build_house(MEDITERRANEAN);
        assert_eq!(g.board[MEDITERRANEAN].houses(), 0);
        assert_eq!(g.players[0].money, 10);
    }

    #[test]
    fn a_mortgaged_group_cannot_be_built_on() {
        let mut g = brown_monopoly();
        g.board[BALTIC].set_mortgaged(true);
        g.build_house(MEDITERRANEAN);
        assert_eq!(g.board[MEDITERRANEAN].houses(), 0);
    }

    // --- the bank's stock ---------------------------------------------------

    #[test]
    fn the_bank_starts_with_the_full_stock() {
        let g = game(2, 1500);
        assert_eq!(g.houses_left, TOTAL_HOUSES);
        assert_eq!(g.hotels_left, TOTAL_HOTELS);
    }

    #[test]
    fn building_draws_houses_out_of_the_bank() {
        let mut g = brown_monopoly();
        g.build_house(MEDITERRANEAN);
        g.build_house(BALTIC);
        assert_eq!(g.houses_left, TOTAL_HOUSES - 2);
        assert_eq!(g.hotels_left, TOTAL_HOTELS);
    }

    #[test]
    fn a_hotel_trades_four_houses_back_in() {
        let mut g = brown_monopoly();
        for _ in 0..HOTEL {
            g.build_house(MEDITERRANEAN);
            g.build_house(BALTIC);
        }
        assert_eq!(g.hotels_left, TOTAL_HOTELS - 2);
        assert_eq!(g.houses_left, TOTAL_HOUSES, "both groups of four came back");
    }

    #[test]
    fn an_empty_house_stock_blocks_building() {
        let mut g = brown_monopoly();
        g.houses_left = 0;
        g.build_house(MEDITERRANEAN);
        assert_eq!(g.board[MEDITERRANEAN].houses(), 0);
        assert_eq!(g.players[0].money, 1500, "and costs nothing");
    }

    #[test]
    fn an_empty_hotel_stock_blocks_the_fifth_house() {
        let mut g = brown_monopoly();
        g.hotels_left = 0;
        set_houses(&mut g, MEDITERRANEAN, 4);
        set_houses(&mut g, BALTIC, 4);
        g.build_house(MEDITERRANEAN);
        assert_eq!(g.board[MEDITERRANEAN].houses(), 4);
    }

    #[test]
    fn selling_returns_houses_to_the_bank() {
        let mut g = brown_monopoly();
        g.build_house(MEDITERRANEAN);
        g.build_house(BALTIC);
        g.sell_house(0, MEDITERRANEAN);
        assert_eq!(g.houses_left, TOTAL_HOUSES - 1);
    }

    #[test]
    fn breaking_a_hotel_takes_four_houses_back_out() {
        let mut g = brown_monopoly();
        set_houses(&mut g, MEDITERRANEAN, HOTEL);
        set_houses(&mut g, BALTIC, HOTEL);
        g.hotels_left = TOTAL_HOTELS - 2;
        g.sell_house(0, MEDITERRANEAN);
        assert_eq!(g.board[MEDITERRANEAN].houses(), 4);
        assert_eq!(g.houses_left, TOTAL_HOUSES - 4);
        assert_eq!(g.hotels_left, TOTAL_HOTELS - 1);
    }

    #[test]
    fn a_hotel_cannot_be_broken_without_houses_to_break_it_into() {
        let mut g = brown_monopoly();
        set_houses(&mut g, MEDITERRANEAN, HOTEL);
        set_houses(&mut g, BALTIC, HOTEL);
        g.houses_left = 3;
        g.sell_house(0, MEDITERRANEAN);
        assert_eq!(g.board[MEDITERRANEAN].houses(), HOTEL, "the trade cannot be made");
        assert_eq!(g.houses_left, 3);
    }

    #[test]
    fn a_bankrupt_estate_returns_its_buildings_to_the_bank() {
        let mut g = game(2, 1500);
        own_group(&mut g, ColorGroup::Brown, 0);
        g.build_house(MEDITERRANEAN);
        g.build_house(BALTIC);
        assert_eq!(g.houses_left, TOTAL_HOUSES - 2);
        g.bankrupt(0, None);
        assert_eq!(g.houses_left, TOTAL_HOUSES);
    }

    // --- selling ------------------------------------------------------------

    #[test]
    fn a_house_sells_back_for_half() {
        let mut g = brown_monopoly();
        set_houses(&mut g, MEDITERRANEAN, 1);
        set_houses(&mut g, BALTIC, 1);
        g.sell_house(0, MEDITERRANEAN);
        assert_eq!(g.board[MEDITERRANEAN].houses(), 0);
        assert_eq!(g.players[0].money, 1525);
    }

    #[test]
    fn selling_must_stay_even_across_the_group() {
        let mut g = brown_monopoly();
        set_houses(&mut g, MEDITERRANEAN, 1);
        set_houses(&mut g, BALTIC, 2);
        g.sell_house(0, MEDITERRANEAN);
        assert_eq!(g.board[MEDITERRANEAN].houses(), 1, "sell off the taller street first");
    }

    #[test]
    fn selling_from_a_bare_street_is_refused() {
        let mut g = brown_monopoly();
        g.sell_house(0, MEDITERRANEAN);
        assert_eq!(g.players[0].money, 1500);
    }

    // --- mortgages ----------------------------------------------------------

    #[test]
    fn a_mortgage_pays_half_the_printed_price() {
        let mut g = game(2, 1500);
        own(&mut g, MEDITERRANEAN, 0);
        g.mortgage(0, MEDITERRANEAN);
        assert!(g.board[MEDITERRANEAN].is_mortgaged());
        assert_eq!(g.players[0].money, 1530);
    }

    #[test]
    fn lifting_a_mortgage_costs_the_value_plus_ten_percent() {
        let mut g = game(2, 1500);
        own(&mut g, MEDITERRANEAN, 0);
        g.mortgage(0, MEDITERRANEAN);
        g.unmortgage(0, MEDITERRANEAN);
        assert!(!g.board[MEDITERRANEAN].is_mortgaged());
        assert_eq!(g.players[0].money, 1497, "1500 + 30 - 33");
    }

    #[test]
    fn lifting_is_refused_without_the_cash() {
        let mut g = game(2, 1500);
        own(&mut g, MEDITERRANEAN, 0);
        g.board[MEDITERRANEAN].set_mortgaged(true);
        g.players[0].money = 10;
        g.unmortgage(0, MEDITERRANEAN);
        assert!(g.board[MEDITERRANEAN].is_mortgaged());
        assert_eq!(g.players[0].money, 10);
    }

    #[test]
    fn mortgaging_is_blocked_while_the_group_holds_buildings() {
        let mut g = brown_monopoly();
        set_houses(&mut g, BALTIC, 1);
        g.mortgage(0, MEDITERRANEAN);
        assert!(!g.board[MEDITERRANEAN].is_mortgaged(), "sell the group's houses first");
    }

    #[test]
    fn railroads_mortgage_freely() {
        let mut g = game(2, 1500);
        own(&mut g, READING_RR, 0);
        g.mortgage(0, READING_RR);
        assert!(g.board[READING_RR].is_mortgaged());
        assert_eq!(g.players[0].money, 1600);
    }

    #[test]
    fn mortgaging_twice_is_refused() {
        let mut g = game(2, 1500);
        own(&mut g, MEDITERRANEAN, 0);
        g.mortgage(0, MEDITERRANEAN);
        g.mortgage(0, MEDITERRANEAN);
        assert_eq!(g.players[0].money, 1530, "paid out once");
    }

    #[test]
    fn the_mortgage_list_needs_something_to_list() {
        let mut g = game(2, 1500);
        g.open_mortgages();
        assert!(matches!(g.modal, Modal::None));

        own(&mut g, MEDITERRANEAN, 0);
        g.open_mortgages();
        assert!(matches!(g.modal, Modal::Estate(_)));
    }

    #[test]
    fn the_estate_popup_closes_on_escape() {
        let mut g = brown_monopoly();
        g.open_build();
        assert!(matches!(g.modal, Modal::Estate(_)));
        g.handle_key(KeyCode::Esc);
        assert!(matches!(g.modal, Modal::None));
    }

    #[test]
    fn the_build_popup_buys_and_sells_through_the_cursor() {
        let mut g = brown_monopoly();
        g.open_build();
        g.handle_key(KeyCode::Enter); // build on Mediterranean
        assert_eq!(g.board[MEDITERRANEAN].houses(), 1);
        g.handle_key(KeyCode::Char('s'));
        assert_eq!(g.board[MEDITERRANEAN].houses(), 0);
    }
}
