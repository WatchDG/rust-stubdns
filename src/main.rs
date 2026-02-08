mod config;
mod pool;

use config::{ListenConfig, load_config};
use dnsio::decode_message;
use futures::future::join_all;
use pool::UdpPool;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, UdpSocket};

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

async fn start_udp_server(listen_config: ListenConfig, udp_pool: Arc<UdpPool>) {
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
                let client_socket = udp_pool.get_socket(0);
                let server_addr = udp_pool.get_server_address(0);

                println!("UDP: forwarding query to DNS server {}", server_addr);

                match client_socket.send_to(query_data, &server_addr).await {
                    Ok(_) => {
                        println!("UDP: query sent to {}", server_addr);

                        let mut response_buffer = vec![0u8; 4096];
                        match client_socket.recv_from(&mut response_buffer).await {
                            Ok((response_len, _)) => {
                                println!(
                                    "UDP: received {} bytes response from {}",
                                    response_len, server_addr
                                );
                                println!(
                                    "UDP: response data: {:?}",
                                    &response_buffer[..response_len]
                                );

                                match decode_message(&response_buffer[..response_len]) {
                                    Ok(response_msg) => {
                                        println!("UDP: response message: {:?}", response_msg);
                                    }
                                    Err(e) => {
                                        eprintln!("UDP: error decoding response: {}", e)
                                    }
                                }

                                match socket
                                    .send_to(&response_buffer[..response_len], client_addr)
                                    .await
                                {
                                    Ok(_) => {
                                        println!("UDP: response sent to client {}", client_addr)
                                    }
                                    Err(e) => {
                                        eprintln!("UDP: error sending response to client: {}", e)
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!(
                                    "UDP: error receiving response from {}: {}",
                                    server_addr, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("UDP: error sending query to {}: {}", server_addr, e);
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

    let mut udp_pool = UdpPool::new();

    for server in &config.upstream_servers {
        let server_addr = format!("{}:53", server.host);
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        udp_pool.add(socket, server_addr);
    }

    let udp_pool = Arc::new(udp_pool);

    let mut handles = Vec::new();

    for listen_config in config.listen {
        let tcp_config = listen_config.clone();
        let udp_config = listen_config.clone();
        let udp_pool_clone = udp_pool.clone();

        handles.push(tokio::spawn(async move {
            start_tcp_server(tcp_config).await;
        }));

        handles.push(tokio::spawn(async move {
            start_udp_server(udp_config, udp_pool_clone).await;
        }));
    }

    join_all(handles).await;

    Ok(())
}
