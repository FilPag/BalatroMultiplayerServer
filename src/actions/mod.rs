use crate::game_mode::{GameMode, LobbyOptions};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(tag = "action")]
pub enum Action {
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
    #[serde(rename = "lobbyInfo")]
    LobbyInfo {},
}
