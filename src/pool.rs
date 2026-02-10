use crate::config::{Config, Transport};
use rustls::ClientConfig;
use rustls::pki_types::ServerName;
use rustls_native_certs;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;
use tokio_rustls::{TlsConnector, client::TlsStream};

pub struct TcpSocketConfig {
    pub host: String,
    pub port: u16,
}

pub struct TcpConnection {
    pub config: Arc<TcpSocketConfig>,
    pub stream: Arc<Mutex<TcpStream>>,
}

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

pub enum Connection {
    Udp(Arc<UdpSocket>),
    Tcp(Arc<TcpConnection>),
    Tls(Arc<TlsConnection>),
}

pub struct ConnectionPool {
    connections: Vec<Connection>,
}

impl ConnectionPool {
    pub fn new() -> Self {
        Self {
            connections: Vec::new(),
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
                    let stream = TcpStream::connect(addr).await?;
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
                    let tcp_stream = TcpStream::connect(addr).await?;
                    println!("TLS: TCP connection established");

                    let server_name = ServerName::try_from(auth_name.clone())
                        .map_err(|e| format!("Invalid server name: {}", e))?;
                    let connector = TlsConnector::from(client_config_arc.clone());
                    let tls_stream = connector.connect(server_name, tcp_stream).await?;
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

impl Clone for Connection {
    fn clone(&self) -> Self {
        match self {
            Connection::Udp(socket) => Connection::Udp(socket.clone()),
            Connection::Tcp(socket) => Connection::Tcp(socket.clone()),
            Connection::Tls(socket) => Connection::Tls(socket.clone()),
        }
    }
}

impl Clone for TcpSocketConfig {
    fn clone(&self) -> Self {
        Self {
            host: self.host.clone(),
            port: self.port,
        }
    }
}

impl Clone for TcpConnection {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            stream: Arc::clone(&self.stream),
        }
    }
}

impl Clone for TlsConnectionConfig {
    fn clone(&self) -> Self {
        Self {
            host: self.host.clone(),
            port: self.port,
            client_config: self.client_config.clone(),
            auth_name: self.auth_name.clone(),
        }
    }
}

impl Clone for TlsConnection {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            stream: Arc::clone(&self.stream),
        }
    }
}
