use serde::{Deserialize, Serialize};
use std::fs;
use std::io;

pub const CONNECTION_TIMEOUT: u64 = 10000;
pub const WRITE_TIMEOUT: u64 = 10000;
pub const READ_TIMEOUT: u64 = 10000;
pub const TLS_HANDSHAKE_TIMEOUT: u64 = 10000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub listen: Vec<ListenConfig>,
    #[serde(rename = "upstreamServers")]
    pub upstream_servers: Vec<UpstreamServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListenConfig {
    pub host: String,
    pub interfaces: Vec<InterfaceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Transport {
    Udp,
    Tcp,
    Tls,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceConfig {
    #[serde(rename = "type")]
    pub type_: Transport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(rename = "authName", skip_serializing_if = "Option::is_none")]
    pub auth_name: Option<String>,
    #[serde(rename = "connectionTimeout", skip_serializing_if = "Option::is_none")]
    pub connection_timeout: Option<u64>,
    #[serde(rename = "writeTimeout", skip_serializing_if = "Option::is_none")]
    pub write_timeout: Option<u64>,
    #[serde(rename = "readTimeout", skip_serializing_if = "Option::is_none")]
    pub read_timeout: Option<u64>,
    #[serde(
        rename = "tlsHandshakeTimeout",
        skip_serializing_if = "Option::is_none"
    )]
    pub tls_handshake_timeout: Option<u64>,
}

impl InterfaceConfig {
    pub fn get_port(&self) -> u16 {
        match self.type_ {
            Transport::Udp => self.port.unwrap_or(53),
            Transport::Tcp => self.port.unwrap_or(53),
            Transport::Tls => self.port.unwrap_or(853),
        }
    }

    pub fn get_auth_name(&self) -> String {
        self.auth_name
            .clone()
            .expect("authName is required for TLS interface")
    }

    pub fn get_connection_timeout(&self) -> Option<u64> {
        match self.connection_timeout {
            Some(0) => None,
            Some(timeout) => Some(timeout),
            None => Some(CONNECTION_TIMEOUT),
        }
    }

    pub fn get_write_timeout(&self) -> Option<u64> {
        match self.write_timeout {
            Some(0) => None,
            Some(timeout) => Some(timeout),
            None => Some(WRITE_TIMEOUT),
        }
    }

    pub fn get_read_timeout(&self) -> Option<u64> {
        match self.read_timeout {
            Some(0) => None,
            Some(timeout) => Some(timeout),
            None => Some(READ_TIMEOUT),
        }
    }

    pub fn get_tls_handshake_timeout(&self) -> Option<u64> {
        match self.tls_handshake_timeout {
            Some(0) => None,
            Some(timeout) => Some(timeout),
            None => Some(TLS_HANDSHAKE_TIMEOUT),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamServerConfig {
    pub host: String,
    #[serde(rename = "interfaces")]
    pub interfaces: Vec<InterfaceConfig>,
    #[serde(rename = "connectionTimeout", skip_serializing_if = "Option::is_none")]
    pub connection_timeout: Option<u64>,
    #[serde(rename = "writeTimeout", skip_serializing_if = "Option::is_none")]
    pub write_timeout: Option<u64>,
    #[serde(rename = "readTimeout", skip_serializing_if = "Option::is_none")]
    pub read_timeout: Option<u64>,
    #[serde(
        rename = "tlsHandshakeTimeout",
        skip_serializing_if = "Option::is_none"
    )]
    pub tls_handshake_timeout: Option<u64>,
}

impl UpstreamServerConfig {
    pub fn get_connection_timeout(&self) -> Option<u64> {
        match self.connection_timeout {
            Some(0) => None,
            Some(timeout) => Some(timeout),
            None => Some(CONNECTION_TIMEOUT),
        }
    }

    pub fn get_write_timeout(&self) -> Option<u64> {
        match self.write_timeout {
            Some(0) => None,
            Some(timeout) => Some(timeout),
            None => Some(WRITE_TIMEOUT),
        }
    }

    pub fn get_read_timeout(&self) -> Option<u64> {
        match self.read_timeout {
            Some(0) => None,
            Some(timeout) => Some(timeout),
            None => Some(READ_TIMEOUT),
        }
    }

    pub fn get_tls_handshake_timeout(&self) -> Option<u64> {
        match self.tls_handshake_timeout {
            Some(0) => None,
            Some(timeout) => Some(timeout),
            None => Some(TLS_HANDSHAKE_TIMEOUT),
        }
    }
}

pub fn prepare_config(mut config: Config) -> Config {
    for server in &mut config.upstream_servers {
        let server_timeout = server.get_connection_timeout();
        let server_write_timeout = server.get_write_timeout();
        let server_read_timeout = server.get_read_timeout();
        let server_tls_handshake_timeout = server.get_tls_handshake_timeout();

        for interface in &mut server.interfaces {
            if interface.connection_timeout.is_none() {
                interface.connection_timeout = server_timeout;
            }
            if interface.write_timeout.is_none() {
                interface.write_timeout = server_write_timeout;
            }
            if interface.read_timeout.is_none() {
                interface.read_timeout = server_read_timeout;
            }
            if interface.tls_handshake_timeout.is_none() {
                interface.tls_handshake_timeout = server_tls_handshake_timeout;
            }
        }
    }
    config
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
