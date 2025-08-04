use super::{broadcaster::LobbyBroadcaster, lobby::Lobby};
use crate::talisman_number::TalismanNumber;
use tracing::{debug, error};
use uuid::Uuid;

// KISS: Group related handlers
pub struct LobbyHandlers;

impl LobbyHandlers {
    // DRY: Common pattern - update player state, then broadcast
    fn update_player_and_broadcast<F>(
        lobby: &mut Lobby,
        broadcaster: &LobbyBroadcaster,
        player_id: Uuid,
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

    pub fn handle_play_hand(
        lobby: &mut Lobby,
        broadcaster: &LobbyBroadcaster,
        player_id: Uuid,
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

    pub fn handle_set_location(
        lobby: &mut Lobby,
        broadcaster: &LobbyBroadcaster,
        player_id: Uuid,
        location: String,
    ) {
        Self::update_player_and_broadcast(lobby, broadcaster, player_id, false, |player| {
            player.game_state.location = location;
        });
    }

    pub fn handle_skip(
        lobby: &mut Lobby,
        broadcaster: &LobbyBroadcaster,
        player_id: Uuid,
        blind: u32
    ) {
        Self::update_player_and_broadcast(lobby, broadcaster, player_id, false, |player| {
            player.game_state.skips += 1;
            player.game_state.furthest_blind = blind;
        });
    }

    pub fn handle_update_hands_and_discards(
        lobby: &mut Lobby,
        broadcaster: &LobbyBroadcaster,
        player_id: Uuid,
        hands_max: u8,
        discards_max: u8,
    ) {
        Self::update_player_and_broadcast(lobby, broadcaster, player_id, false, |player| {
            player.game_state.hands_max = hands_max;
            player.game_state.discards_max = discards_max;
        });
    }

    // Multiplayer joker handlers - these broadcast to other players
    pub fn handle_send_phantom(
        broadcaster: &LobbyBroadcaster,
        player_id: Uuid,
        key: String,
    ) {
        debug!("Player {} sending phantom joker: {}", player_id, key);
        broadcaster.broadcast_except(player_id, crate::actions::ServerToClient::SendPhantom { key });
    }

    pub fn handle_remove_phantom(
        broadcaster: &LobbyBroadcaster,
        player_id: Uuid,
        key: String,
    ) {
        debug!("Player {} removing phantom joker: {}", player_id, key);
        broadcaster.broadcast_except(player_id, crate::actions::ServerToClient::RemovePhantom { key });
    }

    pub fn handle_asteroid(
        broadcaster: &LobbyBroadcaster,
        player_id: Uuid,
    ) {
        debug!("Player {} triggered asteroid", player_id);
        broadcaster.broadcast_except(player_id, crate::actions::ServerToClient::Asteroid {});
    }

    pub fn handle_lets_go_gambling_nemesis(
        broadcaster: &LobbyBroadcaster,
        player_id: Uuid,
    ) {
        debug!("Player {} triggered lets go gambling nemesis", player_id);
        broadcaster.broadcast_except(player_id, crate::actions::ServerToClient::LetsGoGamblingNemesis {});
    }

    pub fn set_furthest_blind(
        lobby: &mut Lobby,
        broadcaster: &LobbyBroadcaster,
        player_id: Uuid,
        blind: u32,
    ) {
        debug!("Player {} setting furthest blind to {}", player_id, blind.to_string());
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

    pub fn handle_eat_pizza(
        broadcaster: &LobbyBroadcaster,
        player_id: Uuid,
        discards: u8,
    ) {
        debug!("Player {} eating pizza for {} discards", player_id, discards);
        broadcaster.broadcast_except(player_id, crate::actions::ServerToClient::EatPizza { discards });
    }

    pub fn handle_sold_joker(
        broadcaster: &LobbyBroadcaster,
        player_id: Uuid,
    ) {
        debug!("Player {} sold a joker", player_id);
        broadcaster.broadcast_except(player_id, crate::actions::ServerToClient::SoldJoker {});
    }

    pub fn handle_spent_last_shop(
        broadcaster: &LobbyBroadcaster,
        player_id: Uuid,
        amount: u32,
    ) {
        //TODO fix the vector handling here
        debug!("Player {} spent {} in shop", player_id, amount);
        broadcaster.broadcast(crate::actions::ServerToClient::SpentLastShop { player_id, amount });
    }

    pub fn handle_magnet(
        broadcaster: &LobbyBroadcaster,
        player_id: Uuid,
    ) {
        debug!("Player {} triggered magnet", player_id);
        broadcaster.broadcast_except(player_id, crate::actions::ServerToClient::Magnet {});
    }

    pub fn handle_magnet_response(
        broadcaster: &LobbyBroadcaster,
        player_id: Uuid,
        key: String,
    ) {
        debug!("Player {} responding to magnet with: {}", player_id, key);
        broadcaster.broadcast_except(player_id, crate::actions::ServerToClient::MagnetResponse { key });
    }
}
