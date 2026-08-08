//! Draws the board: an 11x11 grid where only the outer ring holds spaces.

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Clear, Paragraph, Widget, Wrap},
};

use tui_big_text::{BigText, PixelSize};

use crate::player::Player;
use crate::space::Space;
use crate::ui::menu::OPTIONS;
use crate::ui::{centered_rect, selectable_lines};

/// What to draw in the hollow center: the main menu, or the in-game board
/// (keybind bar + card slots), tagged with whose turn it is.
pub enum Overlay {
    Menu { selected: usize },
    Board { turn: usize, breath: f32 },
}

/// 11x11 grid; only the outer ring holds the 40 spaces.
const SIZE: usize = 11;
/// Minimum cell size. Width must fit the longest name + 2 borders; cells grow
/// past this to fill larger terminals.
const MIN_CELL_WIDTH: u16 = 16;
const MIN_CELL_HEIGHT: u16 = 4;
/// Minimum board size; the terminal must be at least this big to render.
pub const BOARD_W: u16 = SIZE as u16 * MIN_CELL_WIDTH;
pub const BOARD_H: u16 = SIZE as u16 * MIN_CELL_HEIGHT;
/// Classic Monopoly board green (#CDE6D0).
pub const BOARD_BG: Color = Color::Rgb(0xCD, 0xE6, 0xD0);
const TITLE_RED: Color = Color::Rgb(0xED, 0x1B, 0x24);
const CHANCE_ORANGE: Color = Color::Rgb(0xF7, 0x94, 0x1D);
const CHEST_GOLD: Color = Color::Rgb(0xC8, 0x96, 0x28);
/// Darker green the highlighted cell breathes toward.
const BOARD_BG_DARK: Color = Color::Rgb(0x8F, 0xA1, 0x91);

/// Blend between the darker and normal board green by `f` (0..1).
fn breathe(f: f32) -> Color {
    let (Color::Rgb(dr, dg, db), Color::Rgb(br, bg, bb)) = (BOARD_BG_DARK, BOARD_BG) else {
        return BOARD_BG;
    };
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * f) as u8;
    Color::Rgb(mix(dr, br), mix(dg, bg), mix(db, bb))
}

pub struct Map<'a> {
    board: &'a [Space],
    players: &'a [Player],
    overlay: Overlay,
}

impl<'a> Map<'a> {
    pub fn new(board: &'a [Space], players: &'a [Player], overlay: Overlay) -> Self {
        Self { board, players, overlay }
    }
}

impl Widget for Map<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Scale cells to fill the terminal, never below the minimum. The caller
        // guarantees the area is at least BOARD_W x BOARD_H.
        let cell_w = (area.width / SIZE as u16).max(MIN_CELL_WIDTH);
        let cell_h = (area.height / SIZE as u16).max(MIN_CELL_HEIGHT);
        let board_w = cell_w * SIZE as u16;
        let board_h = cell_h * SIZE as u16;

        // Center the scaled board; leftover (area % SIZE) becomes a thin margin.
        let [area] =
            Layout::horizontal([Constraint::Length(board_w)]).flex(Flex::Center).areas(area);
        let [area] = Layout::vertical([Constraint::Length(board_h)]).flex(Flex::Center).areas(area);

        // Green fills the board, including the hollow center; cells draw on top.
        Block::new().style(Style::new().bg(BOARD_BG)).render(area, buf);

        // The current player's cell breathes between board green and a darker
        // green; everything else uses the flat board green.
        let breathing = match self.overlay {
            Overlay::Board { turn, breath } => self.players.get(turn).map(|p| (p.position, breath)),
            Overlay::Menu { .. } => None,
        };

        // `areas::<N>` returns a stack array, no per-frame heap allocation.
        let vertical = Layout::vertical([Constraint::Length(cell_h); SIZE]);
        let horizontal = Layout::horizontal([Constraint::Length(cell_w); SIZE]);

        for (r, row) in vertical.areas::<SIZE>(area).into_iter().enumerate() {
            for (c, cell) in horizontal.areas::<SIZE>(row).into_iter().enumerate() {
                let Some(index) = ring_index(r, c, SIZE, SIZE) else {
                    continue; // interior cell
                };
                let bg = match breathing {
                    Some((pos, breath)) if pos == index => breathe(breath),
                    _ => BOARD_BG,
                };
                let owner_icon = self.board[index]
                    .owner()
                    .and_then(|o| self.players.get(o))
                    .map(|p| p.piece.icon());
                render_space(&self.board[index], cell, bg, owner_icon, buf);
                self.render_tokens(index, cell, buf);
            }
        }

        // Hollow center: inset the board by one cell on every side.
        let center =
            Rect::new(area.x + cell_w, area.y + cell_h, board_w - 2 * cell_w, board_h - 2 * cell_h);
        render_center(center, &self.overlay, self.players, self.board, buf);
    }
}

