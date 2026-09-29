//! Every framework-edge error is a coded RFC 9457 problem, and the 500 leaks nothing: its `incidentId` is
//! the correlation id, which resolves to exactly one `request.unhandled-error` log event carrying the cause.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use sqlx::PgPool;

use crate::common::{CAUSE_SENTINEL, PANIC_SENTINEL, app_with_probes, capture_log, empty_request, json_request, send};

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
    assert_problem(
        &send(&app.router, empty_request(Method::GET, "/api/greetings/not-a-uuid")).await,
        400,
        "validation.bad-request",
    );
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn body_refusals_before_the_handler_are_coded(pool: PgPool) {
    let app = app_with_probes(pool);
    let big = format!(r#"{{"name":"{}"}}"#, "x".repeat(web::body::BODY_LIMIT));
    assert_problem(
        &send(&app.router, json_request(Method::POST, "/api/greetings", &big)).await,
        413,
        "request.too-large",
    );
    // With a declared length the limit layer refuses it; without one, the body reader's own limit does.
    let declared = Request::builder()
        .method(Method::POST)
        .uri("/api/greetings")
        .header("content-type", "application/json")
        .header("content-length", big.len())
        .body(Body::from(big.clone()))
        .unwrap();
    assert_problem(&send(&app.router, declared).await, 413, "request.too-large");
    let text = Request::builder()
        .method(Method::POST)
        .uri("/api/greetings")
        .header("content-type", "text/plain")
        .body(Body::from(r#"{"name":"a"}"#))
        .unwrap();
    assert_problem(&send(&app.router, text).await, 415, "request.unsupported-media-type");
    assert_eq!(app.tx.begun(), 0);
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
