use crate::messages::ServerToClient;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::error;

pub struct LobbyBroadcaster {
    player_senders: HashMap<String, mpsc::UnboundedSender<Arc<ServerToClient>>>,
}

impl LobbyBroadcaster {
    pub fn new() -> Self {
        Self {
            player_senders: HashMap::new(),
        }
    }

    pub fn add_player(
        &mut self,
        player_id: String,
        sender: mpsc::UnboundedSender<Arc<ServerToClient>>,
    ) {
        self.player_senders.insert(player_id, sender);
    }

    pub fn remove_player(&mut self, player_id: &str) {
        self.player_senders.remove(player_id);
    }

    pub fn send_to(&self, player_id: &str, response: ServerToClient) {
        if let Some(sender) = self.player_senders.get(player_id) {
            if let Err(e) = sender.send(Arc::new(response)) {
                error!("Failed to send message to {}: {}", player_id, e);
            }
        }
    }

    // DRY: Single broadcast implementation with filter
    fn broadcast_to_filtered<F>(&self, response: ServerToClient, filter: F)
    where
        F: Fn(&str) -> bool,
    {
        let message = Arc::new(response);
        for (player_id, sender) in self.player_senders.iter() {
            if filter(player_id) {
                if let Err(e) = sender.send(Arc::clone(&message)) {
                    error!("Failed to send message to {}: {}", player_id, e);
                }
            }
        }
    }

    pub fn broadcast(&self, response: ServerToClient) {
        self.broadcast_to_filtered(response, |_| true);
    }

    pub fn broadcast_to(&self, player_ids: &[String], response: ServerToClient) {
        let id_set: std::collections::HashSet<&str> =
            player_ids.iter().map(|s| s.as_str()).collect();
        self.broadcast_to_filtered(response, |id| id_set.contains(id));
    }

    pub fn broadcast_except(&self, except: &str, response: ServerToClient) {
        self.broadcast_to_filtered(response, |id| id != except);
    }
}
