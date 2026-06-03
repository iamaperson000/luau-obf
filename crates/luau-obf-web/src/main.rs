use axum::{
    extract::Json,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use luau_obf::{obfuscate, Options};
use serde::{Deserialize, Serialize};

const INDEX_HTML: &str = include_str!("index.html");

#[derive(Deserialize)]
struct ObfuscateRequest {
    source: String,
    seed_hex: Option<String>,
}

#[derive(Serialize)]
struct ObfuscateResponse {
    ok: bool,
    output: Option<String>,
    seed_used: Option<String>,
    error: Option<String>,
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn handle_obfuscate(Json(req): Json<ObfuscateRequest>) -> impl IntoResponse {
    let seed = match parse_seed(req.seed_hex.as_deref()) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ObfuscateResponse {
                    ok: false,
                    output: None,
                    seed_used: None,
                    error: Some(e),
                }),
            );
        }
    };

    let opts = Options { seed, env_binding: None };
    match obfuscate(&req.source, opts) {
        Ok(result) => (
            StatusCode::OK,
            Json(ObfuscateResponse {
                ok: true,
                output: Some(result.output),
                seed_used: Some(hex_of(&result.seed_used)),
                error: None,
            }),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(ObfuscateResponse {
                ok: false,
                output: None,
                seed_used: None,
                error: Some(e.to_string()),
            }),
        ),
    }
}

fn parse_seed(hex: Option<&str>) -> Result<Option<[u8; 32]>, String> {
    let Some(h) = hex else { return Ok(None) };
    let h = h.trim();
    if h.is_empty() {
        return Ok(None);
    }
    if h.len() != 64 {
        return Err(format!("seed must be 64 hex chars (32 bytes); got {}", h.len()));
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        let byte =
            u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).map_err(|_| "invalid hex in seed".to_string())?;
        out[i] = byte;
    }
    Ok(Some(out))
}

fn hex_of(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(index))
        .route("/api/obfuscate", post(handle_obfuscate));

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3000);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("bind");
    let local = listener.local_addr().expect("local_addr");
    println!("luau-obf-web listening on http://localhost:{}", local.port());
    axum::serve(listener, app).await.expect("serve");
}
