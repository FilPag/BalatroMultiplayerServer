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
    ) {
        Self::update_player_and_broadcast(lobby, broadcaster, player_id, false, |player| {
            player.game_state.skips += 1;
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
}
