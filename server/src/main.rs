mod store;

use axum::{
    body::Bytes,
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, Path as AxPath, Query, State},
    http::{header, HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Json, Response},
    routing::{get, post, delete},
    Router,
};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use store::{Event, SqliteStore, Store};
use tower_http::cors::CorsLayer;

#[derive(RustEmbed)]
#[folder = "../dashboard-sveltekit/build/"]
struct Dashboard;

// ---- Config ----

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Config {
    admin_token: String,
}

struct AppState {
    config: tokio::sync::RwLock<Config>,
    store: Arc<dyn Store>,
    data_dir: PathBuf,
}

fn config_path(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("config.json")
}

fn load_config(data_dir: &PathBuf) -> Config {
    let p = config_path(data_dir);
    fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_config(data_dir: &PathBuf, cfg: &Config) -> std::io::Result<()> {
    let p = config_path(data_dir);
    let tmp = p.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(cfg).unwrap())?;
    fs::rename(tmp, p)
}

// ---- Visitor hash (개인정보 보호) ----
// IP + UA + 일자 + 사이트 → SHA-256
// 일자 단위로 회전 → 같은 사람도 다음날엔 다른 해시 (장기 추적 X)

fn visitor_hash(ip: &str, ua: &str, site: &str) -> String {
    let day = chrono::Utc::now().format("%Y%m%d").to_string();
    let mut h = Sha256::new();
    h.update(ip.as_bytes());
    h.update(b"|");
    h.update(ua.as_bytes());
    h.update(b"|");
    h.update(site.as_bytes());
    h.update(b"|");
    h.update(day.as_bytes());
    hex::encode(&h.finalize()[..16])
}

fn classify_device(ua: &str) -> &'static str {
    let l = ua.to_lowercase();
    if l.contains("ipad") || l.contains("tablet") {
        "tablet"
    } else if l.contains("mobile") || l.contains("android") || l.contains("iphone") {
        "mobile"
    } else {
        "desktop"
    }
}

fn classify_browser(ua: &str) -> &'static str {
    let l = ua.to_lowercase();
    if l.contains("edg/") {
        "Edge"
    } else if l.contains("chrome/") && !l.contains("edg/") {
        "Chrome"
    } else if l.contains("firefox") {
        "Firefox"
    } else if l.contains("safari") && !l.contains("chrome") {
        "Safari"
    } else {
        "Other"
    }
}

fn client_ip(headers: &HeaderMap, fallback: SocketAddr) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| fallback.ip().to_string())
}

// ---- Errors ----

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("db: {}", e))
    }
}

// ---- Track endpoint ----

#[derive(Debug, Deserialize)]
struct TrackBody {
    site: String,
    #[serde(default = "default_kind")]
    kind: String,
    path: String,
    #[serde(default)]
    referrer: Option<String>,
}

fn default_kind() -> String {
    "pageview".to_string()
}

async fn track(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    raw: Bytes,
) -> Result<StatusCode, ApiError> {
    // sendBeacon은 text/plain 으로 보낸다 (CORS preflight 회피).
    // Content-Type 가리지 말고 본문만 JSON 파싱한다.
    let body: TrackBody = serde_json::from_slice(&raw)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("invalid body: {e}")))?;
    // 사이트 자동 생성 (없으면) — self-hosted 편의성
    state
        .store
        .ensure_site(&body.site, &body.site)
        .await?;

    let ip = client_ip(&headers, addr);
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let ev = Event {
        site: body.site.clone(),
        kind: body.kind,
        path: body.path,
        referrer: body.referrer.filter(|s| !s.is_empty()),
        visitor_hash: visitor_hash(&ip, ua, &body.site),
        country: None, // TODO: GeoIP (선택 기능)
        device: Some(classify_device(ua).to_string()),
        browser: Some(classify_browser(ua).to_string()),
        ts_ms: chrono::Utc::now().timestamp_millis(),
    };

    state.store.record_event(&ev).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Health ----

async fn health() -> &'static str {
    "ok"
}

// ---- Setup (첫 실행) ----

#[derive(Debug, Deserialize)]
struct SetupBody {
    admin_token: String,
}

