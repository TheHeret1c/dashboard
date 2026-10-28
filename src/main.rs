use axum::{routing::get, Router, Json};
use serde::Serialize;
use sysinfo::{Disk, Disks, System};

#[derive(Serialize)]
struct DiskInfo {
    mount: String,
    total_bytes: u64,
    available_bytes: u64,
}

#[derive(Serialize)]
struct Metrics {
    mem_total_bytes: u64,
    mem_used_bytes: u64,
    disks: Vec<DiskInfo>,
}

async fn health() -> &'static str {
    "ok response"
}

async fn metrics() -> Json<Metrics> {
    let mut sys = System::new();
    sys.refresh_all();

    let disks = Disks::new_with_refreshed_list();
    let disks_info: Vec<DiskInfo> = disks
        .iter()
        .map(|d| DiskInfo {
            mount: d.mount_point().to_string_lossy().to_string(),
            total_bytes: d.total_space(),
            available_bytes: d.available_space(),
        })
        .collect();

    Json(Metrics {
        mem_total_bytes: sys.total_memory(),
        mem_used_bytes: sys.used_memory(),
        disks: disks_info,
    })
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/metrics", get(metrics));

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
