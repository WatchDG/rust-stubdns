mod config;

use config::{ListenConfig, load_config};
use dnsio::decode_message;
use futures::future::join_all;
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

async fn start_udp_server(listen_config: ListenConfig) {
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
                println!("UDP: message: {:?}", decode_message(&buffer[..n]).unwrap());
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

    let mut handles = Vec::new();

    for listen_config in config.listen {
        let tcp_config = listen_config.clone();
        let udp_config = listen_config.clone();

        handles.push(tokio::spawn(async move {
            start_tcp_server(tcp_config).await;
        }));

        handles.push(tokio::spawn(async move {
            start_udp_server(udp_config).await;
        }));
    }

    join_all(handles).await;

    Ok(())
}
