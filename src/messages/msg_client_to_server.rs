use serde::{Deserialize, Serialize};

use crate::{game_mode::{GameMode, LobbyOptions}, talisman_number::TalismanNumber};

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

    #[serde(rename = "failRound")]
    FailRound {},

    #[serde(rename = "sendPlayerDeck")]
    SendPlayerDeck { deck: String },

    #[serde(rename = "sendPlayerJokers")]
    SendPlayerJokers { jokers: String },

    #[serde(rename = "setFurthestBlind")]
    SetFurthestBlind { blind: u32 },

    #[serde(rename = "joinLobby")]
    JoinLobby { code: String },
    #[serde(rename = "leaveLobby")]
    LeaveLobby {},

    #[serde(rename = "updateLobbyOptions")]
    UpdateLobbyOptions { options: LobbyOptions },

    // Game actions (for future expansion)
    #[serde(rename = "setReady")]
    SetReady { is_ready: bool },

    #[serde(rename = "playHand")]
    PlayHand {
        score: TalismanNumber,
        hands_left: u8,
    },

    #[serde(rename = "discard")]
    Discard {},

    #[serde(rename = "setBossBlind")]
    SetBossBlind { key: String, chips: TalismanNumber },

    #[serde(rename = "skip")]
    Skip { blind: u32 },

    #[serde(rename = "setLocation")]
    SetLocation { location: String },

    #[serde(rename = "startGame")]
    StartGame { seed: String, stake: i32 },

    #[serde(rename = "stopGame")]
    StopGame {},

    #[serde(rename = "updateHandsAndDiscards")]
    UpdateHandsAndDiscards { hands_max: u8, discards_max: u8 },

    // Multiplayer joker actions
    #[serde(rename = "sendPhantom")]
    SendPhantom { key: String },

    #[serde(rename = "removePhantom")]
    RemovePhantom { key: String },

    #[serde(rename = "asteroid")]
    Asteroid {
        target: String,
    },

    #[serde(rename = "letsGoGamblingNemesis")]
    LetsGoGamblingNemesis {},

    #[serde(rename = "eatPizza")]
    EatPizza { discards: u8 },

    #[serde(rename = "soldJoker")]
    SoldJoker {},

    #[serde(rename = "startAnteTimer")]
    StartAnteTimer { time: u32 },

    #[serde(rename = "pauseAnteTimer")]
    PauseAnteTimer { time: u32 },

    #[serde(rename = "failTimer")]
    FailTimer {},

    #[serde(rename = "spentLastShop")]
    SpentLastShop { amount: u32 },

    #[serde(rename = "magnet")]
    Magnet {},

    #[serde(rename = "magnetResponse")]
    MagnetResponse { key: String },

    #[serde(rename = "sendMoney")]
    SendMoney { player_id: String },

    #[serde(rename = "return_to_lobby")]
    ReturnToLobby {},

}
