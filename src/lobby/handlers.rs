use super::{broadcaster::LobbyBroadcaster, lobby::Lobby};
use crate::messages::{ClientToServer, ServerToClient};
use crate::{lobby::handlers, talisman_number::TalismanNumber};
use tracing::{debug, error};

// KISS: Group related handlers
pub struct LobbyHandlers;

impl LobbyHandlers {
    // DRY: Common pattern - update player state, then broadcast
    fn update_player_and_broadcast<F>(
        lobby: &mut Lobby,
        broadcaster: &LobbyBroadcaster,
        player_id: &str,
        exclude_player: bool,
        update_fn: F,
    ) where
        F: FnOnce(&mut crate::lobby::game_state::ClientLobbyEntry),
    {
        if let Some(player) = lobby.get_player_mut(player_id) {
            update_fn(player);
            lobby.broadcast_game_state_update(broadcaster, player_id, exclude_player);
        }
    }

    fn handle_play_hand(
        lobby: &mut Lobby,
        broadcaster: &LobbyBroadcaster,
        player_id: &str,
        score: TalismanNumber,
        hands_left: u8,
    ) {
        if let Some(player) = lobby.get_player_mut(player_id) {
            debug!(
                "Player {} played hand with score {} and hands left {}",
                player_id,
                score.to_string(),
                hands_left
            );

            // Update player state
            player.game_state.score = match player.game_state.score.add(&score) {
                Ok(val) => val,
                Err(e) => {
                    error!("Failed to add score for player {}: {}", player_id, e);
                    player.game_state.score.clone()
                }
            };
            player.game_state.hands_left = hands_left;

            // Broadcast and evaluate
            lobby.broadcast_game_state_update(broadcaster, player_id, true);
            lobby.evaluate_online_round(broadcaster);
        }
    }

    fn handle_set_location(
        lobby: &mut Lobby,
        broadcaster: &LobbyBroadcaster,
        player_id: &str,
        location: String,
    ) {
        Self::update_player_and_broadcast(lobby, broadcaster, player_id, false, |player| {
            player.game_state.location = location;
        });
    }

    fn handle_skip(lobby: &mut Lobby, broadcaster: &LobbyBroadcaster, player_id: &str, blind: u32) {
        Self::update_player_and_broadcast(lobby, broadcaster, player_id, false, |player| {
            player.game_state.skips += 1;
            player.game_state.furthest_blind = blind;
        });
    }

    fn handle_update_hands_and_discards(
        lobby: &mut Lobby,
        broadcaster: &LobbyBroadcaster,
        player_id: &str,
        hands_max: u8,
        discards_max: u8,
    ) {
        debug!(
            "Player {} updating hands max to {} and discards max to {}",
            player_id, hands_max, discards_max
        );
        Self::update_player_and_broadcast(lobby, broadcaster, player_id, false, |player| {
            player.game_state.hands_max = hands_max;
            player.game_state.discards_max = discards_max;
        });
    }

    // Multiplayer joker handlers - these broadcast to other players
    fn handle_send_phantom(broadcaster: &LobbyBroadcaster, player_id: &str, key: String) {
        debug!("Player {} sending phantom joker: {}", player_id, key);
        broadcaster.broadcast_except(
            player_id,
            crate::messages::ServerToClient::SendPhantom { key },
        );
    }

    fn handle_remove_phantom(broadcaster: &LobbyBroadcaster, player_id: &str, key: String) {
        debug!("Player {} removing phantom joker: {}", player_id, key);
        broadcaster.broadcast_except(player_id, ServerToClient::RemovePhantom { key });
    }

    fn handle_asteroid(broadcaster: &LobbyBroadcaster, player_id: &str, target: &str) {
        debug!("Player {} sent asteroid to {}", player_id, target);
        broadcaster.send_to(
            player_id,
            ServerToClient::Asteroid {
                sender: target.to_string(),
            },
        );
    }

    fn handle_lets_go_gambling_nemesis(broadcaster: &LobbyBroadcaster, player_id: &str) {
        debug!("Player {} triggered lets go gambling nemesis", player_id);
        broadcaster.broadcast_except(player_id, ServerToClient::LetsGoGamblingNemesis {});
    }

