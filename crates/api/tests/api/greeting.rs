//! The worked example end to end against a real PostgreSQL.

use axum::http::Method;
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
    let long = "x".repeat(api::greeting::NAME_MAX_CHARS + 1);
    let cases = [
        (r#"{"name":"   "}"#.to_owned(), vec![("/name", "validation.required")]),
        (format!(r#"{{"name":"{long}"}}"#), vec![("/name", "validation.too-long")]),
        ("{}".to_owned(), vec![("/name", "validation.required")]),
        (
            r#"{"name":"   ","nickname":"x"}"#.to_owned(),
            vec![("/nickname", "validation.unknown-field"), ("/name", "validation.required")],
        ),
        (r#"{"name":true}"#.to_owned(), vec![("/name", "validation.wrong-type")]),
    ];
    for (body, expected) in cases {
        let reply = send(&app.router, json_request(Method::POST, "/api/greetings", &body)).await;
        assert_eq!(reply.status, 400, "{body}");
        assert_eq!(reply.json["code"], "validation.failed", "{body}");
        let got: Vec<(&str, &str)> = reply.json["errors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e["pointer"].as_str().unwrap(), e["code"].as_str().unwrap()))
            .collect();
        assert_eq!(got, expected, "{body}");
    }
    assert_eq!(app.tx.begun(), 0);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_name_of_exactly_the_maximum_length_is_accepted(pool: PgPool) {
    let app = app(pool);
    let name = "x".repeat(api::greeting::NAME_MAX_CHARS);
    let reply =
        send(&app.router, json_request(Method::POST, "/api/greetings", &format!(r#"{{"name":"{name}"}}"#))).await;
    assert_eq!(reply.status, 201, "{}", reply.text);
}
