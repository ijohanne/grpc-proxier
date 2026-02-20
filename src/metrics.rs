use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http::Response;
use http_body_util::Full;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use prometheus::{
    Encoder, Gauge, Histogram, HistogramOpts, IntCounter, IntCounterVec, Opts, Registry,
    TextEncoder,
};

use crate::error::ProxyError;

pub struct MetricsState {
    pub registry: Registry,
    pub requests_total: IntCounterVec,
    pub request_duration_seconds: Histogram,
    pub auth_failures_total: IntCounterVec,
    pub upstream_errors_total: IntCounter,
    pub active_connections: Gauge,
}

impl MetricsState {
    pub fn new() -> Result<Self, ProxyError> {
        let registry = Registry::new_custom(Some("grpc_proxier".to_owned()), None)
            .map_err(|e| ProxyError::ConfigLoad(format!("metrics registry: {e}")))?;

        let requests_total = IntCounterVec::new(
            Opts::new("requests_total", "Total proxied requests"),
            &["user", "grpc_service", "grpc_method", "grpc_status"],
        )
        .map_err(|e| ProxyError::ConfigLoad(format!("requests_total metric: {e}")))?;

        let request_duration_seconds = Histogram::with_opts(
            HistogramOpts::new(
                "request_duration_seconds",
                "Request latency including upstream time",
            )
            .buckets(vec![
                0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
            ]),
        )
        .map_err(|e| ProxyError::ConfigLoad(format!("request_duration metric: {e}")))?;

        let auth_failures_total = IntCounterVec::new(
            Opts::new(
                "auth_failures_total",
                "Authentication/authorization failures",
            ),
            &["reason"],
        )
        .map_err(|e| ProxyError::ConfigLoad(format!("auth_failures metric: {e}")))?;

        let upstream_errors_total = IntCounter::with_opts(Opts::new(
            "upstream_errors_total",
            "Upstream connection/request errors",
        ))
        .map_err(|e| ProxyError::ConfigLoad(format!("upstream_errors metric: {e}")))?;

        let active_connections = Gauge::with_opts(Opts::new(
            "active_connections",
            "Currently active gRPC connections",
        ))
        .map_err(|e| ProxyError::ConfigLoad(format!("active_connections metric: {e}")))?;

        registry
            .register(Box::new(requests_total.clone()))
            .map_err(|e| ProxyError::ConfigLoad(format!("register requests_total: {e}")))?;
        registry
            .register(Box::new(request_duration_seconds.clone()))
            .map_err(|e| {
                ProxyError::ConfigLoad(format!("register request_duration_seconds: {e}"))
            })?;
        registry
            .register(Box::new(auth_failures_total.clone()))
            .map_err(|e| ProxyError::ConfigLoad(format!("register auth_failures_total: {e}")))?;
        registry
            .register(Box::new(upstream_errors_total.clone()))
            .map_err(|e| ProxyError::ConfigLoad(format!("register upstream_errors_total: {e}")))?;
        registry
            .register(Box::new(active_connections.clone()))
            .map_err(|e| ProxyError::ConfigLoad(format!("register active_connections: {e}")))?;

        Ok(Self {
            registry,
            requests_total,
            request_duration_seconds,
            auth_failures_total,
            upstream_errors_total,
            active_connections,
        })
    }
}

pub async fn serve_metrics(registry: Arc<Registry>, addr: SocketAddr) -> Result<(), ProxyError> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| ProxyError::ServerBind(format!("metrics {addr}: {e}")))?;

    tracing::info!("metrics server listening on {addr}");

    loop {
        let (stream, _) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                tracing::warn!("metrics accept error: {e}");
                continue;
            }
        };

        let registry = Arc::clone(&registry);
        tokio::spawn(async move {
            let service = service_fn(move |_req| {
                let registry = Arc::clone(&registry);
                async move {
                    let encoder = TextEncoder::new();
                    let metric_families = registry.gather();
                    let mut buffer = Vec::new();
                    if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
                        return Ok::<_, hyper::Error>(
                            Response::builder()
                                .status(500)
                                .body(Full::new(Bytes::from(format!("encoding error: {e}"))))
                                .unwrap_or_else(|_| {
                                    Response::new(Full::new(Bytes::from("internal error")))
                                }),
                        );
                    }
                    Ok(Response::builder()
                        .status(200)
                        .header("content-type", encoder.format_type())
                        .body(Full::new(Bytes::from(buffer)))
                        .unwrap_or_else(|_| {
                            Response::new(Full::new(Bytes::from("internal error")))
                        }))
                }
            });

            if let Err(e) = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await
            {
                tracing::debug!("metrics connection error: {e}");
            }
        });
    }
}
