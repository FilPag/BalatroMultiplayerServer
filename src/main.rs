use socket2::{SockRef, TcpKeepalive};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::info;

mod client;
mod game_mode;
mod lobby;
mod lobby_coordinator;
mod messages;
mod talisman_number;
mod utils;
mod test_utils;

use crate::client::handle_client;
use crate::lobby_coordinator::lobby_coordinator;
use crate::messages::CoordinatorMessage;

/// Entry point: starts the TCP server with simple message passing
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8788").await?;
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    info!("Server listening on port 8788");

    // Create the lobby coordinator
    let (coordinator_tx, coordinator_rx) = mpsc::unbounded_channel::<CoordinatorMessage>();

    // Spawn the lobby coordinator task
    tokio::spawn(lobby_coordinator(coordinator_rx));

    loop {
        let (socket, addr) = listener.accept().await?;

        // Configure TCP keep-alive
        let keepalive = TcpKeepalive::new()
            .with_time(Duration::from_secs(10))
            .with_interval(Duration::from_secs(1));
        let sf = SockRef::from(&socket);
        let _ = sf.set_tcp_keepalive(&keepalive);

        // Split the socket for reading and writing
        let (reader, writer) = socket.into_split();

        // Clone the coordinator sender for this client
        let coordinator_tx_clone = coordinator_tx.clone();

        // Spawn a client handler
        tokio::spawn(handle_client(reader, writer, addr, coordinator_tx_clone));
    }
}
