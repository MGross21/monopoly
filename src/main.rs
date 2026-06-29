use color_eyre::eyre::Result;
use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{Clear, ClearType};
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::{DefaultTerminal, Frame};
use std::io::stdout;
use std::time::{Duration, Instant};

mod board;
mod game;
mod player;
mod space;
mod ui;

use crate::board::board;
use crate::game::Game;
use crate::player::Player;
use crate::ui::map::{BOARD_BG, BOARD_H, BOARD_W, Map, Overlay, render_warning};
use crate::ui::menu::{Menu, MenuAction};
use crate::ui::setup::Setup;
use crate::ui::{Confirm, ConfirmResult};

/// How often to wake while a game has live animation/notifications.
const TICK: Duration = Duration::from_millis(33);

/// Top-level screen.
enum App {
    Menu(Menu),
    Setup(Setup),
    Playing(Box<Game>),
}

/// How a key press changes the current screen. Computed while `app` is borrowed,
/// then applied afterwards so we can reassign `app` cleanly.
enum Transition {
    Stay,
    ToMenu,
    ToSetup,
    ToPlaying(Vec<Player>),
    LoadGame(Box<Game>),
    Quit,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    // Decode the dice GIF off-thread while the user is in the menu, so neither
    // starting a game nor the first roll stutters.
    std::thread::spawn(|| {
        let _ = crate::ui::dice::animation();
    });
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
    let mut last = Instant::now();
    let mut quit: Option<Confirm> = None;

    loop {
        terminal.draw(|frame| render(frame, &mut app, quit.as_ref()))?;

        // Keep ticking while playing (the highlight breathes); otherwise block
        // until the next key so we don't spin.
        let timeout = match &app {
            App::Playing(_) => Some(TICK),
            _ => None,
        };
        let event = match timeout {
            Some(t) if event::poll(t)? => Some(event::read()?),
            Some(_) => None,       // timed out, no input this tick
            None => Some(event::read()?), // idle: block here until a key
        };

        // Measure elapsed time AFTER any blocking wait, so a long idle block
        // isn't charged to a roll that starts right after it.
        let now = Instant::now();
        let delta = now - last;
        last = now;
        if let App::Playing(g) = &mut app {
            g.tick(delta);
        }

        let Some(Event::Key(key)) = event else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        // Quit confirmation: q or Ctrl-C opens a Yes/No prompt.
        if let Some(confirm) = quit.as_mut() {
            match confirm.handle_key(key.code) {
                ConfirmResult::Pending => {}
                ConfirmResult::Yes => break,
                ConfirmResult::No => quit = None,
            }
            continue;
        }
        let ctrl_c = key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        if key.code == KeyCode::Char('q') || ctrl_c {
            quit = Some(Confirm::new());
            continue;
        }

        let transition = match &mut app {
            App::Menu(m) => match m.handle_key(key.code) {
                MenuAction::NewGame => Transition::ToSetup,
                MenuAction::LoadGame => match Game::load() {
                    Some(game) => Transition::LoadGame(Box::new(game)),
                    None => Transition::Stay, // no save (or unreadable); stay put
                },
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
                g.handle_key(key.code);
                if g.is_done() {
                    Transition::ToMenu
                } else {
                    Transition::Stay
                }
            }
        };

        match transition {
            Transition::Stay => {}
            Transition::ToMenu => app = App::Menu(Menu::new()),
            Transition::ToSetup => app = App::Setup(Setup::new()),
            Transition::ToPlaying(players) => app = App::Playing(Box::new(Game::new(players))),
            Transition::LoadGame(game) => app = App::Playing(game),
            Transition::Quit => break,
        }
    }
    Ok(())
}

fn render(frame: &mut Frame, app: &mut App, quit: Option<&Confirm>) {
    let area = frame.area();
    if area.width < BOARD_W || area.height < BOARD_H {
        render_warning(area, frame.buffer_mut());
        return;
    }

    // Green table fills the whole screen, behind and around the board.
    frame.render_widget(Block::new().style(Style::new().bg(BOARD_BG)), area);

    // `Map` borrows the board/players. The in-game screen lends its own; the
    // menu/setup screens have no players and lend a throwaway board built here.
    let empty: Vec<Player> = Vec::new();
    let board_owned; // only initialized for the menu/setup screens
    let (spaces, players, overlay) = match &*app {
        App::Menu(m) => {
            board_owned = board();
            (board_owned.as_slice(), empty.as_slice(), Overlay::Menu { selected: m.cursor.selected })
        }
        App::Setup(_) => {
            board_owned = board();
            (board_owned.as_slice(), empty.as_slice(), Overlay::Board { turn: 0, breath: 0.0 })
        }
        App::Playing(g) => (g.board.as_slice(), g.players.as_slice(), g.overlay()),
    };
    frame.render_widget(Map::new(spaces, players, overlay), area);

    match app {
        App::Setup(setup) => setup.render(frame),
        App::Playing(g) => g.render(frame),
        App::Menu(_) => {}
    }

    if let Some(confirm) = quit {
        confirm.render(frame, " Quit the game? ");
    }
}
