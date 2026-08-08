//! Chance and Community Chest: the two decks, what each card does, and applying
//! a drawn card to the current player.
//!
//! Board indices referenced by `AdvanceTo` (see `board.rs`): GO = 0,
//! Reading RR = 5, St. Charles Place = 11, Illinois Ave = 24.

use std::collections::VecDeque;

use ratatui_notifications::Level;

use super::{BOARD_LEN, Game, HOTEL, InfoBox, Modal, Payee, Pending, RentRule};
use crate::space::Space;
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
    /// Advance to the next railroad and pay twice its usual rent.
    NearestRailroad,
    /// Advance to the next utility and pay ten times a fresh roll.
    NearestUtility,
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
        card("Take a walk on the Boardwalk", AdvanceTo(39)),
        card("Advance to the nearest Railroad — pay double rent", NearestRailroad),
        card("Advance to the nearest Railroad — pay double rent", NearestRailroad),
        card("Advance to the nearest Utility — pay 10x your roll", NearestUtility),
        card("Bank pays you a dividend of $50", Collect(50)),
        card("Get Out of Jail Free", GetOutFree),
        card("Go back 3 spaces", Back(3)),
        card("Go directly to Jail", GoToJail),
        card(
            "Make general repairs: $25/house, $100/hotel",
            Repairs { per_house: 25, per_hotel: 100 },
        ),
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
        card("Receive $25 consultancy fee", Collect(25)),
        card("Street repairs: $40/house, $115/hotel", Repairs { per_house: 40, per_hotel: 115 }),
        card("Second prize in a beauty contest — collect $10", Collect(10)),
        card("You inherit $100", Collect(100)),
    ]
}

