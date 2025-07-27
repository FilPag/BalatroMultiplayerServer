use crate::client::ClientData;
use crate::game_mode::{GameMode, LobbyOptions};
use crate::messages::{CoordinatorMessage, LobbyMessage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Simple lobby coordinator that routes messages to individual lobby tasks

#[derive(Serialize, Debug)]
struct Lobby {
    code: String,
    lobby_options: LobbyOptions,
    players: HashMap<Uuid, ClientData>,
}

/// Individual lobby task - handles 2-4 players
pub async fn lobby_task(
    lobby_code: String,
    mut rx: mpsc::UnboundedReceiver<LobbyMessage>,
    ruleset: String,
    game_mode: GameMode,
) {
    let mut lobby = Lobby {
        code: lobby_code.clone(),
        lobby_options: game_mode.get_default_options(),
        players: HashMap::new(),
    };

    // Keep senders separate for communication
    let mut player_senders: HashMap<Uuid, mpsc::UnboundedSender<String>> = HashMap::new();

    println!(
        "Lobby {} started (ruleset: {}, mode: {})",
        lobby_code, ruleset, game_mode
    );

    while let Some(msg) = rx.recv().await {
        match msg {
            LobbyMessage::PlayerJoined {
                player_id,
                response_tx,
                client_data,
            } => {
                player_senders.insert(player_id, response_tx.clone());
                lobby.players.insert(player_id, client_data.clone());

                let _ = response_tx.send(
                    serde_json::json!({
                        "action": "joinedLobby",
                        "player_id": player_id,
                        "lobby_data": lobby
                    })
                    .to_string(),
                );

                let player_joined_message = serde_json::json!({
                    "action": "playerJoinedLobby",
                    "player": client_data,
                });

                broadcast_to_all_except_one(
                    &player_senders,
                    player_id,
                    &player_joined_message.to_string(),
                );

                println!("Player {} joined lobby {}", player_id, lobby_code);
            }

            //TODO migrate the host
            LobbyMessage::PlayerLeft { player_id } => {
                lobby.players.get(&player_id);
                lobby.players.remove(&player_id);
                player_senders.remove(&player_id);

                // Broadcast to remaining players
                let message = serde_json::json!({
                    "action": "playerLeftLobby",
                    "player_id": player_id,
                });

                broadcast_to_all(&player_senders, &message.to_string());

                println!("Player {} left lobby {}", player_id, lobby_code);
                // If no players left, exit the task
                if lobby.players.is_empty() {
                    println!("Lobby {} is empty, shutting down", lobby_code);
                    break;
                }
            }

            LobbyMessage::GetInfo {
                player_id: _,
                response_tx,
            } => {
                let _ = response_tx.send(format!(
                    r#"{{"type":"lobbyInfo","code":"{}","players":{},"ruleset":"{}","gameMode":"{}"}}"#,
                    lobby_code, lobby.players.len(), ruleset, game_mode
                ));
            }
        }
    }

    println!("Lobby {} task ended", lobby_code);
}

fn broadcast_to_all_except_one(
    players: &HashMap<Uuid, mpsc::UnboundedSender<String>>,
    except: Uuid,
    message: &str,
) {
    for (&player_id, sender) in players.iter() {
        if player_id != except {
            let _ = sender.send(message.to_string());
        }
    }
}

/// Broadcast a message to all players in the lobby
fn broadcast_to_all(players: &HashMap<Uuid, mpsc::UnboundedSender<String>>, message: &str) {
    for sender in players.values() {
        let _ = sender.send(message.to_string());
    }
}
