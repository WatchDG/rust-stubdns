use crate::config::{Config, HEALTH_CHECK_INTERVAL_MS};
use crate::connection::{ConnectionManager, ConnectionPool};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

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

    pub fn start(self: Arc<Self>, mut broken_receiver: mpsc::Receiver<usize>) {
        tokio::spawn(async move {
            let mut health_check_interval = interval(Duration::from_millis(HEALTH_CHECK_INTERVAL_MS));
            loop {
                tokio::select! {
                    broken_index = broken_receiver.recv() => {
                        match broken_index {
                            Some(index) => {
                                tracing::info!(
                                    "Watchdog: connection {} marked as broken, reconnecting...",
                                    index
                                );
                                let self_clone = self.clone();
                                tokio::spawn(async move {
                                    if let Some(conn) = self_clone
                                        .connection_pool
                                        .get_connection(index)
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
                                                        .add_connection_at(index, new_conn)
                                                        .await;
                                                    tracing::info!(
                                                        "Watchdog: reconnected connection {} to {}:{}",
                                                        index,
                                                        server.host,
                                                        interface.get_port()
                                                    );
                                                }
                                                Err(e) => tracing::error!(
                                                    "Watchdog: failed to reconnect {}: {}",
                                                    index, e
                                                ),
                                            }
                                        }
                                    }
                                });
                            }
                            None => {
                                tracing::info!("Watchdog: channel closed, shutting down...");
                                break;
                            }
                        }
                    }
                    _ = health_check_interval.tick() => {
                        tracing::debug!("Watchdog: running periodic health check...");
                        let self_clone = self.clone();
                        tokio::spawn(async move {
                            self_clone.run_health_check().await;
                        });
                    }
                }
            }
        });
    }

    async fn run_health_check(&self) {
        let connections = self.connection_pool.connections.lock().await;
        let indices: Vec<usize> = (0..connections.len()).collect();
        drop(connections);

        for index in indices {
            if let Some(conn) = self.connection_pool.get_connection(index).await {
                if !self.connection_manager.check_connection(&conn).await {
                    tracing::debug!("Watchdog: health check found broken connection {}", index);
                    self.connection_pool.mark_broken(index).await;
                }
            }
        }
    }
}