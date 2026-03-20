use crate::config::Config;
use crate::connection::ConnectionPool;
use tokio::sync::mpsc;

pub async fn create_connection_pool(
    config: &Config,
) -> Result<(ConnectionPool, mpsc::Receiver<usize>), Box<dyn std::error::Error + Send + Sync>> {
    let (mut connection_pool, broken_receiver) = ConnectionPool::new();
    connection_pool.initialize(config).await?;
    Ok((connection_pool, broken_receiver))
}
