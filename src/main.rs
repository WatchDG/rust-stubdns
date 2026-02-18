mod config;
mod listen;
mod pool;
mod query;

use config::{Transport, load_config, prepare_config};
use futures::future::join_all;
use listen::{start_tcp_server, start_udp_server};
use pool::create_connection_pool;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = load_config()?;
    let config = prepare_config(config);

    println!("Config: {:#?}", config);

    let (connection_pool, first_server_addr) = create_connection_pool(&config).await?;
    let connection_pool = Arc::new(connection_pool);

    let server_addr = first_server_addr.unwrap_or_default();

    let mut handles = Vec::new();

    for listen_config in config.listen {
        let host = listen_config.host.clone();

        for interface in listen_config.interfaces {
            let port = interface.get_port();
            let connection_pool_clone = connection_pool.clone();
            let server_addr_clone = server_addr.clone();
            let host_clone = host.clone();

            match interface.type_ {
                Transport::Udp => {
                    handles.push(tokio::spawn(async move {
                        start_udp_server(
                            host_clone,
                            port,
                            connection_pool_clone,
                            server_addr_clone,
                        )
                        .await;
                    }));
                }
                Transport::Tcp => {
                    let host_tcp = host.clone();
                    handles.push(tokio::spawn(async move {
                        start_tcp_server(host_tcp, port).await;
                    }));
                }
                Transport::Tls => {
                    eprintln!("TLS listen interface is not yet implemented");
                }
            }
        }
    }

    join_all(handles).await;

    Ok(())
}