impl Map<'_> {
    /// Draws the tokens of any players standing on `index`, wrapped across the
    /// cell's bottom rows so a crowded space doesn't overflow one line.
    fn render_tokens(&self, index: usize, cell: Rect, buf: &mut Buffer) {
        let icons: Vec<&str> =
            self.players.iter().filter(|p| p.position == index).map(|p| p.piece.icon()).collect();
        if icons.is_empty() {
            return;
        }
        // Each emoji token takes ~2 columns plus a space; fit as many per row as
        // the inner width allows, then stack onto rows above.
        let inner_w = cell.width.saturating_sub(2);
        let per_row = (inner_w / 3).max(1) as usize;
        let rows = icons.chunks(per_row);
        let row_count = rows.len() as u16;
        // Anchor the block just above the cell's bottom border.
        let bottom = cell.y + cell.height.saturating_sub(1);
        let top = bottom.saturating_sub(row_count);
        for (r, chunk) in rows.enumerate() {
            let line = chunk.join(" ");
            let area = Rect::new(cell.x + 1, top + r as u16, inner_w, 1);
            Paragraph::new(Line::from(line).centered())
                .style(Style::new().bg(BOARD_BG))
                .render(area, buf);
        }
    }
}

/// Draws the board interior. The main menu shows the title + menu bar; in-game
/// shows the title, a thin keybind bar, then the two card slots.
fn render_center(
    area: Rect,
    overlay: &Overlay,
    players: &[Player],
    board: &[Space],
    buf: &mut Buffer,
) {
    // Title is vertically centered in the whole center; menus/cards sit at the
    // bottom (drawn after, and the title only paints its glyph cells anyway).
    render_title(area, buf);

    match *overlay {
        Overlay::Menu { selected } => {
            let [_, bar] =
                Layout::vertical([Constraint::Min(0), Constraint::Length(4)]).areas(area);
            render_menu_bar(bar, selected, buf);
        }
        Overlay::Board { turn, .. } => {
            // A standings panel (height = one row per player + border) sits above
            // the keybar and card slots. The setup screen has no players, so it
            // collapses to nothing.
            let panel_h = if players.is_empty() { 0 } else { players.len() as u16 + 2 };
            let [_, panel, keys, cards] = Layout::vertical([
                Constraint::Min(0),
                Constraint::Length(panel_h),
                Constraint::Length(1),
                Constraint::Length(8),
            ])
            .areas(area);
            if !players.is_empty() {
                render_panel(panel, players, board, turn, buf);
            }
            render_keybar(keys, turn, buf);

            let [chest, chance] =
                Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).areas(cards);
            render_card_slot(chest, "COMMUNITY CHEST", "\u{f187}", CHEST_GOLD, buf);
            render_card_slot(chance, "CHANCE", "?", CHANCE_ORANGE, buf);
        }
    }
}

/// A standings panel: one row per player with their cash and property count,
/// the current player highlighted. Bankrupt players are dimmed.
fn render_panel(area: Rect, players: &[Player], board: &[Space], turn: usize, buf: &mut Buffer) {
    let area = centered_rect(area, 40, area.height.min(players.len() as u16 + 2));
    let block = Block::bordered()
        .title_top(Line::from(" Players ").centered())
        .style(Style::new().bg(Color::Black).fg(Color::White));
    let inner = block.inner(area);
    Clear.render(area, buf);
    block.render(area, buf);

    let lines: Vec<Line> = players
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let owned = board.iter().filter(|s| s.owner() == Some(i)).count();
            let tag = if p.bankrupt {
                " (out)"
            } else if p.in_jail {
                " [jail]"
            } else {
                ""
            };
            let text = format!(" {} P{}  ${}  {owned} props{tag} ", p.piece.icon(), i + 1, p.money);
            let line = Line::from(text);
            if p.bankrupt {
                line.style(Style::new().fg(Color::DarkGray))
            } else if i == turn {
                line.reversed()
            } else {
                line
            }
        })
        .collect();
    Paragraph::new(lines).render(inner, buf);
}

