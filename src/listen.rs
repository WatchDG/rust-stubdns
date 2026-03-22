use crate::config::TCP_BUFFER_SIZE;
use crate::connection::ConnectionPool;
use crate::query::send_query;
use dnsio::decode_message_ref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

async fn handle_tcp_connection(
    mut socket: TcpStream,
    client_addr: std::net::SocketAddr,
    addr: String,
    connection_pool: Arc<ConnectionPool>,
) {
    tracing::info!(
        "TCP connection established with {} on {}",
        client_addr, addr
    );

    let mut buffer = vec![0u8; TCP_BUFFER_SIZE];
    let n = match socket.read(&mut buffer).await {
        Ok(n) => n,
        Err(e) => {
            tracing::error!("TCP: error reading from {} on {}: {}", client_addr, addr, e);
            return;
        }
    };

    if n == 0 {
        tracing::info!("TCP: connection with {} on {} closed", client_addr, addr);
        return;
    }

    tracing::debug!("TCP: received {} bytes from {} on {}", n, client_addr, addr);
    tracing::trace!("TCP: data: {:?}", &buffer[..n]);

    let query_data = buffer[..n].to_vec();

    match decode_message_ref(&query_data) {
        Ok(msg_ref) => {
            tracing::debug!("TCP: query message ref - header offset: {}, question count: {}",
                msg_ref.header.offset(), msg_ref.question.count);
            if let Ok(header) = msg_ref.header.decode_header(&query_data) {
                tracing::debug!("TCP: query id={}, qd={}, an={}, ns={}, ar={}",
                    header.id, header.qd_count, header.an_count, header.ns_count, header.ar_count);
            }
        }
        Err(e) => {
            tracing::error!("TCP: error decoding message: {}", e);
            return;
        }
    };

    let borrowed_connection = match connection_pool.borrow_first_available().await {
        Some((index, conn)) => {
            tracing::debug!("TCP: borrowed connection {} from pool", index);
            (index, conn)
        }
        None => {
            tracing::error!("TCP: no available connections in pool");
            return;
        }
    };

    let (index, client_connection) = borrowed_connection;
    let server_addr = client_connection.get_server_addr();
    tracing::debug!("TCP: forwarding query to DNS server {}", server_addr);

    let result = send_query(client_connection, &query_data).await;

    connection_pool.return_socket(index).await;
    tracing::debug!("TCP: returned connection {} to pool", index);

    match result {
        Ok(response_data) => {
            tracing::debug!(
                "TCP: received {} bytes response from {}",
                response_data.len(),
                server_addr
            );

            if let Ok(response_ref) = decode_message_ref(&response_data) {
                tracing::debug!("TCP: response message ref - header offset: {}, answer count: {}",
                    response_ref.header.offset(), response_ref.answer.count);
                if let Ok(header) = response_ref.header.decode_header(&response_data) {
                    tracing::debug!("TCP: response id={}, rcode={:?}", header.id, header.flags.r_code);
                }
            }

            let mut length_bytes = (response_data.len() as u16).to_be_bytes().to_vec();
            length_bytes.extend_from_slice(&response_data);

            if let Err(e) = socket.write_all(&length_bytes).await {
                tracing::error!("TCP: error sending response to client: {}", e);
            } else {
                tracing::info!("TCP: response sent to client {}", client_addr);
            }
        }
        Err(e) => {
            tracing::error!("TCP: error sending/receiving query: {}", e);
        }
    }

    tracing::info!("TCP: connection with {} on {} closed", client_addr, addr);
}

