//! Persistence: the serializable slice of a game, and reading/writing it.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Duration;

use ratatui_notifications::{Level, Notifications};
use serde::{Deserialize, Serialize};

use super::cards::fresh_decks;
use super::{Game, Modal, TOTAL_HOTELS, TOTAL_HOUSES};
use crate::player::Player;
use crate::space::Space;

/// Where a game is saved to / loaded from: the platform per-user data dir, e.g.
/// `~/.local/share/monopoly/save.json` on Linux. `None` if no such dir exists.
fn save_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("monopoly").join("save.json"))
}

/// The persistent slice of a game. Transient UI state (the active popup,
/// notifications, the clock) and the card decks are not saved — decks are
/// reshuffled on load since their text is `&'static`.
#[derive(Serialize, Deserialize)]
pub(super) struct Save {
    pub(super) players: Vec<Player>,
    pub(super) board: Vec<Space>,
    pub(super) current: usize,
    pub(super) doubles: u8,
    pub(super) can_roll: bool,
    pub(super) has_rolled: bool,
    // Defaulted so saves written before the bank tracked its stock still load.
    #[serde(default = "full_houses")]
    pub(super) houses_left: u8,
    #[serde(default = "full_hotels")]
    pub(super) hotels_left: u8,
}

fn full_houses() -> u8 {
    TOTAL_HOUSES
}

fn full_hotels() -> u8 {
    TOTAL_HOTELS
}

impl Game {
    /// Load the saved game, or `None` if there's no save or it can't be read.
    pub fn load() -> Option<Self> {
        let data = std::fs::read_to_string(save_path()?).ok()?;
        let save: Save = serde_json::from_str(&data).ok()?;
        Some(Self::from_save(save))
    }

    /// Build a game from saved state, with fresh transient UI and reshuffled
    /// decks.
    pub(super) fn from_save(save: Save) -> Self {
        let (chance, chest) = fresh_decks();
        Self {
            players: save.players,
            board: save.board,
            current: save.current,
            modal: Modal::None,
            notes: Notifications::new().max_concurrent(Some(4)),
            doubles: save.doubles,
            can_roll: save.can_roll,
            has_rolled: save.has_rolled,
            clock: Duration::ZERO,
            done: false,
            chance,
            chest,
            houses_left: save.houses_left,
            hotels_left: save.hotels_left,
            pending: VecDeque::new(),
        }
    }

    pub(super) fn snapshot(&self) -> Save {
        Save {
            players: self.players.clone(),
            board: self.board.clone(),
            current: self.current,
            doubles: self.doubles,
            can_roll: self.can_roll,
            has_rolled: self.has_rolled,
            houses_left: self.houses_left,
            hotels_left: self.hotels_left,
        }
    }

    /// Write the persistent state to the save path, creating the directory if
    /// needed, and toast success or failure.
    pub(super) fn save_game(&mut self) {
        let save = self.snapshot();
        let result = (|| -> Result<PathBuf, String> {
            let path = save_path().ok_or("no data directory")?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let json = serde_json::to_string_pretty(&save).map_err(|e| e.to_string())?;
            std::fs::write(&path, json).map_err(|e| e.to_string())?;
            Ok(path)
        })();
        match result {
            Ok(path) => self.notify(format!("Game saved to {}", path.display()), Level::Info),
            Err(e) => self.notify(format!("Save failed: {e}"), Level::Error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::testkit::*;
    use crate::space::ColorGroup;

    /// Round-trip through JSON without touching the real save file.
    fn reload(game: &Game) -> Game {
        let json = serde_json::to_string(&game.snapshot()).expect("serialize");
        Game::from_save(serde_json::from_str(&json).expect("deserialize"))
    }

    #[test]
    fn a_save_round_trips_through_json() {
        let mut g = game(2, 1500);
        own(&mut g, BOARDWALK, 1);
        set_houses(&mut g, MEDITERRANEAN, 3);
        g.players[0].in_jail = true;
        g.players[0].get_out_free = 1;
        g.current = 1;
        g.doubles = 2;

        let restored = reload(&g);
        assert_eq!(restored.current, 1);
        assert_eq!(restored.doubles, 2);
        assert_eq!(restored.board[BOARDWALK].owner(), Some(1));
        assert_eq!(restored.board[MEDITERRANEAN].houses(), 3);
        assert!(restored.players[0].in_jail);
        assert_eq!(restored.players[0].get_out_free, 1);
    }

    #[test]
    fn transient_ui_state_is_not_saved() {
        let mut g = game(2, 1500);
        g.show_inventory();
        let restored = reload(&g);
        assert!(matches!(restored.modal, Modal::None));
        assert!(restored.pending.is_empty());
        assert!(!restored.is_done());
    }

    #[test]
    fn the_banks_stock_survives_a_round_trip() {
        let mut g = game(2, 1500);
        own_group(&mut g, ColorGroup::Brown, 0);
        g.build_house(MEDITERRANEAN);
        g.build_house(BALTIC);
        let restored = reload(&g);
        assert_eq!(restored.houses_left, TOTAL_HOUSES - 2);
        assert_eq!(restored.hotels_left, TOTAL_HOTELS);
    }

    #[test]
    fn a_save_without_a_stock_field_defaults_to_a_full_bank() {
        // A save written before the bank tracked its stock.
        let json = serde_json::to_string(&game(2, 1500).snapshot()).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
        let mut map = value.as_object().expect("an object").clone();
        map.remove("houses_left");
        map.remove("hotels_left");

        let save: Save = serde_json::from_value(map.into()).expect("old saves still load");
        assert_eq!(save.houses_left, TOTAL_HOUSES);
        assert_eq!(save.hotels_left, TOTAL_HOTELS);
    }
}
