use crate::config::Config;
use crate::connection::ConnectionManager;
use crate::pool::Connection;
use futures::future::join_all;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
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
                        .create_connection(&server_clone, &interface_clone, &server_addr)
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
        self.connections.push(connection);
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
