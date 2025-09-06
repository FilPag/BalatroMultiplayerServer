use super::{broadcaster::LobbyBroadcaster, game_state::ClientLobbyEntry};
use crate::{
    client::ClientProfile,
    game_mode::{CLASH_BASE_DAMAGE, GameMode, LobbyOptions},
    messages::ServerToClient,
    talisman_number::TalismanNumber,
    utils::time_based_string,
};
use rand::rng;
use rand::seq::SliceRandom;
use serde::Serialize;
use std::{collections::HashMap, result};
use tokio::sync::broadcast;
use tracing::{debug, error};

#[derive(Debug)]
pub struct RoundResult {
    pub player_id: String,
    pub won: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Lobby {
    pub code: String,
    pub started: bool,
    pub boss_chips: TalismanNumber,
    pub lobby_options: LobbyOptions,
    stage: i32,
    players: HashMap<String, ClientLobbyEntry>,
    max_players: u8,
}

impl Lobby {
    pub fn new(code: String, ruleset: String, game_mode: GameMode) -> Self {
        let mut new_gamemode = game_mode.get_default_options();
        new_gamemode.ruleset = ruleset;
        Self {
            code,
            started: false,
            boss_chips: TalismanNumber::Regular(0.0),
            lobby_options: new_gamemode,
            players: HashMap::new(),
            stage: 0,
            max_players: game_mode.get_max_players(),
        }
    }

    pub fn get_player_mut(&mut self, player_id: &str) -> Option<&mut ClientLobbyEntry> {
        self.players.get_mut(player_id)
    }

    pub fn players(&self) -> &HashMap<String, ClientLobbyEntry> {
        &self.players
    }

    pub fn is_full(&self) -> bool {
        self.players.len() >= self.max_players as usize
    }

    pub fn randomize_teams(&mut self, team_size: u8) {
        let mut rng = rng();
        let mut player_ids: Vec<String> = self.players.keys().cloned().collect();
        player_ids.shuffle(&mut rng);

        let mut team = 1;
        for (i, player_id) in player_ids.iter().enumerate() {
            if i > 0 && i % team_size as usize == 0 {
                team += 1;
            }
            if let Some(player) = self.players.get_mut(player_id) {
                player.game_state.team = team;
            }
        }
    }

