use argon2::Argon2;
use argon2::password_hash::PasswordHash;
use argon2::password_hash::PasswordVerifier;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::config::{Config, Credentials};
use crate::error::ProxyError;

pub fn authenticate(auth_header: &str, credentials: &Credentials) -> Result<String, ProxyError> {
    let encoded = auth_header
        .strip_prefix("Basic ")
        .ok_or(ProxyError::AuthInvalid)?;

    let decoded = STANDARD
        .decode(encoded.trim())
        .map_err(|_| ProxyError::AuthInvalid)?;

    let decoded_str = String::from_utf8(decoded).map_err(|_| ProxyError::AuthInvalid)?;

    let (username, password) = decoded_str.split_once(':').ok_or(ProxyError::AuthInvalid)?;

    let stored_hash = credentials
        .users
        .get(username)
        .ok_or(ProxyError::AuthInvalid)?;

    let parsed_hash = PasswordHash::new(stored_hash).map_err(|_| ProxyError::AuthInvalid)?;

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .map_err(|_| ProxyError::AuthInvalid)?;

    Ok(username.to_owned())
}

pub fn authorize(username: &str, grpc_path: &str, config: &Config) -> Result<(), ProxyError> {
    let user_config = config
        .users
        .get(username)
        .ok_or_else(|| ProxyError::AuthDenied(format!("no config for user '{username}'")))?;

    // Strip leading slash from path: "/package.Service/Method" → "package.Service/Method"
    let call = grpc_path.strip_prefix('/').unwrap_or(grpc_path);

    for allowed in &user_config.allowed_calls {
        if allowed == "*" || allowed == call {
            return Ok(());
        }
    }

    Err(ProxyError::AuthDenied(format!(
        "user '{username}' not allowed to call '{call}'"
    )))
}