    fn set_furthest_blind(
        lobby: &mut Lobby,
        broadcaster: &LobbyBroadcaster,
        player_id: &str,
        blind: u32,
    ) {
        debug!(
            "Player {} setting furthest blind to {}",
            player_id,
            blind.to_string()
        );
        if let Some(player) = lobby.get_player_mut(player_id) {
            player.game_state.furthest_blind = blind;
            lobby.broadcast_game_state_update(broadcaster, player_id, false);

            // Check for survival mode game end condition
            if lobby.lobby_options.gamemode == crate::game_mode::GameMode::Survival {
                let game_ended = lobby.check_survival_furthest_blind_win(broadcaster, player_id);
                if game_ended {
                    return;
                }
            }
        }
    }

    fn handle_eat_pizza(broadcaster: &LobbyBroadcaster, player_id: &str, discards: u8) {
        debug!(
            "Player {} eating pizza for {} discards",
            player_id, discards
        );
        broadcaster.broadcast_except(
            player_id,
            crate::messages::ServerToClient::EatPizza { discards },
        );
    }

    fn handle_sold_joker(broadcaster: &LobbyBroadcaster, player_id: &str) {
        debug!("Player {} sold a joker", player_id);
        broadcaster.broadcast_except(player_id, crate::messages::ServerToClient::SoldJoker {});
    }

    fn handle_spent_last_shop(broadcaster: &LobbyBroadcaster, player_id: &str, amount: u32) {
        //TODO fix the vector handling here
        debug!("Player {} spent {} in shop", player_id, amount);
        broadcaster.broadcast(crate::messages::ServerToClient::SpentLastShop {
            player_id: player_id.to_string(),
            amount,
        });
    }

    fn handle_magnet(broadcaster: &LobbyBroadcaster, player_id: &str) {
        debug!("Player {} triggered magnet", player_id);
        broadcaster.broadcast_except(player_id, crate::messages::ServerToClient::Magnet {});
    }

    fn handle_magnet_response(broadcaster: &LobbyBroadcaster, player_id: &str, key: String) {
        debug!("Player {} responding to magnet with: {}", player_id, key);
        broadcaster.broadcast_except(
            player_id,
            crate::messages::ServerToClient::MagnetResponse { key },
        );
    }

    fn handle_fail_timer(lobby: &mut Lobby, broadcaster: &LobbyBroadcaster, player_id: &str) {
        debug!("Player {} failed timer", player_id);
        lobby.apply_life_loss(&vec![player_id.to_string()]);
        lobby.broadcast_life_updates(broadcaster, player_id);
        let (game_over, winners, losers) = lobby.check_game_over();
        if game_over {
            lobby.handle_game_end(broadcaster, &winners, &losers);
        }
        broadcaster.broadcast(crate::messages::ServerToClient::PauseAnteTimer {
            time: (lobby.lobby_options.timer_base_seconds),
        });
    }

