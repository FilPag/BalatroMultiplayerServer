use crate::actions::ServerToClient;
use crate::client::{ClientProfile};
use crate::game_mode::{GameMode, LobbyOptions};
use crate::messages::{CoordinatorMessage, LobbyMessage};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use std::collections::HashMap;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Simple lobby coordinator that routes messages to individual lobby tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientLobbyState {
    pub current_lobby: Option<String>,
    pub is_ready: bool,
    pub first_ready: bool,
    pub is_cached: bool,
    pub is_host: bool,
}

// Game state (changes frequently during gameplay)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientGameState {
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
pub struct ClientLobbyEntry{
    pub profile: ClientProfile,
    pub lobby_state: ClientLobbyState,
    pub game_state: Option<ClientGameState>, // None when not in game
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Lobby {
    code: String,
    lobby_options: LobbyOptions,
    players: HashMap<Uuid, ClientLobbyEntry>,
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
    let mut host_id = lobby.players.keys().next().cloned();

    // Keep senders separate for communication
    let mut player_senders: HashMap<Uuid, mpsc::UnboundedSender<String>> = HashMap::new();

    info!(
        "Lobby {} started (ruleset: {}, mode: {})",
        lobby_code, ruleset, game_mode
    );

    // Helper function to send responses safely
    let send_response = |sender: &mpsc::UnboundedSender<String>, response: ServerToClient| {
        let _ = sender.send(response.to_json());
    };

    // Helper function to broadcast to all except one player
    let broadcast_response_except_one = |players: &HashMap<Uuid, mpsc::UnboundedSender<String>>, except: Uuid, response: ServerToClient| {
        let message = response.to_json();
        for (&player_id, sender) in players.iter() {
            if player_id != except {
                let _ = sender.send(message.clone());
            }
        }
    };

    // Helper function to broadcast to all players
    let broadcast_response = |players: &HashMap<Uuid, mpsc::UnboundedSender<String>>, response: ServerToClient| {
        let message = response.to_json();
        for sender in players.values() {
            let _ = sender.send(message.clone());
        }
    };

    while let Some(msg) = rx.recv().await {
        match msg {
            LobbyMessage::PlayerJoined {
                                player_id,
                                client_response_tx,
                                client_profile,
                            } => {
                                player_senders.insert(player_id, client_response_tx.clone());

                                let lobby_entry = ClientLobbyEntry {
                                    profile: client_profile.clone(),
                                    lobby_state: ClientLobbyState {
                                        current_lobby: Some(lobby_code.clone()),
                                        is_ready: lobby.players.is_empty(),
                                        first_ready: false,
                                        is_cached: false,
                                        is_host: lobby.players.is_empty(),
                                    },
                                    game_state: None, // No game state until game starts
                                };

                                lobby.players.insert(player_id, lobby_entry.clone());

                                // Send joined lobby response
                                let joined_response = ServerToClient::joined_lobby(
                                    player_id,
                                    serde_json::to_value(&lobby).unwrap_or_default()
                                );
                                send_response(&client_response_tx, joined_response);

                                // Broadcast player joined to others
                                let player_joined_response = ServerToClient::player_joined_lobby(lobby_entry.clone());
                                broadcast_response_except_one(&player_senders, player_id, player_joined_response);

                                debug!("Player {} joined lobby {}", player_id, lobby_code);
                            }
            LobbyMessage::LeaveLobby{ player_id, coordinator_tx } => {
                                debug!("Player {} leaving lobby {}", player_id, lobby_code);
                                player_senders.remove(&player_id);
                                let leaving_player = lobby.players.remove(&player_id).unwrap();

                                if lobby.players.is_empty() {
                                    let _ = coordinator_tx.send(CoordinatorMessage::LobbyShutdown {
                                        lobby_code: lobby.code.clone(),
                                    });
                                    break;
                                }
                                if leaving_player.lobby_state.is_host {
                                    if let Some((&new_host_player_id, new_host_entry)) = lobby.players.iter_mut().next() {
                                        new_host_entry.lobby_state.is_host = true;
                                        new_host_entry.lobby_state.is_ready = true;
                                        host_id = Some(new_host_player_id);
                                    }
                                }

                                // Broadcast to remaining players
                                let player_left_response = ServerToClient::player_left_lobby(player_id, host_id);
                                broadcast_response(&player_senders, player_left_response);

                                debug!("Player {} left lobby {}", player_id, lobby.code);
                            }
            LobbyMessage::LobbyJoinData {..} => todo!(),
                                //should not do anything here, handled by client

            LobbyMessage::UpdateLobbyOptions { player_id, options } => {
                        // Update the lobby options
                        lobby.lobby_options = options;
                        for player in lobby.players.values_mut() {
                            if player.lobby_state.is_host == false {
                                player.lobby_state.is_ready = false;
                            }
                        }
                        broadcast_response_except_one(&player_senders, player_id, ServerToClient::UpdateLobbyOptions {
                            options: lobby.lobby_options.clone(),
                        });
                    },

            LobbyMessage::SetReady { player_id, is_ready } => {
                if let Some(player) = lobby.players.get_mut(&player_id) {
                    player.lobby_state.is_ready = is_ready;
                    broadcast_response_except_one(&player_senders, player_id, ServerToClient::PlayerReady{
                        player_id,
                        is_ready,
                    });
                }
            }
        }
    }
    debug!("Lobby {} task ended", lobby_code);
}
