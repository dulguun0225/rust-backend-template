//! The strict readers of the inputs outside the body. The test-only `/test/params` declares a required integer
//! query parameter `count`, an optional enumeration `kind` and a UUID header `X-Probe-Id`: an absent, blank,
//! non-parsing, out-of-set, repeated or undeclared input is each one entry at its `in` and declared `name`, and
//! an undeclared header is accepted. Then the query sweep: every documented operation, discovered from the
//! document of exactly what is served, refuses an undeclared query parameter, listing the query parameters it
//! declares — `[]` where it declares none — and the test-only `/test/lenient`, which reads no query, is the
//! negative control: the sweep must report it, and only it.

use std::collections::BTreeSet;

use axum::body::Body;
use axum::http::{HeaderValue, Method, Request};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::common::{app_with_probes, empty_request, json_request, send};

const ID: &str = "0192f0c1-8b39-7cc4-9a41-6f8e62a4a1b2";
const SENTINEL: &str = "sentinel-c40e";

fn probe(query: &str, headers: &[(&str, HeaderValue)]) -> Request<Body> {
    let mut request = Request::builder().method(Method::GET).uri(format!("/test/params?{query}"));
    for (name, value) in headers {
        request = request.header(*name, value.clone());
    }
    request.body(Body::empty()).unwrap()
}

/// The headers sent with one request.
type Headers<'a> = Vec<(&'a str, HeaderValue)>;

fn text(value: &'static str) -> HeaderValue {
    HeaderValue::from_static(value)
}

