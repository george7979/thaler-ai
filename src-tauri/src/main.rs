#![windows_subsystem = "windows"]

mod anonymizer;

use anonymizer::Anonymizer;
use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use tokio::sync::Mutex;

struct AppState {
    anonymizer: Mutex<Anonymizer>,
    last_heartbeat: Mutex<std::time::Instant>,
    logs: Arc<std::sync::Mutex<Vec<String>>>,
}

impl AppState {
    fn log(&self, msg: &str) {
        let ts = chrono::Local::now().format("%H:%M:%S").to_string();
        let entry = format!("[{}] {}", ts, msg);
        eprintln!("{}", entry);
        if let Ok(mut logs) = self.logs.lock() {
            logs.push(entry);
        }
    }
}

#[tokio::main]
async fn main() {
    let state = Arc::new(AppState {
        anonymizer: Mutex::new(Anonymizer::new()),
        last_heartbeat: Mutex::new(std::time::Instant::now()),
        logs: Arc::new(std::sync::Mutex::new(Vec::new())),
    });

    let app = Router::new()
        // Static frontend
        .route("/", get(serve_index))
        .route("/main.js", get(serve_js))
        .route("/style.css", get(serve_css))
        // API
        .route("/api/check-ollama", get(check_ollama))
        .route("/api/list-models", get(list_models))
        .route("/api/get-config", get(get_config))
        .route("/api/set-config", post(set_config))
        .route("/api/load-file", post(load_file))
        .route("/api/anonymize", post(anonymize_text))
        .route("/api/get-mapping", get(get_mapping))
        .route("/api/export-map", get(export_map))
        .route("/api/export-anon-native", get(export_anon_native))
        .route("/api/deanonymize", post(deanonymize))
        .route("/api/deanonymize-docx", post(deanonymize_docx))
        .route("/api/logs", get(get_logs))
        .route("/api/heartbeat", post(heartbeat))
        .route("/api/shutdown", post(shutdown))
        .with_state(state.clone())
        .layer(axum::extract::DefaultBodyLimit::max(50 * 1024 * 1024)); // 50 MB upload limit

    // Find available port starting from 3000
    let mut port = 3000u16;
    let listener = loop {
        let addr = format!("127.0.0.1:{}", port);
        match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => break l,
            Err(_) => {
                eprintln!("Port {} zajęty, próbuję {}...", port, port + 1);
                port += 1;
                if port > 3100 {
                    eprintln!("Brak wolnego portu w zakresie 3000-3100");
                    std::process::exit(1);
                }
            }
        }
    };
    println!("Thaler AI → http://127.0.0.1:{}", port);

    // Open browser after server starts listening
    let url = format!("http://127.0.0.1:{}", port);
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        // Try Windows browser first (WSL2), then fall back to open::that
        let wsl_open = std::process::Command::new("cmd.exe")
            .args(["/C", "start", &url])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if wsl_open.is_err() {
            let _ = open::that(&url);
        }
    });

    // Watchdog: shut down if no heartbeat for 120s
    // Browser throttles setInterval in background tabs (~1/min in Chrome),
    // so 120s gives enough margin. visibilitychange on client side sends
    // immediate heartbeat when user returns to tab.
    let watchdog_state = state.clone();
    tokio::spawn(async move {
        // Give browser 15s to load before checking
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            let last = *watchdog_state.last_heartbeat.lock().await;
            if last.elapsed() > std::time::Duration::from_secs(120) {
                eprintln!("Brak heartbeat od 120s — zamykam serwer.");
                std::process::exit(0);
            }
        }
    });

    axum::serve(listener, app).await.unwrap();
}

// --- Static files (embedded) ---

async fn serve_index() -> Html<String> {
    let html = include_str!("../../src/index.html")
        .replace("{{VERSION}}", env!("CARGO_PKG_VERSION"));
    Html(html)
}

async fn serve_js() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "application/javascript; charset=utf-8")],
        include_str!("../../src/main.js"),
    )
}

async fn serve_css() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "text/css; charset=utf-8")],
        include_str!("../../src/style.css"),
    )
}

// --- API handlers ---

async fn check_ollama(State(state): State<Arc<AppState>>) -> Result<Json<String>, (StatusCode, String)> {
    let anon = state.anonymizer.lock().await;
    anon.check_connection().await
        .map(Json)
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, e))
}

async fn list_models(State(state): State<Arc<AppState>>) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    let anon = state.anonymizer.lock().await;
    anon.list_models().await
        .map(Json)
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, e))
}

