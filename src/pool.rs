use crate::config::{Config, Transport};
use crate::connection::{ConnectionManager, ConnectionPool};
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
    let mut connection_pool = ConnectionPool::new();

    for server in &config.upstream_servers {
        for interface_config in &server.interfaces {
            let port = interface_config.get_port();
            let server_addr = format!("{}:{}", server.host, port);

            match interface_config.type_ {
                Transport::Udp => {
                    let udp_connection = ConnectionManager::create_udp_connection(
                        server,
                        interface_config,
                        &server_addr,
                    )
                    .await?;
                    connection_pool.add_udp(udp_connection);
                }
                Transport::Tcp => {
                    let tcp_connection = ConnectionManager::create_tcp_connection(
                        server,
                        interface_config,
                        &server_addr,
                    )
                    .await?;
                    connection_pool.add_tcp(tcp_connection);
                }
                Transport::Tls => {
                    let tls_connection = ConnectionManager::create_tls_connection(
                        server,
                        interface_config,
                        &server_addr,
                    )
                    .await?;
                    connection_pool.add_tls(tls_connection);
                }
            }
        }
    }

    Ok(connection_pool)
}
