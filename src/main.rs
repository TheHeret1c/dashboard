use axum::{routing::get, Router};

async fn health() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/health", get(health));

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
