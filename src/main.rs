use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;
use sysinfo::{Disks, System};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use axum::response::Html;


#[derive(Serialize, Clone)]
struct DiskInfo {
    mount: String,
    total_bytes: u64,
    available_bytes: u64,
}

#[derive(Serialize, Clone, Default)]
struct Metrics {
    mem_total_bytes: u64,
    mem_used_bytes: u64,
    cpu_percent: f32,
    disks: Vec<DiskInfo>,
}

type Shared = Arc<RwLock<Metrics>>;

async fn health() -> &'static str {
    "ok response"
}

async fn metrics(State(state): State<Shared>) -> Json<Metrics> {
    let m = state.read().unwrap().clone();
    Json(m)
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

#[tokio::main]
async fn main() {
    let shared = Arc::new(RwLock::new(Metrics::default()));

    let for_task = shared.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(3));
        let mut sys = System::new();
        loop {
            ticker.tick().await;
            sys.refresh_memory();
            sys.refresh_cpu_usage();

            let disks = Disks::new_with_refreshed_list();
            let disks_info: Vec<DiskInfo> = disks
                .iter()
                .map(|d| DiskInfo {
                    mount: d.mount_point().to_string_lossy().to_string(),
                    total_bytes: d.total_space(),
                    available_bytes: d.available_space(),
                })
                .collect();

            let fresh = Metrics {
                mem_total_bytes: sys.total_memory(),
                mem_used_bytes: sys.used_memory(),
                cpu_percent: sys.global_cpu_usage(),
                disks: disks_info,
            };

            *for_task.write().unwrap() = fresh;
        }
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/metrics", get(metrics))
        .route("/", get(index))
        .with_state(shared.clone());

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
