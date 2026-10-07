//! The strict-body sweep: every operation the document declares a request body for, discovered from the
//! document of exactly what is served, refuses an undeclared member, listing as `allowed` exactly the members
//! its request schema declares, each path variable sent in the body, each
//! member sent as the wrong JSON type, each member sent twice, each member sent as an object that repeats a
//! member of its own, broken JSON and a missing body — each a 400 that opens no transaction
//! and puts no sentinel value in any response or log line. The test-only `/test/probes/{probeId}` keeps the
//! path-variable case non-vacuous; the test-only `/test/lenient` reads its body leniently and is the negative
//! control: the sweep must report it, and only it.

use std::collections::BTreeSet;

use axum::http::Method;
use serde_json::{Map, Value};
use sqlx::PgPool;

use crate::common::{TestApp, app, app_with_probes, capture_log, json_request, send};

const SENTINEL: &str = "sentinel-5be1";

/// One refusal case: its label, the body sent, and the `validation.failed` entry it must produce — its pointer,
/// code and, where the case fixes them, params — or `None` when it must be `validation.malformed-body`.
struct Case {
    label: String,
    body: String,
    entry: Option<(String, &'static str)>,
    params: Option<Value>,
}

struct Operation {
    method: Method,
    path: String,
    members: Map<String, Value>,
}

fn operations(document: &Value) -> Vec<Operation> {
    let mut out = Vec::new();
    for (path, item) in document["paths"].as_object().into_iter().flatten() {
        for (method, operation) in item.as_object().into_iter().flatten() {
            let Some(reference) = operation["requestBody"]["content"]["application/json"]["schema"]["$ref"].as_str()
            else {
                continue;
            };
            let name = reference.trim_start_matches("#/components/schemas/");
            let mut members = Map::new();
            for (member, schema) in
                document["components"]["schemas"][name]["properties"].as_object().into_iter().flatten()
            {
                let valid = match schema["type"].as_str() {
                    Some("integer" | "number") => Value::from(1),
                    Some("boolean") => Value::Bool(true),
                    _ => Value::String("valid".to_owned()),
                };
                members.insert(member.clone(), valid);
            }
            out.push(Operation { method: method.to_uppercase().parse().unwrap(), path: path.clone(), members });
        }
    }
    out
}

/// Every way `operation` fails to refuse, as text; empty when it refuses each case correctly.
async fn sweep(app: &TestApp, operation: &Operation) -> Vec<String> {
    let variables = web::body::template_variables(&operation.path);
    let mut uri = operation.path.clone();
    for variable in &variables {
        uri = uri.replace(&format!("{{{variable}}}"), &platform::ids::new_id().to_string());
    }
    let with = |name: &str, value: Value| {
        let mut members = operation.members.clone();
        members.insert(name.to_owned(), value);
        Value::Object(members).to_string()
    };
    // The schema's members, sorted: what the refusal of an undeclared member must list.
    let allowed: Vec<&String> = operation.members.keys().collect();
    let mut cases = vec![Case {
        label: "an undeclared member".to_owned(),
        body: with("undeclaredMember", Value::from(SENTINEL)),
        entry: Some(("/undeclaredMember".to_owned(), "validation.unknown-field")),
        params: Some(serde_json::json!({ "allowed": allowed })),
    }];
    for variable in &variables {
        cases.push(Case {
            label: format!("path variable {variable} in the body"),
            body: with(variable, Value::from(SENTINEL)),
            entry: Some((format!("/{variable}"), "validation.identifier-in-path")),
            params: None,
        });
    }
    // `{"member":<first>,<first again, when given,><every other member>}`: a Value cannot hold a repetition.
    let raw = |name: &str, first: &str, again: Option<&str>| {
        let mut others = operation.members.clone();
        others.remove(name);
        let key = Value::from(name);
        let again = again.map(|value| format!(",{key}:{value}")).unwrap_or_default();
        let rest = Value::Object(others).to_string();
        let rest = rest.strip_prefix('{').unwrap().strip_suffix('}').unwrap();
        let rest = if rest.is_empty() { String::new() } else { format!(",{rest}") };
        format!("{{{key}:{first}{again}{rest}}}")
    };
    for member in operation.members.keys() {
        cases.push(Case {
            label: format!("member {member} as an array"),
            body: with(member, Value::Array(vec![Value::from(SENTINEL)])),
            entry: Some((format!("/{member}"), "validation.wrong-type")),
            params: None,
        });
        let valid = operation.members[member].to_string();
        cases.push(Case {
            label: format!("member {member} twice"),
            body: raw(member, &valid, Some(&Value::from(SENTINEL).to_string())),
            entry: Some((format!("/{member}"), "validation.duplicate-member")),
            params: None,
        });
        cases.push(Case {
            label: format!("member {member} as an object repeating a member"),
            body: raw(member, &format!(r#"{{"repeated":"{SENTINEL}","repeated":"{SENTINEL}"}}"#), None),
            entry: Some((format!("/{member}/repeated"), "validation.duplicate-member")),
            params: None,
        });
    }
    cases.push(Case {
        label: "broken JSON".to_owned(),
        body: format!(r#"{{"{SENTINEL}":"#),
        entry: None,
        params: None,
    });
    cases.push(Case { label: "a missing body".to_owned(), body: String::new(), entry: None, params: None });

    let mut failures = Vec::new();
    for Case { label: case, body, entry: expected_entry, params } in cases {
        let before = app.tx.begun();
        let reply = send(&app.router, json_request(operation.method.clone(), &uri, &body)).await;
        let label = format!("{} {}: {case}", operation.method, operation.path);
        let expected_code = if expected_entry.is_some() { "validation.failed" } else { "validation.malformed-body" };
        if reply.status != 400 || reply.json["code"] != expected_code {
            failures.push(format!("{label}: answered {} {}", reply.status, reply.text));
            continue;
        }
        if let Some((pointer, code)) = expected_entry {
            let found = reply.json["errors"].as_array().into_iter().flatten().any(|e| {
                e["pointer"] == pointer.as_str()
                    && e["code"] == code
                    && params.as_ref().is_none_or(|p| e["params"] == *p)
            });
            if !found {
                failures.push(format!("{label}: no {code} at {pointer} with params {params:?} in {}", reply.text));
            }
        }
        if app.tx.begun() != before {
            failures.push(format!("{label}: opened a transaction"));
        }
        if reply.text.contains(SENTINEL) {
            failures.push(format!("{label}: echoed the value sent"));
        }
    }
    failures
}

#[sqlx::test(migrator = "db::MIGRATOR")]
async fn every_body_taking_operation_refuses_strictly_and_the_lenient_probe_is_caught(pool: PgPool) {
    let app = app_with_probes(pool);
    let (log, _guard) = capture_log();
    let document = serde_json::to_value(&app.document).unwrap();
    let mut reported = BTreeSet::new();
    let mut unexpected = Vec::new();
    for operation in operations(&document) {
        let failures = sweep(&app, &operation).await;
        if operation.path == "/test/lenient" {
            if !failures.is_empty() {
                reported.insert(operation.path.clone());
            }
        } else {
            unexpected.extend(failures);
        }
    }
    assert!(unexpected.is_empty(), "{unexpected:#?}");
    assert_eq!(reported, BTreeSet::from(["/test/lenient".to_owned()]), "the negative control was not caught");
    assert!(!log.text().contains(SENTINEL), "a sentinel value reached the log");
}

/// The sweep reaches every body-taking operation the committed document declares: the served operations and
/// the documented ones are the same set, since both come from the one router.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn the_swept_operations_are_the_documented_ones(pool: PgPool) {
    let served: BTreeSet<String> = operations(&serde_json::to_value(&app(pool).document).unwrap())
        .into_iter()
        .map(|o| format!("{} {}", o.method, o.path))
        .collect();
    let documented =
        tokio::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../openapi/v1.json")).await.unwrap();
    let documented: BTreeSet<String> =
        operations(&documented.parse().unwrap()).into_iter().map(|o| format!("{} {}", o.method, o.path)).collect();
    assert!(!served.is_empty());
    assert_eq!(served, documented);
}

/// The reader records at most 100 entries and counts the rest: one entry per undeclared member, each listing the
/// members allowed, would otherwise let a body buy a response many times its size.
#[sqlx::test(migrator = "db::MIGRATOR")]
async fn a_body_of_101_undeclared_members_answers_100_entries_and_counts_the_rest(pool: PgPool) {
    let app = app(pool);
    let undeclared: String = (0..101).map(|n| format!(r#","m{n:03}":0"#)).collect();
    let reply =
        send(&app.router, json_request(Method::POST, "/api/greetings", &format!(r#"{{"name":"Ada"{undeclared}}}"#)))
            .await;
    assert_eq!(reply.status, 400, "{}", reply.text);
    let errors = reply.json["errors"].as_array().unwrap();
    assert_eq!((errors.len(), &reply.json["errorsOmitted"]), (100, &Value::from(1)));
    let entry = serde_json::json!({ "pointer": "/m000", "code": "validation.unknown-field", "params": { "allowed": ["name"] } });
    assert_eq!(errors[0], entry);
    assert_eq!(app.tx.begun(), 0);
}
