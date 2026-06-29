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
    GameOver(usize), // winning player index
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

/// A Chance / Community Chest card animation playing in the center.
struct CardDraw {
    clip: Clip,
    title: &'static str,
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

        // Advance a card draw; close it when it finishes.
        if let Modal::Card(card) = &mut self.modal {
            card.clip.tick(dice::card_animation(), delta);
            if card.clip.finished(dice::card_animation()) {
                self.modal = Modal::None;
            }
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

            // A card animation blocks input; Space/Enter skips it.
            Modal::Card(_) => {
                if matches!(key, KeyCode::Char(' ') | KeyCode::Enter) {
                    self.modal = Modal::None;
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
            TurnAction::ViewInventory => self.show_inventory(),
            TurnAction::Trade => self.notify("Trading is not implemented yet", Level::Warn),
            TurnAction::Mortgages => self.notify("Mortgages are not implemented yet", Level::Warn),
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
            Space::Chance => self.draw_card(" Chance "),
            Space::CommunityChest => self.draw_card(" Community Chest "),
            space if space.is_ownable() => match space.owner() {
                None => self.notify("Unowned — open the menu to buy it", Level::Info),
                Some(owner) if owner != self.current => {
                    let rent = self.rent(pos, owner, total);
                    self.pay_player(owner, rent);
                }
                Some(_) => {} // your own property
            },
            _ => {}
        }
    }

    /// Start the card-draw animation for a Chance / Community Chest landing.
    fn draw_card(&mut self, title: &'static str) {
        self.modal = Modal::Card(CardDraw {
            clip: Clip::new(),
            title,
        });
        self.notify(format!("Player {} draws a card", self.current + 1), Level::Info);
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
                dice::render_clip(frame, dice::card_animation(), &card.clip, card.title)
            }
            Modal::Menu(menu) => menu.render(frame, self.current, self.players[self.current].money),
            Modal::ConfirmEnd(confirm) => confirm.render(frame, " End your turn? "),
            Modal::Info(info) => crate::ui::info_popup(frame, &info.title, &info.lines),
            Modal::Jail(menu) => choice_popup(frame, " In Jail ", &menu.labels(), menu.cursor.selected),
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
