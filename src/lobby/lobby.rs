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
use std::collections::HashMap;
use tracing::{debug, error};

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
        self.players.values().any(|p| p.game_state.lives == 0)
    }

    // Game logic - kept in lobby for now but could be moved to game_logic module
    pub fn evaluate_online_round(&mut self, broadcaster: &LobbyBroadcaster) -> bool {
        if !self.all_players_done() {
            return false;
        }

        debug!("Evaluating online battle for lobby {}", self.code);

        let (winners, losers) = self.check_round_victory();
        self.apply_life_loss(&losers);
        self.broadcast_all_game_states(broadcaster);

        // Use unified game over check
        let (game_ended, final_winners, final_losers) = self.check_game_over();

        if game_ended {
            self.handle_game_end(broadcaster, &final_winners, &final_losers);
        } else {
            self.reset_scores();
            self.send_outcome_messages(broadcaster, &winners, &losers, false);
        }

        game_ended
    }

    //TODO should work with survival where players wait for other to fail or reach the same or a latter round
    pub fn handle_player_fail_round(
        &mut self,
        player_id: &str,
        broadcaster: &LobbyBroadcaster,
    ) -> bool {
        debug!("Player {} failed a round in lobby {}", player_id, self.code);

        if self.lobby_options.death_on_round_loss {
            self.apply_life_loss(&vec![player_id.to_string()]);
        }

        self.broadcast_life_updates(broadcaster, player_id);

        // Use unified game over check
        let (game_ended, winners, losers) = self.check_game_over();

        if game_ended {
            self.handle_game_end(broadcaster, &winners, &losers);
        }

        game_ended
    }

    fn check_round_victory(&self) -> (Vec<String>, Vec<String>) {
        match self.lobby_options.gamemode {
            GameMode::CoopSurvival => {
                if self.get_total_score() > self.boss_chips {
                    (Vec::new(), Vec::new())
                } else {
                    (Vec::new(), self.players.keys().cloned().collect())
                }
            }
            GameMode::Clash => {
                let mut sorted_players = self
                    .players
                    .iter()
                    .filter(|(_, p)| p.lobby_state.in_game)
                    .collect::<Vec<(&String, &ClientLobbyEntry)>>();
                sorted_players.sort_by(|a, b| b.1.game_state.score.cmp(&a.1.game_state.score));
                let top_score = sorted_players[0].1.game_state.score.clone();
                let mut winners = Vec::new();
                let mut losers = Vec::new();
                for (id, entry) in sorted_players {
                    if entry.game_state.score == top_score {
                        winners.push(id.clone());
                    } else {
                        losers.push(id.clone());
                    }
                }
                (winners, losers)
            }

            GameMode::Survival => {
                //Compare furthest blind in games state of all players. player with furthest blind wins
                self.get_survival_winners_losers()
            }
            _ => {
                if self.players.len() < 2 {
                    error!("Not enough players to evaluate round");
                    return (Vec::new(), Vec::new());
                }

                // Find the actual highest score
                let top_score = self
                    .players
                    .values()
                    .map(|p| &p.game_state.score)
                    .max()
                    .unwrap(); // Safe because we checked players.len() >= 2

                let mut winners: Vec<String> = Vec::new();
                let mut losers: Vec<String> = Vec::new();
                for (player_id, player) in &self.players {
                    if &player.game_state.score == top_score {
                        winners.push(player_id.clone());
                    } else {
                        losers.push(player_id.clone());
                    }
                }
                (winners, losers)
            }
        }
    }

    fn determine_game_end_results(&self) -> (Vec<String>, Vec<String>) {
        match self.lobby_options.gamemode {
            GameMode::CoopSurvival => (Vec::new(), self.players.keys().cloned().collect()),
            _ => {
                let mut winners = Vec::new();
                let mut losers = Vec::new();

                for (player_id, player) in &self.players {
                    if player.game_state.lives > 0 {
                        winners.push(player_id.clone());
                    } else {
                        losers.push(player_id.clone());
                    }
                }

                (winners, losers)
            }
        }
    }

    pub fn apply_life_loss(&mut self, losers: &[String]) {
        if losers.is_empty() {
            return;
        }

        match self.lobby_options.gamemode {
            GameMode::CoopSurvival => {
                for player in self.players.values_mut() {
                    player.game_state.lives = player.game_state.lives.saturating_sub(1);
                }
            }
            GameMode::Clash => {
                for (i, loser_id) in losers.iter().enumerate() {
                    if let Some(player) = self.players.get_mut(loser_id) {
                        let damage = CLASH_BASE_DAMAGE[self.stage as usize] + (i as u8) + 1;
                        player.game_state.lives = player.game_state.lives.saturating_sub(damage);
                    }
                }
                self.stage += 1;
            }
            _ => {
                for loser_id in losers {
                    if let Some(player) = self.players.get_mut(loser_id) {
                        player.game_state.lives = player.game_state.lives.saturating_sub(1);
                    }
                }
            }
        }
    }

    pub fn handle_game_end(
        &self,
        broadcaster: &LobbyBroadcaster,
        winners: &[String],
        losers: &[String],
    ) {
        debug!("Game Over in lobby {}, ending game", self.code);
        self.send_outcome_messages(broadcaster, winners, losers, true);
    }

    fn send_outcome_messages(
        &self,
        broadcaster: &LobbyBroadcaster,
        winners: &[String],
        losers: &[String],
        is_game_end: bool,
    ) {
        for player_id in winners {
            let message = if is_game_end {
                debug!("Player {} won the game in lobby {}", player_id, self.code);
                ServerToClient::WinGame {}
            } else {
                debug!("Player {} won the round in lobby {}", player_id, self.code);
                ServerToClient::EndPvp { won: true }
            };
            broadcaster.send_to(player_id, message);
        }

        for player_id in losers {
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
        broadcaster.broadcast(ServerToClient::StartBlind {});
        self.broadcast_ready_states(broadcaster);
    }

    // Survival mode helper methods
    fn all_players_dead(&self) -> bool {
        let all_dead = self.players.values().all(|p| p.game_state.lives == 0);
        for (id, player) in &self.players {
            debug!("Player {} has {} lives", id, player.game_state.lives);
        }
        return all_dead;
    }

    fn all_other_players_dead(&self, except_player_id: &str) -> bool {
        self.players
            .iter()
            .filter(|(id, _)| id.as_str() != except_player_id)
            .all(|(_, p)| p.game_state.lives == 0)
    }

    fn get_max_furthest_blind(&self) -> u32 {
        self.players
            .values()
            .map(|p| p.game_state.furthest_blind)
            .max()
            .unwrap_or(0)
    }

    fn get_survival_winners_losers(&self) -> (Vec<String>, Vec<String>) {
        let max_furthest_blind = self.get_max_furthest_blind();

        let mut winners = Vec::new();
        let mut losers = Vec::new();

        for (player_id, player) in &self.players {
            if player.game_state.furthest_blind == max_furthest_blind {
                winners.push(player_id.clone());
            } else {
                losers.push(player_id.clone());
            }
        }
        (winners, losers)
    }

    /// Returns (is_game_over, winners, losers)
    pub fn check_game_over(&self) -> (bool, Vec<String>, Vec<String>) {
        match self.lobby_options.gamemode {
            GameMode::Survival => {
                // Game over if all players are dead
                if self.all_players_dead() {
                    let (winners, losers) = self.get_survival_winners_losers();
                    (true, winners, losers)
                } else {
                    (false, Vec::new(), Vec::new())
                }
            }
            GameMode::CoopSurvival => {
                // Game over if any player is dead (everyone loses together)
                if self.is_someone_dead() {
                    (true, Vec::new(), self.players.keys().cloned().collect())
                } else {
                    (false, Vec::new(), Vec::new())
                }
            }
            GameMode::Clash => {
                // Game over if any player is dead
                if self.is_someone_dead() {
                    let (winners, losers) = self.determine_game_end_results();
                    (true, winners, losers)
                } else {
                    (false, Vec::new(), Vec::new())
                }
            }
            _ => {
                // Standard PvP modes - game over if someone is dead
                if self.is_someone_dead() {
                    let (winners, losers) = self.determine_game_end_results();
                    (true, winners, losers)
                } else {
                    (false, Vec::new(), Vec::new())
                }
            }
        }
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

    pub fn check_survival_furthest_blind_win(
        &mut self,
        broadcaster: &LobbyBroadcaster,
        player_id: &str,
    ) -> bool {
        // Only check if all other players are dead and this player has the max furthest blind
        if self.all_other_players_dead(player_id) {
            if let Some(player) = self.players.get(player_id) {
                if player.game_state.furthest_blind == self.get_max_furthest_blind() {
                    let (winners, losers) = self.get_survival_winners_losers();
                    self.handle_game_end(broadcaster, &winners, &losers);
                    return true;
                }
            }
        }
        false
    }
}
