//! Dice-roll popup: plays the dice GIF as ASCII, then shows two random dice
//! and their total.

use std::io::Cursor;
use std::sync::OnceLock;
use std::time::Duration;

use image::AnimationDecoder;
use image::codecs::gif::GifDecoder;
use image::imageops::FilterType;
use ratatui::{
    Frame,
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Clear, Paragraph},
};

use crate::ui::centered_rect;

/// ASCII output size of the GIF frames (cols x rows).
const FRAME_COLS: u16 = 48;
const FRAME_ROWS: u16 = 18;
/// Luminance ramp, dark to light.
const RAMP: &[u8] = b" .:-=+*#%@";
/// How long each GIF frame is shown.
const FRAME_TIME: Duration = Duration::from_millis(40);

/// The dice GIF, decoded once and cached (decoding is slow).
pub fn animation() -> &'static Animation {
    static ANIM: OnceLock<Animation> = OnceLock::new();
    ANIM.get_or_init(|| Animation::load(include_bytes!("../../assets/dice.gif")))
}

/// The card-draw GIF (Chance / Community Chest), decoded once and cached.
pub fn card_animation() -> &'static Animation {
    static ANIM: OnceLock<Animation> = OnceLock::new();
    ANIM.get_or_init(|| Animation::load(include_bytes!("../../assets/moving_card.gif")))
}

/// A GIF decoded into colored ASCII frames.
pub struct Animation {
    frames: Vec<Text<'static>>,
}

impl Animation {
    /// Decode embedded GIF bytes into ASCII frames.
    fn load(bytes: &[u8]) -> Self {
        let decoder = GifDecoder::new(Cursor::new(bytes)).expect("decode gif");
        let frames = decoder.into_frames().collect_frames().expect("read gif frames");
        let frames = frames.iter().map(|f| asciify(f.buffer())).collect();
        Self { frames }
    }

    fn len(&self) -> usize {
        self.frames.len()
    }

    fn frame(&self, index: usize) -> &Text<'static> {
        &self.frames[index.min(self.frames.len() - 1)]
    }
}

/// Plays an `Animation` once, frame by frame.
pub struct Clip {
    frame: usize,
    elapsed: Duration,
}

impl Clip {
    pub fn new() -> Self {
        Self { frame: 0, elapsed: Duration::ZERO }
    }

    pub fn finished(&self, anim: &Animation) -> bool {
        self.frame + 1 >= anim.len()
    }

    pub fn tick(&mut self, anim: &Animation, delta: Duration) {
        if self.finished(anim) {
            return;
        }
        self.elapsed += delta;
        while self.elapsed >= FRAME_TIME {
            self.elapsed -= FRAME_TIME;
            if self.frame + 1 < anim.len() {
                self.frame += 1;
            } else {
                break;
            }
        }
    }
}

/// A dice roll: plays the GIF, then settles on two random values.
pub struct Roll {
    clip: Clip,
    result: Option<(u8, u8)>,
}

impl Roll {
    pub fn new() -> Self {
        Self { clip: Clip::new(), result: None }
    }

    /// True while the GIF is still playing (no dice rolled yet).
    pub fn animating(&self) -> bool {
        self.result.is_none()
    }

    /// The rolled dice, once the animation has finished.
    pub fn result(&self) -> Option<(u8, u8)> {
        self.result
    }

    /// Advance the GIF; roll the dice once it finishes.
    pub fn tick(&mut self, anim: &Animation, delta: Duration) {
        if self.result.is_some() {
            return;
        }
        self.clip.tick(anim, delta);
        if self.clip.finished(anim) {
            self.result = Some((rand::random_range(1..=6), rand::random_range(1..=6)));
        }
    }
}

/// Draws the roll popup centered over the board.
pub fn render(frame: &mut Frame, anim: &Animation, roll: &Roll) {
    match roll.result {
        None => render_animation(frame, anim.frame(roll.clip.frame), " Rolling "),
        Some((a, b)) => render_result(frame, a, b),
    }
}

/// Draws a playing clip (e.g. the card-draw animation) as a titled popup.
pub fn render_clip(frame: &mut Frame, anim: &Animation, clip: &Clip, title: &str) {
    render_animation(frame, anim.frame(clip.frame), title);
}

fn render_animation(frame: &mut Frame, ascii: &Text<'static>, title: &str) {
    let area = centered_rect(frame.area(), FRAME_COLS + 2, FRAME_ROWS + 2);
    let block = Block::bordered().title(title.to_string()).style(Style::new().bg(Color::Black));
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(ascii.clone()).centered(), inner);
}

fn render_result(frame: &mut Frame, a: u8, b: u8) {
    let area = centered_rect(frame.area(), 24, 9);
    let block = Block::bordered()
        .title(" Roll ")
        .style(Style::new().bg(Color::Black).fg(Color::White).bold());
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    // Two boxed dice side by side, then the total.
    let (left, right) = (die_box(a), die_box(b));
    let mut lines: Vec<Line> =
        left.iter().zip(&right).map(|(l, r)| Line::from(format!("{l}   {r}")).centered()).collect();
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
    // Nearest is far cheaper than bilinear and indistinguishable at this size.
    let small =
        image::imageops::resize(rgba, FRAME_COLS as u32, FRAME_ROWS as u32, FilterType::Nearest);
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
