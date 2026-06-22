//! Draws the board: an 11x11 grid where only the outer ring holds spaces.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Paragraph, Widget},
};

use crate::board::board;
use crate::space::Space;

/// 11x11 grid; only the outer ring holds the 40 spaces.
const SIZE: usize = 11;
/// Cell size in terminal cells. Width must fit the longest name + 2 borders.
const CELL_WIDTH: u16 = 16;
const CELL_HEIGHT: u16 = 4;
/// Classic Monopoly board green (#CDE6D0).
pub const BOARD_BG: Color = Color::Rgb(0xCD, 0xE6, 0xD0);

pub struct Map {
    board: Vec<Space>,
}

impl Map {
    pub fn default() -> Self {
        Self { board: board() }
    }
}

impl Widget for Map {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Center the fixed-size board within whatever area we're given.
        let board_w = SIZE as u16 * CELL_WIDTH;
        let board_h = SIZE as u16 * CELL_HEIGHT;
        let [area] = Layout::horizontal([Constraint::Length(board_w)])
            .flex(Flex::Center)
            .areas(area);
        let [area] = Layout::vertical([Constraint::Length(board_h)])
            .flex(Flex::Center)
            .areas(area);

        // Green fills the board, including the hollow center; cells draw on top.
        Block::new().style(Style::new().bg(BOARD_BG)).render(area, buf);

        // `areas::<N>` returns a stack array, no per-frame heap allocation.
        let vertical = Layout::vertical([Constraint::Length(CELL_HEIGHT); SIZE]);
        let horizontal = Layout::horizontal([Constraint::Length(CELL_WIDTH); SIZE]);

        for (r, row) in vertical.areas::<SIZE>(area).into_iter().enumerate() {
            for (c, cell) in horizontal.areas::<SIZE>(row).into_iter().enumerate() {
                let Some(index) = ring_index(r, c, SIZE, SIZE) else {
                    continue; // interior cell
                };
                render_space(&self.board[index], cell, buf);
            }
        }
    }
}

/// Ring cell (row, col) -> board index 0..40, clockwise from GO at the
/// bottom-right corner. Interior cells return `None`.
fn ring_index(r: usize, c: usize, rows: usize, cols: usize) -> Option<usize> {
    let last_r = rows - 1;
    let last_c = cols - 1;

    if r == last_r {
        Some(last_c - c) // bottom row: GO (0) -> Jail (10)
    } else if c == 0 {
        Some(last_c + (last_r - r)) // left column: 11 -> Free Parking (20)
    } else if r == 0 {
        Some(last_c + last_r + c) // top row: 21 -> Go To Jail (30)
    } else if c == last_c {
        Some(last_c + last_r + last_c + r) // right column: 31 -> Boardwalk (39)
    } else {
        None
    }
}

/// Bordered cell: name on top, price/owner on the bottom, border tinted by
/// color group for properties.
fn render_space(space: &Space, area: Rect, buf: &mut Buffer) {
    let mut block = Block::bordered()
        .style(Style::new().bg(BOARD_BG).fg(Color::Black).bold())
        .title_top(Line::from(short_name(space)).centered());

    let detail = detail_line(space);
    if !detail.is_empty() {
        block = block.title_bottom(Line::from(detail).centered());
    }

    let inner = block.inner(area);
    block.render(area, buf);

    // Top inner row is a banner: the color group for properties, otherwise a
    // black strip with a white icon for chance/chest/railroad/tax.
    let banner = Rect::new(inner.x, inner.y, inner.width, 1);
    match space {
        Space::Property(p) => {
            Block::new()
                .style(Style::new().bg(p.group.color()))
                .render(banner, buf);
        }
        _ => {
            if let Some(icon) = space.icon() {
                Paragraph::new(Line::from(icon).centered())
                    .style(Style::new().fg(Color::White).bg(Color::Black).bold())
                    .render(banner, buf);
            }
        }
    }
}

/// Name trimmed to the inner width (cell width minus the two borders).
fn short_name(space: &Space) -> String {
    let max = (CELL_WIDTH - 2) as usize;
    space.name().chars().take(max).collect()
}

/// Owner if bought, else price, else blank.
fn detail_line(space: &Space) -> String {
    match space {
        Space::Property(p) => owned_or_price(p.owner, p.price),
        Space::Railroad(r) => owned_or_price(r.owner, r.price),
        Space::Utility(u) => owned_or_price(u.owner, u.price),
        Space::Tax(amount) => format!("-${amount}"),
        _ => String::new(),
    }
}

/// "P1" if owned, otherwise the price like "$200".
fn owned_or_price(owner: Option<usize>, price: u32) -> String {
    match owner {
        Some(player) => format!("P{}", player + 1),
        None => format!("${price}"),
    }
}
