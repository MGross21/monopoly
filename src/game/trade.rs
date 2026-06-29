//! Hotseat trading: the current player buys one property from, or sells one to,
//! another player for cash. Built in stages, then executed.

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui_notifications::Level;

use super::{Game, Modal};
use crate::ui::{Cursor, choice_popup, info_popup};

const STEP: u32 = 10;

/// Which step of building a trade we're on.
#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Partner,  // choosing who to trade with
    Property, // choosing which property changes hands
    Price,    // setting the cash that moves the other way
}

/// A one-property-for-cash trade. Whoever owns the chosen property sells it; the
/// other side buys for `price`.
pub(super) struct Trade {
    stage: Stage,
    partners: Vec<usize>,
    pcursor: Cursor,
    partner: usize,
    props: Vec<usize>,
    prop_cursor: Cursor,
    price: u32,
}

impl Game {
    /// Open the trade builder. Needs another solvent player to trade with.
    pub(super) fn open_trade(&mut self) {
        let me = self.current;
        let mut partners = self.active_players();
        partners.retain(|&i| i != me);
        if partners.is_empty() {
            self.notify("No one to trade with", Level::Warn);
            return;
        }
        let pcursor = Cursor::new(partners.len());
        let partner = partners[0];
        self.modal = Modal::Trade(Trade {
            stage: Stage::Partner,
            partners,
            pcursor,
            partner,
            props: Vec::new(),
            prop_cursor: Cursor::new(0),
            price: 0,
        });
    }

    /// Properties owned by either `a` or `b`, in board order.
    fn tradeable(&self, a: usize, b: usize) -> Vec<usize> {
        (0..self.board.len())
            .filter(|&i| matches!(self.board[i].owner(), Some(o) if o == a || o == b))
            .collect()
    }

    /// Drive the trade builder. Returns `true` to keep the popup open.
    pub(super) fn trade_input(&mut self, t: &mut Trade, key: KeyCode) -> bool {
        match t.stage {
            Stage::Partner => match key {
                KeyCode::Up => t.pcursor.up(),
                KeyCode::Down => t.pcursor.down(),
                KeyCode::Esc => return false,
                KeyCode::Enter => {
                    t.partner = t.partners[t.pcursor.selected];
                    t.props = self.tradeable(self.current, t.partner);
                    if t.props.is_empty() {
                        self.notify("Neither of you owns anything to trade", Level::Warn);
                        return false;
                    }
                    t.prop_cursor = Cursor::new(t.props.len());
                    t.stage = Stage::Property;
                }
                _ => {}
            },
            Stage::Property => match key {
                KeyCode::Up => t.prop_cursor.up(),
                KeyCode::Down => t.prop_cursor.down(),
                KeyCode::Esc => t.stage = Stage::Partner,
                KeyCode::Enter => {
                    let idx = t.props[t.prop_cursor.selected];
                    t.price = self.board[idx].price().unwrap_or(0); // sensible default
                    t.stage = Stage::Price;
                }
                _ => {}
            },
            Stage::Price => match key {
                KeyCode::Left | KeyCode::Down => t.price = t.price.saturating_sub(STEP),
                KeyCode::Right | KeyCode::Up => t.price += STEP,
                KeyCode::Esc => t.stage = Stage::Property,
                KeyCode::Enter => return self.execute_trade(t),
                _ => {}
            },
        }
        true
    }

    /// Carry out the built trade. Returns `true` to keep the popup open (so the
    /// player can adjust) when the trade can't go through.
    fn execute_trade(&mut self, t: &Trade) -> bool {
        let idx = t.props[t.prop_cursor.selected];
        if self.board[idx].houses() > 0 {
            self.notify("Sell the group's houses before trading it", Level::Warn);
            return true;
        }
        let seller = self.board[idx].owner().unwrap();
        let buyer = if seller == self.current { t.partner } else { self.current };
        if self.players[buyer].money < t.price {
            self.notify(format!("Player {} can't afford ${}", buyer + 1, t.price), Level::Warn);
            return true;
        }
        self.players[buyer].money -= t.price;
        self.players[seller].money += t.price;
        self.board[idx].set_owner(Some(buyer));
        let name = self.board[idx].name().to_string();
        self.notify(
            format!("Player {} bought {name} from Player {} for ${}", buyer + 1, seller + 1, t.price),
            Level::Info,
        );
        false
    }

    pub(super) fn render_trade(&self, frame: &mut Frame, t: &Trade) {
        match t.stage {
            Stage::Partner => {
                let labels: Vec<String> = t
                    .partners
                    .iter()
                    .map(|&p| format!("Player {}  (${})", p + 1, self.players[p].money))
                    .collect();
                choice_popup(frame, " Trade — pick a player ", &labels, t.pcursor.selected);
            }
            Stage::Property => {
                let labels: Vec<String> = t
                    .props
                    .iter()
                    .map(|&i| {
                        let owner = self.board[i].owner().unwrap();
                        format!("{}  (Player {})", self.board[i].name(), owner + 1)
                    })
                    .collect();
                choice_popup(frame, " Trade — pick a property ", &labels, t.prop_cursor.selected);
            }
            Stage::Price => {
                let idx = t.props[t.prop_cursor.selected];
                let owner = self.board[idx].owner().unwrap();
                let buyer = if owner == self.current { t.partner } else { self.current };
                let lines = vec![
                    format!("{} from Player {}", self.board[idx].name(), owner + 1),
                    format!("Player {} pays ${}", buyer + 1, t.price),
                    "[←/→] adjust   [enter] confirm   [esc] back".to_string(),
                ];
                info_popup(frame, " Trade — price ", &lines);
            }
        }
    }
}