async fn get_config(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let anon = state.anonymizer.lock().await;
    Json(serde_json::json!({
        "url": anon.get_ollama_url(),
        "model": anon.get_primary_model()
    }))
}

#[derive(serde::Deserialize)]
struct ConfigPayload {
    url: String,
    model: String,
}

async fn set_config(State(state): State<Arc<AppState>>, Json(payload): Json<ConfigPayload>) -> Json<&'static str> {
    let mut anon = state.anonymizer.lock().await;
    anon.set_config(payload.url, payload.model);
    Json("ok")
}

async fn load_file(State(state): State<Arc<AppState>>, mut multipart: Multipart) -> Result<Json<String>, (StatusCode, String)> {
    while let Ok(Some(field)) = multipart.next_field().await {
        let raw_name = field.file_name().unwrap_or("file").to_string();
        let data: bytes::Bytes = field.bytes().await.map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

        // Sanitize filename — extract only basename, prevent path traversal
        let safe_name = std::path::Path::new(&raw_name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("upload");
        let ext = safe_name.rsplit('.').next().unwrap_or("").to_lowercase();
        let text = match ext.as_str() {
            "xlsx" | "xls" => {
                let tmp = std::env::temp_dir().join(format!("thaler_{}", safe_name));
                std::fs::write(&tmp, &data).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                let result = anonymizer::read_file(tmp.to_str().unwrap());
                let _ = std::fs::remove_file(&tmp);
                // Store original bytes for native export
                let mut anon = state.anonymizer.lock().await;
                anon.store_original_file(data.to_vec(), &ext);
                drop(anon);
                result.map_err(|e| (StatusCode::BAD_REQUEST, e))?
            }
            "docx" => {
                let tmp = std::env::temp_dir().join(format!("thaler_{}", safe_name));
                std::fs::write(&tmp, &data).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
                let result = anonymizer::read_file(tmp.to_str().unwrap());
                let _ = std::fs::remove_file(&tmp);
                // Store original bytes for native export
                let mut anon = state.anonymizer.lock().await;
                anon.store_original_file(data.to_vec(), &ext);
                drop(anon);
                result.map_err(|e| (StatusCode::BAD_REQUEST, e))?
            }
            _ => {
                // Text files — clear any stored binary
                let mut anon = state.anonymizer.lock().await;
                anon.store_original_file(Vec::new(), &ext);
                drop(anon);
                String::from_utf8(data.to_vec())
                    .map_err(|e| (StatusCode::BAD_REQUEST, format!("Nie mogę odczytać tekstu: {}", e)))?
            }
        };

        return Ok(Json(text));
    }
    Err((StatusCode::BAD_REQUEST, "Brak pliku".to_string()))
}

#[derive(serde::Deserialize)]
struct AnonPayload {
    text: String,
    source_file: String,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    randomize_amounts: bool,
}

async fn anonymize_text(State(state): State<Arc<AppState>>, Json(payload): Json<AnonPayload>) -> Result<Json<anonymizer::AnonymizeResult>, (StatusCode, String)> {
    state.log(&format!("Anonimizacja: {} ({} znaków)", payload.source_file, payload.text.len()));
    let mut anon = state.anonymizer.lock().await;
    anon.set_log_sink(state.logs.clone());
    let cats = if payload.categories.is_empty() { None } else { Some(payload.categories) };
    let result = anon.anonymize(&payload.text, &payload.source_file, cats).await;
    anon.clear_log_sink();
    match result {
        Ok(mut r) => {
            // XLSX numeric cells: assign random numbers or deterministic tokens
            if anon.has_original_file() {
                let ext = anon.get_original_ext().to_string();
                if ext == "xlsx" || ext == "xls" {
                    if payload.randomize_amounts {
                        match anon.prepare_random_amounts() {
                            Ok(count) => {
                                state.log(&format!("Losowe kwoty: {} unikalnych wartości liczbowych", count));
                                r.entities_found += count;
                            }
                            Err(e) => {
                                state.log(&format!("BŁĄD losowych kwot: {}", e));
                            }
                        }
                    } else {
                        match anon.prepare_token_amounts() {
                            Ok(count) => {
                                state.log(&format!("Tokeny kwot: {} unikalnych wartości liczbowych → [TH_KWOTA_*]", count));
                                r.entities_found += count;
                            }
                            Err(e) => {
                                state.log(&format!("BŁĄD tokenów kwot: {}", e));
                            }
                        }
                    }
                }
            }
            state.log(&format!("Zakończono: {} encji, model: {}", r.entities_found, r.model_used));
            Ok(Json(r))
        }
        Err(e) => {
            state.log(&format!("BŁĄD: {}", e));
            Err((StatusCode::INTERNAL_SERVER_ERROR, e))
        }
    }
}

async fn get_mapping(State(state): State<Arc<AppState>>) -> Json<Vec<anonymizer::EntityInfo>> {
    let anon = state.anonymizer.lock().await;
    Json(anon.get_mapping())
}

async fn export_map(State(state): State<Arc<AppState>>) -> Result<impl IntoResponse, (StatusCode, String)> {
    let anon = state.anonymizer.lock().await;
    anon.export_map()
        .map(|json_str| (
            StatusCode::OK,
            [("content-type", "application/json; charset=utf-8")],
            json_str,
        ))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

#[derive(serde::Deserialize)]
struct ExportNativeQuery {
    #[serde(default)]
    randomize_amounts: Option<String>,
}

async fn export_anon_native(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(query): axum::extract::Query<ExportNativeQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut anon = state.anonymizer.lock().await;
    let ext = anon.get_original_ext().to_string();
    let randomize = query.randomize_amounts.as_deref() == Some("1");

    match ext.as_str() {
        "docx" => {
            let bytes = anon.export_anon_docx()
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            Ok((
                StatusCode::OK,
                [
                    ("content-type", "application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
                    ("content-disposition", "attachment; filename=\"anonymized.docx\""),
                ],
                bytes,
            ))
        }
        "xlsx" | "xls" => {
            let bytes = anon.export_anon_xlsx(randomize)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            Ok((
                StatusCode::OK,
                [
                    ("content-type", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
                    ("content-disposition", "attachment; filename=\"anonymized.xlsx\""),
                ],
                bytes,
            ))
        }
        _ => Err((StatusCode::BAD_REQUEST, format!("Natywny eksport nie obsługuje .{}", ext))),
    }
}

#[derive(serde::Deserialize)]
struct DeanonPayload {
    anon_text: String,
    map_json: String,
}

async fn deanonymize(Json(payload): Json<DeanonPayload>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let (text, stats) = Anonymizer::deanonymize_with_map(&payload.anon_text, &payload.map_json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(Json(serde_json::json!({
        "text": text,
        "stats": {
            "total": stats.total,
            "found": stats.found,
            "missing": stats.missing
        }
    })))
}

async fn deanonymize_docx(mut multipart: Multipart) -> Result<axum::response::Response, (StatusCode, String)> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut file_name = String::new();
    let mut map_json: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                file_name = field.file_name().unwrap_or("file").to_string();
                let data = field.bytes().await.map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                file_bytes = Some(data.to_vec());
            }
            "map_json" => {
                let data = field.text().await.map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
                map_json = Some(data);
            }
            _ => {}
        }
    }

    let bytes = file_bytes.ok_or((StatusCode::BAD_REQUEST, "Brak pliku".to_string()))?;
    let map = map_json.ok_or((StatusCode::BAD_REQUEST, "Brak mapy JSON".to_string()))?;

    let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();

    fn build_deanon_response(result: Vec<u8>, stats: anonymizer::DeanonStats, content_type: &str, filename: &str) -> axum::response::Response {
        use axum::body::Body;
        let stats_json = serde_json::to_string(&serde_json::json!({
            "total": stats.total, "found": stats.found, "missing": stats.missing
        })).unwrap_or_default();

        axum::response::Response::builder()
            .status(200)
            .header("content-type", content_type)
            .header("content-disposition", format!("attachment; filename=\"{}\"", filename))
            .header("x-deanon-stats", stats_json)
            .body(Body::from(result))
            .unwrap()
    }

    match ext.as_str() {
        "docx" => {
            let (result, stats) = Anonymizer::deanonymize_docx(&bytes, &map)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            Ok(build_deanon_response(result, stats,
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "restored.docx"))
        }
        "xlsx" | "xls" => {
            let (result, stats) = Anonymizer::deanonymize_xlsx(&bytes, &map)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
            Ok(build_deanon_response(result, stats,
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                "restored.xlsx"))
        }
        _ => Err((StatusCode::BAD_REQUEST, format!("Nieobsługiwany format: .{}", ext))),
    }
}

async fn get_logs(State(state): State<Arc<AppState>>) -> Json<Vec<String>> {
    let mut logs = state.logs.lock().unwrap();
    let result = logs.clone();
    logs.clear();
    Json(result)
}

async fn heartbeat(State(state): State<Arc<AppState>>) -> StatusCode {
    *state.last_heartbeat.lock().await = std::time::Instant::now();
    StatusCode::OK
}

async fn shutdown() -> &'static str {
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        std::process::exit(0);
    });
    "bye"
}
