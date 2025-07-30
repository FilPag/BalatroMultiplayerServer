use crate::actions::ServerToClient;
use crate::client::{Client, ClientProfile};
use crate::game_mode::{GameMode, LobbyOptions};
use crate::insane_int::InsaneInt;
use crate::messages::{CoordinatorMessage, LobbyMessage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{debug, info};
use uuid::Uuid;

/// Simple lobby coordinator that routes messages to individual lobby tasks
#[derive(Debug, Clone, Serialize)]
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
    pub round: u32,
    pub furthest_blind: u32,
    pub hands_left: u8,
    pub hands_max: u8,
    pub discards_left: u8,
    pub discards_max: u8,
    pub lives: u8,
    pub lives_blocker: bool,
    pub location: String,
    pub skips: u8,
    pub score: InsaneInt,
    pub highest_score: InsaneInt,
    pub spent_in_shop: Vec<u32>,
}

impl Default for ClientGameState {
    fn default() -> Self {
        Self {
            ante: 0,
            round: 1,
            furthest_blind: 1,
            hands_left: 4,
            hands_max: 4,
            discards_left: 3,
            discards_max: 3,
            lives: 2,
            lives_blocker: false,
            location: String::from("loc_selecting"),
            skips: 0,
            score: InsaneInt::empty(),
            highest_score: InsaneInt::empty(),
            spent_in_shop: Vec::new(),
        }
    }
}

// Complete client data container
#[derive(Debug, Clone, Serialize)]
pub struct ClientLobbyEntry {
    pub profile: ClientProfile,
    pub lobby_state: ClientLobbyState,
    pub game_state: ClientGameState, // None when not in game
}

#[derive(Debug, Clone, Serialize)]
struct Lobby {
    code: String,
    lobby_options: LobbyOptions,
    players: HashMap<Uuid, ClientLobbyEntry>,
}

impl Lobby {
    fn new(code: String, game_mode: GameMode) -> Self {
        Self {
            code,
            lobby_options: game_mode.get_default_options(),
            players: HashMap::new(),
        }
    }

    fn add_player(&mut self, player_id: Uuid, client_profile: ClientProfile) -> ClientLobbyEntry {
        let lobby_entry = ClientLobbyEntry {
            profile: client_profile,
            lobby_state: ClientLobbyState {
                current_lobby: Some(self.code.clone()),
                is_ready: self.players.is_empty(),
                first_ready: false,
                is_cached: false,
                is_host: self.players.is_empty(),
            },
            game_state: ClientGameState::default(),
        };
        
        self.players.insert(player_id, lobby_entry.clone());
        lobby_entry
    }

    fn remove_player(&mut self, player_id: Uuid) -> Option<ClientLobbyEntry> {
        self.players.remove(&player_id)
    }

    fn promote_new_host(&mut self) -> Option<Uuid> {
        if let Some((&new_host_id, new_host_entry)) = self.players.iter_mut().next() {
            new_host_entry.lobby_state.is_host = true;
            new_host_entry.lobby_state.is_ready = true;
            Some(new_host_id)
        } else {
            None
        }
    }

    fn reset_game_states(&mut self) {
        for player in self.players.values_mut() {
            player.lobby_state.is_ready = false;
            player.game_state = ClientGameState::default();
        }
    }

    fn reset_ready_states_to_host_only(&mut self) {
        for player in self.players.values_mut() {
            player.lobby_state.is_ready = player.lobby_state.is_host;
        }
    }

    fn collect_ready_states(&self) -> HashMap<Uuid, bool> {
        return self.players
            .iter()
            .map(|(&id, entry)| (id, entry.lobby_state.is_ready))
            .collect()
    }

    fn start_game(&mut self) {
        for player in self.players.values_mut() {
            player.lobby_state.is_ready = false;
            player.game_state = ClientGameState::default();
            player.game_state.lives = self.lobby_options.starting_lives;
        }
    }

    fn set_player_ready(&mut self, player_id: Uuid, is_ready: bool) -> bool {
        if let Some(player) = self.players.get_mut(&player_id) {
            player.lobby_state.is_ready = is_ready;
            true
        } else {
            false
        }
    }

    fn is_player_host(&self, player_id: Uuid) -> bool {
        self.players
            .get(&player_id)
            .map(|player| player.lobby_state.is_host)
            .unwrap_or(false)
    }

    fn broadcast_game_state_update(&self, broadcaster: &LobbyBroadcaster, player_id: Uuid, exclude_player: bool) {
        if let Some(player) = self.players.get(&player_id) {
            let update = ServerToClient::GameStateUpdate {
                player_id,
                game_state: player.game_state.clone(),
            };
            
            if exclude_player {
                broadcaster.broadcast_except(player_id, update);
            } else {
                broadcaster.broadcast(update);
            }
        }
    }

    fn broadcast_game_state_to_all(&self, broadcaster: &LobbyBroadcaster, player_id: Uuid) {
        self.broadcast_game_state_update(broadcaster, player_id, false);
    }

    fn broadcast_game_state_except_player(&self, broadcaster: &LobbyBroadcaster, player_id: Uuid) {
        self.broadcast_game_state_update(broadcaster, player_id, true);
    }

    fn broadcast_ready_states(&self, broadcaster: &LobbyBroadcaster) {
        let ready_states = self.collect_ready_states();
        broadcaster.broadcast(ServerToClient::LobbyReady { ready_states});
    }

