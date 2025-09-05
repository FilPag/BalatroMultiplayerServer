mod msg_client_to_server;
mod msg_coordinator;
mod msg_server_to_client;

use std::sync::Arc;
use tokio::sync::mpsc;

use crate::client::ClientProfile;

pub use self::msg_client_to_server::*;
pub use self::msg_coordinator::*;
pub use self::msg_server_to_client::*;

#[derive(Debug)]
pub enum LobbyMessage {
    // Regular client actions - easy to handle
    ClientAction {
        client_id: String,
        action: ClientToServer,
    },
    // Special events with all needed data upfront
    ClientJoin {
        client_id: String,
        client_profile: ClientProfile,
        client_response_tx: mpsc::UnboundedSender<Arc<ServerToClient>>,
    },
    ClientLeave {
        client_id: String,
        coordinator_tx: mpsc::UnboundedSender<CoordinatorMessage>,
    },
}
impl LobbyMessage {
    pub fn client_action(client_id: String, action: ClientToServer) -> Self {
        Self::ClientAction { client_id, action }
    }

    pub fn client_join(
        client_id: String,
        client_profile: ClientProfile,
        client_response_tx: mpsc::UnboundedSender<Arc<ServerToClient>>,
    ) -> Self {
        Self::ClientJoin {
            client_id,
            client_profile,
            client_response_tx,
        }
    }
}

#[derive(Debug)]
pub enum LobbyExtra {
    ClientJoinInfo(ClientJoinInfo),
    CoordinatorTx(mpsc::UnboundedSender<CoordinatorMessage>),
}

#[derive(Debug)]
pub struct ClientJoinInfo {
    pub client_profile: ClientProfile,
    pub client_response_tx: mpsc::UnboundedSender<Arc<ServerToClient>>,
}

#[derive(Debug)]
pub struct LobbyJoinData {
    pub lobby_code: String,
    pub lobby_tx: tokio::sync::mpsc::UnboundedSender<LobbyMessage>,
}
