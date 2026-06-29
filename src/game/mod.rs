//! Game state and rules: turns, movement, buying, rent, and the per-turn menu.
//! See GAME_RULES.md for the rules this implements.

use std::time::Duration;

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    style::{Color, Style},
    text::Line,
    widgets::{BorderType, Padding},
};
use ratatui_notifications::{
    Anchor, Animation as ToastAnimation, AutoDismiss, Level, Notification, Notifications,
    SizeConstraint,
};

mod action;
mod cards;

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use cards::{Card, CardEffect, chance_deck, chest_deck};

use crate::board::board;
use crate::player::Player;
use crate::space::{ColorGroup, Space};
use crate::ui::dice::{self, Clip, Roll};
use crate::ui::map::Overlay;
use crate::ui::{Confirm, ConfirmResult, Cursor, choice_popup};
use action::{ActionMenu, TurnAction, action_for_hotkey};

const GO_SALARY: u32 = 200;
const JAIL_INDEX: usize = 10;
const JAIL_BAIL: u32 = 50;
const RAILROAD_BASE_RENT: u32 = 25;

/// Where a game is saved to / loaded from: the platform per-user data dir, e.g.
/// `~/.local/share/monopoly/save.json` on Linux. `None` if no such dir exists.
fn save_path() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|dir| dir.join("monopoly").join("save.json"))
}

/// The persistent slice of a game. Transient UI state (the active popup,
/// notifications, the clock) and the card decks are not saved — decks are
/// reshuffled on load since their text is `&'static`.
#[derive(Serialize, Deserialize)]
struct Save {
    players: Vec<Player>,
    board: Vec<Space>,
    current: usize,
    doubles: u8,
    can_roll: bool,
    has_rolled: bool,
}

pub struct Game {
    pub players: Vec<Player>,
    pub board: Vec<Space>,
    pub current: usize,
    modal: Modal,
    notes: Notifications,
    doubles: u8,
    can_roll: bool,
    has_rolled: bool, // rolled at least once this turn
    clock: Duration,  // drives the breathing highlight
    done: bool,       // game over acknowledged; main returns to the menu
    chance: VecDeque<Card>,
    chest: VecDeque<Card>,
}

/// The single popup the game is currently showing, if any. Holding these in one
/// enum makes the precedence between them explicit (one is active at a time) and
/// keeps `handle_key`/`render` in lockstep.
enum Modal {
    None,
    Roll(Roll),
    Card(CardDraw),
    Menu(ActionMenu),
    ConfirmEnd(Confirm),
    Info(InfoBox),
    Jail(JailMenu),
    Estate(EstateMenu),
    Buy { confirm: Confirm, pos: usize },
    Auction(Auction),
    Trade(Trade),
    GameOver(usize), // winning player index
}

const TRADE_STEP: u32 = 10;

/// Which step of building a trade we're on.
#[derive(Clone, Copy, PartialEq)]
enum TradeStage {
    Partner,  // choosing who to trade with
    Property, // choosing which property changes hands
    Price,    // setting the cash that moves the other way
}

/// A one-property-for-cash trade between the current player and a partner. The
/// property's current owner sells; the other side buys for `price`.
struct Trade {
    stage: TradeStage,
    partners: Vec<usize>,
    pcursor: Cursor,
    partner: usize,
    props: Vec<usize>,
    prop_cursor: Cursor,
    price: u32,
}

const AUCTION_STEP: u32 = 10;

/// A bank auction for one property: players still in `active` take turns to bid
/// up by `AUCTION_STEP` or pass; the last standing high bidder wins.
struct Auction {
    pos: usize,
    active: Vec<usize>, // players still bidding, in seating order
    turn: usize,        // index into `active`
    high_bid: u32,
    high_bidder: Option<usize>,
}

/// Whether the estate popup is mortgaging or building.
#[derive(Clone, Copy)]
enum EstateMode {
    Mortgage,
    Build,
}

/// A list of the current player's properties for mortgaging or building. The
/// board indices in `slots` are fixed for the popup's lifetime; their labels are
/// recomputed from live board state each frame.
struct EstateMenu {
    mode: EstateMode,
    slots: Vec<usize>,
    cursor: Cursor,
}

impl EstateMenu {
    fn new(mode: EstateMode, slots: Vec<usize>) -> Self {
        let cursor = Cursor::new(slots.len());
        Self { mode, slots, cursor }
    }

    fn selected(&self) -> usize {
        self.slots[self.cursor.selected]
    }
}

/// The choices offered to a jailed player at the start of their turn. `Pay` and
/// `Card` only appear when available; `Roll` is always present.
#[derive(Clone, Copy)]
enum JailChoice {
    Pay,
    Roll,
    Card,
}

