//! The edge every request and response passes, outermost first:
//!
//! 1. **correlation**: mints a UUIDv7 `correlation_id`, runs the request inside `platform::log::request_span`
//!    so every log line carries it, and returns it as `x-correlation-id`. A client-supplied value is not
//!    trusted.
//! 2. **problem edge**: every response of status 400 or above leaves as an RFC 9457 problem with a catalog
//!    code. The router's own 404 and 405 and the body limit's 413, `max` the limit, are each coded here. An
//!    unexpected failure — a panic, an [`ApiError::Internal`](crate::problem::ApiError), an uncoded status —
//!    is logged once as `request.unhandled-error` with its cause, and answered with `platform.internal` and
//!    `incidentId`, the correlation id, and nothing else.
//! 3. **panic capture**: tower-http's `CatchPanicLayer`; a panicking handler becomes a 500 rather than a
//!    dropped connection (`Cargo.toml` keeps `panic = "unwind"`, without which the layer does nothing).
//! 4. **body limit**: the limit [`finish`] is given, `REQUEST_BODY_MAX_BYTES` in the service's configuration,
//!    carried on every request for the body reader and applied by tower-http's `RequestBodyLimitLayer`: an
//!    extractor that reads the body stream directly has no limit of its own.

use std::any::Any;
use std::future::Future;
use std::num::NonZeroU32;

use axum::Extension;
use axum::extract::{Request, State};
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

use crate::body::BodyLimit;
use crate::codes::ApiErrorCode;
use crate::problem::{ApiError, PROBLEM_JSON, Problem, Unhandled, problem, problem_response};

static LOG: Log = Log::new(module_path!());

/// The response header carrying the request's correlation id.
pub const CORRELATION_HEADER: HeaderName = HeaderName::from_static("x-correlation-id");

/// The request's correlation id, in the request's extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorrelationId(pub Uuid);

/// The served router and its document: the feature routes with state applied and every edge layer on them,
/// reading no request body longer than `request_body_max_bytes`. Routes enter only through
/// `OpenApiRouter::routes`, so every served route is in the document.
pub fn finish<S>(
    routes: OpenApiRouter<S>,
    state: S,
    request_body_max_bytes: NonZeroU32,
) -> (axum::Router, utoipa::openapi::OpenApi)
where
    S: Clone + Send + Sync + 'static,
{
    let limit = BodyLimit(request_body_max_bytes);
    let (router, document) = routes.split_for_parts();
    let router = router
        .with_state(state)
        .layer(RequestBodyLimitLayer::new(limit.usize()))
        .layer(Extension(limit))
        .layer(CatchPanicLayer::custom(panic_response))
        .layer(middleware::from_fn_with_state(limit, problem_edge))
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

async fn problem_edge(State(limit): State<BodyLimit>, req: Request, next: Next) -> Response {
    let incident = req.extensions().get::<CorrelationId>().map(|c| c.0);
    let response = next.run(req).await;
    code_response(response, incident, limit.bytes())
}

/// Codes a response that is not already a problem; a 413 carries `max_body_bytes`, the limit every body is
/// read under. Public so the edge's mapping is tested without a server.
#[must_use]
pub fn code_response(response: Response, incident: Option<Uuid>, max_body_bytes: u32) -> Response {
    let status = response.status();
    if let Some(Unhandled(cause)) = response.extensions().get::<Unhandled>() {
        return unhandled(cause, status, incident);
    }
    if status.as_u16() < 400 || is_problem(&response) {
        return response;
    }
    let coded = match status {
        StatusCode::BAD_REQUEST => ApiError::rejected(ApiErrorCode::BadRequest),
        StatusCode::NOT_FOUND => ApiError::rejected(ApiErrorCode::NotFound),
        StatusCode::METHOD_NOT_ALLOWED => ApiError::rejected(ApiErrorCode::MethodNotAllowed),
        StatusCode::PAYLOAD_TOO_LARGE => ApiError::too_large(max_body_bytes),
        _ => return unhandled("a response with an uncoded error status", status, incident),
    };
    let allow = response.headers().get(header::ALLOW).cloned();
    let mut coded = coded.into_response();
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
    problem_response(Problem { incident_id: incident.map(|id| id.to_string()), ..problem(code.status(), code.wire()) })
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

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use axum::response::IntoResponse as _;
    use http_body_util::BodyExt as _;
    use serde_json::{Value, json};

    async fn coded(status: StatusCode) -> Value {
        let response = super::code_response(status.into_response(), None, 7);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn a_bare_error_status_is_coded_and_a_413_carries_the_limit() {
        assert_eq!(coded(StatusCode::BAD_REQUEST).await["code"], "validation.bad-request");
        assert_eq!(coded(StatusCode::NOT_FOUND).await["code"], "not-found");
        assert_eq!(coded(StatusCode::METHOD_NOT_ALLOWED).await["code"], "request.method-not-allowed");
        let too_large = coded(StatusCode::PAYLOAD_TOO_LARGE).await;
        assert_eq!((&too_large["code"], &too_large["params"]), (&json!("request.too-large"), &json!({ "max": 7 })));
        assert_eq!(coded(StatusCode::IM_A_TEAPOT).await["code"], "platform.internal");
    }
}
