use tokio::sync::mpsc;
use uuid::Uuid;

use crate::client::ClientData;
use crate::game_mode::GameMode;

/// Messages sent to the lobby coordinator
#[derive(Debug)]
pub enum CoordinatorMessage {
    /// A client wants to create a new lobby
    CreateLobby {
        client_id: Uuid,
        ruleset: String,
        game_mode: GameMode,
        response_tx: mpsc::UnboundedSender<String>,
        client_data: ClientData,
    },
    /// A client wants to join an existing lobby
    JoinLobby {
        client_id: Uuid,
        lobby_code: String,
        response_tx: mpsc::UnboundedSender<String>,
        client_data: ClientData,
    },
    /// Route a message to a specific lobby
    RouteToLobby {
        lobby_code: String,
        message: LobbyMessage,
    },

    LeaveLobby {
        client_id: Uuid,
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
    PlayerJoined {
        player_id: Uuid,
        response_tx: mpsc::UnboundedSender<String>,
        client_data: ClientData,
    },
    /// A player left the lobby
    PlayerLeft { player_id: Uuid },
    /// Get lobby info
    GetInfo {
        player_id: Uuid,
        response_tx: mpsc::UnboundedSender<String>,
    },
}
