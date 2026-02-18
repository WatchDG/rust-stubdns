use crate::config::Transport;
use crate::pool::{Connection, TcpConnection, TlsConnection, UdpConnection};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct ConnectionPool {
    connections: Vec<Connection>,
    borrowed: Arc<Mutex<HashSet<usize>>>,
}

impl ConnectionPool {
    pub fn new() -> Self {
        Self {
            connections: Vec::new(),
            borrowed: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn add_udp(&mut self, udp_connection: UdpConnection) {
        self.connections
            .push(Connection::Udp(Arc::new(udp_connection)));
    }

    pub fn add_tcp(&mut self, tcp_connection: TcpConnection) {
        self.connections
            .push(Connection::Tcp(Arc::new(tcp_connection)));
    }

    pub fn add_tls(&mut self, tls_connection: TlsConnection) {
        self.connections
            .push(Connection::Tls(Arc::new(tls_connection)));
    }

    pub fn get_socket(&self, i: usize) -> Option<Connection> {
        if i < self.connections.len() {
            Some(self.connections[i].clone())
        } else {
            None
        }
    }

    pub fn get_socket_by_type(&self, transport: Transport) -> Option<Connection> {
        self.connections
            .iter()
            .find(|conn| conn.get_transport() == transport)
            .map(|conn| conn.clone())
    }

    pub async fn borrow_socket_by_type(&self, transport: Transport) -> Option<(usize, Connection)> {
        let mut borrowed = self.borrowed.lock().await;
        for (index, conn) in self.connections.iter().enumerate() {
            if conn.get_transport() == transport && !borrowed.contains(&index) {
                borrowed.insert(index);
                return Some((index, conn.clone()));
            }
        }
        None
    }

    pub async fn borrow_first_available(&self) -> Option<(usize, Connection)> {
        let mut borrowed = self.borrowed.lock().await;
        for (index, conn) in self.connections.iter().enumerate() {
            if !borrowed.contains(&index) {
                borrowed.insert(index);
                return Some((index, conn.clone()));
            }
        }
        None
    }

    pub async fn return_socket(&self, index: usize) {
        let mut borrowed = self.borrowed.lock().await;
        borrowed.remove(&index);
    }
}
