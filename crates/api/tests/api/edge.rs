//! Every framework-edge error is a coded RFC 9457 problem, and the 500 leaks nothing: its `incidentId` is
//! the correlation id, which resolves to exactly one `request.unhandled-error` log event carrying the cause.
//! The body limit the router is built with is the one read and the one every 413 reports.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::json;
use sqlx::PgPool;

use crate::common::{BODY_LIMIT, CAUSE_SENTINEL, PANIC_SENTINEL, app_with_probes, capture_log, empty_request, send};

fn assert_problem(reply: &crate::common::Reply, status: u16, code: &str) {
    assert_eq!(reply.status.as_u16(), status, "{}", reply.text);
    assert_eq!(reply.headers["content-type"], "application/problem+json");
    assert_eq!(reply.json["status"], status);
    assert_eq!(reply.json["code"], code, "{}", reply.text);
    assert_eq!(reply.json["type"], "about:blank");
    assert!(reply.headers.contains_key("x-correlation-id"));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn router_refusals_are_coded(pool: PgPool) {
    let app = app_with_probes(pool);
    assert_problem(&send(&app.router, empty_request(Method::GET, "/no/such/route")).await, 404, "not-found");
    let reply = send(&app.router, empty_request(Method::DELETE, "/api/greetings")).await;
    assert_problem(&reply, 405, "request.method-not-allowed");
    assert!(reply.headers.contains_key("allow"));
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_path_variable_that_does_not_parse_is_an_entry_naming_it(pool: PgPool) {
    let app = app_with_probes(pool);
    let reply = send(&app.router, empty_request(Method::GET, "/api/greetings/abc")).await;
    assert_problem(&reply, 400, "validation.failed");
    assert_eq!(
        reply.json,
        json!({
            "type": "about:blank", "title": "Bad Request", "status": 400, "code": "validation.failed",
            "errors": [{ "in": "path", "name": "id", "code": "validation.invalid-value",
                         "params": { "expected": "uuid" }, "detail": "expected uuid" }],
        })
    );
    // Blank, not UTF-8 once decoded, and the UUID forms other than the 36-character one: none is trimmed or
    // parsed leniently.
    for id in [
        "%20",
        "%FF",
        "0192f0c18b397cc49a416f8e62a4a1b2",
        "%7B0192f0c1-8b39-7cc4-9a41-6f8e62a4a1b2%7D",
        "urn:uuid:0192f0c1-8b39-7cc4-9a41-6f8e62a4a1b2",
        "%200192f0c1-8b39-7cc4-9a41-6f8e62a4a1b2",
    ] {
        let reply = send(&app.router, empty_request(Method::GET, &format!("/api/greetings/{id}"))).await;
        assert_eq!(reply.json["errors"], reply_errors_for_id(), "{id}: {}", reply.text);
    }
    // Upper-case hexadecimal digits are the same form: read, and not found.
    let upper =
        send(&app.router, empty_request(Method::GET, "/api/greetings/0192F0C1-8B39-7CC4-9A41-6F8E62A4A1B2")).await;
    assert_problem(&upper, 404, "not-found");
    assert_eq!(app.tx.begun(), 1);
}

fn reply_errors_for_id() -> serde_json::Value {
    json!([{ "in": "path", "name": "id", "code": "validation.invalid-value", "params": { "expected": "uuid" }, "detail": "expected uuid" }])
}

/// A request to `uri` whose body is `body` padded with trailing whitespace to exactly `length` bytes, its
/// length declared in `content-length` or not.
fn sized(uri: &str, body: &str, length: u32, declared: bool) -> Request<Body> {
    let padded = format!("{body:<width$}", width = usize::try_from(length).unwrap());
    assert_eq!(padded.len(), usize::try_from(length).unwrap());
    let mut request = Request::builder().method(Method::POST).uri(uri).header("content-type", "application/json");
    if declared {
        request = request.header("content-length", padded.len());
    }
    request.body(Body::from(padded)).unwrap()
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_body_of_the_limit_is_read_and_one_byte_more_is_refused_with_the_limit(pool: PgPool) {
    let app = app_with_probes(pool);
    let too_large = json!({
        "type": "about:blank", "title": "Payload Too Large", "status": 413, "code": "request.too-large",
        "params": { "max": BODY_LIMIT }, "detail": format!("The request body must be at most {BODY_LIMIT} bytes."),
    });
    // With a declared length the limit layer refuses it; without one, the body reader's own limit does.
    for declared in [true, false] {
        let exact = send(&app.router, sized("/api/greetings", r#"{"name":"Ada"}"#, BODY_LIMIT, declared)).await;
        assert_eq!(exact.status, 201, "declared {declared}: {}", exact.text);
        let over = send(&app.router, sized("/api/greetings", r#"{"name":"Ada"}"#, BODY_LIMIT + 1, declared)).await;
        assert_problem(&over, 413, "request.too-large");
        assert_eq!(over.json, too_large, "declared {declared}");
        // An extractor with its own, larger default limit (axum's String takes 2 MB) is held to the limit too,
        // and the edge's 413 reports it.
        let raw = send(&app.router, sized("/test/lenient", "{}", BODY_LIMIT + 1, declared)).await;
        assert_problem(&raw, 413, "request.too-large");
        assert_eq!(raw.json, too_large, "declared {declared}");
    }
    let text = Request::builder()
        .method(Method::POST)
        .uri("/api/greetings")
        .header("content-type", "text/plain")
        .body(Body::from(r#"{"name":"a"}"#))
        .unwrap();
    assert_problem(&send(&app.router, text).await, 415, "request.unsupported-media-type");
    // The two bodies of exactly the limit, and nothing refused, reached a transaction.
    assert_eq!(app.tx.begun(), 2);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_panic_is_a_500_that_leaks_nothing_and_resolves_to_one_log_event(pool: PgPool) {
    let app = app_with_probes(pool);
    let (log, _guard) = capture_log();
    let reply = send(&app.router, empty_request(Method::GET, "/test/boom")).await;
    assert_unhandled(&reply, &log, PANIC_SENTINEL);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_internal_error_is_a_500_that_leaks_nothing_and_resolves_to_one_log_event(pool: PgPool) {
    let app = app_with_probes(pool);
    let (log, _guard) = capture_log();
    let reply = send(&app.router, empty_request(Method::GET, "/test/internal")).await;
    assert_unhandled(&reply, &log, CAUSE_SENTINEL);
}

fn assert_unhandled(reply: &crate::common::Reply, log: &platform::log::Capture, sentinel: &str) {
    assert_problem(reply, 500, "platform.internal");
    assert!(!reply.text.contains(sentinel), "the 500 leaked its cause: {}", reply.text);
    let incident = reply.json["incidentId"].as_str().unwrap();
    assert_eq!(reply.headers["x-correlation-id"], incident);
    let events: Vec<_> = log
        .events()
        .into_iter()
        .filter(|e| e["fields"]["event"] == "request.unhandled-error" && e["span"]["correlation_id"] == incident)
        .collect();
    assert_eq!(events.len(), 1, "{}", log.text());
    assert!(events[0]["fields"]["cause"].as_str().unwrap().contains(sentinel));
    assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR);
}
