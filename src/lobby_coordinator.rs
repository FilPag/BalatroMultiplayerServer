use crate::lobby::lobby_task;
use crate::messages::{CoordinatorMessage, LobbyMessage};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{info};

/// Simple lobby coordinator that routes messages to individual lobby tasks
pub async fn lobby_coordinator(mut rx: mpsc::UnboundedReceiver<CoordinatorMessage>) {
    let mut lobby_senders: HashMap<String, mpsc::UnboundedSender<LobbyMessage>> = HashMap::new();
    let mut client_lobbies: HashMap<uuid::Uuid, String> = HashMap::new();

    info!("Lobby coordinator started");

    while let Some(msg) = rx.recv().await {
        match msg {
            CoordinatorMessage::CreateLobby {
                client_id,
                ruleset,
                game_mode,
                client_profile,
                request_tx,
                client_response_tx,
            } => {
                // Generate a simple lobby code
                let lobby_code = generate_lobby_code();

                // Create the lobby task
                let (lobby_tx, lobby_rx) = mpsc::unbounded_channel();
                lobby_senders.insert(lobby_code.clone(), lobby_tx.clone());
                client_lobbies.insert(client_id, lobby_code.clone());
                // Spawn the lobby task
                tokio::spawn(lobby_task(lobby_code.clone(), lobby_rx, ruleset, game_mode));

                let _ = lobby_tx.send(LobbyMessage::PlayerJoined {
                    player_id: client_id,
                    client_profile: client_profile.clone(),
                    client_response_tx: client_response_tx.clone(),
                });

                // Give client communication channel to lobby
                let _ = request_tx.send(LobbyMessage::LobbyJoinData {
                    lobby_code: lobby_code.clone(),
                    lobby_tx: lobby_tx.clone(),
                });
            }

            CoordinatorMessage::JoinLobby {
                client_id,
                lobby_code,
                request_tx,
                client_response_tx,
                client_profile,
            } => {
                if let Some(lobby_tx) = lobby_senders.get(&lobby_code) {
                    // Give client communication channel to lobby
                    let _ = request_tx.send(LobbyMessage::LobbyJoinData {
                        lobby_code: lobby_code.clone(),
                        lobby_tx: lobby_tx.clone(),
                    });
                    // Try to forward to lobby task
                    if let Err(_) = lobby_tx.send(LobbyMessage::PlayerJoined {
                        player_id: client_id,
                        client_profile: client_profile.clone(),
                        client_response_tx: client_response_tx.clone(),
                    }) {
                        // Failed to send to lobby, send error response
                        let _ = client_response_tx.send(
                            serde_json::json!({
                                "action": "error",
                                "message": "Failed to join lobby"
                            })
                            .to_string(),
                        );
                    } else {
                        client_lobbies.insert(client_id, lobby_code.clone());
                    }
                } else {
                    // Lobby doesn't exist
                    let _ = client_response_tx.send(
                        serde_json::json!({
                            "action": "error",
                            "message": "Lobby not found"
                        })
                        .to_string(),
                    );
                }
            }

            CoordinatorMessage::LobbyShutdown { lobby_code } => {
                lobby_senders.remove(&lobby_code);
            }

            CoordinatorMessage::ClientDisconnected { client_id, coordinator_tx} => {
                if let Some(lobby_code) = client_lobbies.remove(&client_id) {
                let lobby_tx = lobby_senders.get(&lobby_code);
                let _ = lobby_tx.unwrap().send(LobbyMessage::LeaveLobby {
                    player_id: client_id,
                    coordinator_tx,
                });
                }
            }
        }
    }
}

/// Generate a simple 4-character lobby code
fn generate_lobby_code() -> String {
    use rand::Rng;
    let chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    (0..5)
        .map(|_| chars.chars().nth(rng.random_range(0..chars.len())).unwrap())
        .collect()
}
