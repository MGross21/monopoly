//! Draws the board: an 11x11 grid where only the outer ring holds spaces.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Paragraph, Widget},
};

use tui_big_text::{BigText, PixelSize};

use crate::board::board;
use crate::space::Space;

/// 11x11 grid; only the outer ring holds the 40 spaces.
const SIZE: usize = 11;
/// Cell size in terminal cells. Width must fit the longest name + 2 borders.
const CELL_WIDTH: u16 = 16;
const CELL_HEIGHT: u16 = 4;
/// Full board size; the terminal must be at least this big to render.
pub const BOARD_W: u16 = SIZE as u16 * CELL_WIDTH;
pub const BOARD_H: u16 = SIZE as u16 * CELL_HEIGHT;
/// Classic Monopoly board green (#CDE6D0).
pub const BOARD_BG: Color = Color::Rgb(0xCD, 0xE6, 0xD0);
const TITLE_RED: Color = Color::Rgb(0xED, 0x1B, 0x24);
const CHANCE_ORANGE: Color = Color::Rgb(0xF7, 0x94, 0x1D);
const CHEST_GOLD: Color = Color::Rgb(0xC8, 0x96, 0x28);

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
        // Center the fixed-size board within whatever area we're given. The
        // caller guarantees the area is at least BOARD_W x BOARD_H.
        let [area] = Layout::horizontal([Constraint::Length(BOARD_W)])
            .flex(Flex::Center)
            .areas(area);
        let [area] = Layout::vertical([Constraint::Length(BOARD_H)])
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

        // Hollow center: inset the board by one cell on every side.
        let center = Rect::new(
            area.x + CELL_WIDTH,
            area.y + CELL_HEIGHT,
            BOARD_W - 2 * CELL_WIDTH,
            BOARD_H - 2 * CELL_HEIGHT,
        );
        render_center(center, buf);
    }
}

/// Draws the board interior: Community Chest slot, big MONOPOLY title, Chance slot.
fn render_center(area: Rect, buf: &mut Buffer) {
    let [top, middle, bottom] = Layout::vertical([
        Constraint::Percentage(34),
        Constraint::Percentage(32),
        Constraint::Percentage(34),
    ])
    .areas(area);

    render_card_slot(top, "COMMUNITY CHEST", "\u{f187}", CHEST_GOLD, buf);
    render_title(middle, buf);
    render_card_slot(bottom, "CHANCE", "?", CHANCE_ORANGE, buf);
}

/// Big block-glyph "MONOPOLY", centered in `area`.
fn render_title(area: Rect, buf: &mut Buffer) {
    let title = BigText::builder()
        .pixel_size(PixelSize::Full)
        .centered()
        .lines(vec!["MONOPOLY".into()])
        .style(Style::new().fg(TITLE_RED).bold())
        .build();

    // A glyph is 8 rows tall; center that band vertically.
    let [band] = Layout::vertical([Constraint::Length(8)])
        .flex(Flex::Center)
        .areas(area);
    title.render(band, buf);
}

/// A bordered card pile with a centered label and icon.
fn render_card_slot(area: Rect, label: &str, icon: &str, color: Color, buf: &mut Buffer) {
    let area = centered(area, 40, 7);
    let block = Block::bordered()
        .title_top(Line::from(label).centered())
        .style(Style::new().fg(color).bold());

    let inner = block.inner(area);
    block.render(area, buf);

    let [icon_row] = Layout::vertical([Constraint::Length(1)])
        .flex(Flex::Center)
        .areas(inner);
    Paragraph::new(Line::from(icon).centered())
        .style(Style::new().fg(color).bold())
        .render(icon_row, buf);
}

/// Centers a `width` x `height` rect inside `area`.
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    area
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
