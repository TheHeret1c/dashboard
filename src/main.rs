use axum::{extract::State, routing::get, Json, Router, extract::Query};
use serde::{Deserialize, Serialize};
use sysinfo::{Disks, System};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use axum::response::Html;
use rusqlite::Connection;
use std::time::{SystemTime, UNIX_EPOCH};
use rusqlite::params;


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

#[derive(Deserialize)]
struct HistoryQuery {
    minutes: Option<i64>,
}

#[derive(Serialize)]
struct HistoryPoint {
    ts: i64,
    cpu: f64,
    mem_used: f64,
    mem_total: f64,
}

struct AppState {
    latest: RwLock<Metrics>,
    db: Mutex<rusqlite::Connection>,
}
type Shared = Arc<AppState>;

async fn health() -> &'static str {
    "ok response"
}

async fn metrics(State(state): State<Shared>) -> Json<Metrics> {
    let m = state.latest.read().unwrap().clone();
    Json(m)
}

async fn history(State(state): State<Shared>, Query(q): Query<HistoryQuery>) -> Json<Vec<HistoryPoint>> {
    let minutes = q.minutes.unwrap_or(60).clamp(1, 7 * 24 * 60);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let since = now - minutes * 60;
    let bucket = std::cmp::max(3, minutes * 60 / 300);

    let conn = state.db.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT (ts / ?2) * ?2 AS bucket, AVG(cpu), AVG(mem_used), AVG(mem_total)
                 FROM metrics
                 WHERE ts >= ?1
                 GROUP BY bucket
                 ORDER BY bucket"
        ).unwrap();

    let points = stmt
        .query_map(params![since, bucket], |row| {
            Ok(HistoryPoint {
                ts: row.get(0)?,
                cpu: row.get(1)?,
                mem_used: row.get(2)?,
                mem_total: row.get(3)?,
            })
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    Json(points)
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

#[tokio::main]
async fn main() {
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "dashboard.db".to_string());
    let conn = Connection::open(&db_path).unwrap();

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS metrics (
            ts INTEGER NOT NULL,
            cpu REAL NOT NULL,
            mem_used INTEGER NOT NULL,
            mem_total INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_metrics_ts ON metrics(ts);",
    ).unwrap();

    let shared = Arc::new(AppState {
        latest: RwLock::new(Metrics::default()),
        db: Mutex::new(conn),
    });

    let for_task = shared.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(3));
        let mut sys = System::new();

        let mut first = true;
        loop {
            ticker.tick().await;
            sys.refresh_memory();
            sys.refresh_cpu_usage();

            let cpu = sys.global_cpu_usage();
            let mem_used = sys.used_memory();
            let mem_total = sys.total_memory();

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
                mem_total_bytes: mem_total,
                mem_used_bytes: mem_used,
                cpu_percent: cpu,
                disks: disks_info,
            };

            *for_task.latest.write().unwrap() = fresh;

            if first {
                first = false;
            } else {
                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;

                let conn = for_task.db.lock().unwrap();
                conn.execute(
                    "INSERT INTO metrics (ts, cpu, mem_used, mem_total) VALUES (?1, ?2, ?3, ?4)",
                    params![ts, cpu as f64, mem_used as i64, mem_total as i64],
                ).unwrap();
            }
        }
    });

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/metrics", get(metrics))
        .route("/", get(index))
        .route("/api/history", get(history))
        .with_state(shared.clone());

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
