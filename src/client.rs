use crate::actions::Action;
use crate::messages::{CoordinatorMessage, LobbyMessage};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc;
use uuid::Uuid;

// Core client identity and connection info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientProfile {
    pub id: Uuid,
    pub username: String,
    pub colour: u8, // 0-255 instead of string
    pub mod_hash: String,
}

// Lobby-specific state (can change per lobby)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LobbyState {
    pub current_lobby: Option<String>,
    pub is_ready: bool,
    pub first_ready: bool,
    pub is_cached: bool,
    pub is_host: bool,
}

// Game state (changes frequently during gameplay)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub ante: u32,
    pub furthest_blind: u32,
    pub hands_left: u32,
    pub hands_max: u32,
    pub discards_left: u32,
    pub discards_max: u32,
    pub lives: u32,
    pub lives_blocker: bool,
    pub location: String,
    pub skips: u32,
    pub score: u64, // Simplified from InsaneInt
    pub highest_score: u64,
    pub spent_in_shop: Vec<u32>,
}

// Complete client data container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientData {
    pub profile: ClientProfile,
    pub lobby_state: LobbyState,
    pub game_state: Option<GameState>, // None when not in game
}

impl Default for ClientData {
    fn default() -> Self {
        Self {
            profile: ClientProfile {
                id: Uuid::new_v4(),
                username: "Guest".to_string(),
                colour: 1,
                mod_hash: "NULL".to_string(),
            },
            lobby_state: LobbyState {
                current_lobby: None,
                is_ready: false,
                first_ready: false,
                is_cached: true,
                is_host: false,
            },
            game_state: None,
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
    let client_id = Uuid::new_v4();

    // Create channels for this client
    let (response_tx, mut response_rx) = mpsc::unbounded_channel::<String>();
    let (writer_tx, writer_rx) = mpsc::unbounded_channel::<String>();

    println!("Client {} connected from {}", client_id, addr);

    // Send initial handshake
    let _ = writer_tx.send(
        serde_json::json!({
          "action": "connected",
          "clientId": client_id.to_string()
        })
        .to_string(),
    );

    // Spawn task to handle writing to the client socket
    let write_task = tokio::spawn(handle_client_writer(socket_writer, writer_rx));

    // Spawn task to forward responses to writer
    let response_forward_task = tokio::spawn(async move {
        print!("Forwarding responses for client {}: ", client_id);
        while let Some(message) = response_rx.recv().await {
            let _ = writer_tx.send(message);
        }
    });

    // Track client state
    let mut client_state: ClientData = ClientData::default();
    client_state.profile.id = client_id;

    // Read from client
    let mut reader = tokio::io::BufReader::new(socket_reader);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                println!("Client {} disconnected", client_id);
                break;
            }
            Ok(_) => {
                // Parse action
                match serde_json::from_str::<Action>(&line) {
                    Ok(action) => {
                        handle_client_action(
                            client_id,
                            action,
                            &mut client_state,
                            &coordinator_tx,
                            &response_tx,
                        )
                        .await;
                    }
                    Err(e) => {
                        println!("Failed to parse action from {}: {}", addr, e);
                    }
                }
            }
            Err(e) => {
                println!("Client {} read error: {}", addr, e);
                break;
            }
        }
    }

    // Cleanup on disconnect
    let _ = coordinator_tx.send(CoordinatorMessage::ClientDisconnected { client_id });

    // Cancel background tasks
    write_task.abort();
    response_forward_task.abort();

    println!("Client {} cleanup complete", client_id);
}

/// Handle writing messages to the client socket
async fn handle_client_writer(mut writer: OwnedWriteHalf, mut rx: mpsc::UnboundedReceiver<String>) {
    while let Some(message) = rx.recv().await {
        let message_with_newline = format!("{}\n", message);
        if let Err(e) = writer.write_all(message_with_newline.as_bytes()).await {
            println!("Failed to write to client: {}", e);
            break;
        }
    }
}

/// Handle individual client actions using message passing
async fn handle_client_action(
    client_id: Uuid,
    action: Action,
    client_data: &mut ClientData,
    coordinator_tx: &mpsc::UnboundedSender<CoordinatorMessage>,
    response_tx: &mpsc::UnboundedSender<String>,
) {
    match action {
        Action::KeepAlive {} => {
            // Simple keep-alive response
            let _ = response_tx.send(serde_json::json!({ "action": "a" }).to_string());
        }

        Action::Version { version } => {
            println!("Client {} version: {}", client_id, version);
            let _ = response_tx.send(r#"{"type":"versionOk"}"#.to_string());
        }

        Action::SetClientData {
            username: new_username,
            colour: new_colour,
            mod_hash: new_mod_hash,
        } => {
            client_data.profile.username = new_username.clone();
            client_data.profile.colour = new_colour as u8; // Convert i32 to u8
            client_data.profile.mod_hash = new_mod_hash.clone();

            println!(
                "Client {} set client data: username={}, colour={}, mod_hash={}",
                client_id, new_username, new_colour, new_mod_hash
            );
        }

        Action::CreateLobby { ruleset, game_mode } => {
            let _ = coordinator_tx.send(CoordinatorMessage::CreateLobby {
                client_id,
                ruleset,
                game_mode,
                response_tx: response_tx.clone(),
                client_data: client_data.clone(),
            });
        }

        Action::JoinLobby { code } => {
            client_data.lobby_state.current_lobby = Some(code.clone());
            let _ = coordinator_tx.send(CoordinatorMessage::JoinLobby {
                client_id,
                lobby_code: code,
                response_tx: response_tx.clone(),
                client_data: client_data.clone(),
            });
        }

        Action::LeaveLobby {} => {
            let _ = coordinator_tx.send(CoordinatorMessage::LeaveLobby { client_id });
            client_data.lobby_state.current_lobby = None; // Clear lobby state
        }

        Action::LobbyInfo {} => {
            if let Some(lobby_code) = client_data.lobby_state.current_lobby.as_ref() {
                let _ = coordinator_tx.send(CoordinatorMessage::RouteToLobby {
                    lobby_code: lobby_code.clone(),
                    message: LobbyMessage::GetInfo {
                        player_id: client_id,
                        response_tx: response_tx.clone(),
                    },
                });
            }
        }
    }
}
