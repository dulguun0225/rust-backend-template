//! serde and the schema agree, per type. utoipa ignores serde attributes it does not read — `serialize_with`,
//! `with`, `transparent`, `into`/`from`, `alias` among them — while they still change the JSON, so every
//! component of the document has a sample here: the sample's JSON must validate against its schema, and a
//! request type's JSON must bind back, through the service's own reader, to the value it came from. A new
//! component with no sample fails `every_component_has_a_sample`.

use std::collections::BTreeSet;
use std::fmt::Debug;

use api::greeting::{CreateGreetingRequest, GreetingFieldCode, GreetingView};
use serde::Serialize;
use serde_json::{Value, json};
use time::macros::datetime;
use web::codes::ApiFieldCode;
use web::problem::{FieldError, FieldParam, Location, Problem};

struct Sample {
    name: &'static str,
    json: Value,
    binds_back: Option<bool>,
}

fn response<T: Serialize + utoipa::ToSchema>(value: &T) -> Sample {
    Sample { name: leak(T::name().into_owned()), json: serde_json::to_value(value).unwrap(), binds_back: None }
}

fn request<T>(value: &T) -> Sample
where
    T: Serialize + serde::de::DeserializeOwned + utoipa::ToSchema + PartialEq + Clone + Debug,
{
    let json = serde_json::to_value(value).unwrap();
    let members = web::body::read_members(json.to_string().as_bytes()).unwrap();
    let back = web::body::bind::<T>(members, &BTreeSet::new()).validate(|t, _| Some(t.clone()));
    Sample { name: leak(T::name().into_owned()), json, binds_back: Some(back.is_ok_and(|b| &b == value)) }
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn samples() -> Vec<Sample> {
    vec![
        request(&CreateGreetingRequest { name: "Ada".to_owned() }),
        response(&GreetingView {
            id: platform::ids::new_id(),
            name: "Ada".to_owned(),
            message: "Hello, Ada!".to_owned(),
            created_at: datetime!(2026-09-29 10:00:00.123456 UTC),
        }),
        response(&Problem {
            kind: "about:blank".to_owned(),
            title: "Bad Request".to_owned(),
            status: 400,
            code: "validation.failed".to_owned(),
            params: [("max".to_owned(), FieldParam::Integer(65_536))].into(),
            detail: Some("detail".to_owned()),
            errors: Some(vec![
                FieldError::new("/name", GreetingFieldCode::Required),
                FieldError::new("/name", GreetingFieldCode::TooLong { max: 100 }),
                FieldError::at(Location::Query, "q", ApiFieldCode::UnknownField { allowed: &["a", "b"] }),
            ]),
            errors_omitted: Some(1),
            incident_id: Some(platform::ids::new_id().to_string()),
        }),
        response(&FieldError::new("/a", ApiFieldCode::WrongType { expected: "string" }).with_detail("expected string")),
        response(&FieldError::at(Location::Header, "X-Id", ApiFieldCode::InvalidValue { expected: "uuid" })),
    ]
}

fn validator(document: &Value, name: &str) -> jsonschema::Validator {
    let schema = json!({ "$ref": format!("#/components/schemas/{name}"), "components": document["components"] });
    jsonschema::draft202012::new(&schema).unwrap()
}

#[test]
fn every_component_has_a_sample() {
    let document = serde_json::to_value(api::openapi()).unwrap();
    let components: BTreeSet<&str> =
        document["components"]["schemas"].as_object().unwrap().keys().map(String::as_str).collect();
    let sampled: BTreeSet<&str> = samples().iter().map(|s| s.name).collect();
    assert_eq!(components, sampled);
}

#[test]
fn every_sample_validates_against_its_schema_and_every_request_binds_back() {
    let document = serde_json::to_value(api::openapi()).unwrap();
    for sample in samples() {
        let errors: Vec<String> =
            validator(&document, sample.name).iter_errors(&sample.json).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{}: {} does not match its schema: {errors:?}", sample.name, sample.json);
        assert_ne!(sample.binds_back, Some(false), "{}: {} does not bind back to its value", sample.name, sample.json);
    }
}

/// The entry schema is one of a body entry and a parameter entry, never both: an entry naming a pointer and a
/// location together, or neither, does not validate.
#[test]
fn an_entry_names_a_pointer_or_a_location_and_name_never_both() {
    let document = serde_json::to_value(api::openapi()).unwrap();
    let entry = validator(&document, "FieldError");
    assert!(entry.is_valid(&json!({ "pointer": "/a", "code": "validation.required" })));
    assert!(entry.is_valid(&json!({ "in": "query", "name": "a", "code": "validation.required" })));
    assert!(!entry.is_valid(&json!({ "pointer": "/a", "in": "query", "name": "a", "code": "validation.required" })));
    assert!(!entry.is_valid(&json!({ "code": "validation.required" })));
    assert!(!entry.is_valid(&json!({ "in": "body", "name": "a", "code": "validation.required" })));
}

/// The negative control: a serde attribute utoipa ignores changes the JSON and the schema does not say so.
#[test]
fn a_serde_attribute_the_schema_ignores_is_caught() {
    #[derive(Serialize, utoipa::ToSchema)]
    struct Divergent {
        #[serde(serialize_with = "as_number")]
        amount: String,
    }
    fn as_number<S: serde::Serializer>(value: &str, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(value.len().try_into().unwrap_or(0))
    }
    let document = json!({ "components": { "schemas": { "Divergent": serde_json::to_value(<Divergent as utoipa::PartialSchema>::schema()).unwrap() } } });
    let sample = response(&Divergent { amount: "12.30".to_owned() });
    assert!(!validator(&document, "Divergent").is_valid(&sample.json), "the divergence went unseen");
}
