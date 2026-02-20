use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use http::{Request, Response, Uri};
use http_body_util::{Either, Full};
use hyper::body::Incoming;
use hyper_util::client::legacy::Client;

use crate::auth;
use crate::config::{Config, Credentials};
use crate::error::ProxyError;
use crate::metrics::MetricsState;

type ProxyBody = Either<Incoming, Full<Bytes>>;

pub struct AppState {
    pub config: Config,
    pub credentials: Credentials,
    pub skip_auth: bool,
    pub metrics: MetricsState,
    pub upstream_client: Client<hyper_util::client::legacy::connect::HttpConnector, Incoming>,
}

pub async fn handle_request(
    req: Request<Incoming>,
    state: Arc<AppState>,
) -> Result<Response<ProxyBody>, std::convert::Infallible> {
    let start = Instant::now();
    let path = req.uri().path().to_owned();

    match handle_request_inner(req, &state, &path).await {
        Ok((response, username)) => {
            let duration = start.elapsed().as_secs_f64();
            let (service, method) = parse_grpc_path(&path);

            let grpc_status = response
                .headers()
                .get("grpc-status")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("0")
                .to_owned();

            state.metrics.request_duration_seconds.observe(duration);
            state
                .metrics
                .requests_total
                .with_label_values(&[username.as_str(), service, method, &grpc_status])
                .inc();

            Ok(response.map(Either::Left))
        }
        Err(proxy_err) => {
            let (service, method) = parse_grpc_path(&path);

            match &proxy_err {
                ProxyError::AuthMissing | ProxyError::AuthInvalid | ProxyError::AuthDenied(_) => {
                    state
                        .metrics
                        .auth_failures_total
                        .with_label_values(&[proxy_err.auth_failure_reason()])
                        .inc();
                    state
                        .metrics
                        .requests_total
                        .with_label_values(&[
                            "_unauthenticated",
                            service,
                            method,
                            &proxy_err.grpc_status_code().to_string(),
                        ])
                        .inc();
                }
                ProxyError::UpstreamConnect(_) | ProxyError::UpstreamRequest(_) => {
                    state.metrics.upstream_errors_total.inc();
                    state
                        .metrics
                        .requests_total
                        .with_label_values(&[
                            "_error",
                            service,
                            method,
                            &proxy_err.grpc_status_code().to_string(),
                        ])
                        .inc();
                }
                _ => {}
            }

            tracing::warn!("{proxy_err}");
            Ok(proxy_err.to_grpc_response().map(Either::Right))
        }
    }
}

async fn handle_request_inner(
    req: Request<Incoming>,
    state: &AppState,
    path: &str,
) -> Result<(Response<Incoming>, String), ProxyError> {
    let username = if state.skip_auth {
        tracing::debug!(path = %path, "proxying request (auth skipped)");
        "Anonymous".to_owned()
    } else {
        let auth_header = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(ProxyError::AuthMissing)?;

        let username = auth::authenticate(auth_header, &state.credentials)?;
        auth::authorize(&username, path, &state.config)?;

        tracing::debug!(user = %username, path = %path, "proxying request");
        username
    };

    let upstream_uri: Uri = format!("http://{}{}", state.config.upstream_address, path)
        .parse()
        .map_err(|e| ProxyError::UpstreamConnect(format!("invalid upstream URI: {e}")))?;

    let (mut parts, body) = req.into_parts();
    parts.uri = upstream_uri;
    parts.headers.remove("authorization");

    let upstream_req = Request::from_parts(parts, body);

    let response = state
        .upstream_client
        .request(upstream_req)
        .await
        .map_err(|e| ProxyError::UpstreamRequest(e.to_string()))?;

    Ok((response, username))
}

fn parse_grpc_path(path: &str) -> (&str, &str) {
    let trimmed = path.strip_prefix('/').unwrap_or(path);
    match trimmed.rsplit_once('/') {
        Some((service, method)) => (service, method),
        None => (trimmed, "unknown"),
    }
}
