use crate::config::Config;
use crate::connection::ConnectionPool;

pub async fn create_connection_pool(
    config: &Config,
) -> Result<ConnectionPool, Box<dyn std::error::Error + Send + Sync>> {
    let mut connection_pool = ConnectionPool::new();
    connection_pool.initialize(config).await?;
    Ok(connection_pool)
}
