use crate::config::{InterfaceConfig, Transport, UpstreamServerConfig, CONNECTION_CHECK_TIMEOUT_MS};
use crate::connection::{
    Connection, TcpConnection, TcpSocketConfig, TlsConnection, TlsConnectionConfig, UdpConfig,
    UdpConnection,
};
use rustls::ClientConfig;
use rustls::pki_types::ServerName;
use rustls_native_certs;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncWriteExt, Interest};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;

pub struct ConnectionManager;

impl ConnectionManager {
    pub async fn create_connection(
        &self,
        server: &UpstreamServerConfig,
        interface_config: &InterfaceConfig,
    ) -> Result<Connection, Box<dyn std::error::Error + Send + Sync>> {
        match interface_config.type_ {
            Transport::Udp => {
                let udp = self.create_udp_connection(server, interface_config).await?;
                Ok(Connection::Udp(Arc::new(udp)))
            }
            Transport::Tcp => {
                let tcp = self.create_tcp_connection(server, interface_config).await?;
                Ok(Connection::Tcp(Arc::new(tcp)))
            }
            Transport::Tls => {
                let tls = self.create_tls_connection(server, interface_config).await?;
                Ok(Connection::Tls(Arc::new(tls)))
            }
        }
    }

    pub async fn create_udp_connection(
        &self,
        server: &UpstreamServerConfig,
        interface_config: &InterfaceConfig,
    ) -> Result<UdpConnection, Box<dyn std::error::Error + Send + Sync>> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let port = interface_config.get_port();
        let read_timeout = interface_config.get_read_timeout();
        let server_addr = format!("{}:{}", server.host, port);
        let config = UdpConfig {
            host: server.host.clone(),
            port,
            server_addr,
            read_timeout,
        };
        Ok(UdpConnection {
            config: Arc::new(config),
            socket: Arc::new(socket),
        })
    }

    pub async fn create_tcp_connection(
        &self,
        server: &UpstreamServerConfig,
        interface_config: &InterfaceConfig,
    ) -> Result<TcpConnection, Box<dyn std::error::Error + Send + Sync>> {
        let port = interface_config.get_port();
        let server_addr = format!("{}:{}", server.host, port);
        let write_timeout = interface_config.get_write_timeout();
        let read_timeout = interface_config.get_read_timeout();
        let tcp_config = TcpSocketConfig {
            host: server.host.clone(),
            port,
            server_addr: server_addr.clone(),
            write_timeout,
            read_timeout,
        };
        let addr: SocketAddr = server_addr.parse()?;
        tracing::info!("TCP: connecting to {} at startup", server_addr);

        let connection_timeout = interface_config.get_connection_timeout();
        let stream = if let Some(timeout_ms) = connection_timeout {
            let duration = Duration::from_millis(timeout_ms);
            tracing::debug!("TCP: using connection timeout of {} ms", timeout_ms);
            timeout(duration, TcpStream::connect(addr))
                .await
                .map_err(|_| format!("TCP connection timeout after {} ms", timeout_ms))?
                .map_err(|e| format!("TCP connection error: {}", e))?
        } else {
            tracing::debug!("TCP: no connection timeout");
            TcpStream::connect(addr).await?
        };

        tracing::info!("TCP: connection established to {}", server_addr);
        Ok(TcpConnection {
            config: Arc::new(tcp_config),
            stream: Arc::new(Mutex::new(stream)),
        })
    }

    pub async fn create_tls_connection(
        &self,
        server: &UpstreamServerConfig,
        interface_config: &InterfaceConfig,
    ) -> Result<TlsConnection, Box<dyn std::error::Error + Send + Sync>> {
        let port = interface_config.get_port();
        let server_addr = format!("{}:{}", server.host, port);
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

        tracing::info!("TLS: connecting to {} at startup", server_addr);
        let tcp_connection = self.create_tcp_connection(server, interface_config).await?;
        let tcp_stream = {
            let stream_guard = tcp_connection.stream.lock().await;
            let addr: SocketAddr = server_addr.parse()?;
            let connection_timeout = interface_config.get_connection_timeout();
            drop(stream_guard);
            drop(tcp_connection);
            if let Some(timeout_ms) = connection_timeout {
                let duration = Duration::from_millis(timeout_ms);
                tracing::debug!("TLS: using connection timeout of {} ms", timeout_ms);
                timeout(duration, TcpStream::connect(addr))
                    .await
                    .map_err(|_| format!("TLS TCP connection timeout after {} ms", timeout_ms))?
                    .map_err(|e| format!("TLS TCP connection error: {}", e))?
            } else {
                tracing::debug!("TLS: no connection timeout");
                TcpStream::connect(addr).await?
            }
        };
        tracing::info!("TLS: TCP connection established");

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
        tracing::info!("TLS: TLS handshake completed to {}", server_addr);

        let tcp_config = TcpSocketConfig {
            host: server.host.clone(),
            port,
            server_addr: server_addr.clone(),
            write_timeout: interface_config.get_write_timeout(),
            read_timeout: interface_config.get_read_timeout(),
        };
        let tls_handshake_timeout = interface_config.get_tls_handshake_timeout();
        let tls_config = TlsConnectionConfig {
            tcp_config: Arc::new(tcp_config),
            client_config: client_config_arc,
            auth_name,
            tls_handshake_timeout,
        };
        Ok(TlsConnection {
            config: Arc::new(tls_config),
            stream: Arc::new(Mutex::new(tls_stream)),
        })
    }

    pub async fn check_connection(&self, connection: &Connection) -> bool {
        match connection {
            Connection::Udp(udp_conn) => self.check_udp_connection(udp_conn).await,
            Connection::Tcp(tcp_conn) => self.check_tcp_connection(tcp_conn).await,
            Connection::Tls(tls_conn) => self.check_tls_connection(tls_conn).await,
        }
    }

    pub async fn check_udp_connection(&self, connection: &UdpConnection) -> bool {
        connection.socket.local_addr().is_ok()
    }

    pub async fn check_tcp_connection(&self, connection: &TcpConnection) -> bool {
        let stream_guard = connection.stream.lock().await;
        if stream_guard.peer_addr().is_err() {
            return false;
        }
        let ready = match timeout(
            Duration::from_millis(CONNECTION_CHECK_TIMEOUT_MS),
            stream_guard.ready(Interest::READABLE | Interest::WRITABLE),
        )
        .await
        {
            Ok(Ok(ready)) => ready,
            _ => return false,
        };
        !ready.is_read_closed() && !ready.is_write_closed()
    }

    pub async fn check_tls_connection(&self, connection: &TlsConnection) -> bool {
        let stream_guard = connection.stream.lock().await;
        let (tcp_stream, _) = stream_guard.get_ref();
        if tcp_stream.peer_addr().is_err() {
            return false;
        }
        let ready = match timeout(
            Duration::from_millis(CONNECTION_CHECK_TIMEOUT_MS),
            tcp_stream.ready(Interest::READABLE | Interest::WRITABLE),
        )
        .await
        {
            Ok(Ok(ready)) => ready,
            _ => return false,
        };
        !ready.is_read_closed() && !ready.is_write_closed()
    }

    pub async fn close_udp_connection(
        &self,
        connection: &UdpConnection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // UDP sockets don't have explicit shutdown, dropping is sufficient
        // But we can log the closure
        tracing::debug!(
            "Closing UDP connection to {}",
            connection.config.server_addr
        );
        Ok(())
    }

    pub async fn close_tcp_connection(
        &self,
        connection: &TcpConnection,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut stream_guard = connection.stream.lock().await;
        tracing::debug!(
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
        tracing::debug!(
            "Closing TLS connection to {}",
            connection.config.tcp_config.server_addr
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
