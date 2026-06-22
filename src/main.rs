use ratatui::Frame;
use color_eyre::eyre::Result;
use ratatui::DefaultTerminal;
// use crossterm::event::{self, Event, KeyCode};
use ratatui::backend::CrosstermBackend;
use std::io::stdout;

mod map;
use crate::map::Map;

fn run(terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
    terminal.clear()?;
    loop {
        terminal.draw(|frame| render(frame))?;

    }
}

fn render(frame: &mut Frame) {
    frame.render_widget(Map::default(), frame.area());
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = DefaultTerminal::new(backend)?;
    run(&mut terminal)
}