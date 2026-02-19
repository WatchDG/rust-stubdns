use crate::config::{Config, InterfaceConfig, Transport, UpstreamServerConfig};
use crate::connection::ConnectionManager;
use crate::pool::{Connection, TcpConnection, TlsConnection, UdpConnection};
use futures::future::join_all;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;

#[derive(Clone)]
pub struct ConnectionPool {
    connections: Vec<Connection>,
    borrowed: Arc<Mutex<HashSet<usize>>>,
}

impl ConnectionPool {
    pub fn new() -> Self {
        Self {
            connections: Vec::new(),
            borrowed: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub async fn initialize(
        &mut self,
        config: &Config,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut handles = Vec::new();
        let pool_arc = Arc::new(Mutex::new(self.clone()));
        let manager_clone = connection_manager.clone();

        for server in &config.upstream_servers {
            for interface_config in &server.interfaces {
                let server_clone = server.clone();
                let interface_clone = interface_config.clone();
                let pool_clone = pool_arc.clone();
                let manager_clone_inner = manager_clone.clone();

                handles.push(tokio::spawn(Self::create_interface_connection(
                    manager_clone_inner,
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

        *self = Arc::try_unwrap(pool_arc)
            .map_err(|_| {
                Box::<dyn std::error::Error + Send + Sync>::from("Failed to unwrap pool arc")
            })?
            .into_inner();

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
                            println!("Pool: UDP connection established to {}", server_addr);
                            let mut pool = connection_pool.lock().await;
                            pool.add_udp(udp_connection);
                            return Ok(());
                        }
                        Err(e) => {
                            eprintln!(
                                "Pool: Failed to establish UDP connection to {}: {}. Retrying...",
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
                            println!("Pool: TCP connection established to {}", server_addr);
                            let mut pool = connection_pool.lock().await;
                            pool.add_tcp(tcp_connection);
                            return Ok(());
                        }
                        Err(e) => {
                            eprintln!(
                                "Pool: Failed to establish TCP connection to {}: {}. Retrying...",
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
                            println!("Pool: TLS connection established to {}", server_addr);
                            let mut pool = connection_pool.lock().await;
                            pool.add_tls(tls_connection);
                            return Ok(());
                        }
                        Err(e) => {
                            eprintln!(
                                "Pool: Failed to establish TLS connection to {}: {}. Retrying...",
                                server_addr, e
                            );
                            sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            }
        }
    }

    pub fn add_udp(&mut self, udp_connection: UdpConnection) {
        self.connections
            .push(Connection::Udp(Arc::new(udp_connection)));
    }

    pub fn add_tcp(&mut self, tcp_connection: TcpConnection) {
        self.connections
            .push(Connection::Tcp(Arc::new(tcp_connection)));
    }

    pub fn add_tls(&mut self, tls_connection: TlsConnection) {
        self.connections
            .push(Connection::Tls(Arc::new(tls_connection)));
    }

    pub fn get_socket(&self, i: usize) -> Option<Connection> {
        if i < self.connections.len() {
            Some(self.connections[i].clone())
        } else {
            None
        }
    }

    pub fn get_socket_by_type(&self, transport: Transport) -> Option<Connection> {
        self.connections
            .iter()
            .find(|conn| conn.get_transport() == transport)
            .map(|conn| conn.clone())
    }

    pub async fn borrow_socket_by_type(&self, transport: Transport) -> Option<(usize, Connection)> {
        let mut borrowed = self.borrowed.lock().await;
        for (index, conn) in self.connections.iter().enumerate() {
            if conn.get_transport() == transport && !borrowed.contains(&index) {
                borrowed.insert(index);
                return Some((index, conn.clone()));
            }
        }
        None
    }

    pub async fn borrow_first_available(&self) -> Option<(usize, Connection)> {
        let mut borrowed = self.borrowed.lock().await;
        for (index, conn) in self.connections.iter().enumerate() {
            if !borrowed.contains(&index) {
                borrowed.insert(index);
                return Some((index, conn.clone()));
            }
        }
        None
    }

    pub async fn return_socket(&self, index: usize) {
        let mut borrowed = self.borrowed.lock().await;
        borrowed.remove(&index);
    }

    pub fn replace_connection(&mut self, index: usize, connection: Connection) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if index >= self.connections.len() {
            return Err("Index out of bounds".into());
        }
        self.connections[index] = connection;
        Ok(())
    }

    pub fn get_connection(&self, index: usize) -> Option<Connection> {
        if index < self.connections.len() {
            Some(self.connections[index].clone())
        } else {
            None
        }
    }
}
