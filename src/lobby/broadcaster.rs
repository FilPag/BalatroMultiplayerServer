use crate::actions::ServerToClient;
use std::collections::HashMap;
use tokio::sync::mpsc;

pub struct LobbyBroadcaster {
    player_senders: HashMap<String, mpsc::UnboundedSender<ServerToClient>>,
}

impl LobbyBroadcaster {
    pub fn new() -> Self {
        Self {
            player_senders: HashMap::new(),
        }
    }

    pub fn add_player(&mut self, player_id: String, sender: mpsc::UnboundedSender<ServerToClient>) {
        self.player_senders.insert(player_id, sender);
    }

    pub fn remove_player(&mut self, player_id: &str) {
        self.player_senders.remove(player_id);
    }

    pub fn send_to(&self, player_id: &str, response: ServerToClient) {
        if let Some(sender) = self.player_senders.get(player_id) {
            let _ = sender.send(response);
        }
    }

    // DRY: Single broadcast implementation with filter
    fn broadcast_to_filtered<F>(&self, response: ServerToClient, filter: F)
    where
        F: Fn(&str) -> bool,
    {
        for (player_id, sender) in self.player_senders.iter(){
            if filter(player_id) {
                let _ = sender.send(response.clone());
            }
        }
    }

    pub fn broadcast(&self, response: ServerToClient) {
        self.broadcast_to_filtered(response, |_| true);
    }

    pub fn broadcast_except(&self, except: &str, response: ServerToClient) {
        self.broadcast_to_filtered(response, |id| id != except);
    }
}
