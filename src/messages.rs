use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::client::{ClientProfile};
use crate::game_mode::GameMode;

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

    LobbyShutdown{
        lobby_code: String,
    },

    /// Client disconnected, clean up from any lobby
    ClientDisconnected {
        client_id: Uuid,
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

    PlayerJoined {
        player_id: Uuid,
        client_response_tx: mpsc::UnboundedSender<String>,
        client_profile: ClientProfile,
    },
    /// A player left the lobby
    LeaveLobby{ player_id: Uuid, coordinator_tx: mpsc::UnboundedSender<CoordinatorMessage> },
    /// Get lobby info
    GetInfo {
        player_id: Uuid,
        response_tx: mpsc::UnboundedSender<String>,
    },
}
