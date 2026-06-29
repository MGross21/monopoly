//! Chance and Community Chest: the two decks, what each card does, and applying
//! a drawn card to the current player.
//!
//! Board indices referenced by `AdvanceTo` (see `board.rs`): GO = 0,
//! Reading RR = 5, St. Charles Place = 11, Illinois Ave = 24.

use std::collections::VecDeque;

use ratatui_notifications::Level;

use super::{BOARD_LEN, Game, HOTEL, InfoBox, Modal};
use crate::ui::dice::Clip;

/// What drawing a card does to the current player.
#[derive(Clone, Copy)]
enum CardEffect {
    /// Receive money from the bank.
    Collect(u32),
    /// Pay money to the bank.
    Pay(u32),
    /// Collect this much from every other player still in the game.
    CollectEach(u32),
    /// Pay this much to every other player still in the game.
    PayEach(u32),
    /// Advance forward to an absolute board index, collecting GO if wrapping.
    AdvanceTo(usize),
    /// Move back this many spaces (never collects GO).
    Back(usize),
    /// Go directly to Jail.
    GoToJail,
    /// Keep a Get Out of Jail Free card.
    GetOutFree,
    /// Pay the bank per house and per hotel you own.
    Repairs { per_house: u32, per_hotel: u32 },
}

/// One card: which deck title to show, its flavor text, and its effect.
#[derive(Clone, Copy)]
pub(super) struct Card {
    pub(super) title: &'static str, // shown by the popup in `mod`
    text: &'static str,
    effect: CardEffect,
}

/// Which deck to draw from.
#[derive(Clone, Copy)]
pub(super) enum Deck {
    Chance,
    Chest,
}

/// A drawn card's animation playing in the center; its effect runs once the
/// animation settles (or is skipped).
pub(super) struct CardDraw {
    pub(super) clip: Clip,
    pub(super) card: Card,
}

const CHANCE_TITLE: &str = " Chance ";
const CHEST_TITLE: &str = " Community Chest ";

/// Two freshly shuffled draw piles, one per deck.
pub(super) fn fresh_decks() -> (VecDeque<Card>, VecDeque<Card>) {
    (shuffled(chance_deck()), shuffled(chest_deck()))
}

/// Fisher-Yates shuffle into a draw pile.
fn shuffled(mut cards: Vec<Card>) -> VecDeque<Card> {
    for i in (1..cards.len()).rev() {
        let j = rand::random_range(0..=i);
        cards.swap(i, j);
    }
    cards.into()
}

/// The Chance deck, in printed order (the game shuffles it).
fn chance_deck() -> Vec<Card> {
    use CardEffect::*;
    let card = |text, effect| Card { title: CHANCE_TITLE, text, effect };
    vec![
        card("Advance to GO (collect $200)", AdvanceTo(0)),
        card("Advance to Illinois Ave", AdvanceTo(24)),
        card("Advance to St. Charles Place", AdvanceTo(11)),
        card("Advance to Reading Railroad", AdvanceTo(5)),
        card("Bank pays you a dividend of $50", Collect(50)),
        card("Get Out of Jail Free", GetOutFree),
        card("Go back 3 spaces", Back(3)),
        card("Go directly to Jail", GoToJail),
        card("Make general repairs: $25/house, $100/hotel", Repairs { per_house: 25, per_hotel: 100 }),
        card("Speeding fine — pay $15", Pay(15)),
        card("Chairman of the Board — pay each player $50", PayEach(50)),
        card("Your building loan matures — collect $150", Collect(150)),
    ]
}

/// The Community Chest deck, in printed order (the game shuffles it).
fn chest_deck() -> Vec<Card> {
    use CardEffect::*;
    let card = |text, effect| Card { title: CHEST_TITLE, text, effect };
    vec![
        card("Advance to GO (collect $200)", AdvanceTo(0)),
        card("Bank error in your favor — collect $200", Collect(200)),
        card("Doctor's fee — pay $50", Pay(50)),
        card("From sale of stock you get $50", Collect(50)),
        card("Get Out of Jail Free", GetOutFree),
        card("Go directly to Jail", GoToJail),
        card("Grand Opera Night — collect $50 from every player", CollectEach(50)),
        card("Holiday fund matures — collect $100", Collect(100)),
        card("Income tax refund — collect $20", Collect(20)),
        card("It's your birthday — collect $10 from each player", CollectEach(10)),
        card("Life insurance matures — collect $100", Collect(100)),
        card("Pay hospital fees of $100", Pay(100)),
        card("Pay school fees of $50", Pay(50)),
        card("Street repairs: $40/house, $115/hotel", Repairs { per_house: 40, per_hotel: 115 }),
        card("You inherit $100", Collect(100)),
    ]
}

