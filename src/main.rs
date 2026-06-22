use color_eyre::eyre::Result;
use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{Clear, ClearType};
use ratatui::layout::Alignment;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use std::io::stdout;

mod board;
mod map;
mod player;
mod setup;
mod space;

use crate::map::{BOARD_H, BOARD_W, Map};
use crate::player::Player;
use crate::setup::Setup;

/// Top-level screen: configuring a new game, or playing it.
enum App {
    Setup(Setup),
    #[allow(dead_code)] // players used once game logic lands
    Playing { players: Vec<Player> },
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
    let mut app = App::Setup(Setup::new());

    loop {
        terminal.draw(|frame| render(frame, &app))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
            break;
        }
        if let App::Setup(setup) = &mut app {
            if let Some(players) = setup.handle_key(key.code) {
                app = App::Playing { players };
            }
        }
    }
    Ok(())
}

fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < BOARD_W || area.height < BOARD_H {
        let msg = format!(
            "Terminal too small.\nNeed at least {BOARD_W}x{BOARD_H}, have {}x{}.\nResize, or press q to quit.",
            area.width, area.height
        );
        let warning = Paragraph::new(msg)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        frame.render_widget(warning, area);
        return;
    }

    frame.render_widget(Map::default(), area); // board centers itself
    if let App::Setup(setup) = app {
        setup.render(frame);
    }
}