/// The in-jail action picker.
struct JailMenu {
    choices: Vec<JailChoice>,
    cursor: Cursor,
}

impl JailMenu {
    fn new(can_pay: bool, has_card: bool) -> Self {
        let mut choices = Vec::new();
        if can_pay {
            choices.push(JailChoice::Pay);
        }
        choices.push(JailChoice::Roll);
        if has_card {
            choices.push(JailChoice::Card);
        }
        let cursor = Cursor::new(choices.len());
        Self { choices, cursor }
    }

    fn labels(&self) -> Vec<&'static str> {
        self.choices
            .iter()
            .map(|c| match c {
                JailChoice::Pay => "Pay $50 bail",
                JailChoice::Roll => "Roll for doubles",
                JailChoice::Card => "Use Get Out of Jail Free",
            })
            .collect()
    }

    fn selected(&self) -> JailChoice {
        self.choices[self.cursor.selected]
    }
}

/// A Chance / Community Chest card animation playing in the center. The drawn
/// `card`'s effect is applied once the animation settles (or is skipped).
struct CardDraw {
    clip: Clip,
    card: Card,
}

/// A centered info popup (e.g. owned-property list) dismissed with any key.
struct InfoBox {
    title: String,
    lines: Vec<String>,
}

impl Game {
    pub fn new(players: Vec<Player>) -> Self {
        // Turn order: every player rolls two dice, highest total goes first
        // (ties broken by the earlier player).
        let mut first = 0;
        let mut best = 0u8;
        for i in 0..players.len() {
            let total = rand::random_range(1..=6) + rand::random_range(1..=6);
            if total > best {
                best = total;
                first = i;
            }
        }
        let mut game = Self {
            players,
            board: board(),
            current: first,
            modal: Modal::None,
            // Cap how many toasts stack at once so a single turn's events don't
            // pile up and flicker.
            notes: Notifications::new().max_concurrent(Some(4)),
            doubles: 0,
            can_roll: true,
            has_rolled: false,
            clock: Duration::ZERO,
            done: false,
            chance: shuffled(chance_deck()),
            chest: shuffled(chest_deck()),
        };
        game.notify(format!("Player {} wins the roll-off", first + 1), Level::Info);
        game.notify(format!("Player {}'s turn", first + 1), Level::Info);
        game
    }

    /// True once the winner has been shown and the player dismissed it; `main`
    /// reads this to return to the menu.
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Build a game from saved state, with fresh transient UI and reshuffled
    /// decks.
    fn from_save(save: Save) -> Self {
        Self {
            players: save.players,
            board: save.board,
            current: save.current,
            modal: Modal::None,
            notes: Notifications::new().max_concurrent(Some(4)),
            doubles: save.doubles,
            can_roll: save.can_roll,
            has_rolled: save.has_rolled,
            clock: Duration::ZERO,
            done: false,
            chance: shuffled(chance_deck()),
            chest: shuffled(chest_deck()),
        }
    }

    /// Load the saved game, or `None` if there's no save or it can't be read.
    pub fn load() -> Option<Self> {
        let data = std::fs::read_to_string(save_path()?).ok()?;
        let save: Save = serde_json::from_str(&data).ok()?;
        Some(Self::from_save(save))
    }

    /// Write the persistent state to the save path, creating the directory if
    /// needed, and toast success or failure.
    fn save_game(&mut self) {
        let save = Save {
            players: self.players.clone(),
            board: self.board.clone(),
            current: self.current,
            doubles: self.doubles,
            can_roll: self.can_roll,
            has_rolled: self.has_rolled,
        };
        let result = (|| -> Result<std::path::PathBuf, String> {
            let path = save_path().ok_or("no data directory")?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let json = serde_json::to_string_pretty(&save).map_err(|e| e.to_string())?;
            std::fs::write(&path, json).map_err(|e| e.to_string())?;
            Ok(path)
        })();
        match result {
            Ok(path) => self.notify(format!("Game saved to {}", path.display()), Level::Info),
            Err(e) => self.notify(format!("Save failed: {e}"), Level::Error),
        }
    }

    pub fn overlay(&self) -> Overlay {
        Overlay::Board {
            turn: self.current,
            breath: self.breath(),
        }
    }

    /// Breathing brightness 0..1, a slow sine over the game clock.
    fn breath(&self) -> f32 {
        let t = self.clock.as_secs_f32() * 2.5; // radians/sec
        (t.sin() + 1.0) / 2.0
    }

