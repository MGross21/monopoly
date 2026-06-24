//! Game state and rules: turns, movement, buying, rent, and the per-turn menu.
//! See GAME_RULES.md for the rules this implements.

use std::time::Duration;

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, BorderType, Clear, Padding, Paragraph},
};
use ratatui_notifications::{
    Anchor, Animation as ToastAnimation, AutoDismiss, Level, Notification, Notifications,
    SizeConstraint,
};

use crate::board::board;
use crate::dice::{self, Clip, Roll};
use crate::map::Overlay;
use crate::player::Player;
use crate::space::Space;
use crate::ui::{Confirm, centered_rect};

const GO_SALARY: u32 = 200;
const JAIL_INDEX: usize = 10;
const RAILROAD_BASE_RENT: u32 = 25;

pub struct Game {
    pub players: Vec<Player>,
    pub board: Vec<Space>,
    pub current: usize,
    pub roll: Option<Roll>,
    pub menu: Option<ActionMenu>,
    card: Option<CardDraw>,
    confirm_end: Option<Confirm>,
    info: Option<InfoBox>,
    notes: Notifications,
    doubles: u8,
    can_roll: bool,
    has_rolled: bool, // rolled at least once this turn
    clock: Duration,  // drives the breathing highlight
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
        let mut game = Self {
            players,
            board: board(),
            current: 0,
            roll: None,
            menu: None,
            card: None,
            confirm_end: None,
            info: None,
            // Cap how many toasts stack at once so a single turn's events don't
            // pile up and flicker.
            notes: Notifications::new().max_concurrent(Some(4)),
            doubles: 0,
            can_roll: true,
            has_rolled: false,
            clock: Duration::ZERO,
        };
        game.notify("Player 1's turn", Level::Info);
        game
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

    fn animating(&self) -> bool {
        self.roll.as_ref().is_some_and(Roll::animating)
    }

