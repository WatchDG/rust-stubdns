use crate::config::{Config, Transport};
use rustls::ClientConfig;
use rustls::pki_types::ServerName;
use rustls_native_certs;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_rustls::{TlsConnector, client::TlsStream};

#[derive(Debug, Clone)]
pub struct TcpSocketConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone)]
pub struct TcpConnection {
    pub config: Arc<TcpSocketConfig>,
    pub stream: Arc<Mutex<TcpStream>>,
}

#[derive(Debug, Clone)]
pub struct TlsConnectionConfig {
    pub host: String,
    pub port: u16,
    pub client_config: Arc<ClientConfig>,
    pub auth_name: String,
}

pub struct TlsConnection {
    pub config: Arc<TlsConnectionConfig>,
    pub stream: Arc<Mutex<TlsStream<TcpStream>>>,
}

#[derive(Clone)]
pub enum Connection {
    Udp(Arc<UdpSocket>),
    Tcp(Arc<TcpConnection>),
    Tls(Arc<TlsConnection>),
}

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

    pub fn add_udp(&mut self, udp_socket: UdpSocket) {
        self.connections.push(Connection::Udp(Arc::new(udp_socket)));
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
}

impl Connection {
    pub fn get_transport(&self) -> Transport {
        match self {
            Connection::Udp(_) => Transport::Udp,
            Connection::Tcp(_) => Transport::Tcp,
            Connection::Tls(_) => Transport::Tls,
        }
    }
}

pub async fn create_connection_pool(
    config: &Config,
) -> Result<(ConnectionPool, Option<String>), Box<dyn std::error::Error + Send + Sync>> {
    let mut connection_pool = ConnectionPool::new();
    let mut first_server_addr = None;

    for server in &config.upstream_servers {
        for interface_config in &server.interfaces {
            let port = interface_config.get_port();
            let server_addr = format!("{}:{}", server.host, port);

            match interface_config.type_ {
                Transport::Udp => {
                    let socket = UdpSocket::bind("0.0.0.0:0").await?;
                    connection_pool.add_udp(socket);
                    if first_server_addr.is_none() {
                        first_server_addr = Some(server_addr);
                    }
                }
                Transport::Tcp => {
                    let tcp_config = TcpSocketConfig {
                        host: server.host.clone(),
                        port,
                    };
                    let addr: SocketAddr = server_addr.parse()?;
                    println!("TCP: connecting to {} at startup", server_addr);

                    let server_timeout = server.get_connection_timeout();
                    let connection_timeout =
                        interface_config.get_connection_timeout(server_timeout);
                    let stream = if let Some(timeout_ms) = connection_timeout {
                        let duration = Duration::from_millis(timeout_ms);
                        println!("TCP: using connection timeout of {} ms", timeout_ms);
                        timeout(duration, TcpStream::connect(addr))
                            .await
                            .map_err(|_| format!("TCP connection timeout after {} ms", timeout_ms))?
                            .map_err(|e| format!("TCP connection error: {}", e))?
                    } else {
                        println!("TCP: no connection timeout");
                        TcpStream::connect(addr).await?
                    };

                    println!("TCP: connection established to {}", server_addr);
                    let tcp_connection = TcpConnection {
                        config: Arc::new(tcp_config),
                        stream: Arc::new(Mutex::new(stream)),
                    };
                    connection_pool.add_tcp(tcp_connection);
                    if first_server_addr.is_none() {
                        first_server_addr = Some(server_addr);
                    }
                }
                Transport::Tls => {
                    let auth_name = interface_config.get_auth_name();
                    let mut root_store = rustls::RootCertStore::empty();

                    for cert in rustls_native_certs::load_native_certs()
                        .map_err(|e| format!("Failed to load native certificates: {}", e))?
                    {
                        root_store
                            .add(cert)
                            .map_err(|e| format!("Failed to add certificate: {}", e))?;
                    }

                    let client_config = ClientConfig::builder()
                        .with_root_certificates(root_store)
                        .with_no_client_auth();
                    let client_config_arc = Arc::new(client_config);

                    println!("TLS: connecting to {} at startup", server_addr);
                    let addr: SocketAddr = server_addr.parse()?;

                    let server_timeout = server.get_connection_timeout();
                    let connection_timeout =
                        interface_config.get_connection_timeout(server_timeout);
                    let tcp_stream = if let Some(timeout_ms) = connection_timeout {
                        let duration = Duration::from_millis(timeout_ms);
                        println!("TLS: using connection timeout of {} ms", timeout_ms);
                        timeout(duration, TcpStream::connect(addr))
                            .await
                            .map_err(|_| {
                                format!("TLS TCP connection timeout after {} ms", timeout_ms)
                            })?
                            .map_err(|e| format!("TLS TCP connection error: {}", e))?
                    } else {
                        println!("TLS: no connection timeout");
                        TcpStream::connect(addr).await?
                    };
                    println!("TLS: TCP connection established");

                    let server_name = ServerName::try_from(auth_name.clone())
                        .map_err(|e| format!("Invalid server name: {}", e))?;
                    let connector = TlsConnector::from(client_config_arc.clone());

                    let tls_stream = if let Some(timeout_ms) = connection_timeout {
                        let duration = Duration::from_millis(timeout_ms);
                        timeout(duration, connector.connect(server_name, tcp_stream))
                            .await
                            .map_err(|_| format!("TLS handshake timeout after {} ms", timeout_ms))?
                            .map_err(|e| format!("TLS handshake error: {}", e))?
                    } else {
                        connector.connect(server_name, tcp_stream).await?
                    };
                    println!("TLS: TLS handshake completed to {}", server_addr);

                    let tls_config = TlsConnectionConfig {
                        host: server.host.clone(),
                        port,
                        client_config: client_config_arc,
                        auth_name,
                    };
                    let tls_connection = TlsConnection {
                        config: Arc::new(tls_config),
                        stream: Arc::new(Mutex::new(tls_stream)),
                    };
                    connection_pool.add_tls(tls_connection);
                    if first_server_addr.is_none() {
                        first_server_addr = Some(server_addr);
                    }
                }
            }
        }
    }

    Ok((connection_pool, first_server_addr))
}
