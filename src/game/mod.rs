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
mod auction;
mod cards;
mod debt;
mod estate;
mod jail;
mod rent;
mod save;
#[cfg(test)]
mod testkit;
#[cfg(test)]
mod tests;
mod trade;

use std::collections::VecDeque;

use auction::Auction;
use cards::{Card, CardDraw, Deck, fresh_decks};
use debt::{Debt, Payee};
use estate::EstateMenu;
use jail::JailMenu;
use rent::RentRule;
use trade::Trade;

use crate::board::board;
use crate::player::Player;
use crate::space::Space;
use crate::ui::dice::{self, Roll};
use crate::ui::map::Overlay;
use crate::ui::{Confirm, ConfirmResult};
use action::{ActionMenu, TurnAction, action_for_hotkey};

/// Spaces on the board, indexed clockwise from GO at 0.
const BOARD_LEN: usize = 40;
const GO_SALARY: u32 = 200;
/// House count that represents a hotel.
const HOTEL: u8 = 5;
/// The bank's building stock. Running out is part of the game: no one can build
/// until someone else sells.
const TOTAL_HOUSES: u8 = 32;
const TOTAL_HOTELS: u8 = 12;

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
    houses_left: u8,
    hotels_left: u8,
    pending: VecDeque<Pending>,
}

/// A prompt waiting for the screen to clear. Only one popup shows at a time, so
/// work that would need a second one queues up here instead.
enum Pending {
    /// Auction this board index off for the bank.
    Auction(usize),
    /// Bill a player, which may open a liquidation popup of its own.
    Charge { who: usize, amount: u32, payee: Payee },
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
    Debt(Debt),
    GameOver(usize), // winning player index
}

/// A centered info popup (e.g. owned-property list) dismissed with any key.
struct InfoBox {
    title: String,
    lines: Vec<String>,
}

