//! Bank auctions: when a player declines to buy, the property is auctioned to
//! the highest bidder among the remaining players.

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui_notifications::Level;

use super::{Game, Modal};

const STEP: u32 = 10;

/// One property up for auction. Players in `active` take turns bidding up by
/// `STEP` or passing; the last standing high bidder wins.
pub(super) struct Auction {
    pos: usize,
    active: Vec<usize>, // players still bidding, in seating order
    turn: usize,        // index into `active`
    high_bid: u32,
    high_bidder: Option<usize>,
}

impl Game {
    /// Put `pos` up for auction among all players still in the game.
    pub(super) fn start_auction(&mut self, pos: usize) {
        let active = self.active_players();
        if active.is_empty() {
            return;
        }
        let name = self.board[pos].name().to_string();
        self.notify(format!("{name} goes to auction"), Level::Info);
        self.modal = Modal::Auction(Auction { pos, active, turn: 0, high_bid: 0, high_bidder: None });
    }

    /// Handle one auction key press. Returns `true` to keep the auction open.
    pub(super) fn auction_input(&mut self, auc: &mut Auction, key: KeyCode) -> bool {
        match key {
            KeyCode::Char('b') => self.auction_bid(auc),
            KeyCode::Char('p') => self.auction_pass(auc),
            KeyCode::Esc => {
                self.notify("Auction cancelled", Level::Warn);
                false
            }
            _ => true,
        }
    }

    fn auction_bid(&mut self, auc: &mut Auction) -> bool {
        let cur = auc.active[auc.turn];
        let next = auc.high_bid + STEP;
        if self.players[cur].money < next {
            self.notify(format!("Player {} can't afford ${next}", cur + 1), Level::Warn);
            return true;
        }
        auc.high_bid = next;
        auc.high_bidder = Some(cur);
        self.notify(format!("Player {} bids ${next}", cur + 1), Level::Info);
        // A lone remaining bidder wins immediately.
        if auc.active.len() == 1 {
            self.finish_auction(auc);
            return false;
        }
        auc.turn = (auc.turn + 1) % auc.active.len();
        true
    }

    fn auction_pass(&mut self, auc: &mut Auction) -> bool {
        let cur = auc.active[auc.turn];
        self.notify(format!("Player {} passes", cur + 1), Level::Info);
        auc.active.remove(auc.turn);
        if auc.active.is_empty() {
            self.notify("No bids — the property stays with the bank", Level::Warn);
            return false;
        }
        if auc.turn >= auc.active.len() {
            auc.turn = 0;
        }
        // Once only the high bidder is left, they win.
        if auc.active.len() == 1 && auc.high_bidder == Some(auc.active[0]) {
            self.finish_auction(auc);
            return false;
        }
        true
    }

    fn finish_auction(&mut self, auc: &Auction) {
        if let Some(winner) = auc.high_bidder {
            self.players[winner].money -= auc.high_bid;
            self.board[auc.pos].set_owner(Some(winner));
            let name = self.board[auc.pos].name().to_string();
            self.notify(format!("Player {} won {name} for ${}", winner + 1, auc.high_bid), Level::Info);
        }
    }

    pub(super) fn render_auction(&self, frame: &mut Frame, auc: &Auction) {
        let cur = auc.active[auc.turn];
        let high = match auc.high_bidder {
            Some(w) => format!("High bid: ${} (Player {})", auc.high_bid, w + 1),
            None => "No bids yet".to_string(),
        };
        let lines = vec![
            format!("Property: {}", self.board[auc.pos].name()),
            high,
            format!("Player {}'s turn", cur + 1),
            format!("[b] bid +${STEP}   [p] pass   [esc] cancel"),
        ];
        crate::ui::info_popup(frame, " Auction ", &lines);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testkit::*;

    fn bidding(count: usize) -> Game {
        let mut g = game(count, 1500);
        g.start_auction(BOARDWALK);
        g
    }

    #[test]
    fn declining_the_buy_prompt_starts_an_auction() {
        let mut g = game(2, 1500);
        place(&mut g, 0, BOARDWALK - 5);
        g.apply_roll(2, 3);
        assert!(matches!(g.modal, Modal::Buy { .. }));
        g.handle_key(KeyCode::Enter); // the prompt defaults to "No"
        assert!(matches!(g.modal, Modal::Auction(_)));
    }

    #[test]
    fn each_bid_raises_the_price_by_one_step() {
        let mut g = bidding(2);
        g.handle_key(KeyCode::Char('b'));
        g.handle_key(KeyCode::Char('b'));
        let Modal::Auction(auc) = &g.modal else { panic!("expected the auction") };
        assert_eq!(auc.high_bid, STEP * 2);
        assert_eq!(auc.high_bidder, Some(1));
    }

    #[test]
    fn bidding_passes_the_turn_round_the_table() {
        let mut g = bidding(3);
        g.handle_key(KeyCode::Char('b'));
        let Modal::Auction(auc) = &g.modal else { panic!("expected the auction") };
        assert_eq!(auc.active[auc.turn], 1);
    }

    #[test]
    fn the_last_bidder_left_wins_the_lot() {
        let mut g = bidding(2);
        g.handle_key(KeyCode::Char('b')); // player 0 bids $10
        g.handle_key(KeyCode::Char('p')); // player 1 passes
        assert!(matches!(g.modal, Modal::None), "the auction is over");
        assert_eq!(g.board[BOARDWALK].owner(), Some(0));
        assert_eq!(g.players[0].money, 1490);
    }

    #[test]
    fn nobody_bidding_leaves_the_lot_with_the_bank() {
        let mut g = bidding(2);
        g.handle_key(KeyCode::Char('p'));
        g.handle_key(KeyCode::Char('p'));
        assert!(matches!(g.modal, Modal::None));
        assert_eq!(g.board[BOARDWALK].owner(), None);
        assert_eq!(g.players[0].money, 1500);
    }

    #[test]
    fn a_bid_beyond_your_cash_is_refused() {
        let mut g = bidding(2);
        g.players[0].money = 5;
        g.handle_key(KeyCode::Char('b'));
        let Modal::Auction(auc) = &g.modal else { panic!("expected the auction") };
        assert_eq!(auc.high_bidder, None);
        assert_eq!(auc.active[auc.turn], 0, "and the turn does not move on");
    }

    #[test]
    fn eliminated_players_are_not_invited() {
        let mut g = game(3, 1500);
        g.players[1].bankrupt = true;
        g.start_auction(BOARDWALK);
        let Modal::Auction(auc) = &g.modal else { panic!("expected the auction") };
        assert_eq!(auc.active, vec![0, 2]);
    }

    #[test]
    fn a_three_way_auction_settles_on_the_survivor() {
        let mut g = bidding(3);
        g.handle_key(KeyCode::Char('b')); // 0 bids 10
        g.handle_key(KeyCode::Char('b')); // 1 bids 20
        g.handle_key(KeyCode::Char('p')); // 2 out
        g.handle_key(KeyCode::Char('b')); // 0 bids 30
        g.handle_key(KeyCode::Char('p')); // 1 out, leaving 0 as high bidder
        assert_eq!(g.board[BOARDWALK].owner(), Some(0));
        assert_eq!(g.players[0].money, 1470);
    }

    #[test]
    fn cancelling_leaves_the_lot_unsold() {
        let mut g = bidding(2);
        g.handle_key(KeyCode::Char('b'));
        g.handle_key(KeyCode::Esc);
        assert!(matches!(g.modal, Modal::None));
        assert_eq!(g.board[BOARDWALK].owner(), None);
        assert_eq!(g.players[0].money, 1500, "an unsold lot costs nothing");
    }
}