    /// Advance time: clock, notifications, and any playing animation.
    pub fn tick(&mut self, delta: Duration) {
        self.clock += delta;
        self.notes.tick(delta);

        // Advance a roll; apply its result once the dice settle.
        let mut finished_roll = None;
        if let Modal::Roll(roll) = &mut self.modal {
            let was_animating = roll.animating();
            roll.tick(dice::animation(), delta);
            if was_animating && !roll.animating() {
                finished_roll = roll.result();
            }
        }
        if let Some((a, b)) = finished_roll {
            self.apply_roll(a, b);
        }

        // Advance a card draw; apply its effect once the animation settles.
        let mut finished_card = None;
        if let Modal::Card(card) = &mut self.modal {
            card.clip.tick(dice::card_animation(), delta);
            if card.clip.finished(dice::card_animation()) {
                finished_card = Some(card.card);
            }
        }
        if let Some(card) = finished_card {
            self.modal = Modal::None;
            self.finish_card(card);
        }
    }

    // --- input ---------------------------------------------------------------

    pub fn handle_key(&mut self, key: KeyCode) {
        match &mut self.modal {
            // The game is over; any key returns to the menu (via `main`).
            Modal::GameOver(_) => self.done = true,

            // An info popup blocks everything; any key dismisses it.
            Modal::Info(_) => self.modal = Modal::None,

            // End-turn confirmation takes priority.
            Modal::ConfirmEnd(confirm) => match confirm.handle_key(key) {
                ConfirmResult::Pending => {}
                ConfirmResult::Yes => {
                    self.modal = Modal::None;
                    self.end_turn();
                }
                ConfirmResult::No => self.modal = Modal::None,
            },

            Modal::Menu(menu) => match key {
                KeyCode::Up => menu.prev(),
                KeyCode::Down => menu.next(),
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Enter => {
                    let action = menu.selected();
                    self.modal = Modal::None;
                    self.run(action);
                }
                _ => {}
            },

            Modal::Jail(menu) => match key {
                KeyCode::Up => menu.cursor.up(),
                KeyCode::Down => menu.cursor.down(),
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Enter => {
                    let choice = menu.selected();
                    self.resolve_jail_choice(choice);
                }
                _ => {}
            },

            Modal::Estate(menu) => match key {
                KeyCode::Up => menu.cursor.up(),
                KeyCode::Down => menu.cursor.down(),
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Enter => {
                    let idx = menu.selected();
                    match menu.mode {
                        EstateMode::Mortgage => self.toggle_mortgage(idx),
                        EstateMode::Build => self.build_house(idx),
                    }
                }
                // Sell a house back (build mode only).
                KeyCode::Char('s') => {
                    let idx = menu.selected();
                    if matches!(menu.mode, EstateMode::Build) {
                        self.sell_house(idx);
                    }
                }
                _ => {}
            },

            // Buy-or-auction prompt for the property just landed on.
            Modal::Buy { confirm, pos } => {
                let pos = *pos;
                match confirm.handle_key(key) {
                    ConfirmResult::Pending => {}
                    ConfirmResult::Yes => {
                        self.modal = Modal::None;
                        self.buy_current();
                    }
                    ConfirmResult::No => {
                        self.modal = Modal::None;
                        self.start_auction(pos);
                    }
                }
            }

            // Auction bidding. Take the auction out so the bid/pass handlers get
            // full access to `self`; put it back unless the auction has ended.
            Modal::Auction(_) => {
                if let Modal::Auction(mut auc) = std::mem::replace(&mut self.modal, Modal::None)
                    && self.auction_input(&mut auc, key)
                {
                    self.modal = Modal::Auction(auc);
                }
            }

            // Trade builder; same take-out-and-replace pattern as the auction.
            Modal::Trade(_) => {
                if let Modal::Trade(mut t) = std::mem::replace(&mut self.modal, Modal::None)
                    && self.trade_input(&mut t, key)
                {
                    self.modal = Modal::Trade(t);
                }
            }

            // A card animation blocks input; Space/Enter skips straight to the
            // card's effect.
            Modal::Card(card) => {
                if matches!(key, KeyCode::Char(' ') | KeyCode::Enter) {
                    let drawn = card.card;
                    self.modal = Modal::None;
                    self.finish_card(drawn);
                }
            }

            // Dismiss the dice popup once the animation is done.
            Modal::Roll(roll) => {
                if matches!(key, KeyCode::Char(' ') | KeyCode::Enter) && !roll.animating() {
                    self.modal = Modal::None;
                }
            }

            Modal::None => match key {
                KeyCode::Char('m') | KeyCode::Enter => self.modal = Modal::Menu(ActionMenu::new()),
                KeyCode::Char(' ') => self.start_roll(),
                // Action hotkeys work outside the menu for faster turns.
                KeyCode::Char(c) => {
                    if let Some(action) = action_for_hotkey(c) {
                        self.run(action);
                    }
                }
                _ => {}
            },
        }
    }

