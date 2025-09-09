use crate::messages::{
    ClientToServer, CoordinatorMessage, LobbyJoinData, LobbyMessage, ServerToClient,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info};
use uuid::Uuid;

// Core client identity and connection info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientProfile {
    pub id: String,
    pub username: String,
    pub colour: u8, // 0-255 instead of string
    pub mod_hash: String,
}

#[derive(Debug, Clone)]
pub struct Client {
    pub lobby_channel: Option<mpsc::UnboundedSender<LobbyMessage>>,
    pub coordinator_channel: Option<mpsc::UnboundedSender<CoordinatorMessage>>,
    pub profile: ClientProfile,
    pub current_lobby: Option<String>,
}

impl Client {
    pub fn new(coordinator_channel: Option<mpsc::UnboundedSender<CoordinatorMessage>>) -> Self {
        Self {
            lobby_channel: None,
            coordinator_channel: coordinator_channel,
            profile: ClientProfile {
                id: Uuid::new_v4().to_string(),
                username: "Guest".to_string(),
                colour: 0,
                mod_hash: "".to_string(),
            },
            current_lobby: None,
        }
    }

    pub fn send_to_coordinator(
        &self,
        message: CoordinatorMessage,
    ) -> Result<(), mpsc::error::SendError<CoordinatorMessage>> {
        if let Some(coordinator_tx) = &self.coordinator_channel {
            coordinator_tx.send(message)
        } else {
            Err(mpsc::error::SendError(message))
        }
    }
    pub fn send_to_lobby(
        &self,
        message: ClientToServer,
    ) -> Result<(), mpsc::error::SendError<LobbyMessage>> {
        if let Some(lobby_tx) = &self.lobby_channel {
            lobby_tx.send(LobbyMessage::client_action(
                self.profile.id.clone(),
                message,
            ))
        } else {
            Err(mpsc::error::SendError(LobbyMessage::client_action(
                self.profile.id.clone(),
                message,
            )))
        }
    }
}

// Helper errors for reading a single ClientToServer action
#[derive(Debug)]
enum ReadActionError {
    Io(std::io::Error),
    EmptyFrame,
    Oversized { len: usize, max: usize },
    Malformed(rmp_serde::decode::Error),
}

impl std::fmt::Display for ReadActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadActionError::Io(e) => write!(f, "io error: {e}"),
            ReadActionError::EmptyFrame => write!(f, "empty frame"),
            ReadActionError::Oversized { len, max } => {
                write!(f, "oversized frame {len} > {max}")
            }
            ReadActionError::Malformed(e) => write!(f, "malformed message: {e}"),
        }
    }
}

impl std::error::Error for ReadActionError {}

const MAX_MESSAGE_SIZE: usize = 256 * 1024; // 256 KiB safety cap

// Read one action from the socket; uses '?' for IO steps
async fn read_client_action(reader: &mut OwnedReadHalf) -> Result<ClientToServer, ReadActionError> {
    let mut length_bytes = [0u8; 4];
    reader
        .read_exact(&mut length_bytes)
        .await
        .map_err(ReadActionError::Io)?;
    let length = u32::from_be_bytes(length_bytes) as usize;
    if length == 0 {
        return Err(ReadActionError::EmptyFrame);
    }
    if length > MAX_MESSAGE_SIZE {
        return Err(ReadActionError::Oversized {
            len: length,
            max: MAX_MESSAGE_SIZE,
        });
    }
    let mut buf = vec![0u8; length];
    reader
        .read_exact(&mut buf)
        .await
        .map_err(ReadActionError::Io)?;
    rmp_serde::from_slice::<ClientToServer>(&buf).map_err(ReadActionError::Malformed)
}