impl Game {
    /// Steps forward from `who`'s square to the next space matching `wanted`.
    /// Counts from the *next* square, so "nearest" never means "stay put".
    fn steps_to(&self, who: usize, wanted: impl Fn(&Space) -> bool) -> usize {
        let pos = self.players[who].position;
        (1..=BOARD_LEN).find(|n| wanted(&self.board[(pos + n) % BOARD_LEN])).unwrap_or(0)
    }

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
                self.notify(
                    format!("Player {} collects ${n} from each player", who + 1),
                    Level::Info,
                );
                // Each payer settles in turn, liquidating if they must, so the
                // popups queue up rather than fighting over the screen.
                for i in self.active_players() {
                    if i != who {
                        self.queue(Pending::Charge {
                            who: i,
                            amount: n,
                            payee: Payee::Player(who),
                        });
                    }
                }
                self.run_pending();
            }
            CardEffect::PayEach(n) => {
                let others: Vec<usize> =
                    self.active_players().into_iter().filter(|&i| i != who).collect();
                let owed = n * others.len() as u32;
                self.charge(who, owed, Payee::Split(others));
            }
            CardEffect::AdvanceTo(dest) => {
                let pos = self.players[who].position;
                self.advance(who, (dest + BOARD_LEN - pos) % BOARD_LEN);
            }
            CardEffect::NearestRailroad => {
                let steps = self.steps_to(who, |s| matches!(s, Space::Railroad(_)));
                self.advance_under(who, steps, RentRule::DoubleRailroad);
            }
            CardEffect::NearestUtility => {
                let steps = self.steps_to(who, |s| matches!(s, Space::Utility(_)));
                let roll: usize = rand::random_range(1..=6) + rand::random_range(1..=6);
                self.notify(
                    format!("Player {} throws {roll} for the utility", who + 1),
                    Level::Info,
                );
                self.advance_under(who, steps, RentRule::TenTimesUtility(roll));
            }
            CardEffect::Back(n) => {
                let new = (self.players[who].position + BOARD_LEN - n) % BOARD_LEN;
                self.players[who].position = new;
                let name = self.board[new].name().to_string();
                self.notify(format!("Player {} moved back to {name}", who + 1), Level::Info);
                self.resolve_landing(new, n, RentRule::Normal);
            }
            CardEffect::GoToJail => self.send_to_jail(),
            CardEffect::GetOutFree => {
                self.players[who].get_out_free += 1;
                self.notify(
                    format!("Player {} kept a Get Out of Jail Free card", who + 1),
                    Level::Info,
                );
            }
            CardEffect::Repairs { per_house, per_hotel } => {
                let mut houses = 0;
                let mut hotels = 0;
                for space in self.estate(who) {
                    match space.houses() {
                        0 => {}
                        HOTEL => hotels += 1,
                        n => houses += u32::from(n),
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

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::*;
    use crate::game::testkit::*;
    use crate::space::ColorGroup;

    // --- the decks ----------------------------------------------------------

    #[test]
    fn both_decks_are_dealt_and_shuffled_whole() {
        let (chance, chest) = fresh_decks();
        assert_eq!(chance.len(), chance_deck().len());
        assert_eq!(chest.len(), chest_deck().len());
    }

    #[test]
    fn each_deck_holds_exactly_one_get_out_of_jail_free() {
        for deck in [chance_deck(), chest_deck()] {
            let count = deck.iter().filter(|c| matches!(c.effect, CardEffect::GetOutFree)).count();
            assert_eq!(count, 1);
        }
    }

    #[test]
    fn chance_carries_the_full_sixteen() {
        assert_eq!(chance_deck().len(), 16);
    }

    #[test]
    fn chance_has_two_nearest_railroad_cards_and_one_utility() {
        let deck = chance_deck();
        let count = |f: fn(&CardEffect) -> bool| deck.iter().filter(|c| f(&c.effect)).count();
        assert_eq!(count(|e| matches!(e, CardEffect::NearestRailroad)), 2);
        assert_eq!(count(|e| matches!(e, CardEffect::NearestUtility)), 1);
    }

    #[test]
    fn every_advance_target_is_on_the_board() {
        for deck in [chance_deck(), chest_deck()] {
            for card in deck {
                if let CardEffect::AdvanceTo(dest) = card.effect {
                    assert!(dest < BOARD_LEN, "{} points off the board", card.text);
                }
            }
        }
    }

    #[test]
    fn drawing_recycles_the_card_to_the_bottom() {
        let mut g = game(2, 1500);
        let top = g.chance.front().expect("a full deck").text;
        let size = g.chance.len();
        g.draw_card(Deck::Chance);
        assert_eq!(g.chance.len(), size, "the deck never shrinks");
        assert_eq!(g.chance.back().expect("a full deck").text, top);
        assert!(matches!(g.modal, Modal::Card(_)));
    }

    #[test]
    fn drawing_cycles_through_the_whole_deck_before_repeating() {
        let mut g = game(2, 1500);
        let size = g.chest.len();
        let first = g.chest.front().expect("a full deck").text;
        for _ in 0..size {
            g.draw_card(Deck::Chest);
        }
        assert_eq!(g.chest.front().expect("a full deck").text, first);
    }

    // --- effects ------------------------------------------------------------

    #[test]
    fn collect_credits_the_bank() {
        let mut g = game(2, 1500);
        g.apply_card(CardEffect::Collect(150));
        assert_eq!(g.players[0].money, 1650);
    }

    #[test]
    fn pay_debits_the_bank() {
        let mut g = game(2, 1500);
        g.apply_card(CardEffect::Pay(15));
        assert_eq!(g.players[0].money, 1485);
    }

    #[test]
    fn collect_each_takes_from_every_rival() {
        let mut g = game(3, 1500);
        g.apply_card(CardEffect::CollectEach(50));
        assert_eq!(g.players[0].money, 1600);
        assert_eq!(g.players[1].money, 1450);
        assert_eq!(g.players[2].money, 1450);
    }

    #[test]
    fn collect_each_skips_eliminated_players() {
        let mut g = game(3, 1500);
        g.players[2].bankrupt = true;
        g.apply_card(CardEffect::CollectEach(50));
        assert_eq!(g.players[0].money, 1550);
        assert_eq!(g.players[2].money, 1500);
    }

    #[test]
    fn collect_each_makes_a_short_payer_liquidate() {
        let mut g = game(3, 1500);
        g.players[1].money = 10;
        own(&mut g, MEDITERRANEAN, 1);
        own(&mut g, BALTIC, 1);
        g.apply_card(CardEffect::CollectEach(50));

        assert!(matches!(g.modal, Modal::Debt(_)), "player 2 must raise the cash");
        assert!(!g.players[1].bankrupt);
        assert_eq!(g.players[2].money, 1500, "player 3 waits their turn in the queue");
    }

    #[test]
    fn collect_each_resumes_after_the_liquidation_closes() {
        let mut g = game(3, 1500);
        g.players[1].money = 10;
        own(&mut g, MEDITERRANEAN, 1);
        own(&mut g, BALTIC, 1);
        g.apply_card(CardEffect::CollectEach(50));

        g.handle_key(KeyCode::Enter); // mortgage Mediterranean
        g.handle_key(KeyCode::Down);
        g.handle_key(KeyCode::Enter); // mortgage Baltic, clearing the debt

        assert!(matches!(g.modal, Modal::None), "the queue drained");
        assert_eq!(g.players[1].money, 20, "10 + 30 + 30 - 50");
        assert_eq!(g.players[2].money, 1450, "and player 3 paid once the screen cleared");
        assert_eq!(g.players[0].money, 1600, "the drawer collected from both");
    }

    #[test]
    fn pay_each_pays_every_rival() {
        let mut g = game(3, 1500);
        g.apply_card(CardEffect::PayEach(50));
        assert_eq!(g.players[0].money, 1400);
        assert_eq!(g.players[1].money, 1550);
        assert_eq!(g.players[2].money, 1550);
    }

    #[test]
    fn pay_each_beyond_your_means_opens_liquidation() {
        let mut g = game(3, 1500);
        g.players[0].money = 10;
        own(&mut g, MEDITERRANEAN, 0);
        own(&mut g, BALTIC, 0);
        g.apply_card(CardEffect::PayEach(25));
        assert!(matches!(g.modal, Modal::Debt(_)));
        assert!(!g.players[0].bankrupt);
    }

    #[test]
    fn advancing_forward_collects_go_on_the_way_round() {
        let mut g = game(2, 1500);
        place(&mut g, 0, GO_TO_JAIL);
        g.apply_card(CardEffect::AdvanceTo(0));
        assert_eq!(g.players[0].position, 0);
        assert_eq!(g.players[0].money, 1700);
    }

    #[test]
    fn advancing_without_wrapping_pays_nothing() {
        let mut g = game(2, 1500);
        place(&mut g, 0, CHANCE_LOW);
        g.apply_card(CardEffect::AdvanceTo(ILLINOIS));
        assert_eq!(g.players[0].position, ILLINOIS);
        assert_eq!(g.players[0].money, 1500);
    }

    #[test]
    fn advancing_onto_a_rival_property_charges_rent() {
        let mut g = game(2, 1500);
        own(&mut g, ILLINOIS, 1);
        place(&mut g, 0, CHANCE_LOW);
        g.apply_card(CardEffect::AdvanceTo(ILLINOIS));
        assert_eq!(g.players[0].money, 1480);
        assert_eq!(g.players[1].money, 1520);
    }

    #[test]
    fn the_nearest_railroad_is_the_next_one_forward() {
        let mut g = game(2, 1500);
        place(&mut g, 0, CHANCE_LOW);
        g.apply_card(CardEffect::NearestRailroad);
        assert_eq!(g.players[0].position, PENNSYLVANIA_RR);
    }

    #[test]
    fn the_nearest_railroad_wraps_past_go() {
        let mut g = game(2, 1500);
        place(&mut g, 0, 36); // the Chance between Short Line and Park Place
        g.apply_card(CardEffect::NearestRailroad);
        assert_eq!(g.players[0].position, READING_RR);
        assert_eq!(g.players[0].money, 1700, "and collects the salary on the way");
    }

    #[test]
    fn the_nearest_railroad_charges_double_rent() {
        let mut g = game(2, 1500);
        own(&mut g, PENNSYLVANIA_RR, 1);
        place(&mut g, 0, CHANCE_LOW);
        g.apply_card(CardEffect::NearestRailroad);
        assert_eq!(g.players[0].money, 1450, "2 x the $25 one-railroad rate");
        assert_eq!(g.players[1].money, 1550);
    }

    #[test]
    fn an_unowned_nearest_railroad_is_offered_for_sale() {
        let mut g = game(2, 1500);
        place(&mut g, 0, CHANCE_LOW);
        g.apply_card(CardEffect::NearestRailroad);
        assert!(matches!(g.modal, Modal::Buy { pos, .. } if pos == PENNSYLVANIA_RR));
    }

    #[test]
    fn the_nearest_utility_is_the_next_one_forward() {
        let mut g = game(2, 1500);
        place(&mut g, 0, CHANCE_LOW);
        g.apply_card(CardEffect::NearestUtility);
        assert_eq!(g.players[0].position, ELECTRIC_CO);
    }

    #[test]
    fn the_nearest_utility_charges_ten_times_a_fresh_roll() {
        let mut g = game(2, 1500);
        own(&mut g, ELECTRIC_CO, 1);
        place(&mut g, 0, CHANCE_LOW);
        g.apply_card(CardEffect::NearestUtility);

        // The throw is random, so pin the range: 10 x (2..=12), never the
        // 4x-the-distance rate a normal landing would have charged.
        let paid = 1500 - g.players[0].money;
        assert!((20..=120).contains(&paid), "paid {paid}");
        assert_eq!(paid % 10, 0);
        assert_eq!(g.players[1].money, 1500 + paid);
    }

    #[test]
    fn a_single_utility_still_bills_ten_times_under_the_card() {
        let mut g = game(2, 1500);
        own(&mut g, ELECTRIC_CO, 1);
        // A normal landing with one utility owned would be 4x, not 10x.
        assert_eq!(g.rent(ELECTRIC_CO, 1, 6, RentRule::Normal), 24);
        assert_eq!(g.rent(ELECTRIC_CO, 1, 6, RentRule::TenTimesUtility(6)), 60);
    }

    #[test]
    fn going_back_never_collects_go() {
        let mut g = game(2, 1500);
        place(&mut g, 0, 1);
        g.apply_card(CardEffect::Back(3));
        assert_eq!(g.players[0].position, LUXURY_TAX);
        assert_eq!(g.players[0].money, 1400, "the tax it landed on, and no GO salary");
    }

    #[test]
    fn going_back_resolves_the_new_square() {
        let mut g = game(2, 1500);
        place(&mut g, 0, CHANCE_LOW);
        g.apply_card(CardEffect::Back(3));
        assert_eq!(g.players[0].position, INCOME_TAX);
        assert_eq!(g.players[0].money, 1300, "and pays the tax it lands on");
    }

    #[test]
    fn the_jail_card_sends_you_straight_there() {
        let mut g = game(2, 1500);
        place(&mut g, 0, ILLINOIS);
        g.apply_card(CardEffect::GoToJail);
        assert!(g.players[0].in_jail);
        assert_eq!(g.players[0].position, JAIL);
        assert_eq!(g.players[0].money, 1500);
    }

    #[test]
    fn get_out_free_cards_accumulate() {
        let mut g = game(2, 1500);
        g.apply_card(CardEffect::GetOutFree);
        g.apply_card(CardEffect::GetOutFree);
        assert_eq!(g.players[0].get_out_free, 2);
    }

    #[test]
    fn repairs_bill_per_house_and_per_hotel() {
        let mut g = game(2, 1500);
        own_group(&mut g, ColorGroup::Brown, 0);
        set_houses(&mut g, MEDITERRANEAN, 3);
        set_houses(&mut g, BALTIC, HOTEL);
        g.apply_card(CardEffect::Repairs { per_house: 25, per_hotel: 100 });
        assert_eq!(g.players[0].money, 1325, "3 houses at 25, one hotel at 100");
    }

    #[test]
    fn repairs_cost_nothing_when_you_have_built_nothing() {
        let mut g = game(2, 1500);
        own_group(&mut g, ColorGroup::Brown, 0);
        g.apply_card(CardEffect::Repairs { per_house: 25, per_hotel: 100 });
        assert_eq!(g.players[0].money, 1500);
    }

    // --- presentation -------------------------------------------------------

    #[test]
    fn a_resolved_card_shows_its_text() {
        let mut g = game(2, 1500);
        let card = Card {
            title: CHANCE_TITLE,
            text: "Bank pays you a dividend of $50",
            effect: CardEffect::Collect(50),
        };
        g.finish_card(card);
        match &g.modal {
            Modal::Info(info) => {
                assert_eq!(info.lines, vec!["Bank pays you a dividend of $50".to_string()]);
                assert_eq!(info.title, CHANCE_TITLE);
            }
            _ => panic!("expected the card text"),
        }
    }

    #[test]
    fn a_card_that_chains_into_a_popup_skips_its_text() {
        let mut g = game(2, 1500);
        place(&mut g, 0, CHANCE_LOW);
        let card = Card {
            title: CHANCE_TITLE,
            text: "Advance to Illinois Ave",
            effect: CardEffect::AdvanceTo(ILLINOIS),
        };
        g.finish_card(card);
        assert!(matches!(g.modal, Modal::Buy { .. }), "the buy prompt wins");
    }

    #[test]
    fn a_card_that_jails_you_ends_the_turn() {
        let mut g = game(2, 1500);
        let card =
            Card { title: CHANCE_TITLE, text: "Go directly to Jail", effect: CardEffect::GoToJail };
        g.finish_card(card);
        assert!(!g.can_roll);
    }

    #[test]
    fn landing_on_chance_draws_a_card() {
        let mut g = game(2, 1500);
        place(&mut g, 0, CHANCE_LOW - 2);
        g.apply_roll(1, 1);
        assert!(matches!(g.modal, Modal::Card(_)));
    }
}
