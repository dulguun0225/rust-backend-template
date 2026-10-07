//! The committed OpenAPI document (`openapi/v1.json`): generated here, diffed here, and held to the
//! request-body contract and to the refusals each operation declares: 400 on every one, since any operation
//! refuses an undeclared query parameter, and 413 where it takes a body. The wall reruns this file under another timezone and locale, lints the document
//! with vacuum, and compares it with the base branch's copy with oasdiff.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

const DOCUMENT: &str = "openapi/v1.json";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn generated() -> Value {
    serde_json::to_value(api::openapi()).unwrap()
}

#[tokio::test]
async fn the_document_matches_its_committed_copy() {
    let expected = format!("{}\n", serde_json::to_string_pretty(&generated()).unwrap());
    let committed = tokio::fs::read_to_string(root().join(DOCUMENT)).await.unwrap_or_default();
    if committed != expected {
        let actual = root().join("target").join(DOCUMENT);
        tokio::fs::create_dir_all(actual.parent().unwrap()).await.unwrap();
        tokio::fs::write(&actual, &expected).await.unwrap();
        panic!("the OpenAPI document changed: review {} and copy it to {DOCUMENT}", actual.display());
    }
}

/// Every operation with a request body, as (operation id, path template, request schema name).
fn body_operations(document: &Value) -> Vec<(String, String, Option<String>)> {
    let mut out = Vec::new();
    for (path, item) in document["paths"].as_object().into_iter().flatten() {
        for (_, operation) in item.as_object().into_iter().flatten() {
            let Some(body) = operation.get("requestBody") else { continue };
            let reference = body["content"]["application/json"]["schema"]["$ref"].as_str();
            let schema = reference.and_then(|r| r.strip_prefix("#/components/schemas/")).map(str::to_owned);
            out.push((operation["operationId"].as_str().unwrap_or(path).to_owned(), path.clone(), schema));
        }
    }
    out
}

/// Each check returns the operation ids that break it.
fn request_schemas_are_named_components(document: &Value) -> BTreeSet<String> {
    body_operations(document).into_iter().filter(|(_, _, schema)| schema.is_none()).map(|(id, _, _)| id).collect()
}

fn request_schemas_are_closed(document: &Value) -> BTreeSet<String> {
    body_operations(document)
        .into_iter()
        .filter(|(_, _, schema)| {
            schema
                .as_ref()
                .is_some_and(|s| document["components"]["schemas"][s]["additionalProperties"] != Value::Bool(false))
        })
        .map(|(id, _, _)| id)
        .collect()
}

