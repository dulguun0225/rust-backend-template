//! The edge every request and response passes, outermost first:
//!
//! 1. **correlation**: mints a UUIDv7 `correlation_id`, runs the request inside `platform::log::request_span`
//!    so every log line carries it, and returns it as `x-correlation-id`. A client-supplied value is not
//!    trusted.
//! 2. **problem edge**: every response of status 400 or above leaves as an RFC 9457 problem with a catalog
//!    code. The router's own 404 and 405, a path segment of the wrong type, the body limit's 413: each is
//!    coded here. An unexpected failure — a panic, an [`ApiError::Internal`](crate::problem::ApiError), an
//!    uncoded status — is logged once as `request.unhandled-error` with its cause, and answered with
//!    `platform.internal` and `incidentId`, the correlation id, and nothing else.
//! 3. **panic capture**: tower-http's `CatchPanicLayer`; a panicking handler becomes a 500 rather than a
//!    dropped connection (`Cargo.toml` keeps `panic = "unwind"`, without which the layer does nothing).
//! 4. **body limit**: tower-http's `RequestBodyLimitLayer` at [`BODY_LIMIT`]: an extractor that reads the body
//!    stream directly has no limit of its own.

use std::any::Any;
use std::future::Future;

use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use platform::catalog::WireError;
use platform::log::{Log, LogEvent, LogField};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tracing::Instrument as _;
use utoipa_axum::router::OpenApiRouter;
use uuid::Uuid;

use crate::body::BODY_LIMIT;
use crate::codes::ApiErrorCode;
use crate::problem::{PROBLEM_JSON, Unhandled, problem_response};

static LOG: Log = Log::new(module_path!());

/// The response header carrying the request's correlation id.
pub const CORRELATION_HEADER: HeaderName = HeaderName::from_static("x-correlation-id");

/// The request's correlation id, in the request's extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorrelationId(pub Uuid);

/// The served router and its document: the feature routes with state applied and every edge layer on them.
/// Routes enter only through `OpenApiRouter::routes`, so every served route is in the document.
pub fn finish<S>(routes: OpenApiRouter<S>, state: S) -> (axum::Router, utoipa::openapi::OpenApi)
where
    S: Clone + Send + Sync + 'static,
{
    let (router, document) = routes.split_for_parts();
    let router = router
        .with_state(state)
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT))
        .layer(CatchPanicLayer::custom(panic_response))
        .layer(middleware::from_fn(problem_edge))
        .layer(middleware::from_fn(correlate));
    (router, document)
}

/// Serves `router` on `listener` until `shutdown` completes, then drains in-flight requests.
///
/// # Errors
/// The listener's I/O error.
pub async fn serve(
    listener: tokio::net::TcpListener,
    router: axum::Router,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, router).with_graceful_shutdown(shutdown).await
}

async fn correlate(mut req: Request, next: Next) -> Response {
    let id = platform::ids::new_id();
    req.extensions_mut().insert(CorrelationId(id));
    let mut response = next.run(req).instrument(platform::log::request_span(id)).await;
    if let Ok(value) = HeaderValue::from_str(&id.to_string()) {
        response.headers_mut().insert(CORRELATION_HEADER, value);
    }
    response
}

async fn problem_edge(req: Request, next: Next) -> Response {
    let incident = req.extensions().get::<CorrelationId>().map(|c| c.0);
    let response = next.run(req).await;
    code_response(response, incident)
}

/// Codes a response that is not already a problem. Public so the edge's mapping is tested without a server.
#[must_use]
pub fn code_response(response: Response, incident: Option<Uuid>) -> Response {
    let status = response.status();
    if let Some(Unhandled(cause)) = response.extensions().get::<Unhandled>() {
        return unhandled(cause, status, incident);
    }
    if status.as_u16() < 400 || is_problem(&response) {
        return response;
    }
    let code = match status {
        StatusCode::BAD_REQUEST => ApiErrorCode::BadRequest,
        StatusCode::NOT_FOUND => ApiErrorCode::NotFound,
        StatusCode::METHOD_NOT_ALLOWED => ApiErrorCode::MethodNotAllowed,
        StatusCode::PAYLOAD_TOO_LARGE => ApiErrorCode::PayloadTooLarge,
        StatusCode::UNSUPPORTED_MEDIA_TYPE => ApiErrorCode::UnsupportedMediaType,
        _ => return unhandled("a response with an uncoded error status", status, incident),
    };
    let allow = response.headers().get(header::ALLOW).cloned();
    let mut coded = problem_response(code.status(), code.wire(), None, None, None);
    if let Some(allow) = allow {
        coded.headers_mut().insert(header::ALLOW, allow);
    }
    coded
}

fn unhandled(cause: &str, status: StatusCode, incident: Option<Uuid>) -> Response {
    let mut fields = vec![LogField::count("status", u64::from(status.as_u16()))];
    if let Some(id) = incident {
        fields.push(LogField::id("incident_id", id));
    }
    LOG.event_with_cause(LogEvent::RequestUnhandledError, cause, &fields);
    let code = ApiErrorCode::Internal;
    problem_response(code.status(), code.wire(), None, None, incident.map(|id| id.to_string()))
}

fn is_problem(response: &Response) -> bool {
    response.headers().get(header::CONTENT_TYPE).is_some_and(|v| v.as_bytes() == PROBLEM_JSON.as_bytes())
}

fn panic_response(payload: Box<dyn Any + Send + 'static>) -> Response {
    let cause = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with a non-string payload".to_owned());
    let mut response = StatusCode::INTERNAL_SERVER_ERROR.into_response();
    response.extensions_mut().insert(Unhandled(format!("panic: {cause}")));
    response
}
