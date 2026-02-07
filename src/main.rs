use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, UdpSocket};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let port = 8053;

    // Start TCP server
    let tcp_handle = tokio::spawn(async move {
        let listener = match TcpListener::bind(format!("0.0.0.0:{}", port)).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("TCP: error binding to port {}: {}", port, e);
                return;
            }
        };
        println!("TCP server started on port {}", port);

        loop {
            let (mut socket, addr) = match listener.accept().await {
                Ok((s, a)) => (s, a),
                Err(e) => {
                    eprintln!("TCP: error accepting connection: {}", e);
                    continue;
                }
            };

            println!("TCP connection established with {}", addr);

            let mut buffer = vec![0u8; 1024];
            match socket.read(&mut buffer).await {
                Ok(n) => {
                    if n > 0 {
                        println!("TCP: received {} bytes from {}", n, addr);
                        println!("TCP: data: {:?}", &buffer[..n]);
                    }
                }
                Err(e) => {
                    eprintln!("TCP: error reading from {}: {}", addr, e);
                }
            }

            println!("TCP: connection with {} closed", addr);
        }
    });

    // Start UDP server
    let udp_handle = tokio::spawn(async move {
        let socket = match UdpSocket::bind(format!("0.0.0.0:{}", port)).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("UDP: error binding to port {}: {}", port, e);
                return;
            }
        };
        println!("UDP server started on port {}", port);

        loop {
            let mut buffer = vec![0u8; 1024];
            match socket.recv_from(&mut buffer).await {
                Ok((n, addr)) => {
                    println!("UDP: received {} bytes from {}", n, addr);
                    println!("UDP: data: {:?}", &buffer[..n]);
                }
                Err(e) => {
                    eprintln!("UDP: error receiving data: {}", e);
                }
            }
        }
    });

    // Wait for both servers to complete
    tokio::select! {
        result = tcp_handle => {
            if let Err(e) = result {
                eprintln!("TCP server exited with error: {:?}", e);
            }
        }
        result = udp_handle => {
            if let Err(e) = result {
                eprintln!("UDP server exited with error: {:?}", e);
            }
        }
    }

    Ok(())
}
