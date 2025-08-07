use crate::{client::ClientProfile, talisman_number::TalismanNumber};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct ClientLobbyState {
    pub current_lobby: Option<String>,
    pub is_ready: bool,
    pub first_ready: bool,
    pub is_cached: bool,
    pub is_host: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientGameState {
    pub ante: u32,
    pub round: u32,
    pub furthest_blind: u32,
    pub hands_left: u8,
    pub hands_max: u8,
    pub discards_left: u8,
    pub discards_max: u8,
    pub lives: u8,
    pub lives_blocker: bool,
    pub location: String,
    pub skips: u8,
    pub score: TalismanNumber,
    pub highest_score: TalismanNumber,
    pub spent_in_shop: Vec<u32>,
    pub team: u8
}

impl Default for ClientGameState {
    fn default() -> Self {
        Self {
            ante: 0,
            round: 1,
            furthest_blind: 1,
            hands_left: 4,
            hands_max: 4,
            discards_left: 3,
            discards_max: 3,
            lives: 2,
            lives_blocker: false,
            location: String::from("loc_selecting"),
            skips: 0,
            score: TalismanNumber::Regular(0.0),
            highest_score: TalismanNumber::Regular(0.0),
            spent_in_shop: Vec::new(),
            team: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientLobbyEntry {
    pub profile: ClientProfile,
    pub lobby_state: ClientLobbyState,
    pub game_state: ClientGameState,
}

impl ClientLobbyEntry {
    // DRY: Centralized player creation logic
    pub fn new(profile: ClientProfile, lobby_code: String, is_host: bool, starting_lives: u8) -> Self {
        let mut game_state = ClientGameState::default();
        game_state.lives = starting_lives;

        Self {
            profile,
            lobby_state: ClientLobbyState {
                current_lobby: Some(lobby_code),
                is_ready: is_host, // Host starts ready
                first_ready: false,
                is_cached: false,
                is_host,
            },
            game_state,
        }
    }

    pub fn reset_for_game(&mut self, starting_lives: u8) {
        self.lobby_state.is_ready = false;
        self.game_state = ClientGameState::default();
        self.game_state.lives = starting_lives;
    }
}