/// Simple client handler using message passing
pub async fn handle_client(
    socket_reader: OwnedReadHalf,
    socket_writer: OwnedWriteHalf,
    addr: SocketAddr,
    coordinator_tx: mpsc::UnboundedSender<CoordinatorMessage>,
) {
    // Create channels for this client - use Vec<u8> for MessagePack compatibility
    let (writer_tx, writer_rx) = mpsc::unbounded_channel::<Arc<ServerToClient>>();

    let mut client: Client = Client::new(Some(coordinator_tx.clone()));
    let client_id = client.profile.id.clone();

    info!("Client {} connected from {}", client_id, addr);

    // Send initial handshake
    let connected_response = Arc::new(ServerToClient::connected(client_id.clone()));
    let _ = writer_tx.send(connected_response);

    // Spawn task to handle writing to the client socket
    let write_task = tokio::spawn(handle_client_writer(socket_writer, writer_rx));

    let mut reader = socket_reader;

    // ---- Read loop using helper ----
    loop {
        match read_client_action(&mut reader).await {
            Ok(action) => {
                if let Err(e) =
                    handle_client_action(client_id.clone(), action, &mut client, &writer_tx).await
                {
                    error!("Action error for client {}: {}", client_id, e);
                    let _ = writer_tx.send(Arc::new(ServerToClient::error(&format!(
                        "Action failed: {}",
                        e
                    ))));
                }
            }
            Err(ReadActionError::EmptyFrame) => {
                error!("Client {} sent empty frame", client_id);
                let _ = writer_tx.send(Arc::new(ServerToClient::error("Empty message")));
                continue;
            }
            Err(ReadActionError::Oversized { len, max }) => {
                error!(
                    "Client {} sent oversized frame ({} > {})",
                    client_id, len, max
                );
                let _ = writer_tx.send(Arc::new(ServerToClient::error("Message too large")));
                break; // Protocol abuse -> disconnect
            }
            Err(ReadActionError::Malformed(e)) => {
                error!("Failed to parse MessagePack from {}: {}", addr, e);
                let _ = writer_tx.send(Arc::new(ServerToClient::error("Malformed message")));
                continue; // Allow next messages
            }
            Err(ReadActionError::Io(e)) => {
                info!("Client {} disconnected: {}", client_id, e);
                break;
            }
        }
    }

    // Cleanup on disconnect
    let _ = coordinator_tx.send(CoordinatorMessage::ClientDisconnected {
        client_id: client_id.clone(),
        coordinator_tx: coordinator_tx.clone(),
    });

    // Cancel background tasks
    write_task.abort();

    debug!("Client cleanup complete");
}

/// Handle writing messages to the client socket
async fn handle_client_writer(
    mut writer: OwnedWriteHalf,
    mut rx: mpsc::UnboundedReceiver<Arc<ServerToClient>>,
) {
    while let Some(message) = rx.recv().await {
        // Send 4-byte length header + MessagePack data
        let buff = message.to_msgpack();

        let length = buff.len() as u32;
        let length_bytes = length.to_be_bytes();

        if let Err(e) = writer.write_all(&length_bytes).await {
            error!("Failed to write length header: {}", e);
            break;
        }
        if let Err(e) = writer.write_all(&buff).await {
            error!("Failed to write MessagePack data: {}", e);
            break;
        }
    }
}

