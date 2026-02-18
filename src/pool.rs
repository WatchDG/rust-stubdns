use crate::config::{Config, Transport};
use crate::connection::{ConnectionPool, ConnectionWatchdog};
use rustls::ClientConfig;
use std::sync::Arc;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;
use tokio_rustls::client::TlsStream;

#[derive(Debug, Clone)]
pub struct UdpConnection {
    pub server_addr: String,
    pub socket: Arc<UdpSocket>,
    pub read_timeout: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct TcpSocketConfig {
    pub host: String,
    pub port: u16,
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
    pub host: String,
    pub port: u16,
    pub client_config: Arc<ClientConfig>,
    pub auth_name: String,
    pub write_timeout: Option<u64>,
    pub read_timeout: Option<u64>,
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
    pub fn get_transport(&self) -> Transport {
        match self {
            Connection::Udp(_) => Transport::Udp,
            Connection::Tcp(_) => Transport::Tcp,
            Connection::Tls(_) => Transport::Tls,
        }
    }

    pub fn get_server_addr(&self) -> String {
        match self {
            Connection::Udp(udp_conn) => udp_conn.server_addr.clone(),
            Connection::Tcp(tcp_conn) => {
                format!("{}:{}", tcp_conn.config.host, tcp_conn.config.port)
            }
            Connection::Tls(tls_conn) => {
                format!("{}:{}", tls_conn.config.host, tls_conn.config.port)
            }
        }
    }
}

pub async fn create_connection_pool(
    config: &Config,
) -> Result<ConnectionPool, Box<dyn std::error::Error + Send + Sync>> {
    let connection_pool = ConnectionPool::new();
    let connection_pool = Arc::new(Mutex::new(connection_pool));

    let watchdog = ConnectionWatchdog::new(config.clone(), connection_pool.clone());
    watchdog.create_all_connections().await?;

    let pool_guard = connection_pool.lock().await;
    Ok(pool_guard.clone())
}
