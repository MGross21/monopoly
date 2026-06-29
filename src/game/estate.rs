//! Property management: mortgaging/unmortgaging and building or selling houses,
//! both presented as a list of the current player's holdings.

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui_notifications::Level;

use super::{Game, HOTEL, Modal};
use crate::space::{ColorGroup, Space};
use crate::ui::{Cursor, choice_popup};

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
        let me = self.current;
        let slots: Vec<usize> =
            (0..self.board.len()).filter(|&i| self.board[i].owner() == Some(me)).collect();
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
                Mode::Mortgage => self.toggle_mortgage(menu.selected()),
                Mode::Build => self.build_house(menu.selected()),
            },
            // Sell a house back (build mode only).
            KeyCode::Char('s') if menu.mode == Mode::Build => self.sell_house(menu.selected()),
            _ => {}
        }
        true
    }

    /// Toggle a property's mortgage: lift it for half price + 10% interest, or
    /// take the mortgage and receive half its price. Houses block mortgaging.
    fn toggle_mortgage(&mut self, idx: usize) {
        let me = self.current;
        if self.board[idx].houses() > 0 {
            self.notify("Sell the houses on this group first", Level::Warn);
            return;
        }
        let half = self.board[idx].price().unwrap_or(0) / 2;
        let name = self.board[idx].name().to_string();
        if self.board[idx].is_mortgaged() {
            let lift = half + half / 10; // value + 10% interest
            if self.players[me].money < lift {
                self.notify(format!("Need ${lift} to unmortgage {name}"), Level::Error);
                return;
            }
            self.players[me].money -= lift;
            self.board[idx].set_mortgaged(false);
            self.notify(format!("Player {} unmortgaged {name} for ${lift}", me + 1), Level::Info);
        } else {
            self.players[me].money += half;
            self.board[idx].set_mortgaged(true);
            self.notify(format!("Player {} mortgaged {name} for ${half}", me + 1), Level::Info);
        }
    }

    /// Build one house (or the hotel) on `idx`, enforcing even building.
    fn build_house(&mut self, idx: usize) {
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
        if self.players[me].money < cost {
            self.notify(format!("Need ${cost} to build here"), Level::Error);
            return;
        }
        self.players[me].money -= cost;
        if let Space::Property(p) = &mut self.board[idx] {
            p.houses += 1;
        }
        let name = self.board[idx].name().to_string();
        let what = if houses + 1 == HOTEL { "a hotel" } else { "a house" };
        self.notify(format!("Player {} built {what} on {name} (-${cost})", me + 1), Level::Info);
    }

    /// Sell one house back to the bank for half its cost, even-build enforced.
    fn sell_house(&mut self, idx: usize) {
        let me = self.current;
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
        if let Space::Property(p) = &mut self.board[idx] {
            p.houses -= 1;
        }
        let refund = cost / 2;
        self.players[me].money += refund;
        let name = self.board[idx].name().to_string();
        self.notify(format!("Player {} sold a house on {name} (+${refund})", me + 1), Level::Info);
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

    /// Is any property in `group` mortgaged? (Blocks building.)
    fn group_any_mortgaged(&self, group: ColorGroup) -> bool {
        self.board.iter().any(|s| match s {
            Space::Property(p) => p.group == group && p.base.mortgaged,
            _ => false,
        })
    }

    pub(super) fn render_estate(&self, frame: &mut Frame, menu: &EstateMenu) {
        let title = match menu.mode {
            Mode::Mortgage => " Mortgages ",
            Mode::Build => " Build Houses (Enter buy, s sell) ",
        };
        let lines: Vec<String> = menu
            .slots
            .iter()
            .map(|&i| {
                let s = &self.board[i];
                match menu.mode {
                    Mode::Mortgage => {
                        let half = s.price().unwrap_or(0) / 2;
                        if s.is_mortgaged() {
                            format!("{}  [unmortgage ${}]", s.name(), half + half / 10)
                        } else {
                            format!("{}  [mortgage +${half}]", s.name())
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
        choice_popup(frame, title, &lines, menu.cursor.selected);
    }
}
