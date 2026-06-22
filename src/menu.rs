//! Main menu shown on the board before a game starts.

use crossterm::event::KeyCode;

/// Menu entries, in display order.
pub const OPTIONS: [&str; 1] = ["Start New Game"];

/// What a key press asked the menu to do.
pub enum MenuAction {
    None,
    NewGame,
    Quit,
}

pub struct Menu {
    pub selected: usize,
}

impl Menu {
    pub fn new() -> Self {
        Self { selected: 0 }
    }

    pub fn handle_key(&mut self, key: KeyCode) -> MenuAction {
        match key {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(OPTIONS.len() - 1),
            KeyCode::Enter => {
                if OPTIONS[self.selected] == "Start New Game" {
                    return MenuAction::NewGame;
                }
            }
            KeyCode::Esc => return MenuAction::Quit,
            _ => {}
        }
        MenuAction::None
    }
}
