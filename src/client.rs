use crate::actions::{ClientToServer, ServerToClient};
use crate::messages::{CoordinatorMessage, LobbyMessage};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info};
use uuid::Uuid;

// Core client identity and connection info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientProfile {
    pub id: Uuid,
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
                id: Uuid::new_v4(),
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
    // Create channels for this client
    let (writer_tx, writer_rx) = mpsc::unbounded_channel::<String>();

    let mut client: Client = Client::new(Some(coordinator_tx.clone()));

    info!("Client {} connected from {}", client.profile.id, addr);

    // Send initial handshake
    let connected_response = ServerToClient::connected(client.profile.id);
    let _ = writer_tx.send(connected_response.to_json());

    // Spawn task to handle writing to the client socket
    let write_task = tokio::spawn(handle_client_writer(socket_writer, writer_rx));

    // Track client state

    // Read from client
    let mut reader = tokio::io::BufReader::new(socket_reader);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                info!("Client {} disconnected", client.profile.id);
                break;
            }
            Ok(_) => {
                // Parse action
                match serde_json::from_str::<ClientToServer>(&line) {
                    Ok(action) => {
                        handle_client_action(client.profile.id, action, &mut client, &writer_tx)
                            .await;
                    }
                    Err(e) => {
                        error!("Failed to parse action from {}: {}", addr, e);
                    }
                }
            }
            Err(e) => {
                error!("Client {} read error: {}", addr, e);
                break;
            }
        }
    }

    // Cleanup on disconnect
    let _ = coordinator_tx.send(CoordinatorMessage::ClientDisconnected {
        client_id: client.profile.id,
        coordinator_tx: coordinator_tx.clone(),
    });

    // Cancel background tasks
    write_task.abort();

    debug!("Client {} cleanup complete", client.profile.id);
}

/// Handle writing messages to the client socket
async fn handle_client_writer(mut writer: OwnedWriteHalf, mut rx: mpsc::UnboundedReceiver<String>) {
    while let Some(message) = rx.recv().await {
        let message_with_newline = format!("{}\n", message);
        if let Err(e) = writer.write_all(message_with_newline.as_bytes()).await {
            error!("Failed to write to client: {}", e);
            break;
        }
    }
}

/// Handle individual client actions using message passing
async fn handle_client_action(
    client_id: Uuid,
    action: ClientToServer,
    client: &mut Client,
    response_tx: &mpsc::UnboundedSender<String>,
) {
    match action {
        ClientToServer::KeepAlive {} => {
            // Simple keep-alive response
            let response = ServerToClient::KeepAliveResponse {};
            let _ = response_tx.send(response.to_json());
        }
        ClientToServer::Version { version } => {
            debug!("Client {} version: {}", client_id, version);
            let response = ServerToClient::VersionOk {};
            let _ = response_tx.send(response.to_json());
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
            let _ = client.send_to_coordinator(CoordinatorMessage::CreateLobby {
                client_id,
                ruleset,
                game_mode,
                client_response_tx: response_tx.clone(),
                client_profile: client.profile.clone(),
                request_tx: tx,
            });

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
                        let _ = response_tx.send(error_response.to_json());
                    }
                }
            }
        }
        ClientToServer::JoinLobby { code } => {
            let (tx, rx) = oneshot::channel::<LobbyMessage>();
            let _ = client.send_to_coordinator(CoordinatorMessage::JoinLobby {
                client_id,
                lobby_code: code,
                client_response_tx: response_tx.clone(),
                client_profile: client.profile.clone(),
                request_tx: tx,
            });

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
                        let _ = response_tx.send(error_response.to_json());
                    }
                }
            }
        }
        ClientToServer::LeaveLobby {} => {
            info!("Client {} leaving lobby", client_id);
            match client.lobby_channel.as_ref() {
                Some(_) => {
                    if let Some(coordinator_tx) = client.coordinator_channel.clone() {
                        if let Err(e) = client.send_to_lobby(LobbyMessage::LeaveLobby {
                            player_id: client_id,
                            coordinator_tx,
                        }) {
                            error!("Failed to send LeaveLobby for client {}: {}", client_id, e);
                        }
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
            let _ = client.send_to_lobby(LobbyMessage::UpdateLobbyOptions {
                player_id: client_id,
                options,
            });
        }
        ClientToServer::SetReady { is_ready } => {
            let _ = client.send_to_lobby(LobbyMessage::SetReady {
                player_id: client_id,
                is_ready,
            });
        }
        ClientToServer::SetLocation { location } => {
            let _ = client.send_to_lobby(LobbyMessage::SetLocation {
                player_id: client_id,
                location,
            });
        }
        ClientToServer::StartGame { seed, stake } => {
            let _ = client.send_to_lobby(LobbyMessage::StartGame {
                player_id: client_id,
                seed: seed.clone(),
                stake: stake.clone(),
            });
        }
        ClientToServer::StopGame {} => {
            let _ = client.send_to_lobby(LobbyMessage::StopGame {
                player_id: client_id,
            });
        }
        ClientToServer::UpdateHandsAndDiscards {
            hands_max,
            discards_max,
        } => {
            let _ = client.send_to_lobby(LobbyMessage::UpdateHandsAndDiscards {
                player_id: client_id,
                hands_max,
                discards_max,
            });
        }
        ClientToServer::PlayHand { score, hands_left } => {
            let _ = client.send_to_lobby(LobbyMessage::PlayHand {
                player_id: client_id,
                score,
                hands_left,
            });
        }
        ClientToServer::Discard {} => todo!(),
        ClientToServer::SetBossBlind { key, chips } => {
            let _ = client.send_to_lobby(LobbyMessage::SetBossBlind {
                player_id: client_id,
                key,
                chips,
            });
        }
    }
}