    /// Advance time: clock, notifications, and any playing animation.
    pub fn tick(&mut self, delta: Duration) {
        self.clock += delta;
        self.notes.tick(delta);

        // Advance a roll; apply its result once the dice settle.
        let mut finished_roll = None;
        if let Some(roll) = &mut self.roll {
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
        if let Some(card) = &mut self.card {
            card.clip.tick(dice::card_animation(), delta);
            if card.clip.finished(dice::card_animation()) {
                self.card = None;
            }
        }
    }

    // --- input ---------------------------------------------------------------

    pub fn handle_key(&mut self, key: KeyCode) {
        // An info popup blocks everything; any key dismisses it.
        if self.info.is_some() {
            self.info = None;
            return;
        }

        // End-turn confirmation takes priority.
        if self.confirm_end.is_some() {
            match key {
                KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right => {
                    if let Some(c) = &mut self.confirm_end {
                        c.toggle();
                    }
                }
                KeyCode::Enter => {
                    let yes = self.confirm_end.as_ref().is_some_and(Confirm::is_yes);
                    self.confirm_end = None;
                    if yes {
                        self.end_turn();
                    }
                }
                KeyCode::Esc => self.confirm_end = None,
                _ => {}
            }
            return;
        }

        if let Some(menu) = &mut self.menu {
            match key {
                KeyCode::Up => menu.prev(),
                KeyCode::Down => menu.next(),
                KeyCode::Esc => self.menu = None,
                KeyCode::Enter => {
                    let action = menu.selected();
                    self.menu = None;
                    self.run(action);
                }
                _ => {}
            }
            return;
        }

        // A card animation blocks input; Space/Enter skips it.
        if self.card.is_some() {
            if matches!(key, KeyCode::Char(' ') | KeyCode::Enter) {
                self.card = None;
            }
            return;
        }

        if self.roll.is_some() {
            // Dismiss the dice popup once the animation is done.
            if matches!(key, KeyCode::Char(' ') | KeyCode::Enter) && !self.animating() {
                self.roll = None;
            }
            return;
        }

        match key {
            KeyCode::Char('m') | KeyCode::Enter => self.menu = Some(ActionMenu::new()),
            KeyCode::Char(' ') => self.start_roll(),
            // Action hotkeys work outside the menu for faster turns.
            KeyCode::Char(c) => {
                if let Some(action) = action_for_hotkey(c) {
                    self.run(action);
                }
            }
            _ => {}
        }
    }

    fn run(&mut self, action: TurnAction) {
        match action {
            TurnAction::RollDice => self.start_roll(),
            TurnAction::BuyProperty => self.buy_current(),
            TurnAction::EndTurn => {
                if self.has_rolled {
                    self.confirm_end = Some(Confirm::new());
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
        if self.roll.is_some() {
            return; // already rolling
        }
        if !self.can_roll {
            self.notify("You already rolled — end your turn", Level::Warn);
            return;
        }
        self.roll = Some(Roll::new());
    }

    /// Apply a finished dice roll: move, pass GO, resolve the landing, doubles.
    fn apply_roll(&mut self, a: u8, b: u8) {
        let total = (a + b) as usize;
        let who = self.current;
        let old = self.players[who].position;
        let passed_go = old + total >= 40;
        let new = (old + total) % 40;
        self.players[who].position = new;
        self.has_rolled = true;

        self.notify(format!("Player {} rolled {a} + {b} = {}", who + 1, a + b), Level::Info);
        if passed_go {
            self.players[who].money += GO_SALARY;
            self.notify(format!("Player {} passed GO (+${GO_SALARY})", who + 1), Level::Info);
        }
        let name = self.board[new].name().to_string();
        self.notify(format!("Player {} landed on {name}", who + 1), Level::Info);

        self.resolve_landing(new, total);

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

        // If a card animation started on landing, drop the dice popup so it shows.
        if self.card.is_some() {
            self.roll = None;
        }
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
        self.card = Some(CardDraw {
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
        self.current = (self.current + 1) % self.players.len();
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
        self.info = Some(InfoBox {
            title: format!(" Player {} — ${} ", me + 1, self.players[me].money),
            lines,
        });
    }

    // --- money & helpers -----------------------------------------------------

    fn send_to_jail(&mut self) {
        self.players[self.current].position = JAIL_INDEX;
        self.doubles = 0;
        self.notify(format!("Player {} was sent to Jail", self.current + 1), Level::Warn);
    }

    fn pay_bank(&mut self, amount: u32) {
        let who = self.current;
        self.players[who].money = self.players[who].money.saturating_sub(amount);
        self.notify(format!("Player {} paid ${amount} in tax", who + 1), Level::Warn);
    }

    fn pay_player(&mut self, owner: usize, rent: u32) {
        let who = self.current;
        self.players[who].money = self.players[who].money.saturating_sub(rent);
        self.players[owner].money += rent;
        self.notify(
            format!("Player {} paid ${rent} rent to Player {}", who + 1, owner + 1),
            Level::Warn,
        );
    }

    /// Rent owed for the space at `pos`, owned by `owner`.
    fn rent(&self, pos: usize, owner: usize, total: usize) -> u32 {
        match &self.board[pos] {
            Space::Property(p) => p.rent,
            Space::Railroad(_) => RAILROAD_BASE_RENT * self.count_kind(owner, Kind::Railroad),
            Space::Utility(_) => {
                let multiplier = if self.count_kind(owner, Kind::Utility) == 2 { 10 } else { 4 };
                total as u32 * multiplier
            }
            _ => 0,
        }
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
        if let Some(roll) = &self.roll {
            dice::render(frame, dice::animation(), roll);
        }
        if let Some(card) = &self.card {
            dice::render_clip(frame, dice::card_animation(), &card.clip, card.title);
        }
        if let Some(menu) = &self.menu {
            menu.render(frame, self.current, self.players[self.current].money);
        }
        if let Some(confirm) = &self.confirm_end {
            confirm.render(frame, " End your turn? ");
        }
        if let Some(info) = &self.info {
            crate::ui::info_popup(frame, &info.title, &info.lines);
        }
        let area = frame.area();
        self.notes.render(frame, area);
    }
}

enum Kind {
    Railroad,
    Utility,
}

// --- per-turn action menu ---------------------------------------------------

#[derive(Clone, Copy)]
pub enum TurnAction {
    RollDice,
    BuyProperty,
    Trade,
    ViewInventory,
    Mortgages,
    EndTurn,
}

const ACTIONS: [TurnAction; 6] = [
    TurnAction::RollDice,
    TurnAction::BuyProperty,
    TurnAction::Trade,
    TurnAction::ViewInventory,
    TurnAction::Mortgages,
    TurnAction::EndTurn,
];

fn action_label(action: TurnAction) -> &'static str {
    match action {
        TurnAction::RollDice => "Roll Dice",
        TurnAction::BuyProperty => "Buy Property",
        TurnAction::Trade => "Trade",
        TurnAction::ViewInventory => "View Inventory",
        TurnAction::Mortgages => "Mortgages",
        TurnAction::EndTurn => "End Turn",
    }
}

/// Hotkey for an action, usable inside or outside the menu.
fn action_hotkey(action: TurnAction) -> char {
    match action {
        TurnAction::RollDice => 'r',
        TurnAction::BuyProperty => 'b',
        TurnAction::Trade => 't',
        TurnAction::ViewInventory => 'i',
        TurnAction::Mortgages => 'g',
        TurnAction::EndTurn => 'e',
    }
}

fn action_for_hotkey(c: char) -> Option<TurnAction> {
    ACTIONS.into_iter().find(|&a| action_hotkey(a) == c)
}

pub struct ActionMenu {
    selected: usize,
}

impl ActionMenu {
    fn new() -> Self {
        Self { selected: 0 }
    }

    fn prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn next(&mut self) {
        self.selected = (self.selected + 1).min(ACTIONS.len() - 1);
    }

    fn selected(&self) -> TurnAction {
        ACTIONS[self.selected]
    }

    fn render(&self, frame: &mut Frame, current: usize, money: u32) {
        // A blank row separates "End Turn" (the last action) from the rest.
        let gap = 1u16;
        let area = centered_rect(frame.area(), 28, ACTIONS.len() as u16 + gap + 2);
        let block = Block::bordered()
            .title_top(Line::from(format!(" Player {} — ${money} ", current + 1)).centered())
            .style(Style::new().bg(Color::Black).fg(Color::White).bold());
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        let last = ACTIONS.len() - 1;
        let mut lines: Vec<Line> = Vec::new();
        for (i, &action) in ACTIONS.iter().enumerate() {
            if i == last {
                lines.push(Line::from("")); // gap before End Turn
            }
            let label = format!("{} ({})", action_label(action), action_hotkey(action));
            let line = Line::from(label).centered();
            lines.push(if i == self.selected { line.reversed() } else { line });
        }
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