fn invalid(location: &str, name: &str, expected: &str) -> Value {
    json!({ "in": location, "name": name, "code": "validation.invalid-value",
            "params": { "expected": expected }, "detail": format!("expected {expected}") })
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn each_failing_input_is_one_entry_at_its_location_and_declared_name(pool: PgPool) {
    let app = app_with_probes(pool);
    let id = || vec![("X-Probe-Id", text(ID))];
    let count = invalid("query", "count", "int32");
    let header = invalid("header", "X-Probe-Id", "uuid");
    let cases: Vec<(&str, Headers<'_>, Value)> = vec![
        ("", id(), json!({ "in": "query", "name": "count", "code": "validation.required" })),
        ("count=", id(), count.clone()),
        ("count=%20", id(), count.clone()),
        ("count=+1", id(), count.clone()),
        ("count=abc", id(), count.clone()),
        ("count=1.0", id(), count.clone()),
        ("count=%FF", id(), count.clone()),
        ("count=3000000000", id(), count),
        ("count=1&count=1", id(), json!({ "in": "query", "name": "count", "code": "validation.duplicate-member" })),
        (
            "count=1&cuont=1",
            id(),
            json!({ "in": "query", "name": "cuont", "code": "validation.unknown-field",
                    "params": { "allowed": ["count", "kind"] } }),
        ),
        (
            "count=1&kind=Plain",
            id(),
            json!({ "in": "query", "name": "kind", "code": "validation.unknown-value",
                    "params": { "allowed": ["fancy", "plain"] } }),
        ),
        ("count=1", vec![], json!({ "in": "header", "name": "X-Probe-Id", "code": "validation.required" })),
        ("count=1", vec![("x-probe-id", text(""))], header.clone()),
        ("count=1", vec![("X-PROBE-ID", text("0192f0c18b397cc49a416f8e62a4a1b2"))], header.clone()),
        ("count=1", vec![("X-Probe-Id", text("{0192f0c1-8b39-7cc4-9a41-6f8e62a4a1b2}"))], header.clone()),
        ("count=1", vec![("X-Probe-Id", HeaderValue::from_bytes(b"\xff").unwrap())], header),
        (
            "count=1",
            vec![("X-Probe-Id", text(ID)), ("x-probe-id", text(ID))],
            json!({ "in": "header", "name": "X-Probe-Id", "code": "validation.duplicate-member" }),
        ),
    ];
    for (query, headers, expected) in cases {
        let reply = send(&app.router, probe(query, &headers)).await;
        let label = format!("?{query} {headers:?}");
        assert_eq!(reply.status, 400, "{label}: {}", reply.text);
        assert_eq!(reply.json["code"], "validation.failed", "{label}");
        assert_eq!(reply.json["errors"], json!([expected]), "{label}");
        assert_eq!(reply.json.get("errorsOmitted"), None, "{label}");
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn declared_inputs_are_read_as_sent_and_an_undeclared_header_is_accepted(pool: PgPool) {
    let app = app_with_probes(pool);
    for (query, kind) in [("count=-7&kind=fancy", "fancy"), ("count=-7", "none")] {
        let headers = [("x-probe-id", text(ID)), ("X-Undeclared", text("anything")), ("x-undeclared", text("again"))];
        let reply = send(&app.router, probe(query, &headers)).await;
        assert_eq!(reply.status, 204, "{query}: {}", reply.text);
        assert_eq!(
            (&reply.headers["x-count"], &reply.headers["x-kind"], &reply.headers["x-probe-id"]),
            (&text("-7"), &HeaderValue::from_static(kind), &text(ID))
        );
    }
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn at_most_100_entries_are_recorded_and_the_rest_counted(pool: PgPool) {
    let app = app_with_probes(pool);
    let undeclared: Vec<String> = (0..101).map(|n| format!("u{n:03}=1")).collect();
    let reply =
        send(&app.router, probe(&format!("count=1&{}", undeclared.join("&")), &[("X-Probe-Id", text(ID))])).await;
    assert_eq!(reply.status, 400, "{}", reply.text);
    assert_eq!(reply.json["errors"].as_array().map(Vec::len), Some(100));
    assert_eq!(reply.json["errorsOmitted"], 1);
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn every_operation_refuses_an_undeclared_query_parameter_and_the_lenient_probe_is_caught(pool: PgPool) {
    let app = app_with_probes(pool);
    let document = serde_json::to_value(&app.document).unwrap();
    let mut reported = BTreeSet::new();
    let mut unexpected = Vec::new();
    let mut swept = 0_u32;
    for (path, item) in document["paths"].as_object().into_iter().flatten() {
        for (method, operation) in item.as_object().into_iter().flatten() {
            let mut allowed: Vec<&str> = operation["parameters"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|p| p["in"] == "query")
                .filter_map(|p| p["name"].as_str())
                .collect();
            allowed.sort_unstable();
            let mut uri = path.clone();
            for variable in web::body::template_variables(path) {
                uri = uri.replace(&format!("{{{variable}}}"), &platform::ids::new_id().to_string());
            }
            let uri = format!("{uri}?undeclaredParameter={SENTINEL}");
            let method: Method = method.to_uppercase().parse().unwrap();
            let request = if operation.get("requestBody").is_some() {
                json_request(method.clone(), &uri, "{}")
            } else {
                empty_request(method.clone(), &uri)
            };
            let reply = send(&app.router, request).await;
            swept = swept.saturating_add(1);
            let expected = json!({ "in": "query", "name": "undeclaredParameter", "code": "validation.unknown-field",
                                   "params": { "allowed": allowed } });
            let found = reply.status == 400
                && reply.json["code"] == "validation.failed"
                && reply.json["errors"].as_array().into_iter().flatten().any(|e| *e == expected);
            let failure = if !found {
                Some(format!("{method} {path}: no {expected} in {} {}", reply.status, reply.text))
            } else if reply.text.contains(SENTINEL) {
                Some(format!("{method} {path}: echoed the value sent"))
            } else {
                None
            };
            if path == "/test/lenient" {
                if failure.is_some() {
                    reported.insert(path.clone());
                }
            } else {
                unexpected.extend(failure);
            }
        }
    }
    assert!(swept > 2, "the sweep reached {swept} operations");
    assert!(unexpected.is_empty(), "{unexpected:#?}");
    assert_eq!(reported, BTreeSet::from(["/test/lenient".to_owned()]), "the negative control was not caught");
}