/// Thin keybind bar showing whose turn it is and the global keys.
fn render_keybar(area: Rect, turn: usize, buf: &mut Buffer) {
    let text = format!("Player {}'s turn   ·   m Menu   ·   Space Roll   ·   q Quit", turn + 1);
    Paragraph::new(Line::from(text).centered())
        .style(Style::new().bg(TITLE_RED).fg(Color::White).bold())
        .render(area, buf);
}

/// Red menu bar with the main-menu options; the selected row is highlighted.
fn render_menu_bar(area: Rect, selected: usize, buf: &mut Buffer) {
    let area = centered_rect(area, 30, OPTIONS.len() as u16 + 2);
    let block = Block::bordered().style(Style::new().bg(TITLE_RED).fg(Color::White).bold());
    let inner = block.inner(area);
    block.render(area, buf);
    Paragraph::new(selectable_lines(&OPTIONS, selected)).render(inner, buf);
}

/// Width of the big "MONOPOLY" title: 8 glyphs * 8 pixel columns.
const BIG_TITLE_W: u16 = 8 * 8;

/// Styled "terminal too small" screen, on the green board background. Shows the
/// big title when there's room, the bold size message, and a bottom banner.
pub fn render_warning(area: Rect, buf: &mut Buffer) {
    Block::new().style(Style::new().bg(BOARD_BG)).render(area, buf);

    let [body, banner] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);

    let message = Paragraph::new(format!(
        "Terminal too small\nNeed at least {BOARD_W} x {BOARD_H}, have {} x {}",
        area.width, area.height
    ))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true })
    .style(Style::new().fg(Color::Black).bold());

    if body.width >= BIG_TITLE_W && body.height >= 12 {
        let [title, _, msg] =
            Layout::vertical([Constraint::Length(8), Constraint::Length(1), Constraint::Length(3)])
                .flex(Flex::Center)
                .areas(body);
        render_title(title, buf);
        message.render(msg, buf);
    } else {
        let [msg] = Layout::vertical([Constraint::Length(4)]).flex(Flex::Center).areas(body);
        message.render(msg, buf);
    }

    Paragraph::new(Line::from("resize the terminal  ·  press q to quit").centered())
        .style(Style::new().fg(Color::White).bg(Color::Black).bold())
        .render(banner, buf);
}

/// "MONOPOLY" in big block glyphs, upscaled to fill `area` and centered.
///
/// `tui-big-text`'s largest size is 8x8 cells per glyph, so to go bigger we
/// render it once into a scratch buffer, then copy each cell as a `scale`x`scale`
/// block. `scale` auto-fits the area (1x on the cramped warning screen, 2x+ on
/// the board).
fn render_title(area: Rect, buf: &mut Buffer) {
    const TW: u16 = 8 * 8; // "MONOPOLY" = 8 glyphs of 8 columns
    const TH: u16 = 8; // glyph height
    const MAX_SCALE: u16 = 2;

    let scale = (area.width / TW).min(area.height / TH).clamp(1, MAX_SCALE);

    // Render the title at base size into a scratch buffer.
    let rect = Rect::new(0, 0, TW, TH);
    let mut scratch = Buffer::empty(rect);
    BigText::builder()
        .pixel_size(PixelSize::Full)
        .lines(vec!["MONOPOLY".into()])
        .style(Style::new().fg(TITLE_RED).bold())
        .build()
        .render(rect, &mut scratch);

    // Blit each scratch cell as a scale x scale block, centered in `area`.
    let (dw, dh) = (TW * scale, TH * scale);
    let x0 = area.x + area.width.saturating_sub(dw) / 2;
    let y0 = area.y + area.height.saturating_sub(dh) / 2;
    for y in 0..TH {
        for x in 0..TW {
            let Some(src) = scratch.cell((x, y)).cloned() else {
                continue;
            };
            // Skip blank cells so the green board shows through behind the text.
            if src.symbol() == " " || src.symbol().is_empty() {
                continue;
            }
            for dy in 0..scale {
                for dx in 0..scale {
                    let pos = (x0 + x * scale + dx, y0 + y * scale + dy);
                    if let Some(dest) = buf.cell_mut(pos) {
                        *dest = src.clone();
                    }
                }
            }
        }
    }
}

