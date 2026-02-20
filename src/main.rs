mod auth;
mod config;
mod error;
mod metrics;
mod proxy;

use std::sync::Arc;

use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tracing_subscriber::EnvFilter;

use crate::error::ProxyError;
use crate::metrics::MetricsState;
use crate::proxy::AppState;

#[tokio::main]
async fn main() -> Result<(), ProxyError> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let skip_auth = std::env::var("NO_AUTH").is_ok_and(|v| v == "1" || v == "true");

    let config_path = std::env::var("CONFIG_PATH")
        .map_err(|_| ProxyError::ConfigLoad("CONFIG_PATH env var not set".to_owned()))?;

    let config = config::load_config(&config_path)?;

    let credentials = if skip_auth {
        config::Credentials::empty()
    } else {
        let credentials_path = std::env::var("CREDENTIALS_FILE").map_err(|_| {
            ProxyError::CredentialsLoad("CREDENTIALS_FILE env var not set".to_owned())
        })?;
        config::load_credentials(&credentials_path)?
    };

    if skip_auth {
        tracing::warn!(
            listen = %config.listen_address,
            upstream = %config.upstream_address,
            metrics = %config.metrics_address,
            "starting grpc-proxier with authentication DISABLED"
        );
    } else {
        tracing::info!(
            listen = %config.listen_address,
            upstream = %config.upstream_address,
            metrics = %config.metrics_address,
            users = config.users.len(),
            "starting grpc-proxier"
        );
    }

    let metrics = MetricsState::new()?;
    let metrics_registry = Arc::new(metrics.registry.clone());
    let metrics_addr = config.metrics_address;

    let upstream_client: Client<_, Incoming> = Client::builder(TokioExecutor::new())
        .http2_only(true)
        .build_http();

    let state = Arc::new(AppState {
        config,
        credentials,
        skip_auth,
        metrics,
        upstream_client,
    });

    tokio::spawn(crate::metrics::serve_metrics(
        metrics_registry,
        metrics_addr,
    ));

    let listener = tokio::net::TcpListener::bind(state.config.listen_address)
        .await
        .map_err(|e| ProxyError::ServerBind(format!("{}: {e}", state.config.listen_address)))?;

    tracing::info!("proxy server listening on {}", state.config.listen_address);

    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                tracing::warn!("accept error: {e}");
                continue;
            }
        };

        let state = Arc::clone(&state);
        state.metrics.active_connections.inc();

        tokio::spawn(async move {
            tracing::debug!(%peer_addr, "new connection");

            let conn_state = Arc::clone(&state);
            let service = service_fn(move |req| {
                let state = Arc::clone(&conn_state);
                proxy::handle_request(req, state)
            });

            let result = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                .serve_connection(TokioIo::new(stream), service)
                .await;

            state.metrics.active_connections.dec();

            if let Err(e) = result {
                tracing::debug!(%peer_addr, "connection closed: {e}");
            }
        });
    }
}
