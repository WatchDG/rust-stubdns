use rustls::ClientConfig;
use std::sync::Arc;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;
use tokio_rustls::client::TlsStream;

#[derive(Debug, Clone)]
pub struct UdpConfig {
    pub host: String,
    pub port: u16,
    pub server_addr: String,
    pub read_timeout: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct UdpConnection {
    pub config: Arc<UdpConfig>,
    pub socket: Arc<UdpSocket>,
}

#[derive(Debug, Clone)]
pub struct TcpSocketConfig {
    pub host: String,
    pub port: u16,
    pub server_addr: String,
    pub write_timeout: Option<u64>,
    pub read_timeout: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct TcpConnection {
    pub config: Arc<TcpSocketConfig>,
    pub stream: Arc<Mutex<TcpStream>>,
}

#[derive(Debug, Clone)]
pub struct TlsConnectionConfig {
    pub tcp_config: Arc<TcpSocketConfig>,
    pub client_config: Arc<ClientConfig>,
    pub auth_name: String,
    pub tls_handshake_timeout: Option<u64>,
}

pub struct TlsConnection {
    pub config: Arc<TlsConnectionConfig>,
    pub stream: Arc<Mutex<TlsStream<TcpStream>>>,
}

#[derive(Clone)]
pub enum Connection {
    Udp(Arc<UdpConnection>),
    Tcp(Arc<TcpConnection>),
    Tls(Arc<TlsConnection>),
}

impl Connection {
    pub fn get_server_addr(&self) -> String {
        match self {
            Connection::Udp(udp_conn) => udp_conn.config.server_addr.clone(),
            Connection::Tcp(tcp_conn) => tcp_conn.config.server_addr.clone(),
            Connection::Tls(tls_conn) => tls_conn.config.tcp_config.server_addr.clone(),
        }
    }
}
