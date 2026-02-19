use crate::config::{InterfaceConfig, UpstreamServerConfig};
use crate::pool::{
    Connection, TcpConnection, TcpSocketConfig, TlsConnection, TlsConnectionConfig, UdpConnection,
};
use rustls::ClientConfig;
use rustls::pki_types::ServerName;
use rustls_native_certs;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;

pub struct ConnectionManager;

impl ConnectionManager {
    pub async fn create_udp_connection(
        &self,
        _server: &UpstreamServerConfig,
        interface_config: &InterfaceConfig,
        server_addr: &str,
    ) -> Result<UdpConnection, Box<dyn std::error::Error + Send + Sync>> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let read_timeout = interface_config.get_read_timeout();
        Ok(UdpConnection {
            server_addr: server_addr.to_string(),
            socket: Arc::new(socket),
            read_timeout,
        })
    }

    pub async fn create_tcp_connection(
        &self,
        server: &UpstreamServerConfig,
        interface_config: &InterfaceConfig,
        server_addr: &str,
    ) -> Result<TcpConnection, Box<dyn std::error::Error + Send + Sync>> {
        let port = interface_config.get_port();
        let write_timeout = interface_config.get_write_timeout();
        let read_timeout = interface_config.get_read_timeout();
        let tcp_config = TcpSocketConfig {
            host: server.host.clone(),
            port,
            write_timeout,
            read_timeout,
        };
        let addr: SocketAddr = server_addr.parse()?;
        println!("TCP: connecting to {} at startup", server_addr);

        let connection_timeout = interface_config.get_connection_timeout();
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
        Ok(TcpConnection {
            config: Arc::new(tcp_config),
            stream: Arc::new(Mutex::new(stream)),
        })
    }

    pub async fn create_tls_connection(
        &self,
        server: &UpstreamServerConfig,
        interface_config: &InterfaceConfig,
        server_addr: &str,
    ) -> Result<TlsConnection, Box<dyn std::error::Error + Send + Sync>> {
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
        let tcp_connection = self
            .create_tcp_connection(server, interface_config, server_addr)
            .await?;
        let tcp_stream = {
            let stream_guard = tcp_connection.stream.lock().await;
            let addr: SocketAddr =
                format!("{}:{}", server.host, interface_config.get_port()).parse()?;
            let connection_timeout = interface_config.get_connection_timeout();
            drop(stream_guard);
            drop(tcp_connection);
            if let Some(timeout_ms) = connection_timeout {
                let duration = Duration::from_millis(timeout_ms);
                println!("TLS: using connection timeout of {} ms", timeout_ms);
                timeout(duration, TcpStream::connect(addr))
                    .await
                    .map_err(|_| format!("TLS TCP connection timeout after {} ms", timeout_ms))?
                    .map_err(|e| format!("TLS TCP connection error: {}", e))?
            } else {
                println!("TLS: no connection timeout");
                TcpStream::connect(addr).await?
            }
        };
        println!("TLS: TCP connection established");

        let server_name = ServerName::try_from(auth_name.clone())
            .map_err(|e| format!("Invalid server name: {}", e))?;
        let connector = TlsConnector::from(client_config_arc.clone());

        let tls_handshake_timeout = interface_config.get_tls_handshake_timeout();
        let tls_stream = if let Some(timeout_ms) = tls_handshake_timeout {
            let duration = Duration::from_millis(timeout_ms);
            timeout(duration, connector.connect(server_name, tcp_stream))
                .await
                .map_err(|_| format!("TLS handshake timeout after {} ms", timeout_ms))?
                .map_err(|e| format!("TLS handshake error: {}", e))?
        } else {
            connector.connect(server_name, tcp_stream).await?
        };
        println!("TLS: TLS handshake completed to {}", server_addr);

        let port = interface_config.get_port();
        let write_timeout = interface_config.get_write_timeout();
        let read_timeout = interface_config.get_read_timeout();
        let tls_handshake_timeout = interface_config.get_tls_handshake_timeout();
        let tls_config = TlsConnectionConfig {
            host: server.host.clone(),
            port,
            client_config: client_config_arc,
            auth_name,
            write_timeout,
            read_timeout,
            tls_handshake_timeout,
        };
        Ok(TlsConnection {
            config: Arc::new(tls_config),
            stream: Arc::new(Mutex::new(tls_stream)),
        })
    }

    pub async fn close_udp_connection(
        &self,
        connection: &UdpConnection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // UDP sockets don't have explicit shutdown, dropping is sufficient
        // But we can log the closure
        println!("Closing UDP connection to {}", connection.server_addr);
        Ok(())
    }

    pub async fn close_tcp_connection(
        &self,
        connection: &TcpConnection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut stream_guard = connection.stream.lock().await;
        println!(
            "Closing TCP connection to {}:{}",
            connection.config.host, connection.config.port
        );
        stream_guard.shutdown().await?;
        Ok(())
    }

    pub async fn close_tls_connection(
        &self,
        connection: &TlsConnection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut stream_guard = connection.stream.lock().await;
        println!(
            "Closing TLS connection to {}:{}",
            connection.config.host, connection.config.port
        );
        stream_guard.shutdown().await?;
        Ok(())
    }

    pub async fn close_connection(
        &self,
        connection: &Connection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match connection {
            Connection::Udp(udp_conn) => self.close_udp_connection(udp_conn).await,
            Connection::Tcp(tcp_conn) => self.close_tcp_connection(tcp_conn).await,
            Connection::Tls(tls_conn) => self.close_tls_connection(tls_conn).await,
        }
    }
}
