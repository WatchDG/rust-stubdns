mod config;
mod pool;

use config::{ListenConfig, Transport, load_config};
use dnsio::decode_message;
use futures::future::join_all;
use pool::{Socket, SocketPool, TlsSocket};
use rustls::ClientConfig;
use rustls::pki_types::ServerName;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpSocket, TcpStream, UdpSocket};
use tokio_rustls::TlsConnector;

async fn send_query(
    socket: Socket,
    query_data: &[u8],
    server_addr: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    match socket {
        Socket::Udp(udp_socket) => {
            println!(
                "Query: sending {} bytes via UDP to {}",
                query_data.len(),
                server_addr
            );
            udp_socket.send_to(query_data, server_addr).await?;
            println!("Query: UDP query sent, waiting for response");
            let mut response_buffer = vec![0u8; 4096];
            let (response_len, _) = udp_socket.recv_from(&mut response_buffer).await?;
            println!("Query: received {} bytes response via UDP", response_len);
            Ok(response_buffer[..response_len].to_vec())
        }
        Socket::Tcp(_tcp_socket) => {
            println!(
                "Query: sending {} bytes via TCP to {}",
                query_data.len(),
                server_addr
            );
            let addr: SocketAddr = server_addr.parse()?;
            let mut stream = TcpStream::connect(addr).await?;
            println!("Query: TCP connection established");

            let mut length_bytes = (query_data.len() as u16).to_be_bytes().to_vec();
            length_bytes.extend_from_slice(query_data);
            stream.write_all(&length_bytes).await?;
            println!("Query: TCP query sent, waiting for response");

            let mut length_buffer = [0u8; 2];
            stream.read_exact(&mut length_buffer).await?;
            let response_length = u16::from_be_bytes(length_buffer) as usize;

            let mut response_buffer = vec![0u8; response_length];
            stream.read_exact(&mut response_buffer).await?;
            println!("Query: received {} bytes response via TCP", response_length);
            Ok(response_buffer)
        }
        Socket::Tls(tls_socket) => {
            println!(
                "Query: sending {} bytes via TLS to {}",
                query_data.len(),
                server_addr
            );
            let addr: SocketAddr = server_addr.parse()?;
            let stream = TcpStream::connect(addr).await?;
            println!("Query: TCP connection established for TLS");

            let auth_name = tls_socket.auth_name.clone();
            let server_name = ServerName::try_from(auth_name)
                .map_err(|e| format!("Invalid server name: {}", e))?;
            let connector = TlsConnector::from(tls_socket.config.clone());
            let mut tls_stream = connector.connect(server_name, stream).await?;
            println!("Query: TLS handshake completed");

            let mut length_bytes = (query_data.len() as u16).to_be_bytes().to_vec();
            length_bytes.extend_from_slice(query_data);
            tls_stream.write_all(&length_bytes).await?;
            println!("Query: TLS query sent, waiting for response");

            let mut length_buffer = [0u8; 2];
            tls_stream.read_exact(&mut length_buffer).await?;
            let response_length = u16::from_be_bytes(length_buffer) as usize;

            let mut response_buffer = vec![0u8; response_length];
            tls_stream.read_exact(&mut response_buffer).await?;
            println!("Query: received {} bytes response via TLS", response_length);
            Ok(response_buffer)
        }
    }
}

async fn start_tcp_server(listen_config: ListenConfig) {
    let addr = format!("{}:{}", listen_config.host, listen_config.port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("TCP: error binding to {}: {}", addr, e);
            return;
        }
    };
    println!("TCP server started on {}", addr);

    loop {
        let (mut socket, client_addr) = match listener.accept().await {
            Ok((s, a)) => (s, a),
            Err(e) => {
                eprintln!("TCP: error accepting connection on {}: {}", addr, e);
                continue;
            }
        };

        println!(
            "TCP connection established with {} on {}",
            client_addr, addr
        );

        let mut buffer = vec![0u8; 1024];
        match socket.read(&mut buffer).await {
            Ok(n) => {
                if n > 0 {
                    println!("TCP: received {} bytes from {} on {}", n, client_addr, addr);
                    println!("TCP: data: {:?}", &buffer[..n]);
                    println!("TCP: message: {:?}", decode_message(&buffer[..n]).unwrap());
                }
            }
            Err(e) => {
                eprintln!("TCP: error reading from {} on {}: {}", client_addr, addr, e);
            }
        }

        println!("TCP: connection with {} on {} closed", client_addr, addr);
    }
}

