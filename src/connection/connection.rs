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

use crate::config::{InterfaceConfig, Transport, UpstreamServerConfig};

impl Connection {
    pub fn get_server_addr(&self) -> String {
        match self {
            Connection::Udp(udp_conn) => udp_conn.config.server_addr.clone(),
            Connection::Tcp(tcp_conn) => tcp_conn.config.server_addr.clone(),
            Connection::Tls(tls_conn) => tls_conn.config.tcp_config.server_addr.clone(),
        }
    }

    pub fn to_server_and_interface_config(
        &self,
    ) -> Option<(UpstreamServerConfig, InterfaceConfig)> {
        match self {
            Connection::Udp(udp_conn) => {
                let interface = InterfaceConfig {
                    type_: Transport::Udp,
                    port: Some(udp_conn.config.port),
                    auth_name: None,
                    connection_timeout: None,
                    write_timeout: None,
                    read_timeout: udp_conn.config.read_timeout,
                    tls_handshake_timeout: None,
                };
                let server = UpstreamServerConfig {
                    host: udp_conn.config.host.clone(),
                    interfaces: vec![interface.clone()],
                    connection_timeout: None,
                    write_timeout: None,
                    read_timeout: None,
                    tls_handshake_timeout: None,
                };
                Some((server, interface))
            }
            Connection::Tcp(tcp_conn) => {
                let interface = InterfaceConfig {
                    type_: Transport::Tcp,
                    port: Some(tcp_conn.config.port),
                    auth_name: None,
                    connection_timeout: None,
                    write_timeout: tcp_conn.config.write_timeout,
                    read_timeout: tcp_conn.config.read_timeout,
                    tls_handshake_timeout: None,
                };
                let server = UpstreamServerConfig {
                    host: tcp_conn.config.host.clone(),
                    interfaces: vec![interface.clone()],
                    connection_timeout: None,
                    write_timeout: None,
                    read_timeout: None,
                    tls_handshake_timeout: None,
                };
                Some((server, interface))
            }
            Connection::Tls(tls_conn) => {
                let tcp_config = &tls_conn.config.tcp_config;
                let interface = InterfaceConfig {
                    type_: Transport::Tls,
                    port: Some(tcp_config.port),
                    auth_name: Some(tls_conn.config.auth_name.clone()),
                    connection_timeout: None,
                    write_timeout: tcp_config.write_timeout,
                    read_timeout: tcp_config.read_timeout,
                    tls_handshake_timeout: tls_conn.config.tls_handshake_timeout,
                };
                let server = UpstreamServerConfig {
                    host: tcp_config.host.clone(),
                    interfaces: vec![interface.clone()],
                    connection_timeout: None,
                    write_timeout: None,
                    read_timeout: None,
                    tls_handshake_timeout: None,
                };
                Some((server, interface))
            }
        }
    }
}
