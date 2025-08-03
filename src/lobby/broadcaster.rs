use crate::actions::ServerToClient;
use std::collections::HashMap;
use tokio::sync::mpsc;
use uuid::Uuid;

pub struct LobbyBroadcaster {
    player_senders: HashMap<Uuid, mpsc::UnboundedSender<String>>,
}

impl LobbyBroadcaster {
    pub fn new() -> Self {
        Self {
            player_senders: HashMap::new(),
        }
    }

    pub fn add_player(&mut self, player_id: Uuid, sender: mpsc::UnboundedSender<String>) {
        self.player_senders.insert(player_id, sender);
    }

    pub fn remove_player(&mut self, player_id: Uuid) {
        self.player_senders.remove(&player_id);
    }

    pub fn send_to(&self, player_id: Uuid, response: ServerToClient) {
        if let Some(sender) = self.player_senders.get(&player_id) {
            let _ = sender.send(response.to_json());
        }
    }

    // DRY: Single broadcast implementation with filter
    fn broadcast_to_filtered<F>(&self, response: ServerToClient, filter: F)
    where
        F: Fn(Uuid) -> bool,
    {
        let message = response.to_json();
        for (&player_id, sender) in self.player_senders.iter() {
            if filter(player_id) {
                let _ = sender.send(message.clone());
            }
        }
    }

    pub fn broadcast(&self, response: ServerToClient) {
        self.broadcast_to_filtered(response, |_| true);
    }

    pub fn broadcast_except(&self, except: Uuid, response: ServerToClient) {
        self.broadcast_to_filtered(response, |id| id != except);
    }
}
