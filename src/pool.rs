use std::sync::Arc;
use tokio::net::UdpSocket;

pub struct UdpPool {
    sockets: Vec<Arc<UdpSocket>>,
    server_addresses: Vec<String>,
}

impl UdpPool {
    pub fn new() -> Self {
        Self {
            sockets: Vec::new(),
            server_addresses: Vec::new(),
        }
    }

    pub fn add(&mut self, udp_socket: UdpSocket, server_address: String) {
        self.sockets.push(Arc::new(udp_socket));
        self.server_addresses.push(server_address);
    }

    pub fn get_socket(&self, i: usize) -> Arc<UdpSocket> {
        self.sockets[i].clone()
    }

    pub fn get_server_address(&self, i: usize) -> &str {
        &self.server_addresses[i]
    }
}
