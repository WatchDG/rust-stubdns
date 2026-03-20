mod config;
mod connection;
mod listen;
mod query;
mod utils;

use config::{Transport, load_config, prepare_config};
use connection::ConnectionWatchdog;
use tokio::signal;
use tokio::task::JoinSet;
use listen::{start_tcp_server, start_udp_server};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use utils::create_connection_pool;

async fn shutdown_signal(shutdown_flag: Arc<AtomicBool>) {
    signal::ctrl_c()
        .await
        .expect("Failed to listen for shutdown signal");
    shutdown_flag.store(true, Ordering::SeqCst);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    let config = load_config()?;
    let config = prepare_config(config);

    tracing::info!("Config: {:#?}", config);

    let (connection_pool, broken_receiver) = create_connection_pool(&config).await?;
    let connection_pool = Arc::new(connection_pool);
    let watchdog = Arc::new(ConnectionWatchdog::new(
        config.clone(),
        connection_pool.clone(),
    ));
    watchdog.clone().start(broken_receiver);

    let mut join_set = JoinSet::new();
    let shutdown_flag = Arc::new(AtomicBool::new(false));

    for listen_config in config.listen {
        let host = listen_config.host.clone();

        for interface in listen_config.interfaces {
            let port = interface.get_port();
            let connection_pool_clone = connection_pool.clone();
            let host_clone = host.clone();
            let shutdown_flag_clone = shutdown_flag.clone();

            match interface.type_ {
                Transport::Udp => {
                    join_set.spawn(async move {
                        start_udp_server(host_clone, port, connection_pool_clone, shutdown_flag_clone).await;
                    });
                }
                Transport::Tcp => {
                    let host_tcp = host.clone();
                    join_set.spawn(async move {
                        start_tcp_server(host_tcp, port, connection_pool_clone, shutdown_flag_clone).await;
                    });
                }
                Transport::Tls => {
                    tracing::warn!("TLS listen interface is not yet implemented");
                }
            }
        }
    }

    let shutdown_flag_clone = shutdown_flag.clone();
    let signal_handle = tokio::spawn(async move {
        shutdown_signal(shutdown_flag_clone).await;
    });

    tokio::select! {
        _ = signal_handle => {
            tracing::info!("Received shutdown signal, stopping servers...");
        }
        result = join_set.join_next() => {
            match result {
                Some(Ok(())) => {}
                Some(Err(e)) => {
                    tracing::error!("Server error: {:?}", e);
                }
                None => {}
            }
        }
    }

    join_set.abort_all();

    while join_set.join_next().await.is_some() {}

    tracing::info!("Server shutdown complete");

    Ok(())
}