fn members_are_scalars(document: &Value) -> BTreeSet<String> {
    let scalar =
        |p: &Value| p["type"].as_str().is_some_and(|t| ["string", "integer", "number", "boolean"].contains(&t));
    body_operations(document)
        .into_iter()
        .filter(|(_, _, schema)| {
            schema.as_ref().is_some_and(|s| {
                document["components"]["schemas"][s]["properties"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .any(|(_, p)| !scalar(p))
            })
        })
        .map(|(id, _, _)| id)
        .collect()
}

fn no_member_names_a_path_variable(document: &Value) -> BTreeSet<String> {
    body_operations(document)
        .into_iter()
        .filter(|(_, path, schema)| {
            let variables = web::body::template_variables(path);
            schema.as_ref().is_some_and(|s| {
                document["components"]["schemas"][s]["properties"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .any(|(name, _)| variables.contains(name))
            })
        })
        .map(|(id, _, _)| id)
        .collect()
}

fn no_request_schema_serves_two_operations(document: &Value) -> BTreeSet<String> {
    let mut by_schema: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, _, schema) in body_operations(document) {
        if let Some(schema) = schema {
            by_schema.entry(schema).or_default().push(id);
        }
    }
    by_schema.into_values().filter(|ids| ids.len() > 1).flatten().collect()
}

/// Every operation, as (operation id, operation).
fn all_operations(document: &Value) -> Vec<(String, &Value)> {
    let mut out = Vec::new();
    for (path, item) in document["paths"].as_object().into_iter().flatten() {
        for (_, operation) in item.as_object().into_iter().flatten() {
            out.push((operation["operationId"].as_str().unwrap_or(path).to_owned(), operation));
        }
    }
    out
}

fn has_parameters(operation: &Value) -> bool {
    operation["parameters"].as_array().is_some_and(|p| !p.is_empty())
}

fn every_operation_declares_400(document: &Value) -> BTreeSet<String> {
    all_operations(document)
        .into_iter()
        .filter(|(_, operation)| operation["responses"].get("400").is_none())
        .map(|(id, _)| id)
        .collect()
}

fn body_operations_declare_413(document: &Value) -> BTreeSet<String> {
    all_operations(document)
        .into_iter()
        .filter(|(_, operation)| operation.get("requestBody").is_some() && operation["responses"].get("413").is_none())
        .map(|(id, _)| id)
        .collect()
}

type Check = fn(&Value) -> BTreeSet<String>;

const CHECKS: [(&str, Check); 7] = [
    ("request_schemas_are_named_components", request_schemas_are_named_components),
    ("request_schemas_are_closed", request_schemas_are_closed),
    ("members_are_scalars", members_are_scalars),
    ("no_member_names_a_path_variable", no_member_names_a_path_variable),
    ("no_request_schema_serves_two_operations", no_request_schema_serves_two_operations),
    ("every_operation_declares_400", every_operation_declares_400),
    ("body_operations_declare_413", body_operations_declare_413),
];

#[test]
fn every_request_body_meets_the_contract() {
    let document = generated();
    assert!(!body_operations(&document).is_empty(), "no body-taking operation: the checks below would pass vacuously");
    assert!(
        all_operations(&document).iter().any(|(_, operation)| has_parameters(operation)),
        "no operation with a parameter: the path and query sweeps would pass vacuously"
    );
    for (name, check) in CHECKS {
        assert!(check(&document).is_empty(), "{name}: {:?}", check(&document));
    }
}

/// The negative control: each fixture operation breaks exactly one check, and each check reports exactly the
/// operations that break it.
#[test]
fn each_check_reports_exactly_the_fixture_operations_that_break_it() {
    let op = |id: &str, schema: Value| json!({ "operationId": id, "requestBody": { "content": { "application/json": { "schema": schema } } }, "responses": { "400": {}, "413": {} } });
    let reference = |name: &str| json!({ "$ref": format!("#/components/schemas/{name}") });
    let document = json!({
        "paths": {
            "/inline": { "post": op("inline", json!({ "type": "object" })) },
            "/open": { "post": op("open", reference("Open")) },
            "/nested": { "post": op("nested", reference("Nested")) },
            "/things/{thingId}": { "put": op("pathMember", reference("PathMember")) },
            "/shared/a": { "post": op("sharedA", reference("Shared")) },
            "/shared/b": { "post": op("sharedB", reference("Shared")) },
            "/fine": { "post": op("fine", reference("Fine")) },
            "/fine/{fineId}": { "get": { "operationId": "fineParameter", "parameters": [{ "in": "path", "name": "fineId" }], "responses": { "400": {} } } },
            "/no400": { "get": { "operationId": "no400", "responses": { "200": {} } } },
            "/no413": { "post": { "operationId": "no413", "requestBody": { "content": { "application/json": { "schema": reference("Small") } } }, "responses": { "400": {} } } },
        },
        "components": { "schemas": {
            "Open": { "type": "object", "properties": { "a": { "type": "string" } } },
            "Nested": { "type": "object", "additionalProperties": false, "properties": { "a": { "type": "object" } } },
            "PathMember": { "type": "object", "additionalProperties": false, "properties": { "thingId": { "type": "string" } } },
            "Shared": { "type": "object", "additionalProperties": false, "properties": { "a": { "type": "string" } } },
            "Fine": { "type": "object", "additionalProperties": false, "properties": { "a": { "type": "integer" } } },
            "Small": { "type": "object", "additionalProperties": false, "properties": { "a": { "type": "boolean" } } },
        } }
    });
    let expected: BTreeMap<&str, Vec<&str>> = BTreeMap::from([
        ("request_schemas_are_named_components", vec!["inline"]),
        ("request_schemas_are_closed", vec!["open"]),
        ("members_are_scalars", vec!["nested"]),
        ("no_member_names_a_path_variable", vec!["pathMember"]),
        ("no_request_schema_serves_two_operations", vec!["sharedA", "sharedB"]),
        ("every_operation_declares_400", vec!["no400"]),
        ("body_operations_declare_413", vec!["no413"]),
    ]);
    for (name, check) in CHECKS {
        let got: Vec<String> = check(&document).into_iter().collect();
        assert_eq!(got, expected[name], "{name}");
    }
}
