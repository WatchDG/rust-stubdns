use serde::{Deserialize, Serialize};
use std::fs;
use std::io;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub listen: Vec<ListenConfig>,
    #[serde(rename = "upstreamServers")]
    pub upstream_servers: Vec<UpstreamServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListenConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Transport {
    Udp,
    Tcp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    pub transport: Transport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

impl TransportConfig {
    pub fn get_port(&self) -> u16 {
        match self.transport {
            Transport::Udp => self.port.unwrap_or(53),
            Transport::Tcp => self.port.unwrap_or(53),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamServerConfig {
    pub host: String,
    pub transports: Vec<TransportConfig>,
}

pub fn load_config() -> Result<Config, Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<String> = std::env::args().collect();
    let mut config_path = None;

    for arg in args.iter() {
        if arg.starts_with("--config=") {
            config_path = Some(arg.strip_prefix("--config=").unwrap().to_string());
            break;
        }
    }

    let config_path = config_path.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Usage: stubdns --config=<config_path>",
        )
    })?;

    let config_content = fs::read_to_string(&config_path)?;
    let config: Config = serde_json::from_str(&config_content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    Ok(config)
}
