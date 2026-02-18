use crate::config::{Config, InterfaceConfig, Transport, UpstreamServerConfig};
use crate::connection::ConnectionManager;
use crate::connection::ConnectionPool;
use futures::future::join_all;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;

pub struct ConnectionWatchdog {
    config: Config,
    connection_pool: Arc<Mutex<ConnectionPool>>,
    connection_manager: Arc<ConnectionManager>,
}

impl ConnectionWatchdog {
    pub fn new(config: Config, connection_pool: Arc<Mutex<ConnectionPool>>) -> Self {
        Self {
            config,
            connection_pool,
            connection_manager: Arc::new(ConnectionManager),
        }
    }

    pub fn start(&self) {
        for server in &self.config.upstream_servers {
            for interface_config in &server.interfaces {
                let server_clone = server.clone();
                let interface_clone = interface_config.clone();
                let pool_clone = self.connection_pool.clone();
                let manager_clone = self.connection_manager.clone();

                tokio::spawn(Self::watch_interface(
                    manager_clone,
                    server_clone,
                    interface_clone,
                    pool_clone,
                ));
            }
        }
    }

    pub async fn create_all_connections(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut handles = Vec::new();

        for server in &self.config.upstream_servers {
            for interface_config in &server.interfaces {
                let server_clone = server.clone();
                let interface_clone = interface_config.clone();
                let pool_clone = self.connection_pool.clone();
                let manager_clone = self.connection_manager.clone();

                handles.push(tokio::spawn(Self::create_interface_connection(
                    manager_clone,
                    server_clone,
                    interface_clone,
                    pool_clone,
                )));
            }
        }

        let results = join_all(handles).await;
        for result in results {
            result??;
        }

        Ok(())
    }

    async fn create_interface_connection(
        connection_manager: Arc<ConnectionManager>,
        server: UpstreamServerConfig,
        interface_config: InterfaceConfig,
        connection_pool: Arc<Mutex<ConnectionPool>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let port = interface_config.get_port();
        let server_addr = format!("{}:{}", server.host, port);

        loop {
            match interface_config.type_ {
                Transport::Udp => {
                    match connection_manager
                        .create_udp_connection(&server, &interface_config, &server_addr)
                        .await
                    {
                        Ok(udp_connection) => {
                            println!("Watchdog: UDP connection established to {}", server_addr);
                            let mut pool = connection_pool.lock().await;
                            pool.add_udp(udp_connection);
                            return Ok(());
                        }
                        Err(e) => {
                            eprintln!(
                                "Watchdog: Failed to establish UDP connection to {}: {}. Retrying...",
                                server_addr, e
                            );
                            sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
                Transport::Tcp => {
                    match connection_manager
                        .create_tcp_connection(&server, &interface_config, &server_addr)
                        .await
                    {
                        Ok(tcp_connection) => {
                            println!("Watchdog: TCP connection established to {}", server_addr);
                            let mut pool = connection_pool.lock().await;
                            pool.add_tcp(tcp_connection);
                            return Ok(());
                        }
                        Err(e) => {
                            eprintln!(
                                "Watchdog: Failed to establish TCP connection to {}: {}. Retrying...",
                                server_addr, e
                            );
                            sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
                Transport::Tls => {
                    match connection_manager
                        .create_tls_connection(&server, &interface_config, &server_addr)
                        .await
                    {
                        Ok(tls_connection) => {
                            println!("Watchdog: TLS connection established to {}", server_addr);
                            let mut pool = connection_pool.lock().await;
                            pool.add_tls(tls_connection);
                            return Ok(());
                        }
                        Err(e) => {
                            eprintln!(
                                "Watchdog: Failed to establish TLS connection to {}: {}. Retrying...",
                                server_addr, e
                            );
                            sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            }
        }
    }

    async fn watch_interface(
        connection_manager: Arc<ConnectionManager>,
        server: UpstreamServerConfig,
        interface_config: InterfaceConfig,
        connection_pool: Arc<Mutex<ConnectionPool>>,
    ) {
        let port = interface_config.get_port();
        let server_addr = format!("{}:{}", server.host, port);

        loop {
            match interface_config.type_ {
                Transport::Udp => {
                    match connection_manager
                        .create_udp_connection(&server, &interface_config, &server_addr)
                        .await
                    {
                        Ok(udp_connection) => {
                            println!("Watchdog: UDP connection established to {}", server_addr);
                            let mut pool = connection_pool.lock().await;
                            pool.add_udp(udp_connection);
                            break;
                        }
                        Err(e) => {
                            eprintln!(
                                "Watchdog: Failed to establish UDP connection to {}: {}. Retrying...",
                                server_addr, e
                            );
                            sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
                Transport::Tcp => {
                    match connection_manager
                        .create_tcp_connection(&server, &interface_config, &server_addr)
                        .await
                    {
                        Ok(tcp_connection) => {
                            println!("Watchdog: TCP connection established to {}", server_addr);
                            let mut pool = connection_pool.lock().await;
                            pool.add_tcp(tcp_connection);
                            break;
                        }
                        Err(e) => {
                            eprintln!(
                                "Watchdog: Failed to establish TCP connection to {}: {}. Retrying...",
                                server_addr, e
                            );
                            sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
                Transport::Tls => {
                    match connection_manager
                        .create_tls_connection(&server, &interface_config, &server_addr)
                        .await
                    {
                        Ok(tls_connection) => {
                            println!("Watchdog: TLS connection established to {}", server_addr);
                            let mut pool = connection_pool.lock().await;
                            pool.add_tls(tls_connection);
                            break;
                        }
                        Err(e) => {
                            eprintln!(
                                "Watchdog: Failed to establish TLS connection to {}: {}. Retrying...",
                                server_addr, e
                            );
                            sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            }
        }
    }
}
