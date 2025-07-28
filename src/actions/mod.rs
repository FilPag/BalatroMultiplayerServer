use crate::game_mode::{GameMode, LobbyOptions};
use crate::lobby::ClientLobbyEntry;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// Client to Server Actions
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "action")]
pub enum ClientToServer {
    // Connection actions
    #[serde(rename = "k")]
    KeepAlive {},
    #[serde(rename = "version")]
    Version { version: String },
    #[serde(rename = "setClientData")]
    SetClientData {
        username: String,
        colour: u8,
        mod_hash: String,
    },

    // Lobby actions
    #[serde(rename = "createLobby")]
    CreateLobby {
        ruleset: String,
        #[serde(rename = "gameMode")]
        game_mode: GameMode,
    },
    #[serde(rename = "joinLobby")]
    JoinLobby { code: String },
    #[serde(rename = "leaveLobby")]
    LeaveLobby {},
    
    #[serde(rename = "updateLobbyOptions")]
    UpdateLobbyOptions{
        options: LobbyOptions,
    },
    
    // Game actions (for future expansion)
    #[serde(rename = "setReady")]
    SetReady { is_ready: bool },
    #[serde(rename = "updateGameState")]
    UpdateGameState {
        ante: Option<u32>,
        furthest_blind: Option<u32>,
        hands_left: Option<u32>,
        hands_max: Option<u32>,
        discards_left: Option<u32>,
        discards_max: Option<u32>,
        lives: Option<u32>,
        location: Option<String>,
        score: Option<u64>,
    },
}

// Server to Client Actions
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "action")]
pub enum ServerToClient {
    // Connection responses
    #[serde(rename = "connected")]
    Connected {
        #[serde(rename = "clientId")]
        client_id: String,
    },
    #[serde(rename = "a")]
    KeepAliveResponse {},
    #[serde(rename = "versionOk")]
    VersionOk {},
    #[serde(rename = "error")]
    Error { message: String },

    // Lobby responses
    #[serde(rename = "joinedLobby")]
    JoinedLobby {
        player_id: Uuid,
        lobby_data: serde_json::Value, // Using Value to avoid circular dependency
    },
    #[serde(rename = "playerJoinedLobby")]
    PlayerJoinedLobby {
        player: ClientLobbyEntry,
    },
    #[serde(rename = "playerLeftLobby")]
    PlayerLeftLobby {
        player_id: Uuid,
        host_id: Option<Uuid>,
    },
    #[serde(rename = "updateLobbyOptions")]
    UpdateLobbyOptions {
        options: LobbyOptions,
    },

    #[serde(rename = "gameStateUpdate")]
    GameStateUpdate {
        #[serde(rename = "player_id")]
        player_id: Uuid,
        #[serde(rename = "gameState")]
        game_state: serde_json::Value,
    },
    #[serde(rename = "playerReady")]
    PlayerReady {
        player_id: Uuid,
        is_ready: bool,
    },
}

impl ServerToClient {
    // Simple, safe JSON conversion - no unwrapping!
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| r#"{"action":"error","message":"Serialization failed"}"#.to_string())
    }

    // Helper constructors for common responses
    pub fn connected(client_id: Uuid) -> Self {
        Self::Connected {
            client_id: client_id.to_string(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }

    pub fn joined_lobby(player_id: Uuid, lobby_data: serde_json::Value) -> Self {
        Self::JoinedLobby {
            player_id,
            lobby_data,
        }
    }

    pub fn player_joined_lobby(player: ClientLobbyEntry) -> Self {
        Self::PlayerJoinedLobby { player }
    }

    pub fn player_left_lobby(player_id: Uuid, host_id: Option<Uuid>) -> Self {
        Self::PlayerLeftLobby { player_id, host_id }
    }
}