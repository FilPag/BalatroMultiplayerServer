use std::sync::Arc;

use super::{broadcaster::LobbyBroadcaster, handlers::LobbyHandlers, lobby::Lobby};
use crate::{
    client::ClientProfile,
    game_mode::GameMode,
    messages::{CoordinatorMessage, LobbyMessage, ServerToClient},
};
use tokio::sync::mpsc;
use tracing::{debug, info};

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
                handle_client_join(
                    &mut lobby,
                    &mut broadcaster,
                    client_id,
                    client_profile,
                    client_response_tx,
                    &mut host_id,
                );
            }
            LobbyMessage::ClientLeave {
                client_id,
                coordinator_tx,
            } => {
                let shutdown = handle_client_leave(
                    &mut lobby,
                    &mut broadcaster,
                    client_id,
                    coordinator_tx,
                    &mut host_id,
                );
                if shutdown {
                    break;
                }
            }
        }
    }
    info!("Lobby {} task ended", lobby_code);
}

// --- Pure logic extraction ---
pub fn handle_client_join(
    lobby: &mut Lobby,
    broadcaster: &mut LobbyBroadcaster,
    client_id: String,
    client_profile: ClientProfile,
    client_response_tx: mpsc::UnboundedSender<Arc<ServerToClient>>,
    host_id: &mut String,
) {
    if lobby.is_full() {
        let _ = client_response_tx.send(Arc::new(ServerToClient::Error {
            message: String::from("Lobby is full"),
        }));
        return;
    }
    let lobby_entry = lobby.add_player(client_id.clone(), client_profile.clone());
    broadcaster.add_player(client_id.clone(), client_response_tx);

    if lobby.players().len() == 1 {
        *host_id = client_id.clone();
    }

    let player_joined_response = ServerToClient::player_joined_lobby(lobby_entry);
    let joined_response = ServerToClient::joined_lobby(client_id.clone(), lobby.clone());

    broadcaster.send_to(&client_id, joined_response);
    broadcaster.broadcast_except(&client_id, player_joined_response);
    debug!("Player {} joined lobby {}", client_id, lobby.code);
}

pub fn handle_client_leave(
    lobby: &mut Lobby,
    broadcaster: &mut LobbyBroadcaster,
    client_id: String,
    coordinator_tx: mpsc::UnboundedSender<CoordinatorMessage>,
    host_id: &mut String,
) -> bool {
    debug!("Player {} leaving lobby {}", client_id, lobby.code);
    broadcaster.remove_player(&client_id);
    let Some(leaving_player) = lobby.remove_player(&client_id) else {
        return false;
    };
    if lobby.players().is_empty() {
        let _ = coordinator_tx.send(CoordinatorMessage::LobbyShutdown {
            lobby_code: lobby.code.clone(),
        });
        return true; // signal shutdown
    }
    if leaving_player.lobby_state.is_host {
        if let Some(new_host_id) = lobby.promote_new_host() {
            *host_id = new_host_id;
        }
    }
    let player_left_response =
        ServerToClient::player_left_lobby(client_id.clone(), host_id.clone());
    broadcaster.broadcast(player_left_response);
    if lobby.started && lobby.get_player_count_in_game() < 2 {
        lobby.stop_game();
        broadcaster.broadcast(ServerToClient::GameStopped {});
    }
    debug!("Player {} left lobby {}", client_id, lobby.code);
    false
}

mod tests {
    #[allow(unused)]
    use super::*;
    #[allow(unused)]
    use crate::client::ClientProfile;
    #[allow(unused)]
    use crate::messages::ServerToClient;
    #[allow(unused)]
    use crate::test_utils::contains_response_of_type;
    #[allow(unused)]
    use std::sync::Arc;
    #[allow(unused)]
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_client_join() {
        let (response_tx, mut response_rx) = mpsc::unbounded_channel();
        let mut lobby = Lobby::new(
            "TEST".to_string(),
            "default".to_string(),
            GameMode::Attrition,
        );
        let mut broadcaster = LobbyBroadcaster::new();
        let mut host_id = String::new();
        let profile = ClientProfile::default();
        // Not full
        handle_client_join(
            &mut lobby,
            &mut broadcaster,
            "player1".to_string(),
            profile.clone(),
            response_tx.clone(),
            &mut host_id,
        );
        // Should have joined
        let responses: Vec<_> = std::iter::from_fn(|| response_rx.try_recv().ok()).collect();
        let joined_variant = ServerToClient::joined_lobby("player1".to_string(), lobby.clone());
        assert!(contains_response_of_type(&responses, &joined_variant));

        // add second player
        lobby.add_player("player2".to_string(), profile.clone());

        // Try to join when full
        handle_client_join(
            &mut lobby,
            &mut broadcaster,
            "player3".to_string(),
            profile.clone(),
            response_tx.clone(),
            &mut host_id,
        );
        let responses: Vec<_> = std::iter::from_fn(|| response_rx.try_recv().ok()).collect();
        let error_variant = ServerToClient::Error {
            message: "Lobby is full".to_string(),
        };
        assert!(contains_response_of_type(&responses, &error_variant));
    }

    #[tokio::test]
    async fn test_client_leave() {
        let (coordinator_tx, mut coordinator_rx) = mpsc::unbounded_channel();
        let mut lobby = Lobby::new(
            "TEST".to_string(),
            "default".to_string(),
            GameMode::Attrition,
        );
        let mut broadcaster = LobbyBroadcaster::new();
        let mut host_id = String::new();
        let profile = ClientProfile::default();
        // Add player
        lobby.add_player("player1".to_string(), profile.clone());
        // Leave
        let shutdown = handle_client_leave(
            &mut lobby,
            &mut broadcaster,
            "player1".to_string(),
            coordinator_tx.clone(),
            &mut host_id,
        );
        assert!(shutdown, "Should signal shutdown when last player leaves");
        // Check coordinator received shutdown
        let msg = coordinator_rx
            .try_recv()
            .expect("Expected shutdown message");
        match msg {
            CoordinatorMessage::LobbyShutdown { lobby_code } => {
                assert_eq!(lobby_code, "TEST");
            }
            _ => panic!("Expected LobbyShutdown message"),
        }
    }
}
