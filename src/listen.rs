use crate::pool::ConnectionPool;
use crate::query::send_query;
use dnsio::decode_message;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, UdpSocket};

pub async fn start_tcp_server(host: String, port: u16) {
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

pub async fn start_udp_server(
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

                let borrowed_connection = match connection_pool.borrow_first_available().await {
                    Some((index, conn)) => {
                        println!("UDP: borrowed connection {} from pool", index);
                        Some((index, conn))
                    }
                    None => {
                        eprintln!("UDP: no available connections in pool");
                        None
                    }
                };

                if let Some((index, client_connection)) = borrowed_connection {
                    println!("UDP: forwarding query to DNS server {}", server_addr);

                    let result = send_query(client_connection, query_data).await;

                    connection_pool.return_socket(index).await;
                    println!("UDP: returned connection {} to pool", index);

                    match result {
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
            }
            Err(e) => {
                eprintln!("UDP: error receiving data on {}: {}", addr, e);
            }
        }
    }
}
