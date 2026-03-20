use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};

const BEARER_PREFIX: &str = "Bearer ";

#[derive(Clone)]
pub struct ApiKey(pub String);

impl ApiKey {
    pub fn resolve(cli_arg: Option<&str>) -> Self {
        if let Some(key) = cli_arg {
            return Self(key.to_owned());
        }
        if let Ok(key) = std::env::var("MCP_API_KEY")
            && !key.is_empty()
        {
            return Self(key);
        }
        Self(generate_key())
    }
}

fn generate_key() -> String {
    use rand::Rng;
    let bytes: [u8; 32] = rand::rng().random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

pub async fn require_api_key(
    State(api_key): State<ApiKey>,
    request: Request,
    next: Next,
) -> Response {
    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(value) if value.starts_with(BEARER_PREFIX) => {
            let token = &value[BEARER_PREFIX.len()..];
            if constant_time_eq(token.as_bytes(), api_key.0.as_bytes()) {
                return next.run(request).await;
            }
            tracing::warn!("Rejected: invalid API key");
            Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body("Invalid API key".into())
                .unwrap()
        }
        _ => {
            tracing::warn!("Rejected: missing Authorization header");
            Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body("Missing Authorization header. Use: Authorization: Bearer <key>".into())
                .unwrap()
        }
    }
}
