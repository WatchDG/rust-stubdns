use crate::config::Config;
use crate::connection::{ConnectionManager, ConnectionPool};
use std::sync::{Arc, mpsc};

pub struct ConnectionWatchdog {
    config: Config,
    connection_pool: Arc<ConnectionPool>,
    connection_manager: ConnectionManager,
}

impl ConnectionWatchdog {
    pub fn new(config: Config, connection_pool: Arc<ConnectionPool>) -> Self {
        Self {
            config,
            connection_pool,
            connection_manager: ConnectionManager,
        }
    }

    pub fn start(self: Arc<Self>, broken_receiver: mpsc::Receiver<usize>) {
        std::thread::spawn(move || {
            while let Ok(broken_index) = broken_receiver.recv() {
                println!("Watchdog: connection {} marked as broken", broken_index);
                // TODO: reconnect logic
            }
        });
    }
}
