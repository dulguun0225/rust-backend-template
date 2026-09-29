//! What every API test uses: the app over a test database, a request helper, the captured log, and the
//! test-only routes that exercise what the worked example cannot.

use std::sync::Arc;

use api::AppState;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use http_body_util::BodyExt as _;
use platform::clock::FixedClock;
use platform::log::Capture;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::macros::datetime;
use tower::ServiceExt as _;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use web::body::StrictJson;
use web::problem::ApiError;

/// The served router, its document, and the seam whose transaction count a test reads.
pub struct TestApp {
    pub router: axum::Router,
    pub document: utoipa::openapi::OpenApi,
    pub tx: db::Tx,
}

fn state(pool: PgPool) -> AppState {
    AppState { tx: db::Tx::new(pool), clock: Arc::new(FixedClock::at(datetime!(2026-09-29 10:00:00.123456 UTC))) }
}

/// The service exactly as `api::app` serves it.
pub fn app(pool: PgPool) -> TestApp {
    let state = state(pool);
    let tx = state.tx.clone();
    let (router, document) = web::edge::finish(api::routes(), state);
    TestApp { router, document, tx }
}

/// The service plus the test-only routes below.
pub fn app_with_probes(pool: PgPool) -> TestApp {
    let state = state(pool);
    let tx = state.tx.clone();
    let routes = api::routes()
        .merge(OpenApiRouter::new().routes(routes!(update_probe)).routes(routes!(lenient_probe)))
        .merge(OpenApiRouter::new().routes(routes!(boom)).routes(routes!(internal_failure)));
    let (router, document) = web::edge::finish(routes, state);
    TestApp { router, document, tx }
}

/// A response, its body parsed as JSON when it is JSON.
#[derive(Debug)]
pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub text: String,
    pub json: serde_json::Value,
}

pub async fn send(router: &axum::Router, request: Request<Body>) -> Reply {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let json = text.parse().unwrap_or(serde_json::Value::Null);
    Reply { status, headers, text, json }
}

pub fn json_request(method: Method, uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

pub fn empty_request(method: Method, uri: &str) -> Request<Body> {
    Request::builder().method(method).uri(uri).body(Body::empty()).unwrap()
}

/// Installs the service's JSON log format over an in-memory sink for the rest of the test's thread.
pub fn capture_log() -> (Capture, tracing::subscriber::DefaultGuard) {
    let capture = Capture::default();
    let guard =
        tracing::subscriber::set_default(platform::log::json_subscriber(capture.clone(), tracing::Level::DEBUG));
    (capture, guard)
}

/// A request type for the probe route, whose path has a variable the worked example's body route lacks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProbeRequest {
    pub label: String,
    pub count: i64,
    pub active: bool,
}

/// Test only: a strict body under a path variable. Opens one write transaction when the body passes.
#[utoipa::path(put, path = "/test/probes/{probeId}", request_body = ProbeRequest, params(("probeId" = uuid::Uuid, Path)),
    responses((status = 204), (status = 400, body = web::problem::Problem, content_type = "application/problem+json")))]
pub async fn update_probe(
    State(state): State<AppState>,
    Path(_probe_id): Path<uuid::Uuid>,
    StrictJson(body): StrictJson<ProbeRequest>,
) -> Result<StatusCode, ApiError> {
    let _label = body.validate(|p, _| Some(p.label.clone()))?;
    state.tx.write(async |_| Ok::<(), db::DbError>(())).await.map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Test only, the negative control: it documents a strict body and reads the body leniently, so the sweep
/// must report it. Opens one read transaction on every request, so the sweep's zero is a measurement.
#[utoipa::path(post, path = "/test/lenient", request_body = ProbeRequest, responses((status = 200)))]
pub async fn lenient_probe(State(state): State<AppState>, _body: String) -> Result<StatusCode, ApiError> {
    state.tx.read(async |_| Ok::<(), db::DbError>(())).await.map_err(ApiError::internal)?;
    Ok(StatusCode::OK)
}

pub const PANIC_SENTINEL: &str = "panic-sentinel-7f3a";
pub const CAUSE_SENTINEL: &str = "cause-sentinel-91cd";

/// Test only: a handler that panics with a sentinel message.
#[utoipa::path(get, path = "/test/boom", responses((status = 200)))]
pub async fn boom() -> StatusCode {
    std::panic::panic_any(PANIC_SENTINEL)
}

/// Test only: a handler that fails with an internal error carrying a sentinel cause.
#[utoipa::path(get, path = "/test/internal", responses((status = 200)))]
pub async fn internal_failure() -> Result<StatusCode, ApiError> {
    Err(ApiError::internal(CAUSE_SENTINEL))
}
