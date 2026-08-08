//! Hotseat trading: the current player buys one property from, or sells one to,
//! another player for cash. Built in stages, then executed.

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui_notifications::Level;

use super::{Game, Modal};
use crate::ui::{Cursor, choice_popup, info_popup};

const STEP: u32 = 10;

/// Which step of building a trade we're on.
#[derive(Debug, Clone, Copy, PartialEq)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testkit::*;

    /// Walk the builder to the price stage for the only tradeable property.
    fn to_price_stage(g: &mut Game) {
        g.open_trade();
        g.handle_key(KeyCode::Enter); // pick the partner
        g.handle_key(KeyCode::Enter); // pick the property
    }

    #[test]
    fn trading_needs_a_partner() {
        let mut g = game(2, 1500);
        g.players[1].bankrupt = true;
        g.open_trade();
        assert!(matches!(g.modal, Modal::None));
    }

    #[test]
    fn trading_needs_something_to_trade() {
        let mut g = game(2, 1500);
        g.open_trade();
        g.handle_key(KeyCode::Enter);
        assert!(matches!(g.modal, Modal::None), "neither side owns anything");
    }

    #[test]
    fn the_price_defaults_to_the_printed_value() {
        let mut g = game(2, 1500);
        own(&mut g, BOARDWALK, 1);
        to_price_stage(&mut g);
        let Modal::Trade(t) = &g.modal else { panic!("expected the trade builder") };
        assert_eq!(t.price, 400);
    }

    #[test]
    fn the_price_adjusts_in_steps_and_never_goes_negative() {
        let mut g = game(2, 1500);
        own(&mut g, MEDITERRANEAN, 1);
        to_price_stage(&mut g);
        g.handle_key(KeyCode::Right);
        let Modal::Trade(t) = &g.modal else { panic!("expected the trade builder") };
        assert_eq!(t.price, 60 + STEP);

        for _ in 0..20 {
            g.handle_key(KeyCode::Left);
        }
        let Modal::Trade(t) = &g.modal else { panic!("expected the trade builder") };
        assert_eq!(t.price, 0);
    }

    #[test]
    fn buying_from_a_partner_moves_the_deed_and_the_cash() {
        let mut g = game(2, 1500);
        own(&mut g, BOARDWALK, 1);
        to_price_stage(&mut g);
        g.handle_key(KeyCode::Enter);
        assert!(matches!(g.modal, Modal::None));
        assert_eq!(g.board[BOARDWALK].owner(), Some(0));
        assert_eq!(g.players[0].money, 1100);
        assert_eq!(g.players[1].money, 1900);
    }

    #[test]
    fn selling_to_a_partner_moves_the_cash_the_other_way() {
        let mut g = game(2, 1500);
        own(&mut g, BOARDWALK, 0);
        to_price_stage(&mut g);
        g.handle_key(KeyCode::Enter);
        assert_eq!(g.board[BOARDWALK].owner(), Some(1));
        assert_eq!(g.players[0].money, 1900);
        assert_eq!(g.players[1].money, 1100);
    }

    #[test]
    fn a_buyer_who_cannot_pay_keeps_the_builder_open() {
        let mut g = game(2, 1500);
        g.players[0].money = 10;
        own(&mut g, BOARDWALK, 1);
        to_price_stage(&mut g);
        g.handle_key(KeyCode::Enter);
        assert!(matches!(g.modal, Modal::Trade(_)), "so the price can be lowered");
        assert_eq!(g.board[BOARDWALK].owner(), Some(1));
    }

    #[test]
    fn a_built_up_street_cannot_change_hands() {
        let mut g = game(2, 1500);
        own(&mut g, BOARDWALK, 1);
        set_houses(&mut g, BOARDWALK, 1);
        to_price_stage(&mut g);
        g.handle_key(KeyCode::Enter);
        assert!(matches!(g.modal, Modal::Trade(_)));
        assert_eq!(g.board[BOARDWALK].owner(), Some(1), "sell the houses first");
    }

    #[test]
    fn escape_steps_back_through_the_stages() {
        let mut g = game(2, 1500);
        own(&mut g, BOARDWALK, 1);
        to_price_stage(&mut g);
        g.handle_key(KeyCode::Esc);
        let Modal::Trade(t) = &g.modal else { panic!("expected the trade builder") };
        assert_eq!(t.stage, Stage::Property);

        g.handle_key(KeyCode::Esc);
        let Modal::Trade(t) = &g.modal else { panic!("expected the trade builder") };
        assert_eq!(t.stage, Stage::Partner);

        g.handle_key(KeyCode::Esc);
        assert!(matches!(g.modal, Modal::None));
    }

    #[test]
    fn both_sides_holdings_are_on_the_table() {
        let mut g = game(2, 1500);
        own(&mut g, MEDITERRANEAN, 0);
        own(&mut g, BOARDWALK, 1);
        own(&mut g, ORIENTAL, 1);
        assert_eq!(g.tradeable(0, 1), vec![MEDITERRANEAN, ORIENTAL, BOARDWALK]);
    }
}