/// Feed a key to a popup that needs `&mut self` while it is open.
///
/// The popup is taken *out* of `self.modal` first, so its `*_input` method gets
/// the whole game — and can open a different popup, which then survives because
/// we only put this one back when the handler asks to stay open.
macro_rules! drive {
    ($self:ident, $variant:ident, $input:ident, $key:ident) => {{
        if let Modal::$variant(mut state) = std::mem::replace(&mut $self.modal, Modal::None)
            && $self.$input(&mut state, $key)
        {
            $self.modal = Modal::$variant(state);
        }
    }};
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
        let (chance, chest) = fresh_decks();
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
            chance,
            chest,
            houses_left: TOTAL_HOUSES,
            hotels_left: TOTAL_HOTELS,
            pending: VecDeque::new(),
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

    /// Whether anything time-driven is happening — a playing animation or a live
    /// notification. When false, the event loop can block on input instead of
    /// polling (the breathing highlight simply pauses until the next key).
    pub fn needs_tick(&self) -> bool {
        self.notes.has_notification()
            || matches!(&self.modal, Modal::Card(_))
            || matches!(&self.modal, Modal::Roll(roll) if roll.animating())
    }

    pub fn overlay(&self) -> Overlay {
        Overlay::Board { turn: self.current, breath: self.breath() }
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

            Modal::Jail(_) => drive!(self, Jail, jail_input, key),
            Modal::Estate(_) => drive!(self, Estate, estate_input, key),
            // Forced liquidation; has no cancel key, so it stays until the debt
            // is paid off or the player folds.
            Modal::Debt(_) => drive!(self, Debt, debt_input, key),
            Modal::Auction(_) => drive!(self, Auction, auction_input, key),
            Modal::Trade(_) => drive!(self, Trade, trade_input, key),

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
            self.open_jail();
            return;
        }
        self.modal = Modal::Roll(Roll::new());
    }

    /// Move `who` forward `steps`, paying GO salary on a pass and resolving the
    /// landing. Shared by normal and out-of-jail moves.
    fn advance(&mut self, who: usize, steps: usize) {
        self.advance_under(who, steps, RentRule::Normal);
    }

    /// `advance` with the rent `rule` the landing is charged under — the
    /// "advance to the nearest …" cards bill more than the printed rate.
    fn advance_under(&mut self, who: usize, steps: usize, rule: RentRule) {
        let old = self.players[who].position;
        let passed_go = old + steps >= BOARD_LEN;
        let new = (old + steps) % BOARD_LEN;
        self.players[who].position = new;
        if passed_go {
            self.players[who].money += GO_SALARY;
            self.notify(format!("Player {} passed GO (+${GO_SALARY})", who + 1), Level::Info);
        }
        let name = self.board[new].name().to_string();
        self.notify(format!("Player {} landed on {name}", who + 1), Level::Info);
        self.resolve_landing(new, steps, rule);
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

        // Doubles earn another roll; the third in a row goes straight to Jail
        // without moving. Settled before the move so a debt raised on the
        // landing can't disturb the turn state while its popup is open.
        if a == b {
            self.doubles += 1;
            if self.doubles >= 3 {
                self.send_to_jail();
                self.can_roll = false;
                return;
            }
            self.can_roll = true;
            self.notify(format!("Doubles! Player {} rolls again", who + 1), Level::Info);
        } else {
            self.can_roll = false;
        }

        self.advance(who, total);

        // Landing on "Go To Jail" ends the turn — no bonus roll.
        if self.players[who].in_jail {
            self.can_roll = false;
        }
        self.settle_if_bankrupt(who);
    }

    /// What landing on a space demands, read off the board before `self` is
    /// borrowed mutably to act on it.
    fn landing(&self, pos: usize) -> Landing {
        let space = &self.board[pos];
        match space {
            Space::Tax(amount) => Landing::Tax(*amount),
            Space::GoToJail => Landing::Jail,
            Space::Chance => Landing::Draw(Deck::Chance),
            Space::CommunityChest => Landing::Draw(Deck::Chest),
            _ if !space.is_ownable() => Landing::Nothing,
            _ => match space.owner() {
                None => Landing::ForSale(space.price().unwrap_or(0)),
                Some(owner) if owner != self.current => Landing::Owned(owner),
                Some(_) => Landing::Nothing,
            },
        }
    }

    /// React to the space the current player landed on.
    fn resolve_landing(&mut self, pos: usize, total: usize, rule: RentRule) {
        match self.landing(pos) {
            Landing::Tax(amount) => self.pay_bank(amount),
            Landing::Jail => self.send_to_jail(),
            Landing::Draw(deck) => self.draw_card(deck),
            // Offer to buy; declining sends the property to auction.
            Landing::ForSale(price) => {
                let name = self.board[pos].name().to_string();
                self.notify(format!("{name} is for sale (${price})"), Level::Info);
                self.modal = Modal::Buy { confirm: Confirm::new(), pos };
            }
            Landing::Owned(owner) => {
                let rent = self.rent(pos, owner, total, rule);
                self.pay_player(owner, rent);
            }
            Landing::Nothing => {}
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

    /// Indices of players still in the game, in seating order.
    fn active_players(&self) -> Vec<usize> {
        (0..self.players.len()).filter(|&i| !self.players[i].bankrupt).collect()
    }

    /// After a debt was paid, hand off the turn if it bankrupted `who`. Returns
    /// `true` when `who` is out, so the caller can stop processing their turn.
    /// `bankrupt` has already ended the game (`Modal::GameOver`) if no one's left.
    fn settle_if_bankrupt(&mut self, who: usize) -> bool {
        if !self.players[who].bankrupt {
            return false;
        }
        // Only the player whose turn it is hands it off — a card can bankrupt a
        // bystander without ending the drawer's turn.
        if who == self.current && !matches!(self.modal, Modal::GameOver(_)) {
            self.end_turn();
        }
        true
    }

    /// Work queued up behind the popup that's currently open — a bankrupt
    /// estate's auctions, or the rest of a "collect from every player" card.
    fn queue(&mut self, work: Pending) {
        self.pending.push_back(work);
    }

    /// Start the next queued item, if nothing is on screen. Each popup calls this
    /// as it closes, so the queue drains one prompt at a time.
    fn run_pending(&mut self) {
        while matches!(self.modal, Modal::None) {
            match self.pending.pop_front() {
                Some(Pending::Auction(pos)) => self.start_auction(pos),
                Some(Pending::Charge { who, amount, payee }) => {
                    if !self.players[who].bankrupt {
                        self.charge(who, amount, payee);
                    }
                }
                None => return,
            }
        }
    }

    fn show_inventory(&mut self) {
        let me = self.current;
        let lines: Vec<String> = self
            .estate(me)
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

    fn pay_bank(&mut self, amount: u32) {
        self.charge(self.current, amount, Payee::Bank);
    }

    fn pay_player(&mut self, owner: usize, rent: u32) {
        self.charge(self.current, rent, Payee::Player(owner));
    }

    /// Board indices owned by `who`, in board order.
    fn holdings(&self, who: usize) -> Vec<usize> {
        (0..self.board.len()).filter(|&i| self.board[i].owner() == Some(who)).collect()
    }

    /// Every space owned by `who`, in board order.
    fn estate(&self, who: usize) -> impl Iterator<Item = &Space> {
        self.board.iter().filter(move |s| s.owner() == Some(who))
    }

    /// Eliminate `who`: hand their estate to `creditor` (a player) or back to the
    /// bank, then check whether only one player remains.
    ///
    /// A creditor inherits the deeds as they stand and owes 10% interest on every
    /// mortgaged one. The bank instead auctions each deed off, one at a time.
    fn bankrupt(&mut self, who: usize, creditor: Option<usize>) {
        self.players[who].bankrupt = true;
        let mut interest = 0;
        for idx in self.holdings(who) {
            match creditor {
                Some(to) => {
                    if self.board[idx].is_mortgaged() {
                        interest += self.board[idx].mortgage_value() / 10;
                    }
                    self.board[idx].set_owner(Some(to));
                }
                None => {
                    self.reclaim_buildings(idx);
                    self.board[idx].set_owner(None);
                    self.board[idx].reset_buildings();
                    self.queue(Pending::Auction(idx));
                }
            }
        }
        self.notify(format!("Player {} is out of the game", who + 1), Level::Error);
        self.check_win();
        if matches!(self.modal, Modal::GameOver(_)) {
            self.pending.clear();
            return;
        }
        if let Some(to) = creditor
            && interest > 0
        {
            self.notify(
                format!("Player {} owes ${interest} interest on the mortgages", to + 1),
                Level::Warn,
            );
            self.queue(Pending::Charge { who: to, amount: interest, payee: Payee::Bank });
        }
        self.run_pending();
    }

    /// Return the buildings on `idx` to the bank's stock.
    fn reclaim_buildings(&mut self, idx: usize) {
        match self.board[idx].houses() {
            0 => {}
            HOTEL => self.hotels_left += 1,
            n => self.houses_left += n,
        }
    }

    /// If only one player is left standing, end the game.
    fn check_win(&mut self) {
        let mut alive = (0..self.players.len()).filter(|&i| !self.players[i].bankrupt);
        if let (Some(winner), None) = (alive.next(), alive.next()) {
            self.notify(format!("Player {} wins!", winner + 1), Level::Info);
            self.modal = Modal::GameOver(winner);
        }
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
            Modal::Jail(menu) => self.render_jail(frame, menu),
            Modal::Estate(menu) => self.render_estate(frame, menu),
            Modal::Buy { confirm, pos } => {
                let price = self.board[*pos].price().unwrap_or(0);
                confirm.render(frame, &format!(" Buy {} for ${price}? ", self.board[*pos].name()));
            }
            Modal::Auction(auc) => self.render_auction(frame, auc),
            Modal::Trade(t) => self.render_trade(frame, t),
            Modal::Debt(d) => self.render_debt(frame, d),
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

/// The consequence of landing on a space, free of any borrow on the board.
#[derive(Clone, Copy)]
enum Landing {
    Tax(u32),
    Jail,
    Draw(Deck),
    ForSale(u32),
    Owned(usize),
    Nothing,
}
