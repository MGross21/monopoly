use color_eyre::eyre::Result;
use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{Clear, ClearType};
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::{DefaultTerminal, Frame};
use std::io::stdout;
use std::time::Duration;

mod board;
mod dice;
mod map;
mod menu;
mod player;
mod setup;
mod space;

use crate::dice::{Animation, Roll};
use crate::map::{BOARD_BG, BOARD_H, BOARD_W, Map, Overlay, render_warning};
use crate::menu::{Menu, MenuAction};
use crate::player::Player;
use crate::setup::Setup;

/// Time each GIF frame is shown during a roll.
const FRAME_TIME: Duration = Duration::from_millis(40);

/// Top-level screen.
enum App {
    Menu(Menu),
    Setup(Setup),
    Playing(Game),
}

/// An in-progress game.
struct Game {
    players: Vec<Player>,
    anim: Animation,
    roll: Option<Roll>,
}

impl Game {
    fn new(players: Vec<Player>) -> Self {
        Self {
            players,
            anim: Animation::load(),
            roll: None,
        }
    }

    /// True while a roll's GIF is still playing.
    fn animating(&self) -> bool {
        self.roll.as_ref().is_some_and(Roll::animating)
    }

    /// Advance the current roll's animation by one frame.
    fn tick(&mut self) {
        if let Some(roll) = &mut self.roll {
            roll.tick(&self.anim);
        }
    }

    /// Space starts a roll, or dismisses a finished one.
    fn on_space(&mut self) {
        match &self.roll {
            None => self.roll = Some(Roll::new()),
            Some(roll) if !roll.animating() => self.roll = None,
            Some(_) => {} // mid-animation, ignore
        }
    }
}

/// How a key press changes the current screen. Computed while `app` is borrowed,
/// then applied afterwards so we can reassign `app` cleanly.
enum Transition {
    Stay,
    ToMenu,
    ToSetup,
    ToPlaying(Vec<Player>),
    Quit,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    // `init` enables raw mode + alternate screen so key presses register and
    // the board draws on its own screen; `restore` undoes it on the way out.
    let mut terminal = ratatui::init();
    let result = run(&mut terminal);
    ratatui::restore();
    // Wipe the restored main screen so we don't leave the old scrollback behind.
    execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
    result
}

fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    terminal.clear()?;
    let mut app = App::Menu(Menu::new());

    loop {
        terminal.draw(|frame| render(frame, &app))?;

        // While a roll is animating, wake on a timer to advance frames; when
        // idle, block until the next key so we don't spin.
        let animating = matches!(&app, App::Playing(g) if g.animating());
        if animating && !event::poll(FRAME_TIME)? {
            if let App::Playing(g) = &mut app {
                g.tick();
            }
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if key.code == KeyCode::Char('q') {
            break;
        }

        let transition = match &mut app {
            App::Menu(m) => match m.handle_key(key.code) {
                MenuAction::NewGame => Transition::ToSetup,
                MenuAction::Quit => Transition::Quit,
                MenuAction::None => Transition::Stay,
            },
            App::Setup(s) => {
                if key.code == KeyCode::Esc {
                    Transition::ToMenu
                } else if let Some(players) = s.handle_key(key.code) {
                    Transition::ToPlaying(players)
                } else {
                    Transition::Stay
                }
            }
            App::Playing(g) => {
                if key.code == KeyCode::Char(' ') {
                    g.on_space();
                }
                Transition::Stay
            }
        };

        match transition {
            Transition::Stay => {}
            Transition::ToMenu => app = App::Menu(Menu::new()),
            Transition::ToSetup => app = App::Setup(Setup::new()),
            Transition::ToPlaying(players) => app = App::Playing(Game::new(players)),
            Transition::Quit => break,
        }
    }
    Ok(())
}

fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < BOARD_W || area.height < BOARD_H {
        render_warning(area, frame.buffer_mut());
        return;
    }

    // Green table fills the whole screen, behind and around the board.
    frame.render_widget(Block::new().style(Style::new().bg(BOARD_BG)), area);

    let map = match app {
        App::Menu(m) => Map::new(Vec::new(), Overlay::Menu { selected: m.selected }),
        App::Setup(_) => Map::new(Vec::new(), Overlay::Board),
        App::Playing(g) => Map::new(g.players.clone(), Overlay::Board),
    };
    frame.render_widget(map, area);

    match app {
        App::Setup(setup) => setup.render(frame),
        App::Playing(g) => {
            if let Some(roll) = &g.roll {
                dice::render(frame, &g.anim, roll);
            }
        }
        App::Menu(_) => {}
    }
}
