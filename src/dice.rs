//! Dice-roll popup: plays the dice GIF as ASCII, then shows two random dice
//! and their total.

use std::io::Cursor;

use image::AnimationDecoder;
use image::codecs::gif::GifDecoder;
use image::imageops::FilterType;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Clear, Paragraph},
};

/// ASCII output size of the GIF frames (cols x rows).
const FRAME_COLS: u16 = 48;
const FRAME_ROWS: u16 = 18;
/// Luminance ramp, dark to light.
const RAMP: &[u8] = b" .:-=+*#%@";

/// The dice GIF, decoded once into colored ASCII frames.
pub struct Animation {
    frames: Vec<Text<'static>>,
}

impl Animation {
    /// Decode the embedded GIF into ASCII frames. Done once per game.
    pub fn load() -> Self {
        let bytes = include_bytes!("../assets/dice.gif");
        let decoder = GifDecoder::new(Cursor::new(bytes.as_slice())).expect("decode dice.gif");
        let frames = decoder
            .into_frames()
            .collect_frames()
            .expect("read gif frames");
        let frames = frames.iter().map(|f| asciify(f.buffer())).collect();
        Self { frames }
    }

    fn len(&self) -> usize {
        self.frames.len()
    }
}

/// State of one in-progress roll.
pub struct Roll {
    frame: usize,
    result: Option<(u8, u8)>,
}

impl Roll {
    pub fn new() -> Self {
        Self {
            frame: 0,
            result: None,
        }
    }

    /// True while the GIF is still playing (no dice rolled yet).
    pub fn animating(&self) -> bool {
        self.result.is_none()
    }

    /// Advance one GIF frame; roll the dice once the GIF finishes.
    pub fn tick(&mut self, anim: &Animation) {
        if self.frame + 1 < anim.len() {
            self.frame += 1;
        } else if self.result.is_none() {
            self.result = Some((rand::random_range(1..=6), rand::random_range(1..=6)));
        }
    }
}

/// Draws the roll popup centered over the board.
pub fn render(frame: &mut Frame, anim: &Animation, roll: &Roll) {
    match roll.result {
        None => render_animation(frame, &anim.frames[roll.frame]),
        Some((a, b)) => render_result(frame, a, b),
    }
}

fn render_animation(frame: &mut Frame, ascii: &Text<'static>) {
    let area = popup(frame.area(), FRAME_COLS + 2, FRAME_ROWS + 2);
    let block = Block::bordered()
        .title(" Rolling ")
        .style(Style::new().bg(Color::Black));
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(ascii.clone()).centered(), inner);
}

fn render_result(frame: &mut Frame, a: u8, b: u8) {
    let area = popup(frame.area(), 24, 9);
    let block = Block::bordered()
        .title(" Roll ")
        .style(Style::new().bg(Color::Black).fg(Color::White).bold());
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    // Two boxed dice side by side, then the total.
    let (left, right) = (die_box(a), die_box(b));
    let mut lines: Vec<Line> = left
        .iter()
        .zip(&right)
        .map(|(l, r)| Line::from(format!("{l}   {r}")).centered())
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(format!("Total: {}", a + b)).centered().bold());
    frame.render_widget(Paragraph::new(lines).centered(), inner);
}

/// A die drawn as a 5-line bordered cube.
fn die_box(value: u8) -> Vec<String> {
    let mut lines = vec!["┌─────┐".to_string()];
    for r in 0..3 {
        lines.push(format!("│{}│", die_row(value, r)));
    }
    lines.push("└─────┘".to_string());
    lines
}

/// One row (0..3) of a die face as pips spaced out, e.g. "●   ●".
fn die_row(value: u8, row: u16) -> String {
    let pips = pips(value)[row as usize];
    let cells: Vec<&str> = pips.iter().map(|&on| if on { "●" } else { " " }).collect();
    cells.join(" ")
}

/// Pip layout for a die value on a 3x3 grid (true = filled).
fn pips(value: u8) -> [[bool; 3]; 3] {
    let mut grid = [[false; 3]; 3];
    let cells: &[(usize, usize)] = match value {
        1 => &[(1, 1)],
        2 => &[(0, 0), (2, 2)],
        3 => &[(0, 0), (1, 1), (2, 2)],
        4 => &[(0, 0), (0, 2), (2, 0), (2, 2)],
        5 => &[(0, 0), (0, 2), (1, 1), (2, 0), (2, 2)],
        _ => &[(0, 0), (0, 2), (1, 0), (1, 2), (2, 0), (2, 2)],
    };
    for &(r, c) in cells {
        grid[r][c] = true;
    }
    grid
}

/// Downscale one GIF frame and map it to colored ASCII.
fn asciify(rgba: &image::RgbaImage) -> Text<'static> {
    let small = image::imageops::resize(
        rgba,
        FRAME_COLS as u32,
        FRAME_ROWS as u32,
        FilterType::Triangle,
    );
    let mut lines = Vec::with_capacity(FRAME_ROWS as usize);
    for y in 0..FRAME_ROWS as u32 {
        let mut spans = Vec::with_capacity(FRAME_COLS as usize);
        for x in 0..FRAME_COLS as u32 {
            let [r, g, b, alpha] = small.get_pixel(x, y).0;
            if alpha < 128 {
                spans.push(Span::raw(" "));
                continue;
            }
            let lum = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
            let idx = (lum / 255.0 * (RAMP.len() - 1) as f32) as usize;
            let ch = RAMP[idx] as char;
            spans.push(Span::styled(ch.to_string(), Style::new().fg(Color::Rgb(r, g, b))));
        }
        lines.push(Line::from(spans));
    }
    Text::from(lines)
}

/// Centers a `width` x `height` popup in `area`.
fn popup(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    area
}
