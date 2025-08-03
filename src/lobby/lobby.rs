use super::{broadcaster::LobbyBroadcaster, game_state::ClientLobbyEntry};
use crate::{
    actions::ServerToClient,
    client::ClientProfile,
    game_mode::{GameMode, LobbyOptions},
    talisman_number::TalismanNumber,
    utils::time_based_string,
};
use serde::Serialize;
use std::collections::HashMap;
use tracing::{debug, error};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Lobby {
    pub code: String,
    pub started: bool,
    pub boss_chips: TalismanNumber,
    pub lobby_options: LobbyOptions,
    players: HashMap<Uuid, ClientLobbyEntry>,
}

impl Lobby {
    pub fn new(code: String, game_mode: GameMode) -> Self {
        Self {
            code,
            started: false,
            boss_chips: TalismanNumber::Regular(0.0),
            lobby_options: game_mode.get_default_options(),
            players: HashMap::new(),
        }
    }

    // KISS: Simple accessors
    pub fn get_player(&self, player_id: Uuid) -> Option<&ClientLobbyEntry> {
        self.players.get(&player_id)
    }

    pub fn get_player_mut(&mut self, player_id: Uuid) -> Option<&mut ClientLobbyEntry> {
        self.players.get_mut(&player_id)
    }

    pub fn players(&self) -> &HashMap<Uuid, ClientLobbyEntry> {
        &self.players
    }

    // DRY: Extract common player operations
    pub fn add_player(&mut self, player_id: Uuid, client_profile: ClientProfile) -> ClientLobbyEntry {
        let is_host = self.players.is_empty();
        let entry = ClientLobbyEntry::new(
            client_profile,
            self.code.clone(),
            is_host,
            self.lobby_options.starting_lives,
        );
        self.players.insert(player_id, entry.clone());
        entry
    }

    pub fn remove_player(&mut self, player_id: Uuid) -> Option<ClientLobbyEntry> {
        self.players.remove(&player_id)
    }

    pub fn promote_new_host(&mut self) -> Option<Uuid> {
        if let Some((&new_host_id, new_host_entry)) = self.players.iter_mut().next() {
            new_host_entry.lobby_state.is_host = true;
            new_host_entry.lobby_state.is_ready = true;
            Some(new_host_id)
        } else {
            None
        }
    }

    pub fn is_player_host(&self, player_id: Uuid) -> bool {
        self.players
            .get(&player_id)
            .map(|p| p.lobby_state.is_host)
            .unwrap_or(false)
    }

    // DRY: Consolidated ready state operations
    pub fn reset_ready_states(&mut self) {
        for player in self.players.values_mut() {
            player.lobby_state.is_ready = false;
        }
    }

    pub fn reset_ready_states_to_host_only(&mut self) {
        for player in self.players.values_mut() {
            player.lobby_state.is_ready = player.lobby_state.is_host;
        }
    }

    pub fn set_player_ready(&mut self, player_id: Uuid, is_ready: bool) -> bool {
        if let Some(player) = self.players.get_mut(&player_id) {
            player.lobby_state.is_ready = is_ready;
            true
        } else {
            false
        }
    }

    pub fn collect_ready_states(&self) -> HashMap<Uuid, bool> {
        self.players
            .iter()
            .map(|(&id, entry)| (id, entry.lobby_state.is_ready))
            .collect()
    }

    // Game state management
    pub fn reset_game_states(&mut self) {
        for player in self.players.values_mut() {
            player.reset_for_game(self.lobby_options.starting_lives);
        }
    }

    pub fn start_game(&mut self) {
        self.started = true;
        if (!self.lobby_options.different_seeds)
            && self.lobby_options.custom_seed == String::from("random")
        {
            self.lobby_options.custom_seed = time_based_string(8);
            debug!(
                "Generating time-based seed for lobby {} seed: {}",
                self.code, self.lobby_options.custom_seed
            );
        }
        self.reset_game_states();
    }

    pub fn reset_scores(&mut self) {
        for player in self.players.values_mut() {
            player.game_state.score = TalismanNumber::Regular(0.0);
            player.game_state.hands_left = player.game_state.hands_max;
            player.game_state.discards_left = player.game_state.discards_max;
        }
    }

    pub fn get_total_score(&self) -> TalismanNumber {
        let mut acc = TalismanNumber::Regular(0.0);
        for player in self.players.values() {
            acc = acc.add(&player.game_state.score).unwrap_or(acc.clone());
        }
        acc
    }

    pub fn all_players_done(&self) -> bool {
        self.players.values().all(|p| p.game_state.hands_left == 0)
    }

    pub fn is_someone_dead(&self) -> bool {
        self.players.values().any(|p| p.game_state.lives == 0)
    }

    // Game logic - kept in lobby for now but could be moved to game_logic module
    pub fn evaluate_online_round(&mut self, broadcaster: &LobbyBroadcaster) -> bool {
        if !self.all_players_done() {
            return false;
        }

        debug!("Evaluating boss battle for lobby {}", self.code);

        let (winners, losers) = self.check_round_victory();
        self.apply_round_results(&losers);
        self.broadcast_all_game_states(broadcaster);

        let game_ended = self.is_someone_dead();

        if game_ended {
            self.handle_game_end(broadcaster, &winners, &losers);
        } else {
            self.reset_scores();
            self.send_outcome_messages(broadcaster, &winners, &losers, false);
        }

        game_ended
    }

