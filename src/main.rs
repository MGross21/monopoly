use color_eyre::eyre::Result;
use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{Clear, ClearType};
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::{DefaultTerminal, Frame};
use std::io::stdout;

mod board;
mod map;
mod menu;
mod player;
mod setup;
mod space;

use crate::map::{BOARD_BG, BOARD_H, BOARD_W, Map, Overlay, render_warning};
use crate::menu::{Menu, MenuAction};
use crate::player::Player;
use crate::setup::Setup;

/// Top-level screen.
enum App {
    Menu(Menu),
    Setup(Setup),
    Playing(Game),
}

/// An in-progress game.
struct Game {
    players: Vec<Player>,
}

impl Game {
    fn new(players: Vec<Player>) -> Self {
        Self { players }
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
            App::Playing(_) => Transition::Stay,
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

    if let App::Setup(setup) = app {
        setup.render(frame);
    }
}
