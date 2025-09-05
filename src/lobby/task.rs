use std::sync::Arc;

use super::{broadcaster::LobbyBroadcaster, handlers::LobbyHandlers, lobby::Lobby};
use crate::{
    game_mode::GameMode,
    messages::ServerToClient,
    messages::{CoordinatorMessage, LobbyMessage},
};
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Individual lobby task - handles 2-4 players
pub async fn lobby_task(
    lobby_code: String,
    mut rx: mpsc::UnboundedReceiver<LobbyMessage>,
    ruleset: String,
    game_mode: GameMode,
) {
    let mut lobby = Lobby::new(lobby_code.clone(), ruleset.clone(), game_mode);
    let mut broadcaster = LobbyBroadcaster::new();
    let mut host_id = String::new();

    info!(
        "Lobby {} started (ruleset: {}, mode: {})",
        lobby_code, ruleset, game_mode
    );

    while let Some(msg) = rx.recv().await {
        match msg {
            LobbyMessage::ClientAction { client_id, action } => {
                LobbyHandlers::handle_player_action(&mut lobby, &broadcaster, client_id, action);
            }
            LobbyMessage::ClientJoin {
                client_id,
                client_profile,
                client_response_tx,
            } => {
                if lobby.started {
                    let _ = client_response_tx.send(Arc::new(ServerToClient::Error {
                        message: String::from("Lobby is already started"),
                    }));
                } else if lobby.is_full() {
                    let _ = client_response_tx.send(Arc::new(ServerToClient::Error {
                        message: String::from("Lobby is full"),
                    }));
                }

                broadcaster.add_player(client_id.clone(), client_response_tx);

                let lobby_entry = lobby.add_player(client_id.clone(), client_profile.clone());

                if lobby.players().len() == 1 {
                    host_id = client_id.clone();
                }

                // Send joined lobby response
                let joined_response =
                    ServerToClient::joined_lobby(client_id.clone(), lobby.clone());
                broadcaster.send_to(&client_id, joined_response);

                // Broadcast player joined to others
                let player_joined_response = ServerToClient::player_joined_lobby(lobby_entry);
                broadcaster.broadcast_except(&client_id, player_joined_response);

                debug!("Player {} joined lobby {}", client_id, lobby_code);
            }
            LobbyMessage::ClientLeave {
                client_id,
                coordinator_tx,
            } => {
                debug!("Player {} leaving lobby {}", client_id, lobby_code);
                broadcaster.remove_player(&client_id);
                let leaving_player = lobby.remove_player(&client_id);

                if lobby.players().is_empty() {
                    let _ = coordinator_tx.send(CoordinatorMessage::LobbyShutdown {
                        lobby_code: lobby.code.clone(),
                    });
                    break;
                }

                if let Some(player) = leaving_player {
                    if player.lobby_state.is_host {
                        if let Some(new_host_id) = lobby.promote_new_host() {
                            host_id = new_host_id;
                        }
                    }
                }

                // Broadcast to remaining players
                let player_left_response =
                    ServerToClient::player_left_lobby(client_id.clone(), host_id.clone());
                broadcaster.broadcast(player_left_response);
                lobby.started = false;

                debug!("Player {} left lobby {}", client_id, lobby.code);
            }
        }
    }
    debug!("Lobby {} task ended", lobby_code);
}