    fn run(&mut self, action: TurnAction) {
        match action {
            TurnAction::RollDice => self.start_roll(),
            TurnAction::BuyProperty => self.buy_current(),
            TurnAction::EndTurn => {
                if self.has_rolled {
                    self.modal = Modal::ConfirmEnd(Confirm::new());
                } else {
                    self.notify("Roll the dice before ending your turn", Level::Warn);
                }
            }
            TurnAction::BuildHouses => self.open_build(),
            TurnAction::ViewInventory => self.show_inventory(),
            TurnAction::Trade => self.open_trade(),
            TurnAction::Mortgages => self.open_mortgages(),
            TurnAction::SaveGame => self.save_game(),
        }
    }

    // --- actions -------------------------------------------------------------

    fn start_roll(&mut self) {
        if matches!(self.modal, Modal::Roll(_)) {
            return; // already rolling
        }
        if !self.can_roll {
            self.notify("You already rolled — end your turn", Level::Warn);
            return;
        }
        // A jailed player chooses how to get out before any roll.
        if self.players[self.current].in_jail {
            let p = &self.players[self.current];
            self.modal = Modal::Jail(JailMenu::new(p.money >= JAIL_BAIL, p.get_out_free > 0));
            return;
        }
        self.modal = Modal::Roll(Roll::new());
    }

    /// Move `who` forward `steps`, paying GO salary on a pass and resolving the
    /// landing. Shared by normal and out-of-jail moves.
    fn advance(&mut self, who: usize, steps: usize) {
        let old = self.players[who].position;
        let passed_go = old + steps >= 40;
        let new = (old + steps) % 40;
        self.players[who].position = new;
        if passed_go {
            self.players[who].money += GO_SALARY;
            self.notify(format!("Player {} passed GO (+${GO_SALARY})", who + 1), Level::Info);
        }
        let name = self.board[new].name().to_string();
        self.notify(format!("Player {} landed on {name}", who + 1), Level::Info);
        self.resolve_landing(new, steps);
    }

    /// Apply a finished dice roll: move, pass GO, resolve the landing, doubles.
    fn apply_roll(&mut self, a: u8, b: u8) {
        let total = (a + b) as usize;
        let who = self.current;
        self.has_rolled = true;
        self.notify(format!("Player {} rolled {a} + {b} = {}", who + 1, a + b), Level::Info);

        // A jailed player's roll only tries for doubles to escape.
        if self.players[who].in_jail {
            self.apply_jail_roll(who, a, b, total);
            return;
        }

        self.advance(who, total);

        // If paying rent/tax wiped the player out, the turn is over. `bankrupt`
        // has already either ended the game (Modal::GameOver) or there are still
        // players left, in which case play passes on.
        if self.players[who].bankrupt {
            if !matches!(self.modal, Modal::GameOver(_)) {
                self.end_turn();
            }
            return;
        }

        // Landing on "Go To Jail" ends the turn now — no bonus roll even if the
        // move that put them there was doubles.
        if self.players[who].in_jail {
            self.can_roll = false;
            return;
        }

        // Doubles earn another roll; non-doubles end the right to roll. Three
        // doubles in a row sends you to Jail and ends the turn.
        if a == b {
            self.doubles += 1;
            if self.doubles >= 3 {
                self.send_to_jail();
                self.can_roll = false;
            } else {
                self.can_roll = true;
                self.notify(format!("Doubles! Player {} rolls again", who + 1), Level::Info);
            }
        } else {
            self.can_roll = false;
        }
        // If landing started a card draw, `resolve_landing` already swapped the
        // modal from the dice popup to the card; nothing more to do here.
    }

    /// React to the space the current player landed on.
    fn resolve_landing(&mut self, pos: usize, total: usize) {
        // Clone the space so we can borrow `self` mutably below.
        match self.board[pos].clone() {
            Space::Tax(amount) => self.pay_bank(amount),
            Space::GoToJail => self.send_to_jail(),
            Space::Chance => self.draw_card(Deck::Chance),
            Space::CommunityChest => self.draw_card(Deck::Chest),
            space if space.is_ownable() => match space.owner() {
                None => {
                    // Offer to buy; declining sends the property to auction.
                    let price = space.price().unwrap_or(0);
                    self.notify(format!("{} is for sale (${price})", space.name()), Level::Info);
                    self.modal = Modal::Buy { confirm: Confirm::new(), pos };
                }
                Some(owner) if owner != self.current => {
                    let rent = self.rent(pos, owner, total);
                    self.pay_player(owner, rent);
                }
                Some(_) => {} // your own property
            },
            _ => {}
        }
    }

