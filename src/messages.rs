use tokio::sync::{mpsc, oneshot};

use crate::actions::ServerToClient;
use crate::client::ClientProfile;
use crate::game_mode::{GameMode, LobbyOptions};
use crate::talisman_number::TalismanNumber;

/// Messages sent to the lobby coordinator
#[derive(Debug)]
pub enum CoordinatorMessage {
    /// A client wants to create a new lobby
    CreateLobby {
        client_id: String,
        ruleset: String,
        game_mode: GameMode,
        request_tx: oneshot::Sender<LobbyMessage>,
        client_response_tx: mpsc::UnboundedSender<ServerToClient>,
        client_profile: ClientProfile,
    },
    /// A client wants to join an existing lobby
    JoinLobby {
        client_id: String,
        lobby_code: String,
        request_tx: oneshot::Sender<LobbyMessage>,
        client_response_tx: mpsc::UnboundedSender<ServerToClient>,
        client_profile: ClientProfile,
    },

    LobbyShutdown {
        lobby_code: String,
    },

    /// Client disconnected, clean up from any lobby
    ClientDisconnected {
        client_id: String,
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
        player_id: String,
        options: LobbyOptions,
    },

    PlayerJoined {
        player_id: String,
        client_response_tx: mpsc::UnboundedSender<ServerToClient>,
        client_profile: ClientProfile,
    },
    /// A player left the lobby
    LeaveLobby {
        player_id: String,
        coordinator_tx: mpsc::UnboundedSender<CoordinatorMessage>,
    },

    UpdateHandsAndDiscards {
        player_id: String,
        hands_max: u8,
        discards_max: u8,
    },

    StartGame {
        player_id: String,
        seed: String,
        stake: i32,
    },

    StopGame {
        player_id: String,
    },

    Skip {
        player_id: String,
        blind: u32,
    },

    SetBossBlind {
        player_id: String,
        key: String,
        chips: TalismanNumber,
    },

    PlayHand {
        player_id: String,
        score: TalismanNumber,
        hands_left: u8,
    },

    SetFurthestBlind {
        player_id: String,
        blind: u32,
    },

    SendPlayerJokers {
        player_id: String,
        jokers: String,
    },

    SendPlayerDeck {
        player_id: String,
        deck: String,
    },

    FailRound {
        player_id: String,
    },

    SetLocation {
        player_id: String,
        location: String,
    },

    SetReady {
        player_id: String,
        is_ready: bool,
    },

    // Multiplayer joker actions
    SendPhantom {
        player_id: String,
        key: String,
    },

    RemovePhantom {
        player_id: String,
        key: String,
    },

    Asteroid {
        player_id: String,
        target: String,
    },

    LetsGoGamblingNemesis {
        player_id: String,
    },

    EatPizza {
        player_id: String,
        discards: u8,
    },

    SoldJoker {
        player_id: String,
    },

    StartAnteTimer {
        player_id: String,
        time: u32,
    },
    PauseAnteTimer {
        player_id: String,
        time: u32,
    },

    FailTimer {
        player_id: String,
    },

    SpentLastShop {
        player_id: String,
        amount: u32,
    },

    Magnet {
        player_id: String,
    },

    MagnetResponse {
        player_id: String,
        key: String,
    },

    SendMoney {
        from: String,
        to: String,
    },
}
