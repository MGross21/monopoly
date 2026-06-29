//! Main menu shown on the board before a game starts.

use crossterm::event::KeyCode;

use crate::ui::Cursor;

/// Menu entries, in display order.
pub const OPTIONS: [&str; 2] = ["Start New Game", "Load Game"];

/// What a key press asked the menu to do.
pub enum MenuAction {
    None,
    NewGame,
    LoadGame,
    Quit,
}

pub struct Menu {
    pub cursor: Cursor,
}

impl Menu {
    pub fn new() -> Self {
        Self {
            cursor: Cursor::new(OPTIONS.len()),
        }
    }

    pub fn handle_key(&mut self, key: KeyCode) -> MenuAction {
        match key {
            KeyCode::Up => self.cursor.up(),
            KeyCode::Down => self.cursor.down(),
            KeyCode::Enter => {
                return match OPTIONS[self.cursor.selected] {
                    "Start New Game" => MenuAction::NewGame,
                    "Load Game" => MenuAction::LoadGame,
                    _ => MenuAction::None,
                };
            }
            KeyCode::Esc => return MenuAction::Quit,
            _ => {}
        }
        MenuAction::None
    }
}
