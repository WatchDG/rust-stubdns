use rustls::ClientConfig;
use std::sync::Arc;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;
use tokio_rustls::client::TlsStream;

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
