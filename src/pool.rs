use std::sync::Arc;
use tokio::net::{TcpSocket, UdpSocket};

pub enum Socket {
    Udp(Arc<UdpSocket>),
    Tcp(Arc<TcpSocket>),
}

pub struct SocketPool {
    sockets: Vec<Socket>,
}

impl SocketPool {
    pub fn new() -> Self {
        Self {
            sockets: Vec::new(),
        }
    }

    pub fn add_udp(&mut self, udp_socket: UdpSocket) {
        self.sockets.push(Socket::Udp(Arc::new(udp_socket)));
    }

    pub fn add_tcp(&mut self, tcp_socket: TcpSocket) {
        self.sockets.push(Socket::Tcp(Arc::new(tcp_socket)));
    }

    pub fn get_socket(&self, i: usize) -> Option<Socket> {
        if i < self.sockets.len() {
            Some(self.sockets[i].clone())
        } else {
            None
        }
    }
}

impl Clone for Socket {
    fn clone(&self) -> Self {
        match self {
            Socket::Udp(socket) => Socket::Udp(socket.clone()),
            Socket::Tcp(socket) => Socket::Tcp(socket.clone()),
        }
    }
}