pub async fn start_tcp_server(host: String, port: u16, connection_pool: Arc<ConnectionPool>, shutdown_flag: Arc<AtomicBool>) {
    let addr = format!("{}:{}", host, port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("TCP: error binding to {}: {}", addr, e);
            return;
        }
    };
    tracing::info!("TCP server started on {}", addr);

    loop {
        if shutdown_flag.load(Ordering::SeqCst) {
            tracing::info!("TCP server on {} shutting down", addr);
            break;
        }

        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((socket, client_addr)) => {
                        let addr_clone = addr.clone();
                        let connection_pool = connection_pool.clone();
                        tokio::spawn(async move {
                            handle_tcp_connection(socket, client_addr, addr_clone, connection_pool).await;
                        });
                    }
                    Err(e) => {
                        tracing::error!("TCP: error accepting connection on {}: {}", addr, e);
                    }
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
        }
    }
}

pub async fn start_udp_server(host: String, port: u16, connection_pool: Arc<ConnectionPool>, shutdown_flag: Arc<AtomicBool>) {
    let addr = format!("{}:{}", host, port);
    let socket = Arc::new(match UdpSocket::bind(&addr).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("UDP: error binding to {}: {}", addr, e);
            return;
        }
    });
    tracing::info!("UDP server started on {}", addr);

    loop {
        if shutdown_flag.load(Ordering::SeqCst) {
            tracing::info!("UDP server on {} shutting down", addr);
            break;
        }

        let mut buffer = vec![0u8; TCP_BUFFER_SIZE];
        tokio::select! {
            result = socket.recv_from(&mut buffer) => {
                match result {
                    Ok((n, client_addr)) => {
                        let socket = Arc::clone(&socket);
                        let connection_pool = connection_pool.clone();
                        let query_data = buffer[..n].to_vec();
                        let addr = addr.clone();

                        tokio::spawn(async move {
                            tracing::debug!("UDP: received {} bytes from {} on {}", n, client_addr, addr);
                            tracing::trace!("UDP: data: {:?}", &query_data);

                            match decode_message_ref(&query_data) {
                                Ok(msg_ref) => {
                                    tracing::debug!("UDP: query message ref - header offset: {}, question count: {}",
                                        msg_ref.header.offset(), msg_ref.question.count);
                                    if let Ok(header) = msg_ref.header.decode_header(&query_data) {
                                        tracing::debug!("UDP: query id={}, qd={}, an={}, ns={}, ar={}",
                                            header.id, header.qd_count, header.an_count, header.ns_count, header.ar_count);
                                    }
                                }
                                Err(e) => tracing::error!("UDP: error decoding message: {}", e),
                            }

                            let borrowed_connection = match connection_pool.borrow_first_available().await {
                                Some((index, conn)) => {
                                    tracing::debug!("UDP: borrowed connection {} from pool", index);
                                    Some((index, conn))
                                }
                                None => {
                                    tracing::error!("UDP: no available connections in pool");
                                    None
                                }
                            };

                            if let Some((index, client_connection)) = borrowed_connection {
                                let server_addr = client_connection.get_server_addr();
                                tracing::debug!("UDP: forwarding query to DNS server {}", server_addr);

                                let result = send_query(client_connection, &query_data).await;

                                connection_pool.return_socket(index).await;
                                tracing::debug!("UDP: returned connection {} to pool", index);

                                match result {
                                    Ok(response_data) => {
                                        tracing::debug!(
                                            "UDP: received {} bytes response from {}",
                                            response_data.len(),
                                            server_addr
                                        );

                                        if let Ok(response_ref) = decode_message_ref(&response_data) {
                                            tracing::debug!("UDP: response message ref - header offset: {}, answer count: {}",
                                                response_ref.header.offset(), response_ref.answer.count);
                                            if let Ok(header) = response_ref.header.decode_header(&response_data) {
                                                tracing::debug!("UDP: response id={}, rcode={:?}", header.id, header.flags.r_code);
                                            }
                                        }

                                        match socket.send_to(&response_data, client_addr).await {
                                            Ok(_) => {
                                                tracing::info!("UDP: response sent to client {}", client_addr)
                                            }
                                            Err(e) => {
                                                tracing::error!("UDP: error sending response to client: {}", e)
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!("UDP: error sending/receiving query: {}", e);
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("UDP: error receiving data on {}: {}", addr, e);
                    }
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
        }
    }
}