    pub fn handle_player_fail_round(&mut self, player_id: Uuid, broadcaster: &LobbyBroadcaster) -> bool {
        debug!("Player {} failed a round in lobby {}", player_id, self.code);

        self.apply_fail_round_life_loss(player_id);
        self.broadcast_life_updates(broadcaster, player_id);

        let game_ended = self.is_someone_dead();

        if game_ended {
            let (winners, losers) = self.determine_game_end_results();
            self.handle_game_end(broadcaster, &winners, &losers);
        }

        game_ended
    }

    fn check_round_victory(&self) -> (Vec<Uuid>, Vec<Uuid>) {
        match self.lobby_options.gamemode {
            GameMode::CoopSurvival => {
                if self.get_total_score() > self.boss_chips {
                    (Vec::new(), Vec::new())
                } else {
                    (Vec::new(), self.players.keys().cloned().collect())
                }
            }
            _ => {
                let player_entries: Vec<(&Uuid, &ClientLobbyEntry)> = self.players.iter().collect();
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

    fn determine_game_end_results(&self) -> (Vec<Uuid>, Vec<Uuid>) {
        match self.lobby_options.gamemode {
            GameMode::CoopSurvival => {
                (Vec::new(), self.players.keys().cloned().collect())
            }
            _ => {
                let mut winners = Vec::new();
                let mut losers = Vec::new();

                for (&player_id, player) in &self.players {
                    if player.game_state.lives > 0 {
                        winners.push(player_id);
                    } else {
                        losers.push(player_id);
                    }
                }

                (winners, losers)
            }
        }
    }

    fn apply_round_results(&mut self, losers: &[Uuid]) {
        for &loser in losers {
            if let Some(player) = self.players.get_mut(&loser) {
                player.game_state.lives = player.game_state.lives.saturating_sub(1);
            }
        }
    }

    fn apply_fail_round_life_loss(&mut self, player_id: Uuid) {
        if !self.lobby_options.death_on_round_loss {
            return;
        }

        match self.lobby_options.gamemode {
            GameMode::CoopSurvival => {
                for player in self.players.values_mut() {
                    player.game_state.lives = player.game_state.lives.saturating_sub(1);
                }
            }
            _ => {
                if let Some(player) = self.players.get_mut(&player_id) {
                    player.game_state.lives = player.game_state.lives.saturating_sub(1);
                }
            }
        }
    }

    fn handle_game_end(&self, broadcaster: &LobbyBroadcaster, winners: &[Uuid], losers: &[Uuid]) {
        debug!("Someone is dead in lobby {}, ending game", self.code);
        self.send_outcome_messages(broadcaster, winners, losers, true);
    }

    fn send_outcome_messages(
        &self,
        broadcaster: &LobbyBroadcaster,
        winners: &[Uuid],
        losers: &[Uuid],
        is_game_end: bool,
    ) {
        for &player_id in winners {
            let message = if is_game_end {
                debug!("Player {} won the game in lobby {}", player_id, self.code);
                ServerToClient::WinGame {}
            } else {
                debug!("Player {} won the round in lobby {}", player_id, self.code);
                ServerToClient::EndPvp { won: true }
            };
            broadcaster.send_to(player_id, message);
        }

        for &player_id in losers {
            let message = if is_game_end {
                debug!("Player {} lost the game in lobby {}", player_id, self.code);
                ServerToClient::LoseGame {}
            } else {
                debug!("Player {} lost the round in lobby {}", player_id, self.code);
                ServerToClient::EndPvp { won: false }
            };
            broadcaster.send_to(player_id, message);
        }
    }

    // Broadcasting helpers
    pub fn broadcast_all_game_states(&self, broadcaster: &LobbyBroadcaster) {
        for player in self.players.values() {
            self.broadcast_game_state_update(broadcaster, player.profile.id, false);
        }
    }

    pub fn broadcast_life_updates(&self, broadcaster: &LobbyBroadcaster, player_id: Uuid) {
        if self.lobby_options.gamemode == GameMode::CoopSurvival {
            self.broadcast_all_game_states(broadcaster);
        } else {
            self.broadcast_game_state_update(broadcaster, player_id, false);
        }
    }

    pub fn broadcast_game_state_update(
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

    pub fn broadcast_ready_states(&self, broadcaster: &LobbyBroadcaster) {
        let ready_states = self.collect_ready_states();
        broadcaster.broadcast(ServerToClient::LobbyReady { ready_states });
    }

    pub fn broadcast_ready_states_except(&self, broadcaster: &LobbyBroadcaster, except_player: Uuid) {
        let ready_states = self.collect_ready_states();
        broadcaster.broadcast_except(except_player, ServerToClient::LobbyReady { ready_states });
    }

    pub fn start_online_blind(&mut self, broadcaster: &LobbyBroadcaster) {
        self.reset_ready_states();
        self.reset_scores();
        broadcaster.broadcast(ServerToClient::StartBlind {});
        self.broadcast_ready_states(broadcaster);
    }
}
