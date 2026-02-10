mod config;
mod pool;

use config::{Transport, load_config};
use dnsio::decode_message;
use futures::future::join_all;
use pool::{Connection, ConnectionPool, create_connection_pool};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};

async fn send_query(
    connection: Connection,
    query_data: &[u8],
    server_addr: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    match connection {
        Connection::Udp(udp_socket) => {
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
        Connection::Tcp(tcp_connection) => {
            let tcp_addr = format!(
                "{}:{}",
                tcp_connection.config.host, tcp_connection.config.port
            );
            println!(
                "Query: sending {} bytes via TCP to {}",
                query_data.len(),
                tcp_addr
            );
            println!("Query: using existing TCP connection");

            let mut stream_guard = tcp_connection.stream.lock().await;
            let (mut reader, mut writer) = tokio::io::split(&mut *stream_guard);

            let mut length_bytes = (query_data.len() as u16).to_be_bytes().to_vec();
            length_bytes.extend_from_slice(query_data);
            writer.write_all(&length_bytes).await?;
            println!("Query: TCP query sent, waiting for response");

            let mut length_buffer = [0u8; 2];
            reader.read_exact(&mut length_buffer).await?;
            let response_length = u16::from_be_bytes(length_buffer) as usize;

            let mut response_buffer = vec![0u8; response_length];
            reader.read_exact(&mut response_buffer).await?;
            println!("Query: received {} bytes response via TCP", response_length);
            Ok(response_buffer)
        }
        Connection::Tls(tls_connection) => {
            let tls_addr = format!(
                "{}:{}",
                tls_connection.config.host, tls_connection.config.port
            );
            println!(
                "Query: sending {} bytes via TLS to {}",
                query_data.len(),
                tls_addr
            );
            println!("Query: using existing TLS connection");

            let mut stream_guard = tls_connection.stream.lock().await;
            let (mut reader, mut writer) = tokio::io::split(&mut *stream_guard);

            let mut length_bytes = (query_data.len() as u16).to_be_bytes().to_vec();
            length_bytes.extend_from_slice(query_data);
            writer.write_all(&length_bytes).await?;
            println!("Query: TLS query sent, waiting for response");

            let mut length_buffer = [0u8; 2];
            reader.read_exact(&mut length_buffer).await?;
            let response_length = u16::from_be_bytes(length_buffer) as usize;

            let mut response_buffer = vec![0u8; response_length];
            reader.read_exact(&mut response_buffer).await?;
            println!("Query: received {} bytes response via TLS", response_length);
            Ok(response_buffer)
        }
    }
}

async fn start_tcp_server(host: String, port: u16) {
    let addr = format!("{}:{}", host, port);
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
    host: String,
    port: u16,
    connection_pool: Arc<ConnectionPool>,
    server_addr: String,
) {
    let addr = format!("{}:{}", host, port);
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
                let client_connection = connection_pool.get_socket(0).unwrap();

                println!("UDP: forwarding query to DNS server {}", server_addr);

                match send_query(client_connection, query_data, &server_addr).await {
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

    let (connection_pool, first_server_addr) = create_connection_pool(&config).await?;
    let connection_pool = Arc::new(connection_pool);

    let server_addr = first_server_addr.unwrap_or_default();

    let mut handles = Vec::new();

    for listen_config in config.listen {
        let host = listen_config.host.clone();

        for interface in listen_config.interfaces {
            let port = interface.get_port();
            let connection_pool_clone = connection_pool.clone();
            let server_addr_clone = server_addr.clone();
            let host_clone = host.clone();

            match interface.type_ {
                Transport::Udp => {
                    handles.push(tokio::spawn(async move {
                        start_udp_server(
                            host_clone,
                            port,
                            connection_pool_clone,
                            server_addr_clone,
                        )
                        .await;
                    }));
                }
                Transport::Tcp => {
                    let host_tcp = host.clone();
                    handles.push(tokio::spawn(async move {
                        start_tcp_server(host_tcp, port).await;
                    }));
                }
                Transport::Tls => {
                    eprintln!("TLS listen interface is not yet implemented");
                }
            }
        }
    }

    join_all(handles).await;

    Ok(())
}
