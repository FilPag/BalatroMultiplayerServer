use crate::lobby::lobby_task;
use crate::messages::{CoordinatorMessage, LobbyMessage};
use std::collections::HashMap;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Simple lobby coordinator that routes messages to individual lobby tasks
pub async fn lobby_coordinator(mut rx: mpsc::UnboundedReceiver<CoordinatorMessage>) {
    let mut lobby_senders: HashMap<String, mpsc::UnboundedSender<LobbyMessage>> = HashMap::new();
    let mut client_lobbies: HashMap<Uuid, String> = HashMap::new(); // Track which lobby each client is in

    println!("Lobby coordinator started");

    while let Some(msg) = rx.recv().await {
        match msg {
            CoordinatorMessage::CreateLobby {
                client_id,
                ruleset,
                game_mode,
                response_tx,
                mut client_data,
            } => {
                // Generate a simple lobby code
                let lobby_code = generate_lobby_code();

                // Create the lobby task
                let (lobby_tx, lobby_rx) = mpsc::unbounded_channel();
                lobby_senders.insert(lobby_code.clone(), lobby_tx.clone());
                client_lobbies.insert(client_id, lobby_code.clone());

                // Spawn the lobby task
                tokio::spawn(lobby_task(lobby_code.clone(), lobby_rx, ruleset, game_mode));

                client_data.lobby_state.is_host = true; // Mark this client as the host
                let _ = lobby_tx.send(LobbyMessage::PlayerJoined {
                    player_id: client_id,
                    response_tx: response_tx.clone(),
                    client_data,
                });

                println!("Created lobby: {}", lobby_code);
            }

            CoordinatorMessage::JoinLobby {
                client_id,
                lobby_code,
                response_tx,
                client_data,
            } => {
                if let Some(lobby_tx) = lobby_senders.get(&lobby_code) {
                    // Try to forward to lobby task
                    if let Err(_) = lobby_tx.send(LobbyMessage::PlayerJoined {
                        player_id: client_id,
                        response_tx: response_tx.clone(),
                        client_data,
                    }) {
                        // Failed to send to lobby, send error response
                        let _ = response_tx.send(
                            serde_json::json!({
                                "action": "error",
                                "message": "Failed to join lobby"
                            })
                            .to_string(),
                        );
                    } else {
                        // Successfully sent to lobby, add client to tracking
                        client_lobbies.insert(client_id, lobby_code.clone());
                    }
                } else {
                    // Lobby doesn't exist
                    let _ = response_tx.send(
                        serde_json::json!({
                            "action": "error",
                            "message": "Lobby not found"
                        })
                        .to_string(),
                    );
                }
            }

            CoordinatorMessage::RouteToLobby {
                lobby_code,
                message,
            } => if let Some(lobby_tx) = lobby_senders.get(&lobby_code) {},

            CoordinatorMessage::LeaveLobby { client_id } => {
                println!("Client {} requested to leave lobby", client_id);
                if let Some(lobby_code) = client_lobbies.remove(&client_id) {
                    if let Some(lobby_tx) = lobby_senders.get(&lobby_code) {
                        let _ = lobby_tx.send(LobbyMessage::PlayerLeft {
                            player_id: client_id,
                        });
                    }
                }
            }

            CoordinatorMessage::ClientDisconnected { client_id } => {
                // Remove client from any lobby they were in
                println!("COORDINATOR ----- Client {} disconnected", client_id);
                if let Some(lobby_code) = client_lobbies.remove(&client_id) {
                    if let Some(lobby_tx) = lobby_senders.get(&lobby_code) {
                        let _ = lobby_tx.send(LobbyMessage::PlayerLeft {
                            player_id: client_id,
                        });
                    }
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