async fn setup(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SetupBody>,
) -> Result<StatusCode, ApiError> {
    let mut cfg = state.config.write().await;
    if !cfg.admin_token.is_empty() {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "이미 설정됨".to_string(),
        ));
    }
    cfg.admin_token = body.admin_token;
    save_config(&state.data_dir, &cfg)
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("save: {}", e)))?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Admin auth ----

async fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let cfg = state.config.read().await;
    if cfg.admin_token.is_empty() {
        return Err(ApiError(StatusCode::SERVICE_UNAVAILABLE, "초기 설정 필요".to_string()));
    }
    let auth = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = auth.strip_prefix("Bearer ").unwrap_or("");
    if token != cfg.admin_token {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "잘못된 토큰".to_string()));
    }
    Ok(())
}

// ---- Admin: sites ----

async fn list_sites(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<store::Site>>, ApiError> {
    require_admin(&state, &headers).await?;
    Ok(Json(state.store.list_sites().await?))
}

#[derive(Debug, Deserialize)]
struct CreateSite {
    domain: String,
    #[serde(default)]
    name: Option<String>,
}

async fn create_site(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<CreateSite>,
) -> Result<StatusCode, ApiError> {
    require_admin(&state, &headers).await?;
    let name = body.name.unwrap_or_else(|| body.domain.clone());
    state.store.ensure_site(&body.domain, &name).await?;
    Ok(StatusCode::CREATED)
}

async fn delete_site(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    AxPath(domain): AxPath<String>,
) -> Result<StatusCode, ApiError> {
    require_admin(&state, &headers).await?;
    state.store.delete_site(&domain).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Admin: stats ----

#[derive(Debug, Deserialize)]
struct StatsQuery {
    site: String,
    #[serde(default)]
    from: Option<i64>,
    #[serde(default)]
    to: Option<i64>,
}

async fn stats(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<StatsQuery>,
) -> Result<Json<store::Stats>, ApiError> {
    require_admin(&state, &headers).await?;
    let now = chrono::Utc::now().timestamp_millis();
    let from = q.from.unwrap_or(now - 24 * 3600 * 1000); // 기본: 최근 24h
    let to = q.to.unwrap_or(now);
    Ok(Json(state.store.stats(&q.site, from, to).await?))
}

#[derive(Debug, Deserialize)]
struct LiveQuery {
    site: String,
}

#[derive(Debug, Serialize)]
struct LiveResp {
    active: i64,
}

async fn live(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<LiveQuery>,
) -> Result<Json<LiveResp>, ApiError> {
    require_admin(&state, &headers).await?;
    let since = chrono::Utc::now().timestamp_millis() - 5 * 60 * 1000; // 최근 5분
    let active = state.store.active_visitors(&q.site, since).await?;
    Ok(Json(LiveResp { active }))
}

// ---- Tracker JS (1KB) ----

const TRACKER_JS: &str = include_str!("../../tracker/psmeter.js");

async fn serve_tracker() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/javascript; charset=utf-8")
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(TRACKER_JS.into())
        .unwrap()
}

// ---- Dashboard (SvelteKit build, embedded) ----

async fn serve_dashboard(uri: Uri) -> Response {
    let raw = uri.path().trim_start_matches('/');
    let key_owned: String;
    let key: &str = if raw.is_empty() {
        "index.html"
    } else if Dashboard::get(raw).is_some() {
        raw
    } else if !raw.contains('.') {
        // SPA fallback for client-side routes
        "index.html"
    } else {
        // try index.html under that path (SvelteKit prerendered)
        key_owned = format!("{}/index.html", raw.trim_end_matches('/'));
        if Dashboard::get(&key_owned).is_some() {
            &key_owned
        } else {
            raw
        }
    };

    match Dashboard::get(key) {
        Some(file) => {
            let mime = mime_guess::from_path(key).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(file.data.into_owned().into())
                .unwrap()
        }
        None => {
            // Last-resort SPA fallback
            if let Some(idx) = Dashboard::get("index.html") {
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                    .body(idx.data.into_owned().into())
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body("not found".into())
                    .unwrap()
            }
        }
    }
}

// ---- Admin WebSocket: 실시간 사이트별 active visitors ----

#[derive(Deserialize)]
struct WsQuery {
    token: String,
}

async fn admin_ws(
    State(state): State<Arc<AppState>>,
    Query(q): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    {
        let cfg = state.config.read().await;
        if cfg.admin_token.is_empty() || q.token != cfg.admin_token {
            return (StatusCode::UNAUTHORIZED, "bad token").into_response();
        }
    }
    ws.on_upgrade(move |socket| admin_ws_session(socket, state))
}

#[derive(Serialize)]
struct SiteLive {
    domain: String,
    active: i64,
}

async fn admin_ws_session(mut socket: WebSocket, state: Arc<AppState>) {
    let mut tick = tokio::time::interval(Duration::from_secs(3));
    loop {
        tokio::select! {
            _ = tick.tick() => {
                let since = chrono::Utc::now().timestamp_millis() - 5 * 60 * 1000;
                let sites = match state.store.list_sites().await {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let mut out = Vec::with_capacity(sites.len());
                for s in sites {
                    let active = state.store.active_visitors(&s.domain, since).await.unwrap_or(0);
                    out.push(SiteLive { domain: s.domain, active });
                }
                let payload = serde_json::json!({ "sites": out }).to_string();
                if socket.send(Message::Text(payload)).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

// ---- Main ----

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "psmeter_server=info,tower_http=info".into()),
        )
        .init();

    let data_dir = PathBuf::from(
        std::env::var("PSMETER_DATA_DIR").unwrap_or_else(|_| "./data".to_string()),
    );
    fs::create_dir_all(&data_dir).expect("create data dir");

    let mut config = load_config(&data_dir);
    if let Ok(token) = std::env::var("PSMETER_ADMIN_TOKEN") {
        if !token.is_empty() {
            config.admin_token = token;
        }
    }

    let db_path = data_dir.join("psmeter.db");
    let store = Arc::new(SqliteStore::open(&db_path).expect("open sqlite"));
    tracing::info!("data directory: {}", data_dir.display());
    tracing::info!("sqlite: {}", db_path.display());

    if config.admin_token.is_empty() {
        tracing::warn!("FIRST RUN: open the admin URL to set the admin token");
    }

    let state = Arc::new(AppState {
        config: tokio::sync::RwLock::new(config),
        store,
        data_dir: data_dir.clone(),
    });

    // 백그라운드 롤업: 5분 간격. 이미 완료된 시간/일이면 자동 스킵.
    // 첫 실행 시 백필이 큰 DB라면 시간이 걸리지만 1회성.
    {
        let rollup_state = state.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(5 * 60));
            loop {
                tick.tick().await;
                match rollup_state.store.rollup_hourly().await {
                    Ok(n) if n > 0 => tracing::info!("hourly rollup: {n} new visitor rows"),
                    Ok(_) => {}
                    Err(e) => tracing::warn!("hourly rollup failed: {e}"),
                }
                match rollup_state.store.rollup_daily().await {
                    Ok(n) if n > 0 => tracing::info!("daily rollup: {n} new visitor rows"),
                    Ok(_) => {}
                    Err(e) => tracing::warn!("daily rollup failed: {e}"),
                }
            }
        });
    }

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/track", post(track))
        .route("/api/setup", post(setup))
        .route("/api/admin/sites", get(list_sites).post(create_site))
        .route("/api/admin/sites/:domain", delete(delete_site))
        .route("/api/admin/stats", get(stats))
        .route("/api/admin/live", get(live))
        .route("/api/admin/ws", get(admin_ws))
        .route("/tracker/psmeter.js", get(serve_tracker))
        .fallback(serve_dashboard)
        .layer(CorsLayer::permissive())
        .with_state(state);

    let port: u16 = std::env::var("PSMETER_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3100);
    let bind: std::net::IpAddr = std::env::var("PSMETER_BIND")
        .unwrap_or_else(|_| "0.0.0.0".to_string())
        .parse()
        .expect("invalid PSMETER_BIND");
    let addr = SocketAddr::new(bind, port);

    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    tracing::info!("psmeter listening on http://{}", addr);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("serve");
}
