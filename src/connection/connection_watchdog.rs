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
        tokio::spawn(async move {
            let handle = tokio::runtime::Handle::current();
            tokio::task::spawn_blocking(move || {
                while let Ok(broken_index) = broken_receiver.recv() {
                    println!(
                        "Watchdog: connection {} marked as broken, reconnecting...",
                        broken_index
                    );
                    let self_clone = self.clone();
                    handle.spawn(async move {
                        if let Some(conn) = self_clone
                            .connection_pool
                            .get_connection(broken_index)
                            .await
                        {
                            if let Some((server, interface)) = conn.to_server_and_interface_config()
                            {
                                match self_clone
                                    .connection_manager
                                    .create_connection(&server, &interface)
                                    .await
                                {
                                    Ok(new_conn) => {
                                        self_clone
                                            .connection_pool
                                            .add_connection_at(broken_index, new_conn)
                                            .await;
                                        println!(
                                            "Watchdog: reconnected connection {} to {}:{}",
                                            broken_index,
                                            server.host,
                                            interface.get_port()
                                        );
                                    }
                                    Err(e) => eprintln!(
                                        "Watchdog: failed to reconnect {}: {}",
                                        broken_index, e
                                    ),
                                }
                            }
                        }
                    });
                }
            })
            .await
            .ok();
        });
    }
}
