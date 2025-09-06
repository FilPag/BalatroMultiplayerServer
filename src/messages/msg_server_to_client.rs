use std::collections::HashMap;

use serde::Serialize;

use crate::{game_mode::LobbyOptions, lobby::{lobby::Lobby, ClientGameState, ClientLobbyEntry}};

// Server to Client Actions
#[derive(Serialize, Debug, Clone)]
#[serde(tag = "action")]
pub enum ServerToClient {
    // Connection responses
    #[serde(rename = "connected")]
    Connected { client_id: String },
    #[serde(rename = "a")]
    KeepAliveResponse {},
    #[serde(rename = "versionOk")]
    VersionOk {},
    #[serde(rename = "error")]
    Error { message: String },

    // Lobby responses
    #[serde(rename = "joinedLobby")]
    JoinedLobby {
        player_id: String,
        lobby_data: Lobby, // Using Value to avoid circular dependency
    },
    #[serde(rename = "playerJoinedLobby")]
    PlayerJoinedLobby { player: ClientLobbyEntry },
    #[serde(rename = "playerLeftLobby")]
    PlayerLeftLobby { player_id: String, host_id: String },

    #[serde(rename = "updateLobbyOptions")]
    UpdateLobbyOptions { options: LobbyOptions },

    #[serde(rename = "gameStarted")]
    GameStarted { seed: String, stake: i32 },

    #[serde(rename = "startBlind")]
    StartBlind {},

    #[serde(rename = "gameStopped")]
    GameStopped {},

    #[serde(rename = "loseGame")]
    LoseGame {},

    #[serde(rename = "winGame")]
    WinGame {},

    #[serde(rename = "receivePlayerJokers")]
    ReceivePlayerJokers { player_id: String, jokers: String },

    #[serde(rename = "receivePlayerDeck")]
    ReceivePlayerDeck { player_id: String, deck: String },

    #[serde(rename = "setBossBlind")]
    SetBossBlind { key: String },

    #[serde(rename = "endPvp")]
    EndPvp { won: bool },

    #[serde(rename = "gameStateUpdate")]
    GameStateUpdate {
        player_id: String,
        game_state: ClientGameState,
    },

    #[serde(rename = "resetPlayers")]
    ResetPlayers { players: Vec<ClientLobbyEntry> },

    #[serde(rename = "lobbyReady")]
    LobbyReady { ready_states: HashMap<String, bool> },

    #[serde(rename = "inGameStatuses")]
    InGameStatuses { statuses: HashMap<String, bool> },

    // Multiplayer joker responses
    #[serde(rename = "sendPhantom")]
    SendPhantom { key: String },

    #[serde(rename = "removePhantom")]
    RemovePhantom { key: String },

    #[serde(rename = "asteroid")]
    Asteroid {
        sender: String,
    },

    #[serde(rename = "letsGoGamblingNemesis")]
    LetsGoGamblingNemesis {},

    #[serde(rename = "eatPizza")]
    EatPizza { discards: u8 },

    #[serde(rename = "soldJoker")]
    SoldJoker {},

    #[serde(rename = "spentLastShop")]
    SpentLastShop { player_id: String, amount: u32 },

    #[serde(rename = "startAnteTimer")]
    StartAnteTimer { time: u32 },
    #[serde(rename = "pauseAnteTimer")]
    PauseAnteTimer { time: u32 },

    #[serde(rename = "magnet")]
    Magnet {},

    #[serde(rename = "magnetResponse")]
    MagnetResponse { key: String },

    #[serde(rename = "receivedMoney")]
    ReceivedMoney {},
}

impl ServerToClient {
    // Simple, safe JSON conversion - no unwrapping!
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"action":"error","message":"Serialization failed"}"#.to_string()
        })
    }

    // MessagePack conversion
    pub fn to_msgpack(&self) -> Vec<u8> {
        rmp_serde::to_vec_named(self).unwrap_or_else(|_| {
            // Fallback error message in MessagePack format
            let error_response = ServerToClient::Error {
                message: "Serialization failed".to_string(),
            };
            rmp_serde::to_vec_named(&error_response).unwrap_or_default()
        })
    }

    // Helper constructors for common responses
    pub fn connected(client_id: String) -> Self {
        Self::Connected {
            client_id: client_id,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }

    pub fn joined_lobby(player_id: String, lobby_data: Lobby) -> Self {
        Self::JoinedLobby {
            player_id,
            lobby_data,
        }
    }

    pub fn player_joined_lobby(player: ClientLobbyEntry) -> Self {
        Self::PlayerJoinedLobby { player }
    }

    pub fn player_left_lobby(player_id: String, host_id: String) -> Self {
        Self::PlayerLeftLobby {
            player_id,
            host_id: host_id,
        }
    }
}
