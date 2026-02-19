use crate::config::{Config, InterfaceConfig, Transport, UpstreamServerConfig};
use crate::connection::ConnectionManager;
use crate::connection::ConnectionPool;
use crate::pool::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;

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

    pub async fn initialize_pool(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut pool = self.connection_pool.lock().await;
        pool.initialize(&self.config, self.connection_manager.clone())
            .await
    }

    pub fn start(&self) {
        // Watchdog functionality can be added here if needed
    }

    pub async fn reset_connection(
        &self,
        index: usize,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get the connection from pool
        let connection = {
            let pool = self.connection_pool.lock().await;
            pool.get_connection(index)
                .ok_or("Connection index out of bounds")?
        };

        // Close the old connection
        self.connection_manager.close_connection(&connection).await?;

        // Find corresponding server and interface config
        let (server, interface_config) = self.find_server_and_interface(&connection)?;
        let port = interface_config.get_port();
        let server_addr = format!("{}:{}", server.host, port);

        // Create new connection
        let new_connection = match interface_config.type_ {
            Transport::Udp => {
                let udp_conn = self.connection_manager
                    .create_udp_connection(&server, &interface_config, &server_addr)
                    .await?;
                Connection::Udp(Arc::new(udp_conn))
            }
            Transport::Tcp => {
                let tcp_conn = self.connection_manager
                    .create_tcp_connection(&server, &interface_config, &server_addr)
                    .await?;
                Connection::Tcp(Arc::new(tcp_conn))
            }
            Transport::Tls => {
                let tls_conn = self.connection_manager
                    .create_tls_connection(&server, &interface_config, &server_addr)
                    .await?;
                Connection::Tls(Arc::new(tls_conn))
            }
        };

        // Replace connection in pool
        let mut pool = self.connection_pool.lock().await;
        pool.replace_connection(index, new_connection)?;

        println!("Connection {} reset successfully", index);
        Ok(())
    }

    fn find_server_and_interface(
        &self,
        connection: &Connection,
    ) -> Result<(UpstreamServerConfig, InterfaceConfig), Box<dyn std::error::Error + Send + Sync>> {
        let connection_addr = connection.get_server_addr();

        for server in &self.config.upstream_servers {
            for interface_config in &server.interfaces {
                let port = interface_config.get_port();
                let server_addr = format!("{}:{}", server.host, port);
                
                if server_addr == connection_addr {
                    return Ok((server.clone(), interface_config.clone()));
                }
            }
        }

        Err(format!("Could not find server and interface for connection address: {}", connection_addr).into())
    }
}
