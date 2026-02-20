use bytes::Bytes;
use http::Response;
use http_body_util::Full;

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("failed to load config: {0}")]
    ConfigLoad(String),

    #[error("failed to load credentials: {0}")]
    CredentialsLoad(String),

    #[error("missing authorization header")]
    AuthMissing,

    #[error("invalid credentials")]
    AuthInvalid,

    #[error("call not permitted: {0}")]
    AuthDenied(String),

    #[error("upstream connection failed: {0}")]
    UpstreamConnect(String),

    #[error("upstream request failed: {0}")]
    UpstreamRequest(String),

    #[error("failed to bind server: {0}")]
    ServerBind(String),
}

impl ProxyError {
    pub fn grpc_status_code(&self) -> u8 {
        match self {
            Self::AuthMissing | Self::AuthInvalid => 16, // UNAUTHENTICATED
            Self::AuthDenied(_) => 7,                    // PERMISSION_DENIED
            Self::UpstreamConnect(_) => 14,              // UNAVAILABLE
            Self::UpstreamRequest(_)
            | Self::ConfigLoad(_)
            | Self::CredentialsLoad(_)
            | Self::ServerBind(_) => 13, // INTERNAL
        }
    }

    pub fn auth_failure_reason(&self) -> &'static str {
        match self {
            Self::AuthMissing => "missing",
            Self::AuthInvalid => "invalid",
            Self::AuthDenied(_) => "denied",
            _ => "unknown",
        }
    }

    pub fn to_grpc_response(&self) -> Response<Full<Bytes>> {
        let status_code = self.grpc_status_code();
        let message = self.to_string();

        Response::builder()
            .status(200)
            .header("content-type", "application/grpc")
            .header("grpc-status", status_code.to_string())
            .header("grpc-message", percent_encode(&message))
            .body(Full::new(Bytes::new()))
            .unwrap_or_else(|_| Response::new(Full::new(Bytes::new())))
    }
}

fn percent_encode(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push_str("%20"),
            _ => {
                encoded.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    encoded
}