/// A bordered card pile with a centered label and icon.
fn render_card_slot(area: Rect, label: &str, icon: &str, color: Color, buf: &mut Buffer) {
    let area = centered_rect(area, 40, 7);
    let block = Block::bordered()
        .title_top(Line::from(label).centered())
        .style(Style::new().fg(color).bold());

    let inner = block.inner(area);
    block.render(area, buf);

    let [icon_row] = Layout::vertical([Constraint::Length(1)]).flex(Flex::Center).areas(inner);
    Paragraph::new(Line::from(icon).centered())
        .style(Style::new().fg(color).bold())
        .render(icon_row, buf);
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
fn render_space(space: &Space, area: Rect, bg: Color, owner_icon: Option<&str>, buf: &mut Buffer) {
    let mut block = Block::bordered()
        .style(Style::new().bg(bg).fg(Color::Black).bold())
        .title_top(Line::from(short_name(space, area.width)).centered());

    let detail = detail_line(space, owner_icon);
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
            Block::new().style(Style::new().bg(p.group.color())).render(banner, buf);
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

/// Name trimmed to the inner width (cell width minus the two borders), cut on a
/// char boundary so it can borrow instead of allocating every frame.
fn short_name(space: &Space, cell_width: u16) -> &str {
    let max = cell_width.saturating_sub(2) as usize;
    let name = space.name();
    let end = name.char_indices().nth(max).map_or(name.len(), |(i, _)| i);
    &name[..end]
}

/// Owner (with their token) if bought, else price; "-$amount" for tax; blank
/// otherwise.
fn detail_line(space: &Space, owner_icon: Option<&str>) -> String {
    if space.is_ownable() {
        return match space.owner() {
            Some(player) => format!("P{} {}", player + 1, owner_icon.unwrap_or("")),
            None => format!("${}", space.price().unwrap_or(0)),
        };
    }
    match space {
        Space::Tax(amount) => format!("-${amount}"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::board;
    use crate::space::ColorGroup;

    const RING: usize = SIZE;

    #[test]
    fn the_ring_maps_the_corners_clockwise_from_go() {
        let last = RING - 1;
        assert_eq!(ring_index(last, last, RING, RING), Some(0), "GO, bottom-right");
        assert_eq!(ring_index(last, 0, RING, RING), Some(10), "Jail, bottom-left");
        assert_eq!(ring_index(0, 0, RING, RING), Some(20), "Free Parking, top-left");
        assert_eq!(ring_index(0, last, RING, RING), Some(30), "Go To Jail, top-right");
    }

    #[test]
    fn the_interior_is_not_part_of_the_ring() {
        for r in 1..RING - 1 {
            for c in 1..RING - 1 {
                assert_eq!(ring_index(r, c, RING, RING), None);
            }
        }
    }

    #[test]
    fn the_ring_covers_every_board_index_exactly_once() {
        let mut seen = vec![0u8; board().len()];
        for r in 0..RING {
            for c in 0..RING {
                if let Some(i) = ring_index(r, c, RING, RING) {
                    seen[i] += 1;
                }
            }
        }
        assert!(seen.iter().all(|&n| n == 1), "every space is drawn once: {seen:?}");
    }

    #[test]
    fn a_short_name_is_left_untouched() {
        let space = Space::Go;
        assert_eq!(short_name(&space, 40), "GO");
    }

    #[test]
    fn a_long_name_is_cut_to_the_inner_width() {
        let space = Space::FreeParking;
        assert_eq!(short_name(&space, 8), "Free P", "8 columns less two borders");
        assert_eq!(short_name(&space, 2), "");
    }

    #[test]
    fn trimming_never_splits_a_character() {
        let space = Space::street("Café Münster", ColorGroup::Brown, 60, [1, 2, 3, 4, 5, 6], 50);
        for width in 0..20u16 {
            let cut = short_name(&space, width);
            assert!(space.name().starts_with(cut));
            assert!(cut.chars().count() <= width.saturating_sub(2) as usize);
        }
    }

    #[test]
    fn the_detail_line_shows_the_price_until_it_is_bought() {
        let mut space =
            Space::street("Boardwalk", ColorGroup::DarkBlue, 400, [1, 2, 3, 4, 5, 6], 200);
        assert_eq!(detail_line(&space, None), "$400");
        space.set_owner(Some(0));
        assert_eq!(detail_line(&space, Some("X")), "P1 X");
    }

    #[test]
    fn the_detail_line_shows_taxes_and_nothing_else() {
        assert_eq!(detail_line(&Space::Tax(200), None), "-$200");
        assert_eq!(detail_line(&Space::FreeParking, None), "");
    }
}
