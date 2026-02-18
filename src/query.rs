use crate::pool::Connection;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn send_query(
    connection: Connection,
    query_data: &[u8],
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    match connection {
        Connection::Udp(udp_connection) => {
            println!(
                "Query: sending {} bytes via UDP to {}",
                query_data.len(),
                udp_connection.server_addr
            );
            udp_connection
                .socket
                .send_to(query_data, &udp_connection.server_addr)
                .await?;
            println!("Query: UDP query sent, waiting for response");
            let mut response_buffer = vec![0u8; 4096];
            let (response_len, _) = udp_connection
                .socket
                .recv_from(&mut response_buffer)
                .await?;
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