    pub fn add_player(
        &mut self,
        player_id: String,
        client_profile: ClientProfile,
    ) -> ClientLobbyEntry {
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

    pub fn remove_player(&mut self, player_id: &str) -> Option<ClientLobbyEntry> {
        self.players.remove(player_id)
    }

    pub fn promote_new_host(&mut self) -> Option<String> {
        if let Some((new_host_id, new_host_entry)) = self.players.iter_mut().next() {
            new_host_entry.lobby_state.is_host = true;
            new_host_entry.lobby_state.is_ready = true;
            Some(new_host_id.clone())
        } else {
            None
        }
    }

    pub fn get_alive_player_count(&self) -> usize {
        self.players
            .values()
            .filter(|p| p.game_state.lives > 0 && p.lobby_state.in_game)
            .count()
    }

    pub fn is_player_host(&self, player_id: &str) -> bool {
        self.players
            .get(player_id)
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

    pub fn set_player_ready(&mut self, player_id: &str, is_ready: bool) {
        if let Some(player) = self.players.get_mut(player_id) {
            player.lobby_state.is_ready = is_ready;
        }
    }

    pub fn collect_ready_states(&self) -> HashMap<String, bool> {
        self.players
            .iter()
            .map(|(id, entry)| (id.clone(), entry.lobby_state.is_ready))
            .collect()
    }

    // Game state management
    pub fn reset_game_states(&mut self, in_game: bool) {
        for player in self.players.values_mut() {
            player.reset_for_game(self.lobby_options.starting_lives);
            player.lobby_state.in_game = in_game;
        }
    }

    pub fn start_game(&mut self) {
        self.started = true;
        if !self.lobby_options.different_seeds
            && self.lobby_options.custom_seed == String::from("random")
        {
            self.lobby_options.custom_seed = time_based_string(8);
            debug!(
                "Generating time-based seed for lobby {} seed: {}",
                self.code, self.lobby_options.custom_seed
            );
        }
        self.reset_game_states(true);
    }

    pub fn stop_game(&mut self) {
        self.started = false;
        self.reset_game_states(false);
        self.stage = 0;
        self.boss_chips = TalismanNumber::Regular(0.0);
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
        self.players
            .values()
            .filter(|p| p.lobby_state.in_game)
            .all(|p| p.game_state.hands_left == 0)
    }

    pub fn is_someone_dead(&self) -> bool {
        self.players
            .values()
            .any(|p| p.game_state.lives == 0 && p.lobby_state.in_game)
    }

    pub fn handle_player_fail_round(&mut self, player_id: &str, broadcaster: &LobbyBroadcaster) {
        debug!("Player {} failed a round in lobby {}", player_id, self.code);

        if self.lobby_options.death_on_round_loss {
            self.process_round_outcome(&vec![]);
        }
        self.broadcast_life_updates(broadcaster, player_id);

        // Use unified game over check
        self.check_and_handle_game_over(broadcaster);
    }

    // Game logic - kept in lobby for now but could be moved to game_logic module
    pub fn evaluate_online_round(&mut self, broadcaster: &LobbyBroadcaster) {
        if !self.all_players_done() {
            return;
        }

        debug!("Evaluating online battle for lobby {}", self.code);

        let result = self.determine_round_outcome();
        self.process_round_outcome(&result);

        // Use unified game over check
        let game_over = self.check_and_handle_game_over(broadcaster);
        if game_over == false {
            self.reset_scores();
            self.broadcast_end_round_results(broadcaster, &result);
        } else {
            self.reset_ready_states_to_host_only();
            self.broadcast_ready_states(broadcaster);
            self.started = false;
        }
        self.broadcast_all_game_states(broadcaster);
        broadcaster.broadcast(ServerToClient::InGameStatuses {
            statuses: self.get_in_game_statuses(),
        });
    }

    fn determine_round_outcome(&self) -> Vec<RoundResult> {
        match self.lobby_options.gamemode {
            GameMode::CoopSurvival => {
                let mut results = Vec::new();
                let won = self.get_total_score() > self.boss_chips;
                for (id, _) in &self.players {
                    results.push(RoundResult {
                        player_id: id.clone(),
                        won,
                    });
                }
                return results;
            }
            GameMode::Clash => {
                let mut sorted_players = self
                    .players
                    .iter()
                    .filter(|(_, p)| p.lobby_state.in_game)
                    .collect::<Vec<(&String, &ClientLobbyEntry)>>();
                sorted_players.sort_by(|a, b| b.1.game_state.score.cmp(&a.1.game_state.score));
                let top_score = sorted_players[0].1.game_state.score.clone();

                let mut results = Vec::new();
                for (id, player) in sorted_players {
                    results.push(RoundResult {
                        player_id: id.clone(),
                        won: player.game_state.score == top_score,
                    });
                }
                return results;
            }

            _ => {
                if self.players.len() < 2 {
                    error!("Not enough players to evaluate round");
                    return vec![RoundResult {
                        player_id: String::new(),
                        won: false,
                    }];
                }

                let mut result = vec![];
                // Find the actual highest score
                let top_score = self
                    .players
                    .values()
                    .map(|p| &p.game_state.score)
                    .max()
                    .unwrap(); // Safe because we checked players.len() >= 2

                for (id, player) in &self.players {
                    result.push(RoundResult {
                        player_id: id.clone(),
                        won: &player.game_state.score == top_score,
                    });
                }

                result
            }
        }
    }

    fn broadcast_end_round_results(&self, broadcaster: &LobbyBroadcaster, results: &[RoundResult]) {
        for r in results {
            broadcaster.send_to(&r.player_id, ServerToClient::EndPvp { won: r.won });
        }
    }
    pub fn process_round_outcome(&mut self, result: &[RoundResult]) {
        match self.lobby_options.gamemode {
            GameMode::CoopSurvival => {
                if result.is_empty() || result.iter().all(|r| r.won) {
                    return;
                }
                for player in self.players.values_mut() {
                    player.game_state.lives = player.game_state.lives.saturating_sub(1);
                }
            }
            GameMode::Clash => {
                let mut i = 0;
                for r in result {
                    if !r.won {
                        if let Some(player) = self.players.get_mut(&r.player_id) {
                            let damage = CLASH_BASE_DAMAGE[self.stage as usize] + (i as u8) + 1;
                            player.game_state.lives =
                                player.game_state.lives.saturating_sub(damage);
                            i += 1;
                        }
                    }
                }
                self.stage += 1;
            }
            _ => {
                for r in result {
                    if !r.won {
                        if let Some(player) = self.players.get_mut(&r.player_id) {
                            player.game_state.lives = player.game_state.lives.saturating_sub(1);
                        }
                    }
                }
            }
        }
    }

    pub fn check_and_handle_game_over(&mut self, broadcaster: &LobbyBroadcaster) -> bool {
        match self.lobby_options.gamemode {
            GameMode::Survival => {
                if self.get_alive_player_count() > 1 {
                    return false;
                }

                let (winner_id, _) = self.get_max_furthest_blind();
                let winner_alive = self
                    .players
                    .get(&winner_id)
                    .map_or(false, |p| p.game_state.lives > 0);

                if winner_alive || self.is_all_players_dead() {
                    broadcaster.broadcast_to(&[winner_id.clone()], ServerToClient::WinGame {});
                    broadcaster.broadcast_except(&winner_id, ServerToClient::LoseGame {});
                    return true;
                }

                false
            }
            GameMode::CoopSurvival => {
                // Game over if any player is dead (everyone loses together)
                if self.is_someone_dead() {
                    broadcaster.broadcast(ServerToClient::LoseGame {});
                    true
                } else {
                    false
                }
            }
            GameMode::Clash => {
                if !self.is_someone_dead() {
                    return false;
                }

                let mut dead_players = Vec::new();
                let mut alive_players = Vec::new();

                for (id, player) in self.players.iter_mut() {
                    if player.game_state.lives <= 0 {
                        dead_players.push(id.clone());
                        player.lobby_state.in_game = false;
                    } else {
                        alive_players.push(id.clone())
                    }
                }

                broadcaster.broadcast_to(&dead_players, ServerToClient::LoseGame {});
                // for each dead player set their lobby_state.in_game to false
                if alive_players.len() == 1 {
                    broadcaster.broadcast_to(&alive_players, ServerToClient::WinGame {});
                    return true;
                }

                return false;
            }
            _ => {
                if !self.is_someone_dead() {
                    return false;
                }

                let mut winners = Vec::new();
                let mut losers = Vec::new();

                for (id, player) in self.players.iter() {
                    if player.game_state.lives > 0 {
                        winners.push(id.clone());
                    } else {
                        losers.push(id.clone());
                    }
                }

                broadcaster.broadcast_to(&winners, ServerToClient::WinGame {});
                broadcaster.broadcast_to(&losers, ServerToClient::LoseGame {});
                true
            }
        }
    }

    // Broadcasting helpers
    pub fn broadcast_all_game_states(&self, broadcaster: &LobbyBroadcaster) {
        for player in self.players.values() {
            self.broadcast_game_state_update(broadcaster, &player.profile.id, false);
        }
    }

    pub fn broadcast_life_updates(&self, broadcaster: &LobbyBroadcaster, player_id: &str) {
        if self.lobby_options.gamemode == GameMode::CoopSurvival {
            self.broadcast_all_game_states(broadcaster);
        } else {
            self.broadcast_game_state_update(broadcaster, player_id, false);
        }
    }

    pub fn broadcast_game_state_update(
        &self,
        broadcaster: &LobbyBroadcaster,
        player_id: &str,
        exclude_player: bool,
    ) {
        if let Some(player) = self.players.get(player_id) {
            let update = ServerToClient::GameStateUpdate {
                player_id: player_id.to_string(),
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
        let ready_states = self
            .collect_ready_states()
            .into_iter()
            .map(|(id, ready)| (id, ready))
            .collect::<HashMap<String, bool>>();
        broadcaster.broadcast(ServerToClient::LobbyReady { ready_states });
    }

    pub fn broadcast_ready_states_except(
        &self,
        broadcaster: &LobbyBroadcaster,
        except_player: &str,
    ) {
        let ready_states = self
            .collect_ready_states()
            .into_iter()
            .map(|(id, ready)| (id, ready))
            .collect::<HashMap<String, bool>>();
        broadcaster.broadcast_except(except_player, ServerToClient::LobbyReady { ready_states });
    }

    pub fn start_online_blind(&mut self, broadcaster: &LobbyBroadcaster) {
        self.reset_ready_states();
        self.reset_scores();
        let in_game_player_ids = self
            .players
            .iter()
            .filter(|(_, p)| p.lobby_state.in_game)
            .map(|(id, _)| id.clone())
            .collect::<Vec<String>>();
        broadcaster.broadcast_to(&in_game_player_ids, ServerToClient::StartBlind {});
        self.broadcast_ready_states(broadcaster);
    }

    // Survival mode helper methods
    fn is_all_players_dead(&self) -> bool {
        let all_dead = self.players.values().all(|p| p.game_state.lives == 0);
        for (id, player) in &self.players {
            debug!("Player {} has {} lives", id, player.game_state.lives);
        }
        return all_dead;
    }

    fn get_max_furthest_blind(&self) -> (String, u32) {
        self.players
            .iter()
            .map(|(id, p)| (id.clone(), p.game_state.furthest_blind))
            .max_by_key(|(_, furthest_blind)| *furthest_blind)
            .unwrap_or((String::new(), 0))
    }

    pub fn get_in_game_statuses(&self) -> HashMap<String, bool> {
        self.players
            .iter()
            .map(|(id, entry)| (id.clone(), entry.lobby_state.in_game))
            .collect()
    }

    pub fn get_player_count_in_game(&self) -> usize {
        self.players
            .values()
            .filter(|p| p.lobby_state.in_game)
            .count()
    }
}