    pub fn handle_player_action(
        mut lobby: &mut Lobby,
        broadcaster: &LobbyBroadcaster,
        player_id: String,
        action: ClientToServer,
    ) {
        debug!("Player {} performed action: {:?}", player_id, action);
        match action {
            ClientToServer::PlayHand { score, hands_left } => {
                Self::handle_play_hand(&mut lobby, &broadcaster, &player_id, score, hands_left);
            }
            ClientToServer::SetLocation { location } => {
                Self::handle_set_location(&mut lobby, &broadcaster, &player_id, location);
            }
            ClientToServer::Skip { blind } => {
                Self::handle_skip(&mut lobby, &broadcaster, &player_id, blind);
            }
            ClientToServer::UpdateHandsAndDiscards {
                hands_max,
                discards_max,
            } => {
                Self::handle_update_hands_and_discards(
                    &mut lobby,
                    &broadcaster,
                    &player_id,
                    hands_max,
                    discards_max,
                );
            }
            ClientToServer::FailRound {} => {
                lobby.handle_player_fail_round(&player_id, &broadcaster);
            }
            ClientToServer::UpdateLobbyOptions { options } => {
                lobby.lobby_options = options;
                lobby.reset_ready_states_to_host_only();
                lobby.broadcast_ready_states_except(&broadcaster, &player_id);
                broadcaster.broadcast_except(
                    &player_id,
                    ServerToClient::UpdateLobbyOptions {
                        options: lobby.lobby_options.clone(),
                    },
                );
            }
            ClientToServer::StartGame { seed: _, stake } => {
                if lobby.is_player_host(&player_id) {
                    lobby.start_game();
                    broadcaster.broadcast(ServerToClient::ResetPlayers {
                        players: lobby.players().values().cloned().collect(),
                    });
                    broadcaster.broadcast(ServerToClient::GameStarted {
                        seed: lobby.lobby_options.custom_seed.clone(),
                        stake,
                    });
                    lobby.broadcast_ready_states(&broadcaster);
                    broadcaster.broadcast(ServerToClient::InGameStatuses {
                        statuses: lobby.get_in_game_statuses(),
                    });
                }
            }
            ClientToServer::StopGame {} => {
                lobby.started = false;
                lobby.reset_game_states(false);
                lobby.lobby_options.custom_seed = String::from("random");

                broadcaster.broadcast(ServerToClient::GameStopped {});
                lobby.reset_ready_states_to_host_only();
                lobby.broadcast_ready_states(&broadcaster);
                broadcaster.broadcast(ServerToClient::InGameStatuses {
                    statuses: lobby.get_in_game_statuses(),
                });
            }
            ClientToServer::SetReady { is_ready } => {
                lobby.set_player_ready(&player_id, is_ready);
                if lobby.started {
                    let all_ready = lobby
                        .players()
                        .values()
                        .filter(|p| p.lobby_state.in_game)
                        .all(|p| p.lobby_state.is_ready);
                    if all_ready {
                        lobby.start_online_blind(&broadcaster);
                    }
                } else {
                    lobby.broadcast_ready_states_except(&broadcaster, &player_id);
                }
            }
            ClientToServer::SetBossBlind { key, chips } => {
                if lobby.is_player_host(&player_id) {
                    debug!(
                        "Got SetBossBlind key: {}, chips: {}",
                        key,
                        chips.to_string()
                    );
                    lobby.boss_chips = chips;
                    broadcaster.broadcast_except(&player_id, ServerToClient::SetBossBlind { key });
                }
            }
            ClientToServer::SendPlayerDeck { deck } => {
                broadcaster.broadcast(ServerToClient::ReceivePlayerDeck {
                    player_id: player_id.clone(),
                    deck,
                });
            }
            ClientToServer::SendPhantom { key } => {
                Self::handle_send_phantom(&broadcaster, &player_id, key);
            }
            ClientToServer::RemovePhantom { key } => {
                Self::handle_remove_phantom(&broadcaster, &player_id, key);
            }
            ClientToServer::Asteroid { target } => {
                Self::handle_asteroid(&broadcaster, &target, &player_id);
            }
            ClientToServer::LetsGoGamblingNemesis {} => {
                Self::handle_lets_go_gambling_nemesis(&broadcaster, &player_id);
            }
            ClientToServer::EatPizza { discards } => {
                Self::handle_eat_pizza(&broadcaster, &player_id, discards);
            }
            ClientToServer::SoldJoker {} => {
                Self::handle_sold_joker(&broadcaster, &player_id);
            }
            ClientToServer::SpentLastShop { amount } => {
                Self::handle_spent_last_shop(&broadcaster, &player_id, amount);
            }
            ClientToServer::Magnet {} => {
                Self::handle_magnet(&broadcaster, &player_id);
            }
            ClientToServer::MagnetResponse { key } => {
                Self::handle_magnet_response(&broadcaster, &player_id, key);
            }
            ClientToServer::SetFurthestBlind { blind } => {
                Self::set_furthest_blind(&mut lobby, &broadcaster, &player_id, blind);
            }
            ClientToServer::StartAnteTimer { time } => {
                debug!(
                    "Starting ante timer in lobby {} with time: {}",
                    lobby.code, time
                );
                broadcaster.broadcast_except(&player_id, ServerToClient::StartAnteTimer { time });
            }
            ClientToServer::PauseAnteTimer { time } => {
                debug!(
                    "Pausing ante timer in lobby {} with time: {}",
                    lobby.code, time
                );
                broadcaster.broadcast_except(&player_id, ServerToClient::PauseAnteTimer { time });
            }
            ClientToServer::FailTimer {} => {
                LobbyHandlers::handle_fail_timer(&mut lobby, &broadcaster, &player_id);
            }
            ClientToServer::SendPlayerJokers { jokers } => {
                debug!("Sending jokers for player {}: {}", player_id, jokers);
                broadcaster.broadcast_except(
                    &player_id,
                    ServerToClient::ReceivePlayerJokers {
                        player_id: player_id.clone(),
                        jokers,
                    },
                );
            }
            ClientToServer::SendMoney {
                player_id: target_player_id,
            } => {
                broadcaster.send_to(&target_player_id, ServerToClient::ReceivedMoney {});
            }
            ClientToServer::Discard {} => todo!(),
            other => {
                debug!("Unhandled action from player {}: {:?}", player_id, other);
            }
        }
    }
}
