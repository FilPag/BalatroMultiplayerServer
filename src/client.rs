use crate::actions::{ClientToServer, ServerToClient};
use crate::messages::{CoordinatorMessage, LobbyMessage};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
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
        message: LobbyMessage,
    ) -> Result<(), mpsc::error::SendError<LobbyMessage>> {
        if let Some(lobby_tx) = &self.lobby_channel {
            lobby_tx.send(message)
        } else {
            Err(mpsc::error::SendError(message))
        }
    }
}
/// Simple client handler using message passing
pub async fn handle_client(
    socket_reader: OwnedReadHalf,
    socket_writer: OwnedWriteHalf,
    addr: SocketAddr,
    coordinator_tx: mpsc::UnboundedSender<CoordinatorMessage>,
) {
        // Create channels for this client - use Vec<u8> for MessagePack compatibility
    let (writer_tx, writer_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    let mut client: Client = Client::new(Some(coordinator_tx.clone()));
    let client_id = client.profile.id.clone();

    info!("Client {} connected from {}", client_id, addr);

    // Send initial handshake
    let connected_response = ServerToClient::connected(client_id.clone());
    let _ = writer_tx.send(connected_response.to_msgpack());

    // Spawn task to handle writing to the client socket
    let write_task = tokio::spawn(handle_client_writer(socket_writer, writer_rx));

    // Track client state

    // Read from client
    let mut reader = socket_reader;

    loop {
        // Read 4-byte length header
        let mut length_bytes = [0u8; 4];
        match reader.read_exact(&mut length_bytes).await {
            Ok(_) => {
                let length = u32::from_be_bytes(length_bytes) as usize;
                
                // Read MessagePack data
                let mut msgpack_data = vec![0u8; length];
                debug!("Reading {} bytes from client {}", length, client_id);
                match reader.read_exact(&mut msgpack_data).await {
                    Ok(_) => {
                        // Parse MessagePack
                        match rmp_serde::from_slice::<ClientToServer>(&msgpack_data) {
                            Ok(action) => {
                                if let Err(e) = handle_client_action(client_id.clone(), action, &mut client, &writer_tx).await {
                                    error!("Error handling action for client {}: {}", client_id, e);
                                    let error_response = ServerToClient::error(&format!("Action failed: {}", e));
                                    let _ = writer_tx.send(error_response.to_msgpack());
                                }
                            }
                            Err(e) => {
                                error!("Failed to parse MessagePack from {}: {}", addr, e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to read MessagePack data from {}: {}", addr, e);
                        break;
                    }
                }
            }
            Err(e) => {
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
async fn handle_client_writer(mut writer: OwnedWriteHalf, mut rx: mpsc::UnboundedReceiver<Vec<u8>>) {
    while let Some(message) = rx.recv().await {
        // Send 4-byte length header + MessagePack data
        let length = message.len() as u32;
        let length_bytes = length.to_be_bytes();
        
        if let Err(e) = writer.write_all(&length_bytes).await {
            error!("Failed to write length header: {}", e);
            break;
        }
        if let Err(e) = writer.write_all(&message).await {
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
    response_tx: &mpsc::UnboundedSender<Vec<u8>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match action {
        ClientToServer::KeepAlive {} => {
            // Simple keep-alive response
            let response = ServerToClient::KeepAliveResponse {};
            response_tx.send(response.to_msgpack())?;
        }
        ClientToServer::Version { version } => {
            debug!("Client {} version: {}", client_id, version);
            let response = ServerToClient::VersionOk {};
            response_tx.send(response.to_msgpack())?;
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
            let (tx, rx) = oneshot::channel::<LobbyMessage>();
            client.send_to_coordinator(CoordinatorMessage::CreateLobby {
                client_id,
                ruleset,
                game_mode,
                client_response_tx: response_tx.clone(),
                client_profile: client.profile.clone(),
                request_tx: tx,
            })?;

            if let Ok(lobby_message) = rx.await {
                match lobby_message {
                    LobbyMessage::LobbyJoinData {
                        lobby_code,
                        lobby_tx,
                    } => {
                        client.lobby_channel = Some(lobby_tx);
                        client.current_lobby = Some(lobby_code);
                    }
                    _ => {
                        let error_response = ServerToClient::error("Failed to create lobby");
                        response_tx.send(error_response.to_msgpack())?;
                    }
                }
            }
        }
        ClientToServer::JoinLobby { code } => {
            let (tx, rx) = oneshot::channel::<LobbyMessage>();
            client.send_to_coordinator(CoordinatorMessage::JoinLobby {
                client_id,
                lobby_code: code,
                client_response_tx: response_tx.clone(),
                client_profile: client.profile.clone(),
                request_tx: tx,
            })?;

            if let Ok(lobby_message) = rx.await {
                match lobby_message {
                    LobbyMessage::LobbyJoinData {
                        lobby_code,
                        lobby_tx,
                    } => {
                        client.lobby_channel = Some(lobby_tx);
                        client.current_lobby = Some(lobby_code);
                    }
                    _ => {
                        let error_response = ServerToClient::error("Failed to join lobby");
                        response_tx.send(error_response.to_msgpack())?;
                    }
                }
            }
        }
        ClientToServer::LeaveLobby {} => {
            info!("Client {} leaving lobby", client_id);
            match client.lobby_channel.as_ref() {
                Some(_) => {
                    if let Some(coordinator_tx) = client.coordinator_channel.clone() {
                        client.send_to_lobby(LobbyMessage::LeaveLobby {
                            player_id: client_id,
                            coordinator_tx,
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
        ClientToServer::UpdateLobbyOptions { options } => {
            client.send_to_lobby(LobbyMessage::UpdateLobbyOptions {
                player_id: client_id,
                options,
            })?;
        }
        ClientToServer::SetReady { is_ready } => {
            client.send_to_lobby(LobbyMessage::SetReady {
                player_id: client_id,
                is_ready,
            })?;
        }
        ClientToServer::SetLocation { location } => {
            client.send_to_lobby(LobbyMessage::SetLocation {
                player_id: client_id,
                location,
            })?;
        }
        ClientToServer::StartGame { seed, stake } => {
            client.send_to_lobby(LobbyMessage::StartGame {
                player_id: client_id,
                seed: seed.clone(),
                stake: stake.clone(),
            })?;
        }
        ClientToServer::StopGame {} => {
            client.send_to_lobby(LobbyMessage::StopGame {
                player_id: client_id,
            })?;
        }
        ClientToServer::UpdateHandsAndDiscards {
            hands_max,
            discards_max,
        } => {
            client.send_to_lobby(LobbyMessage::UpdateHandsAndDiscards {
                player_id: client_id,
                hands_max,
                discards_max,
            })?;
        }
        ClientToServer::PlayHand { score, hands_left } => {
            client.send_to_lobby(LobbyMessage::PlayHand {
                player_id: client_id,
                score,
                hands_left,
            })?;
        }
        ClientToServer::Discard {} => todo!(),
        ClientToServer::SetBossBlind { key, chips } => {
            client.send_to_lobby(LobbyMessage::SetBossBlind {
                player_id: client_id,
                key,
                chips,
            })?;
        }
        ClientToServer::Skip { blind } => {
            client.send_to_lobby(LobbyMessage::Skip {
                player_id: client_id,
                blind,
            })?;
        }
        ClientToServer::FailRound {} => {
            client.send_to_lobby(LobbyMessage::FailRound {
                player_id: client_id,
            })?;
        }
        ClientToServer::SendPlayerDeck { deck } => {
            client.send_to_lobby(LobbyMessage::SendPlayerDeck {
                player_id: client_id,
                deck,
            })?;
        }
        ClientToServer::SendPhantom { key } => {
            client.send_to_lobby(LobbyMessage::SendPhantom {
                player_id: client_id,
                key,
            })?;
        }
        ClientToServer::RemovePhantom { key } => {
            client.send_to_lobby(LobbyMessage::RemovePhantom {
                player_id: client_id,
                key,
            })?;
        }
        ClientToServer::Asteroid {} => {
            client.send_to_lobby(LobbyMessage::Asteroid {
                player_id: client_id,
            })?;
        }
        ClientToServer::LetsGoGamblingNemesis {} => {
            client.send_to_lobby(LobbyMessage::LetsGoGamblingNemesis {
                player_id: client_id,
            })?;
        }
        ClientToServer::EatPizza { discards } => {
            client.send_to_lobby(LobbyMessage::EatPizza {
                player_id: client_id,
                discards,
            })?;
        }
        ClientToServer::SoldJoker {} => {
            client.send_to_lobby(LobbyMessage::SoldJoker {
                player_id: client_id,
            })?;
        }
        ClientToServer::SpentLastShop { amount } => {
            client.send_to_lobby(LobbyMessage::SpentLastShop {
                player_id: client_id,
                amount,
            })?;
        }
        ClientToServer::Magnet {} => {
            client.send_to_lobby(LobbyMessage::Magnet {
                player_id: client_id,
            })?;
        }
        ClientToServer::MagnetResponse { key } => {
            client.send_to_lobby(LobbyMessage::MagnetResponse {
                player_id: client_id,
                key,
            })?;
        }
        ClientToServer::SetFurthestBlind { blind } => {
            client.send_to_lobby(LobbyMessage::SetFurthestBlind {
                player_id: client_id,
                blind,
            })?;
        }
        ClientToServer::StartAnteTimer { time } => {
            client.send_to_lobby(LobbyMessage::StartAnteTimer {
                player_id: client_id,
                time,
            })?;
        }
        ClientToServer::PauseAnteTimer { time } => {
            client.send_to_lobby(LobbyMessage::PauseAnteTimer {
                player_id: client_id,
                time,
            })?;
        }
        ClientToServer::FailTimer {} => {
            client.send_to_lobby(LobbyMessage::FailTimer {
                player_id: client_id,
            })?;
        }
        ClientToServer::SendPlayerJokers { jokers } => {
            client.send_to_lobby(LobbyMessage::SendPlayerJokers {
                player_id: client_id,
                jokers,
            })?;
        }
    }
    Ok(())
}
