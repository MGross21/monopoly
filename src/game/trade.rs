//! Hotseat trading: the current player buys one item from, or sells one to,
//! another player for cash. An item is a title deed or a Get Out of Jail Free
//! card. Built in stages, then executed.

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui_notifications::Level;

use super::{Game, Modal};
use crate::ui::{Cursor, choice_popup, info_popup};

const STEP: u32 = 10;

/// What the bail on a Get Out of Jail Free card is worth, used as the opening
/// price when one is put up for trade.
const CARD_VALUE: u32 = 50;

/// Which step of building a trade we're on.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    Partner, // choosing who to trade with
    Item,    // choosing what changes hands
    Price,   // setting the cash that moves the other way
}

/// Something that can change hands.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Item {
    /// A title deed, by board index.
    Deed(usize),
    /// One Get Out of Jail Free card held by this player.
    JailCard(usize),
}

impl Item {
    /// The side giving the item up.
    fn holder(self, game: &Game) -> usize {
        match self {
            Item::Deed(idx) => game.board[idx].owner().expect("only owned deeds are offered"),
            Item::JailCard(who) => who,
        }
    }

    /// The opening asking price.
    fn value(self, game: &Game) -> u32 {
        match self {
            Item::Deed(idx) => game.board[idx].price().unwrap_or(0),
            Item::JailCard(_) => CARD_VALUE,
        }
    }

    fn label(self, game: &Game) -> String {
        match self {
            Item::Deed(idx) => game.board[idx].name().to_string(),
            Item::JailCard(_) => "Get Out of Jail Free".to_string(),
        }
    }
}

