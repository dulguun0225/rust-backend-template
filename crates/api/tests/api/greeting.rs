//! The worked example end to end against a real PostgreSQL.

use axum::http::Method;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::common::{app, capture_log, empty_request, json_request, send};

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_created_greeting_reads_back_at_its_location(pool: PgPool) {
    let app = app(pool);
    let (log, _guard) = capture_log();
    let created = send(&app.router, json_request(Method::POST, "/api/greetings", r#"{"name":"  Ada  "}"#)).await;
    assert_eq!(created.status, 201, "{}", created.text);
    let id = created.json["id"].as_str().unwrap().to_owned();
    assert_eq!(created.headers["location"], format!("/api/greetings/{id}"));
    assert_eq!(created.json["name"], "Ada");
    assert_eq!(created.json["message"], "Hello, Ada!");
    assert_eq!(created.json["createdAt"], "2026-09-29T10:00:00.123456Z");

    let read = send(&app.router, empty_request(Method::GET, &format!("/api/greetings/{id}"))).await;
    assert_eq!(read.status, 200);
    assert_eq!(read.json, created.json);

    let event = log.events().into_iter().find(|e| e["fields"]["message"] == "greeting created").unwrap();
    assert!(event["fields"]["fields"].as_str().unwrap().contains(&id));
    assert!(!log.text().contains("Ada"), "a name reached the log");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn an_unknown_id_is_not_found(pool: PgPool) {
    let app = app(pool);
    let reply =
        send(&app.router, empty_request(Method::GET, &format!("/api/greetings/{}", platform::ids::new_id()))).await;
    assert_eq!(reply.status, 404);
    assert_eq!(reply.json["code"], "not-found");
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn every_refusal_is_one_validation_failed_and_opens_no_transaction(pool: PgPool) {
    let app = app(pool);
    let long = "x".repeat(usize::from(api::greeting::NAME_MAX_CHARS) + 1);
    let required = json!({ "pointer": "/name", "code": "validation.required" });
    let cases = [
        (r#"{"name":"   "}"#.to_owned(), vec![required.clone()]),
        (
            format!(r#"{{"name":"{long}"}}"#),
            vec![json!({ "pointer": "/name", "code": "validation.too-long", "params": { "max": 100 } })],
        ),
        ("{}".to_owned(), vec![required.clone()]),
        (
            r#"{"name":"   ","nickname":"x"}"#.to_owned(),
            vec![json!({ "pointer": "/nickname", "code": "validation.unknown-field" }), required],
        ),
        (
            r#"{"name":true}"#.to_owned(),
            vec![json!({
                "pointer": "/name", "code": "validation.wrong-type",
                "detail": "expected string", "params": { "expected": "string" },
            })],
        ),
        (
            r#"{"name":"Ada","name":"Bob"}"#.to_owned(),
            vec![json!({ "pointer": "/name", "code": "validation.duplicate-member" })],
        ),
        (
            r#"{"name":{"first":"Ada","first":"Bob"}}"#.to_owned(),
            vec![
                json!({
                    "pointer": "/name", "code": "validation.wrong-type",
                    "detail": "expected string", "params": { "expected": "string" },
                }),
                json!({ "pointer": "/name/first", "code": "validation.duplicate-member" }),
            ],
        ),
    ];
    for (body, expected) in cases {
        let reply = send(&app.router, json_request(Method::POST, "/api/greetings", &body)).await;
        assert_eq!(reply.status, 400, "{body}");
        assert_eq!(reply.json["code"], "validation.failed", "{body}");
        assert_eq!(reply.json["errors"], Value::Array(expected), "{body}");
    }
    assert_eq!(app.tx.begun(), 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_name_of_exactly_the_maximum_length_is_accepted(pool: PgPool) {
    let app = app(pool);
    let name = "x".repeat(usize::from(api::greeting::NAME_MAX_CHARS));
    let reply =
        send(&app.router, json_request(Method::POST, "/api/greetings", &format!(r#"{{"name":"{name}"}}"#))).await;
    assert_eq!(reply.status, 201, "{}", reply.text);
}
