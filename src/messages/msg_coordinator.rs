use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

use crate::{
    client::ClientProfile,
    game_mode::GameMode,
    messages::{LobbyJoinData, ServerToClient},
};

#[derive(Debug)]
pub enum CoordinatorMessage {
    /// A client wants to create a new lobby
    CreateLobby {
        client_id: String,
        ruleset: String,
        game_mode: GameMode,
        request_tx: oneshot::Sender<LobbyJoinData>,
        client_response_tx: mpsc::UnboundedSender<Arc<ServerToClient>>,
        client_profile: ClientProfile,
    },
    /// A client wants to join an existing lobby
    JoinLobby {
        client_id: String,
        lobby_code: String,
        request_tx: oneshot::Sender<LobbyJoinData>,
        client_response_tx: mpsc::UnboundedSender<Arc<ServerToClient>>,
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
