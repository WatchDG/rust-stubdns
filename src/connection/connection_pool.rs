use crate::config::Config;
use crate::connection::Connection;
use crate::connection::ConnectionManager;
use futures::future::join_all;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ConnectionPool {
    pub(crate) connections: Arc<Mutex<Vec<Connection>>>,
    available_connections: Arc<Mutex<HashSet<usize>>>,
    borrowed_connections: Arc<Mutex<HashSet<usize>>>,
    broken_connections: Arc<Mutex<HashSet<usize>>>,
    broken_sender: tokio::sync::mpsc::Sender<usize>,
}

impl ConnectionPool {
    pub fn new() -> (Self, tokio::sync::mpsc::Receiver<usize>) {
        let (broken_sender, broken_receiver) = tokio::sync::mpsc::channel(100);
        let pool = Self {
            connections: Arc::new(Mutex::new(Vec::new())),
            available_connections: Arc::new(Mutex::new(HashSet::new())),
            borrowed_connections: Arc::new(Mutex::new(HashSet::new())),
            broken_connections: Arc::new(Mutex::new(HashSet::new())),
            broken_sender,
        };
        (pool, broken_receiver)
    }

    pub async fn initialize(
        &mut self,
        config: &Config,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let connection_manager = Arc::new(ConnectionManager);
        let mut handles = Vec::new();
        let pool_arc = Arc::new(Mutex::new(self.clone()));
        let manager_clone = connection_manager.clone();

        for server in &config.upstream_servers {
            for interface_config in &server.interfaces {
                let server_clone = server.clone();
                let interface_clone = interface_config.clone();
                let pool_clone = pool_arc.clone();
                let manager_clone_inner = manager_clone.clone();

                handles.push(tokio::spawn(async move {
                    let port = interface_clone.get_port();
                    let server_addr = format!("{}:{}", server_clone.host, port);
                    let connection = manager_clone_inner
                        .create_connection(&server_clone, &interface_clone)
                        .await?;
                    println!("Pool: connection established to {}", server_addr);
                    let mut pool = pool_clone.lock().await;
                    pool.add_connection(connection);
                    Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                }));
            }
        }

        let results = join_all(handles).await;
        for result in results {
            result??;
        }

        *self = Arc::try_unwrap(pool_arc)
            .map_err(|_| {
                Box::<dyn std::error::Error + Send + Sync>::from("Failed to unwrap pool arc")
            })?
            .into_inner();

        Ok(())
    }

    fn add_connection(&mut self, connection: Connection) {
        let mut connections = self.connections.try_lock().expect("connections lock");
        let index = connections.len();
        connections.push(connection);
        drop(connections);
        self.available_connections
            .try_lock()
            .expect("available_connections lock")
            .insert(index);
    }

    pub async fn borrow_first_available(&self) -> Option<(usize, Connection)> {
        let connection_manager = ConnectionManager;
        loop {
            let candidate = {
                let mut available = self.available_connections.lock().await;
                let index = available.iter().next().copied()?;
                available.remove(&index);
                drop(available);
                let connections = self.connections.lock().await;
                let conn = connections.get(index)?.clone();
                Some((index, conn))
            };
            match candidate {
                Some((index, conn)) => {
                    if connection_manager.check_connection(&conn).await {
                        let mut borrowed_connections = self.borrowed_connections.lock().await;
                        borrowed_connections.insert(index);
                        return Some((index, conn));
                    } else {
                        self.mark_broken(index).await;
                    }
                }
                None => return None,
            }
        }
    }

    pub async fn return_socket(&self, index: usize) {
        let mut borrowed_connections = self.borrowed_connections.lock().await;
        borrowed_connections.remove(&index);
        let broken = self.broken_connections.lock().await;
        if !broken.contains(&index) {
            drop(broken);
            let mut available = self.available_connections.lock().await;
            available.insert(index);
        }
    }

    pub async fn get_connection(&self, index: usize) -> Option<Connection> {
        let connections = self.connections.lock().await;
        connections.get(index).cloned()
    }

    pub async fn add_connection_at(&self, index: usize, connection: Connection) {
        let mut connections = self.connections.lock().await;
        if index >= connections.len() {
            return;
        }
        connections[index] = connection;
        drop(connections);

        let mut available = self.available_connections.lock().await;
        let mut borrowed = self.borrowed_connections.lock().await;
        let mut broken = self.broken_connections.lock().await;
        broken.remove(&index);
        borrowed.remove(&index);
        available.insert(index);
    }

    pub async fn mark_broken(&self, index: usize) {
        let mut available = self.available_connections.lock().await;
        available.remove(&index);
        drop(available);
        let mut borrowed_connections = self.borrowed_connections.lock().await;
        borrowed_connections.remove(&index);
        drop(borrowed_connections);
        let mut broken = self.broken_connections.lock().await;
        broken.insert(index);
        let _ = self.broken_sender.send(index).await;
    }
}