impl Game {
    /// Draw the top card of a deck (recycling it to the bottom) and start its
    /// animation. The effect is applied once the animation settles.
    pub(super) fn draw_card(&mut self, deck: Deck) {
        let pile = match deck {
            Deck::Chance => &mut self.chance,
            Deck::Chest => &mut self.chest,
        };
        let Some(card) = pile.pop_front() else {
            return;
        };
        pile.push_back(card); // recycle to the bottom of the deck
        self.modal = Modal::Card(CardDraw { clip: Clip::new(), card });
        self.notify(format!("Player {} draws a card", self.current + 1), Level::Info);
    }

    /// Apply a drawn card's effect, then show its text — unless the effect ended
    /// the turn in jail or bankruptcy, or chained into another popup.
    pub(super) fn finish_card(&mut self, card: Card) {
        let who = self.current;
        self.apply_card(card.effect);
        if self.players[who].in_jail {
            self.can_roll = false;
        }
        if self.settle_if_bankrupt(who) {
            return;
        }
        if matches!(self.modal, Modal::None) {
            self.modal = Modal::Info(InfoBox {
                title: card.title.to_string(),
                lines: vec![card.text.to_string()],
            });
        }
    }

    /// Carry out a single card effect for the current player.
    fn apply_card(&mut self, effect: CardEffect) {
        let who = self.current;
        match effect {
            CardEffect::Collect(n) => {
                self.players[who].money += n;
                self.notify(format!("Player {} collected ${n}", who + 1), Level::Info);
            }
            CardEffect::Pay(n) => self.pay_bank(n),
            CardEffect::CollectEach(n) => {
                for i in 0..self.players.len() {
                    if i == who || self.players[i].bankrupt {
                        continue;
                    }
                    let paid = self.players[i].money.min(n);
                    self.players[i].money -= paid;
                    self.players[who].money += paid;
                }
                self.notify(format!("Player {} collected ${n} from each player", who + 1), Level::Info);
            }
            CardEffect::PayEach(n) => {
                let others: Vec<usize> = self.active_players().into_iter().filter(|&i| i != who).collect();
                let owed = n * others.len() as u32;
                if self.players[who].money >= owed {
                    self.players[who].money -= owed;
                    for i in others {
                        self.players[i].money += n;
                    }
                    self.notify(format!("Player {} paid ${n} to each player", who + 1), Level::Warn);
                } else {
                    // Hand out what's left, then go bankrupt to the bank.
                    let mut left = self.players[who].money;
                    for i in others {
                        let paid = left.min(n);
                        self.players[i].money += paid;
                        left -= paid;
                    }
                    self.players[who].money = left;
                    self.bankrupt(who, None);
                }
            }
            CardEffect::AdvanceTo(dest) => {
                let pos = self.players[who].position;
                self.advance(who, (dest + BOARD_LEN - pos) % BOARD_LEN);
            }
            CardEffect::Back(n) => {
                let new = (self.players[who].position + BOARD_LEN - n) % BOARD_LEN;
                self.players[who].position = new;
                let name = self.board[new].name().to_string();
                self.notify(format!("Player {} moved back to {name}", who + 1), Level::Info);
                self.resolve_landing(new, n);
            }
            CardEffect::GoToJail => self.send_to_jail(),
            CardEffect::GetOutFree => {
                self.players[who].get_out_free += 1;
                self.notify(format!("Player {} kept a Get Out of Jail Free card", who + 1), Level::Info);
            }
            CardEffect::Repairs { per_house, per_hotel } => {
                let mut houses = 0;
                let mut hotels = 0;
                for space in &self.board {
                    if space.owner() == Some(who) {
                        match space.houses() {
                            HOTEL => hotels += 1,
                            n => houses += n as u32,
                        }
                    }
                }
                let bill = houses * per_house + hotels * per_hotel;
                if bill > 0 {
                    self.pay_bank(bill);
                }
            }
        }
    }
}