    /// Draw the top card of a deck (recycling it to the bottom) and start its
    /// animation. The effect is applied once the animation settles.
    fn draw_card(&mut self, deck: Deck) {
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

    /// Apply a card's effect, then either show its text or hand off the turn if
    /// it ended in jail or bankruptcy.
    fn finish_card(&mut self, card: Card) {
        let who = self.current;
        self.apply_card(card.effect);
        // A card can jail or bankrupt the drawer; settle the turn accordingly.
        if self.players[who].in_jail {
            self.can_roll = false;
        }
        if self.players[who].bankrupt {
            if !matches!(self.modal, Modal::GameOver(_)) {
                self.end_turn();
            }
            return;
        }
        // Show the card text unless the effect already opened something (e.g. a
        // chained card draw from advancing onto another Chance space).
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
                let others: Vec<usize> =
                    (0..self.players.len()).filter(|&i| i != who && !self.players[i].bankrupt).collect();
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
                let steps = (dest + 40 - pos) % 40;
                self.advance(who, steps);
            }
            CardEffect::Back(n) => {
                let pos = self.players[who].position;
                let new = (pos + 40 - n) % 40;
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
                            5 => hotels += 1,
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

    fn buy_current(&mut self) {
        let pos = self.players[self.current].position;
        let space = &self.board[pos];
        if !space.is_ownable() {
            self.notify("Nothing to buy here", Level::Warn);
            return;
        }
        if space.owner().is_some() {
            self.notify("That property is already owned", Level::Warn);
            return;
        }
        let price = space.price().unwrap_or(0);
        if self.players[self.current].money < price {
            self.notify("Not enough money to buy this", Level::Error);
            return;
        }
        self.players[self.current].money -= price;
        self.board[pos].set_owner(Some(self.current));
        let name = self.board[pos].name().to_string();
        self.notify(format!("Player {} bought {name} for ${price}", self.current + 1), Level::Info);
    }

    fn end_turn(&mut self) {
        self.doubles = 0;
        self.can_roll = true;
        self.has_rolled = false;
        // Skip anyone already eliminated. At least one player is still in (else
        // the game is over), so this loop always terminates.
        loop {
            self.current = (self.current + 1) % self.players.len();
            if !self.players[self.current].bankrupt {
                break;
            }
        }
        self.notify(format!("Player {}'s turn", self.current + 1), Level::Info);
    }

    fn show_inventory(&mut self) {
        let me = self.current;
        let lines: Vec<String> = self
            .board
            .iter()
            .filter(|s| s.owner() == Some(me))
            .map(|s| match s.price() {
                Some(price) => format!("{}  (${price})", s.name()),
                None => s.name().to_string(),
            })
            .collect();
        self.modal = Modal::Info(InfoBox {
            title: format!(" Player {} — ${} ", me + 1, self.players[me].money),
            lines,
        });
    }

    // --- money & helpers -----------------------------------------------------

    fn send_to_jail(&mut self) {
        let who = self.current;
        self.players[who].position = JAIL_INDEX;
        self.players[who].in_jail = true;
        self.players[who].jail_turns = 0;
        self.doubles = 0;
        self.notify(format!("Player {} was sent to Jail", who + 1), Level::Warn);
    }

    /// Act on the jailed player's menu choice. Pay/Card free them and roll
    /// normally; Roll just rolls (jail semantics handled in `apply_jail_roll`).
    fn resolve_jail_choice(&mut self, choice: JailChoice) {
        let who = self.current;
        match choice {
            JailChoice::Pay => {
                self.players[who].money -= JAIL_BAIL; // offered only when affordable
                self.players[who].in_jail = false;
                self.players[who].jail_turns = 0;
                self.notify(format!("Player {} paid ${JAIL_BAIL} bail", who + 1), Level::Warn);
            }
            JailChoice::Card => {
                self.players[who].get_out_free -= 1;
                self.players[who].in_jail = false;
                self.players[who].jail_turns = 0;
                self.notify(format!("Player {} used a Get Out of Jail Free card", who + 1), Level::Info);
            }
            JailChoice::Roll => {}
        }
        self.modal = Modal::Roll(Roll::new());
    }

    /// Resolve a roll made from jail: doubles escape and move; otherwise count a
    /// failed attempt, and on the third pay the $50 bail and move regardless.
    fn apply_jail_roll(&mut self, who: usize, a: u8, b: u8, total: usize) {
        if a == b {
            self.players[who].in_jail = false;
            self.players[who].jail_turns = 0;
            self.notify(format!("Doubles! Player {} leaves Jail", who + 1), Level::Info);
            self.advance(who, total);
        } else {
            self.players[who].jail_turns += 1;
            let n = self.players[who].jail_turns;
            if n >= 3 {
                self.players[who].in_jail = false;
                self.players[who].jail_turns = 0;
                self.notify(format!("Player {} failed three times — pays ${JAIL_BAIL} bail", who + 1), Level::Warn);
                if self.players[who].money >= JAIL_BAIL {
                    self.players[who].money -= JAIL_BAIL;
                    self.advance(who, total);
                } else {
                    self.pay_bank(JAIL_BAIL); // can't cover bail — bankrupt
                }
            } else {
                self.notify(format!("Player {} failed to roll doubles ({n}/3)", who + 1), Level::Warn);
            }
        }
        // Leaving jail never grants a bonus roll; the turn ends after this.
        self.can_roll = false;
        // A landing or the forced bail may have bankrupted the player.
        if self.players[who].bankrupt && !matches!(self.modal, Modal::GameOver(_)) {
            self.end_turn();
        }
    }

    fn pay_bank(&mut self, amount: u32) {
        let who = self.current;
        if self.players[who].money >= amount {
            self.players[who].money -= amount;
            self.notify(format!("Player {} paid ${amount} in tax", who + 1), Level::Warn);
        } else {
            let paid = self.players[who].money;
            self.players[who].money = 0;
            self.notify(
                format!("Player {} owed ${amount} but had ${paid} — bankrupt", who + 1),
                Level::Error,
            );
            self.bankrupt(who, None);
        }
    }

    /// Pay rent from the current player to `owner`. If the player can't cover it,
    /// they hand over everything they have and go bankrupt to `owner`.
    fn pay_player(&mut self, owner: usize, rent: u32) {
        let who = self.current;
        if self.players[who].money >= rent {
            self.players[who].money -= rent;
            self.players[owner].money += rent;
            self.notify(format!("Player {} paid ${rent} rent to Player {}", who + 1, owner + 1), Level::Warn);
        } else {
            let paid = self.players[who].money;
            self.players[who].money = 0;
            self.players[owner].money += paid;
            self.notify(
                format!("Player {} paid ${paid} of ${rent} rent then went bankrupt to Player {}", who + 1, owner + 1),
                Level::Error,
            );
            self.bankrupt(who, Some(owner));
        }
    }

    /// Eliminate `who`: hand their estate to `creditor` (a player) or back to the
    /// bank, then check whether only one player remains.
    fn bankrupt(&mut self, who: usize, creditor: Option<usize>) {
        self.players[who].bankrupt = true;
        for space in &mut self.board {
            if space.owner() == Some(who) {
                space.set_owner(creditor);
                if creditor.is_none() {
                    space.reset_buildings(); // back to the bank's stock
                }
            }
        }
        self.notify(format!("Player {} is out of the game", who + 1), Level::Error);
        self.check_win();
    }

    /// If only one player is left standing, end the game.
    fn check_win(&mut self) {
        let mut alive = (0..self.players.len()).filter(|&i| !self.players[i].bankrupt);
        if let (Some(winner), None) = (alive.next(), alive.next()) {
            self.notify(format!("Player {} wins!", winner + 1), Level::Info);
            self.modal = Modal::GameOver(winner);
        }
    }

    /// Rent owed for the space at `pos`, owned by `owner`. A mortgaged space
    /// collects nothing.
    fn rent(&self, pos: usize, owner: usize, total: usize) -> u32 {
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
            Space::Railroad(o) if o.mortgaged => 0,
            Space::Railroad(_) => RAILROAD_BASE_RENT * self.count_kind(owner, Kind::Railroad),
            Space::Utility(o) if o.mortgaged => 0,
            Space::Utility(_) => {
                let multiplier = if self.count_kind(owner, Kind::Utility) == 2 { 10 } else { 4 };
                total as u32 * multiplier
            }
            _ => 0,
        }
    }

    /// Does `owner` hold every street in `group`? (Required to build or to earn
    /// doubled rent.)
    fn owns_full_group(&self, owner: usize, group: ColorGroup) -> bool {
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

    // --- auctions ------------------------------------------------------------

    /// Put `pos` up for auction among all players still in the game.
    fn start_auction(&mut self, pos: usize) {
        let active: Vec<usize> =
            (0..self.players.len()).filter(|&i| !self.players[i].bankrupt).collect();
        if active.is_empty() {
            return;
        }
        let name = self.board[pos].name().to_string();
        self.notify(format!("{name} goes to auction"), Level::Info);
        self.modal = Modal::Auction(Auction { pos, active, turn: 0, high_bid: 0, high_bidder: None });
    }

    /// Handle one auction key press. Returns `true` to keep the auction open.
    fn auction_input(&mut self, auc: &mut Auction, key: KeyCode) -> bool {
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
        let next = auc.high_bid + AUCTION_STEP;
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
        // If only the high bidder is left, they win.
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

    // --- trade ---------------------------------------------------------------

    /// Open the trade builder with the current player. Needs another solvent
    /// player to trade with.
    fn open_trade(&mut self) {
        let me = self.current;
        let partners: Vec<usize> =
            (0..self.players.len()).filter(|&i| i != me && !self.players[i].bankrupt).collect();
        if partners.is_empty() {
            self.notify("No one to trade with", Level::Warn);
            return;
        }
        let pcursor = Cursor::new(partners.len());
        let partner = partners[0];
        self.modal = Modal::Trade(Trade {
            stage: TradeStage::Partner,
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
    fn trade_input(&mut self, t: &mut Trade, key: KeyCode) -> bool {
        match t.stage {
            TradeStage::Partner => match key {
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
                    t.stage = TradeStage::Property;
                }
                _ => {}
            },
            TradeStage::Property => match key {
                KeyCode::Up => t.prop_cursor.up(),
                KeyCode::Down => t.prop_cursor.down(),
                KeyCode::Esc => t.stage = TradeStage::Partner,
                KeyCode::Enter => {
                    let idx = t.props[t.prop_cursor.selected];
                    t.price = self.board[idx].price().unwrap_or(0); // sensible default
                    t.stage = TradeStage::Price;
                }
                _ => {}
            },
            TradeStage::Price => match key {
                KeyCode::Left | KeyCode::Down => t.price = t.price.saturating_sub(TRADE_STEP),
                KeyCode::Right | KeyCode::Up => t.price += TRADE_STEP,
                KeyCode::Esc => t.stage = TradeStage::Property,
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
        // Whoever owns it sells; the other side buys for the cash.
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

    // --- mortgages & building ------------------------------------------------

    /// Open the mortgage list for the current player's holdings.
    fn open_mortgages(&mut self) {
        let me = self.current;
        let slots: Vec<usize> = (0..self.board.len())
            .filter(|&i| self.board[i].owner() == Some(me))
            .collect();
        if slots.is_empty() {
            self.notify("You don't own anything to mortgage", Level::Warn);
            return;
        }
        self.modal = Modal::Estate(EstateMenu::new(EstateMode::Mortgage, slots));
    }

    /// Open the build list: streets in a fully-owned color group.
    fn open_build(&mut self) {
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
        self.modal = Modal::Estate(EstateMenu::new(EstateMode::Build, slots));
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
        if houses >= 5 {
            self.notify("Already has a hotel", Level::Warn);
            return;
        }
        let (min, _) = self.group_house_bounds(group);
        if houses > min {
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
        let what = if houses + 1 == 5 { "a hotel" } else { "a house" };
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
        let (_, max) = self.group_house_bounds(group);
        if houses < max {
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

    /// One line per slot in the estate popup, reflecting live board state.
    fn estate_labels(&self, menu: &EstateMenu) -> Vec<String> {
        menu.slots
            .iter()
            .map(|&i| {
                let s = &self.board[i];
                match menu.mode {
                    EstateMode::Mortgage => {
                        let half = s.price().unwrap_or(0) / 2;
                        if s.is_mortgaged() {
                            format!("{}  [unmortgage ${}]", s.name(), half + half / 10)
                        } else {
                            format!("{}  [mortgage +${half}]", s.name())
                        }
                    }
                    EstateMode::Build => {
                        let h = s.houses();
                        let level = if h == 5 { "hotel".to_string() } else { format!("{h} house") };
                        format!("{}  {level}  (house ${})", s.name(), s.house_cost())
                    }
                }
            })
            .collect()
    }

    /// How many railroads/utilities `owner` holds.
    fn count_kind(&self, owner: usize, kind: Kind) -> u32 {
        self.board
            .iter()
            .filter(|s| s.owner() == Some(owner))
            .filter(|s| match kind {
                Kind::Railroad => matches!(s, Space::Railroad(_)),
                Kind::Utility => matches!(s, Space::Utility(_)),
            })
            .count() as u32
    }

    fn notify(&mut self, message: impl Into<String>, level: Level) {
        let (accent, tag) = match level {
            Level::Warn => (Color::Rgb(0xF7, 0x94, 0x1D), "warn"),
            Level::Error => (Color::Rgb(0xED, 0x1B, 0x24), "error"),
            _ => (Color::Rgb(0x6C, 0xB6, 0xFF), "info"),
        };
        if let Ok(note) = Notification::new(message.into())
            .level(level)
            .anchor(Anchor::TopRight)
            .animation(ToastAnimation::Fade) // fade is far less glitchy than slide
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(accent))
            .title(Line::from(format!(" {tag} ")))
            .title_style(Style::new().fg(accent).bold())
            .style(Style::new().bg(Color::Rgb(0x16, 0x16, 0x1C)).fg(Color::White))
            .padding(Padding::horizontal(1))
            .margin(1)
            // Fixed height keeps stacking spacing uniform between all toasts.
            .max_size(SizeConstraint::Absolute(46), SizeConstraint::Absolute(3))
            .auto_dismiss(AutoDismiss::After(Duration::from_secs(3)))
            .build()
        {
            let _ = self.notes.add(note);
        }
    }

    // --- rendering of popups & notifications ---------------------------------

    pub fn render(&mut self, frame: &mut Frame) {
        match &self.modal {
            Modal::Roll(roll) => dice::render(frame, dice::animation(), roll),
            Modal::Card(card) => {
                dice::render_clip(frame, dice::card_animation(), &card.clip, card.card.title)
            }
            Modal::Menu(menu) => menu.render(frame, self.current, self.players[self.current].money),
            Modal::ConfirmEnd(confirm) => confirm.render(frame, " End your turn? "),
            Modal::Info(info) => crate::ui::info_popup(frame, &info.title, &info.lines),
            Modal::Jail(menu) => choice_popup(frame, " In Jail ", &menu.labels(), menu.cursor.selected),
            Modal::Estate(menu) => {
                let title = match menu.mode {
                    EstateMode::Mortgage => " Mortgages ",
                    EstateMode::Build => " Build Houses (Enter buy, s sell) ",
                };
                let lines = self.estate_labels(menu);
                choice_popup(frame, title, &lines, menu.cursor.selected);
            }
            Modal::Buy { confirm, pos } => {
                let price = self.board[*pos].price().unwrap_or(0);
                confirm.render(frame, &format!(" Buy {} for ${price}? ", self.board[*pos].name()));
            }
            Modal::Auction(auc) => {
                let cur = auc.active[auc.turn];
                let high = match auc.high_bidder {
                    Some(w) => format!("High bid: ${} (Player {})", auc.high_bid, w + 1),
                    None => "No bids yet".to_string(),
                };
                let lines = vec![
                    format!("Property: {}", self.board[auc.pos].name()),
                    high,
                    format!("Player {}'s turn", cur + 1),
                    format!("[b] bid +${AUCTION_STEP}   [p] pass   [esc] cancel"),
                ];
                crate::ui::info_popup(frame, " Auction ", &lines);
            }
            Modal::Trade(t) => match t.stage {
                TradeStage::Partner => {
                    let labels: Vec<String> = t
                        .partners
                        .iter()
                        .map(|&p| format!("Player {}  (${})", p + 1, self.players[p].money))
                        .collect();
                    choice_popup(frame, " Trade — pick a player ", &labels, t.pcursor.selected);
                }
                TradeStage::Property => {
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
                TradeStage::Price => {
                    let idx = t.props[t.prop_cursor.selected];
                    let owner = self.board[idx].owner().unwrap();
                    let buyer = if owner == self.current { t.partner } else { self.current };
                    let lines = vec![
                        format!("{} from Player {}", self.board[idx].name(), owner + 1),
                        format!("Player {} pays ${}", buyer + 1, t.price),
                        "[←/→] adjust   [enter] confirm   [esc] back".to_string(),
                    ];
                    crate::ui::info_popup(frame, " Trade — price ", &lines);
                }
            },
            Modal::GameOver(winner) => crate::ui::info_popup(
                frame,
                &format!(" Player {} wins! ", winner + 1),
                &["Press any key to return to the menu".to_string()],
            ),
            Modal::None => {}
        }
        let area = frame.area();
        self.notes.render(frame, area);
    }
}

enum Kind {
    Railroad,
    Utility,
}

/// Which card deck to draw from.
#[derive(Clone, Copy)]
enum Deck {
    Chance,
    Chest,
}

/// Fisher-Yates shuffle into a draw pile.
fn shuffled(mut cards: Vec<Card>) -> VecDeque<Card> {
    for i in (1..cards.len()).rev() {
        let j = rand::random_range(0..=i);
        cards.swap(i, j);
    }
    cards.into()
}
