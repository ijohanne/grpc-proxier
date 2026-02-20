use std::collections::HashMap;
use std::net::SocketAddr;

use serde::Deserialize;

use crate::error::ProxyError;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub listen_address: SocketAddr,
    pub upstream_address: String,
    pub metrics_address: SocketAddr,
    #[serde(default)]
    pub users: HashMap<String, UserConfig>,
}

#[derive(Debug, Deserialize)]
pub struct UserConfig {
    pub allowed_calls: Vec<String>,
}

#[derive(Debug)]
pub struct Credentials {
    pub users: HashMap<String, String>,
}

impl Credentials {
    pub fn empty() -> Self {
        Self {
            users: HashMap::new(),
        }
    }
}

pub fn load_config(path: &str) -> Result<Config, ProxyError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ProxyError::ConfigLoad(format!("{path}: {e}")))?;
    toml::from_str(&content).map_err(|e| ProxyError::ConfigLoad(format!("{path}: {e}")))
}

pub fn load_credentials(path: &str) -> Result<Credentials, ProxyError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| ProxyError::CredentialsLoad(format!("{path}: {e}")))?;

    let mut users = HashMap::new();
    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (username, hash) = line.split_once(':').ok_or_else(|| {
            ProxyError::CredentialsLoad(format!(
                "line {}: expected 'username:argon2_hash' format",
                line_num + 1
            ))
        })?;
        users.insert(username.to_owned(), hash.to_owned());
    }

    Ok(Credentials { users })
}
