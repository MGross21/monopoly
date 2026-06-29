//! Chance and Community Chest card data. The effects are applied in
//! `game/mod.rs`; this file is just the decks and what each card does.
//!
//! Board indices referenced by `AdvanceTo` (see `board.rs`): GO = 0,
//! Reading RR = 5, St. Charles Place = 11, Illinois Ave = 24.

/// What drawing a card does to the current player.
#[derive(Clone, Copy)]
pub enum CardEffect {
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
pub struct Card {
    pub title: &'static str,
    pub text: &'static str,
    pub effect: CardEffect,
}

const CHANCE_TITLE: &str = " Chance ";
const CHEST_TITLE: &str = " Community Chest ";

/// The Chance deck, in printed order (the game shuffles it).
pub fn chance_deck() -> Vec<Card> {
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
pub fn chest_deck() -> Vec<Card> {
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
