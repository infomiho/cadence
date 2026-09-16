mod assets;
mod github;
mod render;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tokio::sync::RwLock;

use crate::assets::{
    Fingerprint, LOGO, LOGO_PATH, MASCOT, MASCOT_PATH, OG_IMAGE, OG_IMAGE_PATH, SCREENSHOT,
    SCREENSHOT_PATH, STYLESHEET_PATH,
};
use crate::github::{Github, LATEST_RELEASE_URL, Release};

const REFRESH_INTERVAL: Duration = Duration::from_secs(30 * 60);
const RETRY_INTERVAL: Duration = Duration::from_secs(2 * 60);
const MAX_BACKOFF: Duration = Duration::from_secs(30 * 60);
const HTML_CACHE_CONTROL: &str = "public, max-age=300";
const ASSET_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'none'; \
     style-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

#[derive(Clone)]
struct AppState {
    pages: Arc<RwLock<Arc<Pages>>>,
    github: Github,
}

struct Pages {
    home: String,
    releases: String,
    latest_download: Option<String>,
}

impl Pages {
    fn render(releases: &[Release], fingerprint: &Fingerprint) -> Self {
        Self {
            home: render::home(releases, fingerprint),
            releases: render::releases(releases, fingerprint),
            latest_download: github::latest_with_download(releases)
                .and_then(Release::dmg_url)
                .map(str::to_string),
        }
    }
}

#[tokio::main]
async fn main() {
    let fingerprint = Fingerprint::new();
    let state = AppState {
        pages: Arc::new(RwLock::new(Arc::new(Pages::render(&[], &fingerprint)))),
        github: Github::from_env(),
    };

    spawn_refresh(state.clone(), fingerprint);

    let app = Router::new()
        .route("/", get(home))
        .route("/releases", get(releases_page))
        .route("/download", get(download))
        .route(STYLESHEET_PATH, get(stylesheet))
        .route(LOGO_PATH, get(logo))
        .route(SCREENSHOT_PATH, get(screenshot))
        .route(OG_IMAGE_PATH, get(og_image))
        .route(MASCOT_PATH, get(mascot))
        .route("/healthz", get(healthz))
        .with_state(state)
        .layer(middleware::from_fn(security_headers));

    let port = port();
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("bind port");
    println!("cadence-web listening on http://0.0.0.0:{port}");
    axum::serve(listener, app).await.expect("serve");
}

fn spawn_refresh(state: AppState, fingerprint: Fingerprint) {
    tokio::spawn(async move {
        let mut backoff = RETRY_INTERVAL;

        loop {
            let releases = fetch_releases(&state.github).await;
            let delay = if releases.is_empty() {
                let delay = backoff;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                delay
            } else {
                backoff = RETRY_INTERVAL;
                *state.pages.write().await = Arc::new(Pages::render(&releases, &fingerprint));
                REFRESH_INTERVAL
            };
            tokio::time::sleep(delay).await;
        }
    });
}

async fn fetch_releases(github: &Github) -> Vec<Release> {
    match github.releases().await {
        Ok(releases) => releases,
        Err(error) => {
            eprintln!("failed to fetch releases from GitHub: {error}");
            Vec::new()
        }
    }
}

fn port() -> u16 {
    std::env::var("PORT")
        .ok()
        .and_then(|port| port.parse().ok())
        .unwrap_or(3000)
}

async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    response
}

async fn home(State(state): State<AppState>) -> Response {
    let pages = state.pages.read().await.clone();
    page(pages.home.clone())
}

async fn releases_page(State(state): State<AppState>) -> Response {
    let pages = state.pages.read().await.clone();
    page(pages.releases.clone())
}

async fn download(State(state): State<AppState>) -> Response {
    let target = state.pages.read().await.latest_download.clone();
    let location = target
        .as_deref()
        .and_then(|url| HeaderValue::from_str(url).ok())
        .unwrap_or_else(|| HeaderValue::from_static(LATEST_RELEASE_URL));

    let mut response = StatusCode::FOUND.into_response();
    response.headers_mut().insert(header::LOCATION, location);
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn stylesheet() -> Response {
    asset(
        Bytes::from_static(crate::assets::STYLESHEET.as_bytes()),
        "text/css; charset=utf-8",
    )
}

async fn logo() -> Response {
    asset(Bytes::from_static(LOGO), "image/svg+xml")
}

async fn screenshot() -> Response {
    asset(Bytes::from_static(SCREENSHOT), "image/webp")
}

async fn og_image() -> Response {
    asset(Bytes::from_static(OG_IMAGE), "image/png")
}

async fn mascot() -> Response {
    asset(Bytes::from_static(MASCOT), "image/svg+xml")
}

async fn healthz() -> &'static str {
    "ok"
}

fn page(body: String) -> Response {
    let mut response = body.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(HTML_CACHE_CONTROL),
    );
    response
}

fn asset(body: Bytes, content_type: &'static str) -> Response {
    let mut response = body.into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(ASSET_CACHE_CONTROL),
    );
    response
}