    fn broadcast_ready_states_except(&self, broadcaster: &LobbyBroadcaster, except_player: Uuid) {
        let ready_states = self.collect_ready_states();
        broadcaster.broadcast_except(except_player, ServerToClient::LobbyReady { ready_states });
    }
}

struct LobbyBroadcaster {
    player_senders: HashMap<Uuid, mpsc::UnboundedSender<String>>,
}

impl LobbyBroadcaster {
    fn new() -> Self {
        Self {
            player_senders: HashMap::new(),
        }
    }

    fn add_player(&mut self, player_id: Uuid, sender: mpsc::UnboundedSender<String>) {
        self.player_senders.insert(player_id, sender);
    }

    fn remove_player(&mut self, player_id: Uuid) {
        self.player_senders.remove(&player_id);
    }

    fn send_to(&self, player_id: Uuid, response: ServerToClient) {
        if let Some(sender) = self.player_senders.get(&player_id) {
            let _ = sender.send(response.to_json());
        }
    }

    fn broadcast(&self, response: ServerToClient) {
        let message = response.to_json();
        for sender in self.player_senders.values() {
            let _ = sender.send(message.clone());
        }
    }

    fn broadcast_except(&self, except: Uuid, response: ServerToClient) {
        let message = response.to_json();
        for (&player_id, sender) in self.player_senders.iter() {
            if player_id != except {
                let _ = sender.send(message.clone());
            }
        }
    }
}

/// Individual lobby task - handles 2-4 players
pub async fn lobby_task(
    lobby_code: String,
    mut rx: mpsc::UnboundedReceiver<LobbyMessage>,
    ruleset: String,
    game_mode: GameMode,
) {
    let mut lobby = Lobby::new(lobby_code.clone(), game_mode);
    let mut broadcaster = LobbyBroadcaster::new();
    let mut host_id = Uuid::nil();

    info!(
        "Lobby {} started (ruleset: {}, mode: {})",
        lobby_code, ruleset, game_mode
    );

    while let Some(msg) = rx.recv().await {
        match msg {
            LobbyMessage::PlayerJoined {
                player_id,
                client_response_tx,
                client_profile,
            } => {
                broadcaster.add_player(player_id, client_response_tx.clone());

                let lobby_entry = lobby.add_player(player_id, client_profile.clone());

                if lobby.players.len() == 1 {
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

                if lobby.players.is_empty() {
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
            LobbyMessage::LobbyJoinData { .. } => {
                tracing::warn!("LobbyJoinData handler not implemented");
            }
            LobbyMessage::UpdateLobbyOptions { player_id, options } => {
                lobby.lobby_options = options;
                lobby.reset_ready_states_to_host_only();
                lobby.broadcast_ready_states(&broadcaster);
                broadcaster.broadcast_except(
                    player_id,
                    ServerToClient::UpdateLobbyOptions {
                        options: lobby.lobby_options.clone(),
                    },
                );
            }
            LobbyMessage::StartGame {
                player_id,
                seed,
                stake,
            } => {
                if lobby.is_player_host(player_id) {
                    lobby.start_game();
                    broadcaster.broadcast(ServerToClient::GameStarted { seed, stake });
                }
            }

            LobbyMessage::StopGame { player_id: _ } => {
                lobby.reset_game_states();

                broadcaster.broadcast(ServerToClient::GameStoppend {});
                lobby.reset_ready_states_to_host_only();
                lobby.broadcast_ready_states(&broadcaster);
                broadcaster.broadcast(
                    ServerToClient::ResetPlayers {
                        players: lobby.players.values().cloned().collect(),
                    },
                );
            }

            LobbyMessage::SetReady {
                player_id,
                is_ready,
            } => {
                if lobby.set_player_ready(player_id, is_ready) {
                    lobby.broadcast_ready_states_except(&broadcaster, player_id);
                }
            }
            LobbyMessage::UpdateHandsAndDiscards {
                player_id,
                hands_max,
                discards_max,
            } => {
                if let Some(player) = lobby.players.get_mut(&player_id) {
                    player.game_state.hands_max = hands_max;
                    player.game_state.discards_max = discards_max;
                }
                lobby.broadcast_game_state_to_all(&broadcaster, player_id);
            }
            LobbyMessage::PlayHand {
                player_id,
                score,
                hands_remaining,
            } => {
                if let Some(player) = lobby.players.get_mut(&player_id) {
                    player.game_state.score = InsaneInt::from_string(&score).unwrap_or_else(|e| {
                        tracing::error!("Failed to parse score '{}': {}", score, e);
                        InsaneInt::empty()
                    });
                    player.game_state.hands_left = hands_remaining;
                }
                lobby.broadcast_game_state_except_player(&broadcaster, player_id);
            }
            LobbyMessage::StartOnlineBlind { player_id: _ } => {
                tracing::warn!("StartOnlineBlind handler not implemented");
            }
            LobbyMessage::SetBossBlind {
                player_id: _,
                boss_blind: _,
            } => {
                tracing::warn!("SetBossBlind handler not implemented");
            }
            LobbyMessage::FailRound { player_id: _ } => {
                tracing::warn!("FailRound handler not implemented");
            }
            LobbyMessage::SetLocation {
                player_id,
                location,
            } => {
                if let Some(player) = lobby.players.get_mut(&player_id) {
                    player.game_state.location = location;
                }
                lobby.broadcast_game_state_to_all(&broadcaster, player_id);
            }
            LobbyMessage::SkipBlind { player_id: _ } => {
                tracing::warn!("SkipBlind handler not implemented");
            }
        }
    }
    debug!("Lobby {} task ended", lobby_code);
}
