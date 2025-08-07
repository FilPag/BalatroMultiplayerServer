use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::client::ClientProfile;
use crate::game_mode::{GameMode, LobbyOptions};
use crate::talisman_number::TalismanNumber;

/// Messages sent to the lobby coordinator
#[derive(Debug)]
pub enum CoordinatorMessage {
    /// A client wants to create a new lobby
    CreateLobby {
        client_id: Uuid,
        ruleset: String,
        game_mode: GameMode,
        request_tx: oneshot::Sender<LobbyMessage>,
        client_response_tx: mpsc::UnboundedSender<String>,
        client_profile: ClientProfile,
    },
    /// A client wants to join an existing lobby
    JoinLobby {
        client_id: Uuid,
        lobby_code: String,
        request_tx: oneshot::Sender<LobbyMessage>,
        client_response_tx: mpsc::UnboundedSender<String>,
        client_profile: ClientProfile,
    },

    LobbyShutdown {
        lobby_code: String,
    },

    /// Client disconnected, clean up from any lobby
    ClientDisconnected {
        client_id: Uuid,
        coordinator_tx: mpsc::UnboundedSender<CoordinatorMessage>,
    },
}

/// Messages sent to individual lobby tasks
#[derive(Debug)]
pub enum LobbyMessage {
    /// A player joined this lobby
    LobbyJoinData {
        lobby_code: String,
        lobby_tx: mpsc::UnboundedSender<LobbyMessage>,
    },

    UpdateLobbyOptions {
        player_id: Uuid,
        options: LobbyOptions,
    },

    PlayerJoined {
        player_id: Uuid,
        client_response_tx: mpsc::UnboundedSender<String>,
        client_profile: ClientProfile,
    },
    /// A player left the lobby
    LeaveLobby {
        player_id: Uuid,
        coordinator_tx: mpsc::UnboundedSender<CoordinatorMessage>,
    },

    UpdateHandsAndDiscards {
        player_id: Uuid,
        hands_max: u8,
        discards_max: u8,
    },

    StartGame {
        player_id: Uuid,
        seed: String,
        stake: i32,
    },

    StopGame {
        player_id: Uuid,
    },

    Skip {
        player_id: Uuid,
        blind: u32,
    },

    SetBossBlind {
        player_id: Uuid,
        key: String,
        chips: TalismanNumber,
    },

    PlayHand {
        player_id: Uuid,
        score: TalismanNumber,
        hands_left: u8,
    },

    SetFurthestBlind {
        player_id: Uuid,
        blind: u32,
    },

    SendPlayerJokers {
        player_id: Uuid,
        jokers: String,
    },

    SendPlayerDeck {
        player_id: Uuid,
        deck: String,
    },

    FailRound {
        player_id: Uuid,
    },

    SetLocation {
        player_id: Uuid,
        location: String,
    },

    SetReady {
        player_id: Uuid,
        is_ready: bool,
    },

    // Multiplayer joker actions
    SendPhantom {
        player_id: Uuid,
        key: String,
    },

    RemovePhantom {
        player_id: Uuid,
        key: String,
    },

    Asteroid {
        player_id: Uuid,
    },

    LetsGoGamblingNemesis {
        player_id: Uuid,
    },

    EatPizza {
        player_id: Uuid,
        discards: u8,
    },

    SoldJoker {
        player_id: Uuid,
    },

    StartAnteTimer {
        player_id: Uuid,
        time: u32,
    },
    PauseAnteTimer {
        player_id: Uuid,
        time: u32,
    },

    FailTimer {
        player_id: Uuid,
    },

    SpentLastShop {
        player_id: Uuid,
        amount: u32,
    },

    Magnet {
        player_id: Uuid,
    },

    MagnetResponse {
        player_id: Uuid,
        key: String,
    },
}