/// A one-item-for-cash trade. Whoever holds the chosen item sells it; the other
/// side buys for `price`.
pub(super) struct Trade {
    stage: Stage,
    partners: Vec<usize>,
    pcursor: Cursor,
    partner: usize,
    items: Vec<Item>,
    item_cursor: Cursor,
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
            items: Vec::new(),
            item_cursor: Cursor::new(0),
            price: 0,
        });
    }

    /// Everything either side can put up: deeds in board order, then any Get Out
    /// of Jail Free cards they hold.
    fn tradeable(&self, a: usize, b: usize) -> Vec<Item> {
        let deeds = (0..self.board.len())
            .filter(|&i| matches!(self.board[i].owner(), Some(o) if o == a || o == b))
            .map(Item::Deed);
        let cards = [a, b]
            .into_iter()
            .filter(|&who| self.players[who].get_out_free > 0)
            .map(Item::JailCard);
        deeds.chain(cards).collect()
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
                    t.items = self.tradeable(self.current, t.partner);
                    if t.items.is_empty() {
                        self.notify("Neither of you has anything to trade", Level::Warn);
                        return false;
                    }
                    t.item_cursor = Cursor::new(t.items.len());
                    t.stage = Stage::Item;
                }
                _ => {}
            },
            Stage::Item => match key {
                KeyCode::Up => t.item_cursor.up(),
                KeyCode::Down => t.item_cursor.down(),
                KeyCode::Esc => t.stage = Stage::Partner,
                KeyCode::Enter => {
                    t.price = t.items[t.item_cursor.selected].value(self); // sensible default
                    t.stage = Stage::Price;
                }
                _ => {}
            },
            Stage::Price => match key {
                KeyCode::Left | KeyCode::Down => t.price = t.price.saturating_sub(STEP),
                KeyCode::Right | KeyCode::Up => t.price += STEP,
                KeyCode::Esc => t.stage = Stage::Item,
                KeyCode::Enter => return self.execute_trade(t),
                _ => {}
            },
        }
        true
    }

    /// Carry out the built trade. Returns `true` to keep the popup open (so the
    /// player can adjust) when the trade can't go through.
    fn execute_trade(&mut self, t: &Trade) -> bool {
        let item = t.items[t.item_cursor.selected];
        if let Item::Deed(idx) = item
            && self.group_has_houses(idx)
        {
            self.notify("Sell the group's houses before trading it", Level::Warn);
            return true;
        }
        let seller = item.holder(self);
        let buyer = if seller == self.current { t.partner } else { self.current };
        if self.players[buyer].money < t.price {
            self.notify(format!("Player {} can't afford ${}", buyer + 1, t.price), Level::Warn);
            return true;
        }
        self.players[buyer].money -= t.price;
        self.players[seller].money += t.price;
        match item {
            Item::Deed(idx) => self.board[idx].set_owner(Some(buyer)),
            Item::JailCard(_) => {
                self.players[seller].get_out_free -= 1;
                self.players[buyer].get_out_free += 1;
            }
        }
        let what = item.label(self);
        self.notify(
            format!(
                "Player {} bought {what} from Player {} for ${}",
                buyer + 1,
                seller + 1,
                t.price
            ),
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
            Stage::Item => {
                let labels: Vec<String> = t
                    .items
                    .iter()
                    .map(|&item| {
                        format!("{}  (Player {})", item.label(self), item.holder(self) + 1)
                    })
                    .collect();
                choice_popup(frame, " Trade — pick an item ", &labels, t.item_cursor.selected);
            }
            Stage::Price => {
                let item = t.items[t.item_cursor.selected];
                let seller = item.holder(self);
                let buyer = if seller == self.current { t.partner } else { self.current };
                let lines = vec![
                    format!("{} from Player {}", item.label(self), seller + 1),
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
    use crate::space::ColorGroup;

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
        assert_eq!(t.stage, Stage::Item);

        g.handle_key(KeyCode::Esc);
        let Modal::Trade(t) = &g.modal else { panic!("expected the trade builder") };
        assert_eq!(t.stage, Stage::Partner);

        g.handle_key(KeyCode::Esc);
        assert!(matches!(g.modal, Modal::None));
    }

    #[test]
    fn a_jail_card_can_be_put_up_for_trade() {
        let mut g = game(2, 1500);
        g.players[1].get_out_free = 1;
        assert_eq!(g.tradeable(0, 1), vec![Item::JailCard(1)]);
    }

    #[test]
    fn a_jail_card_opens_at_the_price_of_bail() {
        let mut g = game(2, 1500);
        g.players[1].get_out_free = 1;
        to_price_stage(&mut g);
        let Modal::Trade(t) = &g.modal else { panic!("expected the trade builder") };
        assert_eq!(t.price, CARD_VALUE);
    }

    #[test]
    fn buying_a_jail_card_moves_it_and_the_cash() {
        let mut g = game(2, 1500);
        g.players[1].get_out_free = 1;
        to_price_stage(&mut g);
        g.handle_key(KeyCode::Enter);
        assert_eq!(g.players[0].get_out_free, 1);
        assert_eq!(g.players[1].get_out_free, 0);
        assert_eq!(g.players[0].money, 1450);
        assert_eq!(g.players[1].money, 1550);
    }

    #[test]
    fn selling_your_own_jail_card_moves_the_cash_the_other_way() {
        let mut g = game(2, 1500);
        g.players[0].get_out_free = 1;
        to_price_stage(&mut g);
        g.handle_key(KeyCode::Enter);
        assert_eq!(g.players[0].get_out_free, 0);
        assert_eq!(g.players[1].get_out_free, 1);
        assert_eq!(g.players[0].money, 1550);
    }

    #[test]
    fn only_held_jail_cards_are_offered() {
        let g = game(2, 1500);
        assert!(g.tradeable(0, 1).is_empty(), "nobody holds one");
    }

    #[test]
    fn a_trade_is_blocked_when_the_group_still_has_houses() {
        let mut g = game(2, 1500);
        own_group(&mut g, ColorGroup::Brown, 1);
        set_houses(&mut g, BALTIC, 1);
        to_price_stage(&mut g); // Mediterranean is the first item, and is bare
        g.handle_key(KeyCode::Enter);
        assert!(matches!(g.modal, Modal::Trade(_)));
        assert_eq!(g.board[MEDITERRANEAN].owner(), Some(1), "the group is developed");
    }

    #[test]
    fn both_sides_holdings_are_on_the_table() {
        let mut g = game(2, 1500);
        own(&mut g, MEDITERRANEAN, 0);
        own(&mut g, BOARDWALK, 1);
        own(&mut g, ORIENTAL, 1);
        let deeds: Vec<usize> = g
            .tradeable(0, 1)
            .into_iter()
            .filter_map(|item| match item {
                Item::Deed(idx) => Some(idx),
                Item::JailCard(_) => None,
            })
            .collect();
        assert_eq!(deeds, vec![MEDITERRANEAN, ORIENTAL, BOARDWALK]);
    }
}
