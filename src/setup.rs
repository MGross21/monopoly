//! New-game setup popup: choose player count, starting money, and pieces.
//!
//! Up/Down move between fields, Left/Right change the selected value, Enter
//! starts the game.

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Clear, Paragraph},
};

use crate::player::{Piece, Player};

const MIN_PLAYERS: usize = 2;
const MAX_PLAYERS: usize = 8;
const MONEY_STEP: u32 = 250;
const MIN_MONEY: u32 = 250;
const POPUP_WIDTH: u16 = 40;

pub struct Setup {
    player_count: usize,
    starting_money: u32,
    pieces: Vec<Piece>,
    selected: usize, // 0 = players, 1 = money, 2.. = piece per player
}

impl Setup {
    pub fn new() -> Self {
        let player_count = 4;
        Self {
            player_count,
            starting_money: 1500,
            pieces: default_pieces(player_count),
            selected: 0,
        }
    }

    fn field_count(&self) -> usize {
        2 + self.player_count
    }

    /// Returns `Some(players)` once the user confirms with Enter.
    pub fn handle_key(&mut self, key: KeyCode) -> Option<Vec<Player>> {
        match key {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(self.field_count() - 1),
            KeyCode::Left => self.adjust(-1),
            KeyCode::Right => self.adjust(1),
            KeyCode::Enter => return Some(self.build_players()),
            _ => {}
        }
        None
    }

    fn adjust(&mut self, dir: i32) {
        match self.selected {
            0 => {
                let count = (self.player_count as i32 + dir)
                    .clamp(MIN_PLAYERS as i32, MAX_PLAYERS as i32) as usize;
                self.set_player_count(count);
            }
            1 => {
                self.starting_money = if dir > 0 {
                    self.starting_money + MONEY_STEP
                } else {
                    self.starting_money.saturating_sub(MONEY_STEP).max(MIN_MONEY)
                };
            }
            field => {
                let idx = field - 2;
                self.pieces[idx] = self.next_free(idx, dir);
            }
        }
    }

    /// Next piece in `dir` that no other player holds. Returns the current one
    /// if every other piece is taken.
    fn next_free(&self, idx: usize, dir: i32) -> Piece {
        let current = self.pieces[idx];
        let mut piece = current;
        loop {
            piece = if dir > 0 { piece.next() } else { piece.prev() };
            if piece == current {
                return current; // wrapped all the way around, none free
            }
            let taken = self
                .pieces
                .iter()
                .enumerate()
                .any(|(j, &other)| j != idx && other == piece);
            if !taken {
                return piece;
            }
        }
    }

    fn set_player_count(&mut self, count: usize) {
        self.player_count = count;
        self.pieces = default_pieces(count);
        self.selected = self.selected.min(self.field_count() - 1);
    }

    fn build_players(&self) -> Vec<Player> {
        self.pieces
            .iter()
            .map(|&piece| Player::new(piece, self.starting_money))
            .collect()
    }

    pub fn render(&self, frame: &mut Frame) {
        let height = self.field_count() as u16 + 4; // fields + border + hint
        let area = popup_area(frame.area(), POPUP_WIDTH, height);

        let block = Block::bordered()
            .title(" New Game ")
            .style(Style::new().bg(Color::Black));
        let inner = block.inner(area);

        frame.render_widget(Clear, area); // wipe the board behind the popup
        frame.render_widget(block, area);
        frame.render_widget(Paragraph::new(self.lines()), inner);
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            self.field_line(0, "Players", self.player_count.to_string()),
            self.field_line(1, "Money", format!("${}", self.starting_money)),
        ];
        for (i, piece) in self.pieces.iter().enumerate() {
            let value = format!("{} {}", piece.icon(), piece.label());
            lines.push(self.field_line(2 + i, &format!("Player {}", i + 1), value));
        }
        lines.push(Line::from(""));
        lines.push(Line::from("↑/↓ move  ←/→ change  Enter start").dim());
        lines
    }

    fn field_line(&self, field: usize, label: &str, value: String) -> Line<'static> {
        let line = Line::from(format!("{label:<9} ‹ {value} ›"));
        if field == self.selected {
            line.reversed()
        } else {
            line
        }
    }
}

fn default_pieces(count: usize) -> Vec<Piece> {
    Piece::ALL.into_iter().take(count).collect()
}

/// Centers a `width` x `height` rect inside `area`.
fn popup_area(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    area
}
