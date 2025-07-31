use crate::actions::ServerToClient;
use crate::client::ClientProfile;
use crate::game_mode::{GameMode, LobbyOptions};
use crate::insane_int::InsaneInt;
use crate::messages::{CoordinatorMessage, LobbyMessage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{debug, error, info};
use tracing_subscriber::field::debug;
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
    started: bool,
    boss_chips: InsaneInt,
    lobby_options: LobbyOptions,
    players: HashMap<Uuid, ClientLobbyEntry>,
}

impl Lobby {
    fn new(code: String, game_mode: GameMode) -> Self {
        Self {
            code,
            started: false,
            boss_chips: InsaneInt::empty(),
            lobby_options: game_mode.get_default_options(),
            players: HashMap::new(),
        }
    }

    fn add_player(&mut self, player_id: Uuid, client_profile: ClientProfile) -> ClientLobbyEntry {
        let mut lobby_entry = ClientLobbyEntry {
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

        lobby_entry.game_state.lives = self.lobby_options.starting_lives;
        self.players.insert(player_id, lobby_entry.clone());
        return lobby_entry;
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

    fn reset_ready_states(&mut self) {
        for player in self.players.values_mut() {
            player.lobby_state.is_ready = false;
        }
    }

    fn reset_ready_states_to_host_only(&mut self) {
        for player in self.players.values_mut() {
            player.lobby_state.is_ready = player.lobby_state.is_host;
        }
    }

    fn collect_ready_states(&self) -> HashMap<Uuid, bool> {
        return self
            .players
            .iter()
            .map(|(&id, entry)| (id, entry.lobby_state.is_ready))
            .collect();
    }

    fn start_game(&mut self) {
        self.started = true;
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

    fn reset_scores(&mut self) {
        for player in self.players.values_mut() {
            player.game_state.score = InsaneInt::empty();
            player.game_state.hands_left = player.game_state.hands_max;
            player.game_state.discards_left = player.game_state.discards_max;
        }
    }

    fn get_total_score(&self) -> InsaneInt {
        let mut acc = InsaneInt::empty();
        for player in self.players.values() {
            acc += player.game_state.score.clone();
        }
        return acc;
    }

    fn all_players_done(&self) -> bool {
        self.players.values().all(|p| p.game_state.hands_left == 0)
    }

    fn evaluate_online_round(&mut self, broadcaster: &LobbyBroadcaster) -> bool {
        if !self.all_players_done() {
            return false;
        }

        debug!("Evaluating online round for lobby {}", self.code);

        let (winners, losers) = self.check_round_victory();
        self.apply_round_results(winners.clone(), losers.clone());

        if self.is_someone_dead() {
            debug!("Someone is dead in lobby {}, ending round", self.code);

            for &player_id in winners.iter() {
                broadcaster.send_to(player_id, ServerToClient::WinGame {});
            }

            for &player_id in losers.iter() {
                broadcaster.send_to(player_id, ServerToClient::LoseGame {});
            }
        }

        self.reset_scores();
        self.broadcast_round_end(broadcaster, winners, losers);

        true
    }

    fn check_round_victory(&self) -> (Vec<Uuid>, Vec<Uuid>) {
        match self.lobby_options.gamemode {
            GameMode::CoopSurvival => {
                if self.get_total_score().greater_than(&self.boss_chips) {
                    (self.players.keys().cloned().collect(), Vec::new())
                } else {
                    (Vec::new(), self.players.keys().cloned().collect())
                }
            }
            _ => {
                let mut player_entries: Vec<(&Uuid, &ClientLobbyEntry)> =
                    self.players.iter().collect();
                if player_entries.len() < 2 {
                    error!("Not enough players to evaluate round");
                    return (Vec::new(), Vec::new());
                }

                let top_score = &player_entries[0].1.game_state.score;
                let winners: Vec<Uuid> = player_entries
                    .iter()
                    .filter(|(_, p)| p.game_state.score == *top_score)
                    .map(|(id, _)| **id)
                    .collect();
                let losers: Vec<Uuid> = player_entries
                    .iter()
                    .filter(|(_, p)| p.game_state.score != *top_score)
                    .map(|(id, _)| **id)
                    .collect();

                (winners, losers)
            }
        }
    }

    fn apply_round_results(&mut self, winners: Vec<Uuid>, losers: Vec<Uuid>) {
        for loser in losers {
            if let Some(player) = self.players.get_mut(&loser) {
                player.game_state.lives -= 1;
            }
        }
    }

    fn broadcast_round_end(
        &self,
        broadcaster: &LobbyBroadcaster,
        winners: Vec<Uuid>,
        losers: Vec<Uuid>,
    ) {
        // Broadcast updated game states first
        for player in self.players.values() {
            self.broadcast_game_state_update(broadcaster, player.profile.id, false);
        }

        for &player_id in winners.iter() {
            debug!("Player {} won the round in lobby {}", player_id, self.code);
            broadcaster.send_to(player_id, ServerToClient::EndPvp { won: true });
        }

        for &player_id in losers.iter() {
            debug!("Player {} lost the round in lobby {}", player_id, self.code);
            broadcaster.send_to(player_id, ServerToClient::EndPvp { won: false });
        }
    }

    fn is_someone_dead(&self) -> bool {
        self.players.values().any(|p| p.game_state.lives == 0)
    }

    fn start_online_blind(&mut self, broadcaster: &LobbyBroadcaster) {
        self.reset_ready_states();
        self.reset_scores();
        broadcaster.broadcast(ServerToClient::StartBlind {});
        self.broadcast_ready_states(broadcaster);
    }

    fn is_player_host(&self, player_id: Uuid) -> bool {
        self.players
            .get(&player_id)
            .map(|player| player.lobby_state.is_host)
            .unwrap_or(false)
    }

    fn broadcast_game_state_update(
        &self,
        broadcaster: &LobbyBroadcaster,
        player_id: Uuid,
        exclude_player: bool,
    ) {
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
        broadcaster.broadcast(ServerToClient::LobbyReady { ready_states });
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
                if lobby.started {
                    let _ = client_response_tx.send(
                        ServerToClient::Error {
                            message: String::from("Lobby is already started"),
                        }
                        .to_json(),
                    );
                    continue;
                }

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
                seed,
                stake,
            } => {
                if lobby.is_player_host(player_id) {
                    lobby.start_game();
                    broadcaster.broadcast(ServerToClient::GameStarted { seed, stake });
                    lobby.broadcast_ready_states(&broadcaster);
                }
            }

            LobbyMessage::StopGame { player_id } => {
                lobby.reset_game_states();
                lobby.started = false;

                broadcaster.broadcast(ServerToClient::GameStopped {});
                lobby.reset_ready_states_to_host_only();
                lobby.broadcast_ready_states(&broadcaster);
                broadcaster.broadcast(ServerToClient::ResetPlayers {
                    players: lobby.players.values().cloned().collect(),
                });
            }

            LobbyMessage::SetReady {
                player_id,
                is_ready,
            } => {
                if lobby.set_player_ready(player_id, is_ready) {
                    if lobby.started {
                        let all_ready = lobby.players.values().all(|p| p.lobby_state.is_ready);
                        if all_ready {
                            lobby.start_online_blind(&broadcaster);
                        }
                    } else {
                        lobby.broadcast_ready_states_except(&broadcaster, player_id);
                    }
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
                    lobby.broadcast_game_state_to_all(&broadcaster, player_id);
                }
            }
            LobbyMessage::PlayHand {
                player_id,
                score,
                hands_left,
            } => {
                if let Some(player) = lobby.players.get_mut(&player_id) {
                    debug!(
                        "Player {} played hand with score {} and hands left {}",
                        player_id,
                        score.to_string(),
                        hands_left
                    );
                    player.game_state.score += score;
                    player.game_state.hands_left = hands_left;
                    if lobby.evaluate_online_round(&broadcaster) {
                        // Round was evaluated and ended
                    } else {
                        lobby.broadcast_game_state_except_player(&broadcaster, player_id);
                    }
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