async fn start_udp_server(
    listen_config: ListenConfig,
    socket_pool: Arc<SocketPool>,
    server_addr: String,
) {
    let addr = format!("{}:{}", listen_config.host, listen_config.port);
    let socket = match UdpSocket::bind(&addr).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("UDP: error binding to {}: {}", addr, e);
            return;
        }
    };
    println!("UDP server started on {}", addr);

    loop {
        let mut buffer = vec![0u8; 1024];
        match socket.recv_from(&mut buffer).await {
            Ok((n, client_addr)) => {
                println!("UDP: received {} bytes from {} on {}", n, client_addr, addr);
                println!("UDP: data: {:?}", &buffer[..n]);

                match decode_message(&buffer[..n]) {
                    Ok(msg) => println!("UDP: query message: {:?}", msg),
                    Err(e) => eprintln!("UDP: error decoding message: {}", e),
                }

                let query_data = &buffer[..n];
                let client_socket = socket_pool.get_socket(0).unwrap();

                println!("UDP: forwarding query to DNS server {}", server_addr);

                match send_query(client_socket, query_data, &server_addr).await {
                    Ok(response_data) => {
                        println!(
                            "UDP: received {} bytes response from {}",
                            response_data.len(),
                            server_addr
                        );
                        println!("UDP: response data: {:?}", &response_data);

                        match decode_message(&response_data) {
                            Ok(response_msg) => {
                                println!("UDP: response message: {:?}", response_msg);
                            }
                            Err(e) => {
                                eprintln!("UDP: error decoding response: {}", e)
                            }
                        }

                        match socket.send_to(&response_data, client_addr).await {
                            Ok(_) => {
                                println!("UDP: response sent to client {}", client_addr)
                            }
                            Err(e) => {
                                eprintln!("UDP: error sending response to client: {}", e)
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("UDP: error sending/receiving query: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("UDP: error receiving data on {}: {}", addr, e);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = load_config()?;

    println!("Config: {:#?}", config);

    let mut socket_pool = SocketPool::new();
    let mut first_server_addr = None;

    for server in &config.upstream_servers {
        for interface_config in &server.interfaces {
            let port = interface_config.get_port();
            let server_addr = format!("{}:{}", server.host, port);

            match interface_config.type_ {
                Transport::Udp => {
                    let socket = UdpSocket::bind("0.0.0.0:0").await?;
                    socket_pool.add_udp(socket);
                    if first_server_addr.is_none() {
                        first_server_addr = Some(server_addr);
                    }
                }
                Transport::Tcp => {
                    let socket = TcpSocket::new_v4()?;
                    socket.set_keepalive(true)?;
                    socket_pool.add_tcp(socket);
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
                    let tls_socket = TlsSocket {
                        config: Arc::new(client_config),
                        auth_name,
                    };
                    socket_pool.add_tls(tls_socket);
                    if first_server_addr.is_none() {
                        first_server_addr = Some(server_addr);
                    }
                }
            }
        }
    }

    let socket_pool = Arc::new(socket_pool);

    let server_addr = first_server_addr.unwrap_or_default();

    let mut handles = Vec::new();

    for listen_config in config.listen {
        let tcp_config = listen_config.clone();
        let udp_config = listen_config.clone();
        let socket_pool_clone = socket_pool.clone();
        let server_addr_clone = server_addr.clone();

        handles.push(tokio::spawn(async move {
            start_tcp_server(tcp_config).await;
        }));

        handles.push(tokio::spawn(async move {
            start_udp_server(udp_config, socket_pool_clone, server_addr_clone).await;
        }));
    }

    join_all(handles).await;

    Ok(())
}
