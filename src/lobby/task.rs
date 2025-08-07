use super::{broadcaster::LobbyBroadcaster, handlers::LobbyHandlers, lobby::Lobby};
use crate::{
    actions::ServerToClient,
    game_mode::GameMode,
    messages::{CoordinatorMessage, LobbyMessage},
};
use tokio::sync::mpsc;
use tracing::{debug, info};
use uuid::Uuid;

/// Individual lobby task - handles 2-4 players
pub async fn lobby_task(
    lobby_code: String,
    mut rx: mpsc::UnboundedReceiver<LobbyMessage>,
    ruleset: String,
    game_mode: GameMode,
) {
    let mut lobby = Lobby::new(lobby_code.clone(), ruleset.clone() , game_mode);
    let mut broadcaster = LobbyBroadcaster::new();
    let mut host_id = Uuid::nil();

    info!(
        "Lobby {} started (ruleset: {}, mode: {})",
        lobby_code, ruleset, game_mode
    );

    while let Some(msg) = rx.recv().await {
        match msg {
            LobbyMessage::PlayHand {
                player_id,
                score,
                hands_left,
            } => {
                LobbyHandlers::handle_play_hand(
                    &mut lobby,
                    &broadcaster,
                    player_id,
                    score,
                    hands_left,
                );
            }
            LobbyMessage::SetLocation {
                player_id,
                location,
            } => {
                LobbyHandlers::handle_set_location(&mut lobby, &broadcaster, player_id, location);
            }
            LobbyMessage::Skip { player_id, blind } => {
                LobbyHandlers::handle_skip(&mut lobby, &broadcaster, player_id, blind);
            }
            LobbyMessage::UpdateHandsAndDiscards {
                player_id,
                hands_max,
                discards_max,
            } => {
                LobbyHandlers::handle_update_hands_and_discards(
                    &mut lobby,
                    &broadcaster,
                    player_id,
                    hands_max,
                    discards_max,
                );
            }
            LobbyMessage::FailRound { player_id } => {
                lobby.handle_player_fail_round(player_id, &broadcaster);
            }
            LobbyMessage::PlayerJoined {
                player_id,
                client_response_tx,
                client_profile,
            } => {
                if lobby.started {
                    let _ = client_response_tx.send(
                        ServerToClient::Error {
                            message: String::from("Lobby is already started"),
                        }
                        .to_json(),
                    );
                } else if lobby.is_full() {
                    let _ = client_response_tx.send(
                        ServerToClient::Error {
                            message: String::from("Lobby is full"),
                        }
                        .to_json(),
                    );
                }

                broadcaster.add_player(player_id, client_response_tx.clone());

                let lobby_entry = lobby.add_player(player_id, client_profile.clone());

                if lobby.players().len() == 1 {
                    host_id = player_id;
                }

                // Send joined lobby response
                let joined_response = ServerToClient::joined_lobby(
                    player_id,
                    serde_json::to_value(&lobby).unwrap_or_else(|e| {
                        tracing::error!("Failed to serialize lobby: {}", e);
                        serde_json::Value::Null
                    }),
                );
                broadcaster.send_to(player_id, joined_response);

                // Broadcast player joined to others
                let player_joined_response = ServerToClient::player_joined_lobby(lobby_entry);
                broadcaster.broadcast_except(player_id, player_joined_response);

                debug!("Player {} joined lobby {}", player_id, lobby_code);
            }
            LobbyMessage::LeaveLobby {
                player_id,
                coordinator_tx,
            } => {
                debug!("Player {} leaving lobby {}", player_id, lobby_code);
                broadcaster.remove_player(player_id);
                let leaving_player = lobby.remove_player(player_id);

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
                let player_left_response = ServerToClient::player_left_lobby(player_id, host_id);
                broadcaster.broadcast(player_left_response);

                debug!("Player {} left lobby {}", player_id, lobby.code);
            }
            LobbyMessage::UpdateLobbyOptions { player_id, options } => {
                lobby.lobby_options = options;
                lobby.reset_ready_states_to_host_only();
                lobby.broadcast_ready_states_except(&broadcaster, player_id);
                broadcaster.broadcast_except(
                    player_id,
                    ServerToClient::UpdateLobbyOptions {
                        options: lobby.lobby_options.clone(),
                    },
                );
            }
            LobbyMessage::StartGame {
                player_id,
                seed: _,
                stake,
            } => {
                if lobby.is_player_host(player_id) {
                    lobby.start_game();
                    broadcaster.broadcast(ServerToClient::ResetPlayers {
                        players: lobby.players().values().cloned().collect(),
                    });
                    broadcaster.broadcast(ServerToClient::GameStarted {
                        seed: lobby.lobby_options.custom_seed.clone(),
                        stake,
                    });
                    lobby.broadcast_ready_states(&broadcaster);
                }
            }
            LobbyMessage::StopGame { player_id: _ } => {
                lobby.reset_game_states();
                lobby.started = false;
                lobby.lobby_options.custom_seed = String::from("random");

                broadcaster.broadcast(ServerToClient::GameStopped {});
                lobby.reset_ready_states_to_host_only();
                lobby.broadcast_ready_states(&broadcaster);
            }
            LobbyMessage::SetReady {
                player_id,
                is_ready,
            } => {
                lobby.set_player_ready(player_id, is_ready);
                if lobby.started {
                    let all_ready = lobby.players().values().all(|p| p.lobby_state.is_ready);
                    if all_ready {
                        lobby.start_online_blind(&broadcaster);
                    }
                } else {
                    lobby.broadcast_ready_states_except(&broadcaster, player_id);
                }
            }
            LobbyMessage::SetBossBlind {
                player_id,
                key,
                chips,
            } => {
                if lobby.is_player_host(player_id) {
                    debug!(
                        "Got SetBossBlind key: {}, chips: {}",
                        key,
                        chips.to_string()
                    );
                    lobby.boss_chips = chips;
                    broadcaster.broadcast_except(player_id, ServerToClient::SetBossBlind { key });
                }
            }
            LobbyMessage::SendPlayerDeck { player_id, deck } => {
                broadcaster.broadcast(ServerToClient::ReceivePlayerDeck { player_id, deck });
            }
            LobbyMessage::SendPhantom { player_id, key } => {
                LobbyHandlers::handle_send_phantom(&broadcaster, player_id, key);
            }
            LobbyMessage::RemovePhantom { player_id, key } => {
                LobbyHandlers::handle_remove_phantom(&broadcaster, player_id, key);
            }
            LobbyMessage::Asteroid { player_id } => {
                LobbyHandlers::handle_asteroid(&broadcaster, player_id);
            }
            LobbyMessage::LetsGoGamblingNemesis { player_id } => {
                LobbyHandlers::handle_lets_go_gambling_nemesis(&broadcaster, player_id);
            }
            LobbyMessage::EatPizza {
                player_id,
                discards,
            } => {
                LobbyHandlers::handle_eat_pizza(&broadcaster, player_id, discards);
            }
            LobbyMessage::SoldJoker { player_id } => {
                LobbyHandlers::handle_sold_joker(&broadcaster, player_id);
            }
            LobbyMessage::SpentLastShop { player_id, amount } => {
                LobbyHandlers::handle_spent_last_shop(&broadcaster, player_id, amount);
            }
            LobbyMessage::Magnet { player_id } => {
                LobbyHandlers::handle_magnet(&broadcaster, player_id);
            }
            LobbyMessage::MagnetResponse { player_id, key } => {
                LobbyHandlers::handle_magnet_response(&broadcaster, player_id, key);
            }
            LobbyMessage::LobbyJoinData { .. } => {
                //This won't be handled here, it's for the coordinator to handle
                tracing::warn!("LobbyJoinData handler not implemented");
            }
            LobbyMessage::SetFurthestBlind { player_id, blind } => {
                LobbyHandlers::set_furthest_blind(&mut lobby, &broadcaster, player_id, blind);
            }
            LobbyMessage::StartAnteTimer { player_id, time } => {
                debug!(
                    "Starting ante timer in lobby {} with time: {}",
                    lobby_code, time
                );
                broadcaster.broadcast_except(player_id, ServerToClient::StartAnteTimer { time });
            }
            LobbyMessage::PauseAnteTimer { player_id, time } => {
                debug!(
                    "Pausing ante timer in lobby {} with time: {}",
                    lobby_code, time
                );
                broadcaster.broadcast_except(player_id, ServerToClient::PauseAnteTimer { time });
            }
            LobbyMessage::FailTimer { player_id } => {
                LobbyHandlers::handle_fail_timer(&mut lobby, &broadcaster, player_id);
            }
            LobbyMessage::SendPlayerJokers { player_id, jokers } => {
                debug!("Sending jokers for player {}: {}", player_id, jokers);
                broadcaster.broadcast_except(player_id, ServerToClient::ReceivePlayerJokers { player_id, jokers });
            }
        }
    }
    debug!("Lobby {} task ended", lobby_code);
}