/// Handle individual client actions using message passing
async fn handle_client_action(
    client_id: String,
    action: ClientToServer,
    client: &mut Client,
    response_tx: &mpsc::UnboundedSender<Arc<ServerToClient>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match action {
        ClientToServer::KeepAlive {} => {
            // Simple keep-alive response
            let response = Arc::new(ServerToClient::KeepAliveResponse {});
            response_tx.send(response)?;
        }
        ClientToServer::Version { version } => {
            debug!("Client {} version: {}", client_id, version);
            let response = Arc::new(ServerToClient::VersionOk {});
            response_tx.send(response)?;
        }
        ClientToServer::SetClientData {
            username: new_username,
            colour: new_colour,
            mod_hash: new_mod_hash,
        } => {
            client.profile.username = new_username.clone();
            client.profile.colour = new_colour as u8; // Convert i32 to u8
            client.profile.mod_hash = new_mod_hash.clone();

            debug!(
                "Client {} set client data: username={}, colour={}, mod_hash={}",
                client_id, new_username, new_colour, new_mod_hash
            );
        }
        ClientToServer::CreateLobby { ruleset, game_mode } => {
            let (tx, rx) = oneshot::channel::<LobbyJoinData>();
            client.send_to_coordinator(CoordinatorMessage::CreateLobby {
                client_id,
                ruleset,
                game_mode,
                client_response_tx: response_tx.clone(),
                client_profile: client.profile.clone(),
                request_tx: tx,
            })?;

            if let Ok(LobbyJoinData {
                lobby_code,
                lobby_tx,
            }) = rx.await
            {
                client.lobby_channel = Some(lobby_tx);
                client.current_lobby = Some(lobby_code);
            } else {
                let error_response = Arc::new(ServerToClient::error("Failed to create lobby"));
                response_tx.send(error_response)?;
            }
        }
        ClientToServer::JoinLobby { code } => {
            let (tx, rx) = oneshot::channel::<LobbyJoinData>();
            client.send_to_coordinator(CoordinatorMessage::JoinLobby {
                client_id,
                lobby_code: code,
                client_response_tx: response_tx.clone(),
                client_profile: client.profile.clone(),
                request_tx: tx,
            })?;

            if let Ok(LobbyJoinData {
                lobby_code,
                lobby_tx,
            }) = rx.await
            {
                client.lobby_channel = Some(lobby_tx);
                client.current_lobby = Some(lobby_code);
            } else {
                let error_response = Arc::new(ServerToClient::error("Failed to join lobby"));
                response_tx.send(error_response)?;
            }
        }
        ClientToServer::LeaveLobby {} => {
            info!("Client {} leaving lobby", client_id);
            match client.lobby_channel.as_ref() {
                Some(_) => {
                    if let Some(coordinator_tx) = client.coordinator_channel.clone() {
                        client.send_to_coordinator(CoordinatorMessage::ClientDisconnected {
                            client_id: client_id.clone(),
                            coordinator_tx: coordinator_tx.clone(),
                        })?;
                    } else {
                        error!(
                            "Coordinator channel missing for client {} when leaving lobby",
                            client_id
                        );
                    }
                }
                None => {
                    error!(
                        "Lobby channel missing for client {} when leaving lobby",
                        client_id
                    );
                }
            }

            client.current_lobby = None;
            client.lobby_channel = None;
        }
        _ => {
            client.send_to_lobby(action)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests{
    use super::*;
    use tokio;
    use std::sync::Arc;

    fn contains_response_of_type(responses: &[Arc<ServerToClient>], variant: &ServerToClient) -> bool {
        responses.iter().any(|msg| std::mem::discriminant(&**msg) == std::mem::discriminant(variant))
    }

    async fn test_handle_client_action_helper_async(action: ClientToServer) -> (Client, Vec<Arc<ServerToClient>>) {
        let mut client = Client::new(None);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let client_id = client.profile.id.clone();
        let _ = handle_client_action(client_id, action, &mut client, &tx).await;
        let mut responses = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            responses.push(msg);
        }
        (client, responses)
    }

    #[tokio::test]
    async fn test_handle_client_action_keepalive() {
        let (_client, responses) = test_handle_client_action_helper_async(ClientToServer::KeepAlive {}).await;
        assert!(contains_response_of_type(&responses, &ServerToClient::KeepAliveResponse {}));
    }

    #[tokio::test]
    async fn test_handle_client_action_version() {
        let (_client, responses) = test_handle_client_action_helper_async(ClientToServer::Version { version: "1.0.0".to_string() }).await;
        assert!(contains_response_of_type(&responses, &ServerToClient::VersionOk {}));
    }

    #[tokio::test]
    async fn test_handle_client_action_set_client_data() {
        let (client, _responses) = test_handle_client_action_helper_async(ClientToServer::SetClientData {
            username: "Alice".to_string(),
            colour: 42,
            mod_hash: "abc123".to_string(),
        }).await;
        assert_eq!(client.profile.username, "Alice");
        assert_eq!(client.profile.colour, 42);
        assert_eq!(client.profile.mod_hash, "abc123");
    }

    #[test]
    fn test_client_profile_new_default() {
        let client = Client::new(None);
        assert_eq!(client.profile.username, "Guest");
        assert_eq!(client.profile.colour, 0);
        assert_eq!(client.profile.mod_hash, "");
        assert!(client.lobby_channel.is_none());
        assert!(client.coordinator_channel.is_none());
        assert!(client.current_lobby.is_none());
    }
}