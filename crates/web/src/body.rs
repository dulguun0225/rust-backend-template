//! The one request-body reader. A handler takes `StrictJson<T>`; `clippy.toml` bans `axum::Json`,
//! `axum::Form`, `axum::body::Bytes` and the `serde_json::from_*` functions everywhere but this crate.
//!
//! The reader collects, in one pass over the body's top-level members, every failure the body has:
//!
//! - a member `T` does not declare: `validation.unknown-field`;
//! - a member named after one of the matched route's path variables, whatever its value:
//!   `validation.identifier-in-path` (an identifier travels in the path only);
//! - a member whose JSON type is not the declared one: `validation.wrong-type`, param `expected` and `detail`
//!   `expected <type>`;
//! - a member that appears twice in one object, at any depth: `validation.duplicate-member` at the pointer of
//!   the repeated member, so no value is ever picked from two;
//! - a required member that is absent: `validation.required`.
//!
//! The declared members, their JSON types and the required list are read from `T`'s generated schema — the
//! schema the committed OpenAPI document publishes — so the contract and the reader cannot disagree. Then
//! `T` is deserialized from the members that passed, and a value the type still refuses (a malformed UUID)
//! is `validation.invalid-value` at its pointer. The value reaches the handler only through
//! [`Bound::validate`], which merges these failures with the feature's own field rules into one
//! `validation.failed`: binding failures first, sorted by pointer, then rule failures at pointers that hold
//! none yet. A body that is missing, is not JSON, or is not an object is `validation.malformed-body`; a body
//! over [`BODY_LIMIT`] is `request.too-large`; a body that is not `application/json` is
//! `request.unsupported-media-type`. No detail is ever built from the value sent.

use std::collections::BTreeSet;
use std::fmt;

use axum::extract::{FromRequest, MatchedPath, Request};
use axum::http::header;
use serde::Deserialize;
use serde::de::{DeserializeOwned, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};
use utoipa::PartialSchema;

use crate::codes::{ApiErrorCode, ApiFieldCode};
use crate::problem::{ApiError, FieldError, pointer};

/// The largest request body read, in bytes. `edge::finish` applies the same limit to the declared length.
pub const BODY_LIMIT: usize = 65_536;

/// A request body read strictly. Destructure it and call [`Bound::validate`] before any transaction.
#[derive(Debug)]
pub struct StrictJson<T>(pub Bound<T>);

/// A body that has been read: the value, when the members allowed one to be built, and the binding failures.
#[derive(Debug)]
pub struct Bound<T> {
    value: Option<T>,
    failures: Vec<FieldError>,
}

impl<T> Bound<T> {
    /// The value, once the binding failures and the feature's rules have both passed. `rules` appends a
    /// [`FieldError`] per failed check and returns the validated value, or `None` when any check failed.
    ///
    /// # Errors
    /// One `validation.failed` holding every binding failure and every rule failure, or an internal error
    /// when `rules` returns no value and records no failure.
    pub fn validate<V>(self, rules: impl FnOnce(&T, &mut Vec<FieldError>) -> Option<V>) -> Result<V, ApiError> {
        let mut failures = self.failures;
        failures.sort();
        let Some(value) = self.value else {
            return if failures.is_empty() {
                Err(ApiError::internal("a body with no value and no failure"))
            } else {
                Err(ApiError::Invalid(failures))
            };
        };
        let mut rule_failures = Vec::new();
        let produced = rules(&value, &mut rule_failures);
        let taken: BTreeSet<String> = failures.iter().map(|f| f.pointer.clone()).collect();
        failures.extend(rule_failures.into_iter().filter(|f| !taken.contains(&f.pointer)));
        match produced {
            Some(validated) if failures.is_empty() => Ok(validated),
            Some(_) | None if !failures.is_empty() => Err(ApiError::Invalid(failures)),
            Some(_) | None => Err(ApiError::internal("field rules returned no value and recorded no failure")),
        }
    }
}

impl<S, T> FromRequest<S> for StrictJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + PartialSchema,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let path_variables =
            req.extensions().get::<MatchedPath>().map(|m| template_variables(m.as_str())).unwrap_or_default();
        let is_json =
            req.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(is_json_media_type);
        let bytes = axum::body::to_bytes(req.into_body(), BODY_LIMIT).await.map_err(|e| {
            if is_length_limit(&e) { ApiError::rejected(ApiErrorCode::PayloadTooLarge) } else { ApiError::internal(e) }
        })?;
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Err(ApiError::Malformed("the body is missing".to_owned()));
        }
        if !is_json {
            return Err(ApiError::rejected(ApiErrorCode::UnsupportedMediaType));
        }
        let members = read_members(&bytes)?;
        Ok(Self(bind::<T>(members, &path_variables)))
    }
}

/// Binds the members against `T`'s schema. Public for the reader's own tests and for a test sweeping every
/// request type; handlers never call it.
#[must_use]
pub fn bind<T: DeserializeOwned + PartialSchema>(members: Members, path_variables: &BTreeSet<String>) -> Bound<T> {
    let Members { members, nested_duplicates } = members;
    let schema = serde_json::to_value(T::schema()).unwrap_or(Value::Null);
    let properties = schema.get("properties").and_then(Value::as_object).cloned().unwrap_or_default();
    let required: BTreeSet<String> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).map(str::to_owned).collect())
        .unwrap_or_default();

    let mut failures: Vec<FieldError> =
        nested_duplicates.into_iter().map(|at| FieldError::new(at, ApiFieldCode::DuplicateMember)).collect();
    let mut accepted = Map::new();
    let mut seen = BTreeSet::new();
    for (name, value) in members {
        let at = pointer(&name);
        if !seen.insert(name.clone()) {
            failures.push(FieldError::new(at, ApiFieldCode::DuplicateMember));
            accepted.remove(&name);
            continue;
        }
        if path_variables.contains(&name) {
            failures.push(FieldError::new(at, ApiFieldCode::IdentifierInPath));
            continue;
        }
        let Some(declared) = properties.get(&name) else {
            failures.push(FieldError::new(at, ApiFieldCode::UnknownField));
            continue;
        };
        match type_mismatch(declared, &value) {
            Some(expected) => failures
                .push(FieldError::new(at, ApiFieldCode::WrongType { expected }).with_detail(expected_detail(expected))),
            None => {
                accepted.insert(name, value);
            }
        }
    }
    let failed: BTreeSet<&str> = failures.iter().map(|f| f.pointer.as_str()).collect();
    let mut missing = Vec::new();
    for name in &required {
        let at = pointer(name);
        if !seen.contains(name) && !failed.contains(at.as_str()) {
            missing.push(FieldError::new(at, ApiFieldCode::Required));
        }
    }
    failures.extend(missing);
    if required.iter().any(|name| !accepted.contains_key(name)) {
        return Bound { value: None, failures };
    }
    match serde_path_to_error::deserialize::<_, T>(Value::Object(accepted)) {
        Ok(value) => Bound { value: Some(value), failures },
        Err(e) => {
            // An Option member marked `deserialize_with = "Deserialize::deserialize"` is not in the schema's
            // required list, so its absence surfaces here, as serde's `missing field` error at the root.
            let missing = e
                .inner()
                .to_string()
                .strip_prefix("missing field `")
                .and_then(|rest| rest.split('`').next().map(str::to_owned));
            let failure = match missing {
                Some(field) => FieldError::new(pointer(&field), ApiFieldCode::Required),
                None => FieldError::new(
                    e.path().iter().next().map_or_else(|| "/".to_owned(), |segment| pointer(&segment.to_string())),
                    ApiFieldCode::InvalidValue,
                ),
            };
            // A member already refused above is absent from what was deserialized; its failure stands alone.
            if !failures.iter().any(|f| f.pointer == failure.pointer) {
                failures.push(failure);
            }
            Bound { value: None, failures }
        }
    }
}

/// The JSON type a declared member expects, when `value` is not of it.
fn type_mismatch(declared: &Value, value: &Value) -> Option<&'static str> {
    let types: Vec<&str> = match declared.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
        _ => return None,
    };
    let actual = match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    let fits = types.iter().any(|t| *t == actual || (*t == "number" && actual == "integer"));
    if fits {
        return None;
    }
    types
        .iter()
        .find(|t| **t != "null")
        .map(|t| JSON_TYPES.into_iter().find(|name| name == t).unwrap_or("another type"))
}

/// The JSON types a request member is declared as, null aside: a member is a scalar
/// (crates/api/tests/api/openapi.rs), so no other type is ever expected.
const JSON_TYPES: [&str; 4] = ["string", "boolean", "integer", "number"];

fn expected_detail(json_type: &str) -> &'static str {
    match json_type {
        "string" => "expected string",
        "boolean" => "expected boolean",
        "integer" => "expected integer",
        "number" => "expected number",
        _ => "expected another type",
    }
}

/// A body's top-level members, in order, duplicates kept, and the pointer of every member repeated inside a
/// member's value. `serde_json::Value` keeps one value of a repeated member silently, so the reader records the
/// repetition while it reads, before any value is built.
#[derive(Debug)]
pub struct Members {
    members: Vec<(String, Value)>,
    nested_duplicates: BTreeSet<String>,
}

/// The body's members.
///
/// # Errors
/// `validation.malformed-body` naming where the JSON stopped being well formed, or that it is not an object.
pub fn read_members(bytes: &[u8]) -> Result<Members, ApiError> {
    match serde_json::from_slice::<Members>(bytes) {
        Ok(members) => Ok(members),
        Err(e) if e.classify() == serde_json::error::Category::Data => {
            Err(ApiError::Malformed("the body is not a JSON object".to_owned()))
        }
        Err(e) => Err(ApiError::Malformed(format!(
            "the body is not well-formed JSON: line {}, column {}",
            e.line(),
            e.column()
        ))),
    }
}

impl<'de> Deserialize<'de> for Members {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectMembers;

        impl<'de> Visitor<'de> for ObjectMembers {
            type Value = Members;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Members, A::Error> {
                let mut members = Vec::new();
                let mut nested_duplicates = BTreeSet::new();
                while let Some(name) = map.next_key::<String>()? {
                    let Read(value) =
                        map.next_value_seed(Tracked { at: pointer(&name), duplicates: &mut nested_duplicates })?;
                    members.push((name, value));
                }
                Ok(Members { members, nested_duplicates })
            }
        }

        deserializer.deserialize_map(ObjectMembers)
    }
}

/// Reads one JSON value at pointer `at`, recording the pointer of each member its objects repeat. Depth is
/// bounded by `serde_json`'s recursion limit, which it checks before every array and object.
struct Tracked<'a> {
    at: String,
    duplicates: &'a mut BTreeSet<String>,
}

/// A value [`Tracked`] read. It has no default, so no read can stand in for null by defaulting.
#[derive(Debug)]
struct Read(Value);

impl<'de> DeserializeSeed<'de> for Tracked<'_> {
    type Value = Read;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Read, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Tracked<'_> {
    type Value = Read;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Read, E> {
        Ok(Read(Value::Bool(v)))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Read, E> {
        Ok(Read(Value::from(v)))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Read, E> {
        Ok(Read(Value::from(v)))
    }

    fn visit_f64<E>(self, v: f64) -> Result<Read, E> {
        Ok(Read(Value::from(v)))
    }

    fn visit_str<E>(self, v: &str) -> Result<Read, E> {
        Ok(Read(Value::from(v)))
    }

    fn visit_unit<E>(self) -> Result<Read, E> {
        Ok(Read(Value::Null))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Read, A::Error> {
        let mut items = Vec::new();
        while let Some(Read(item)) =
            seq.next_element_seed(Tracked { at: format!("{}/{}", self.at, items.len()), duplicates: self.duplicates })?
        {
            items.push(item);
        }
        Ok(Read(Value::Array(items)))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Read, A::Error> {
        let mut object = Map::new();
        while let Some(name) = map.next_key::<String>()? {
            let at = format!("{}{}", self.at, pointer(&name));
            let Read(value) = map.next_value_seed(Tracked { at: at.clone(), duplicates: self.duplicates })?;
            if object.contains_key(&name) {
                self.duplicates.insert(at);
            } else {
                object.insert(name, value);
            }
        }
        Ok(Read(Value::Object(object)))
    }
}

/// The variable names of a route template: `/api/greetings/{id}` has `id`; `{*rest}` has `rest`.
#[must_use]
pub fn template_variables(template: &str) -> BTreeSet<String> {
    template
        .split('/')
        .filter_map(|segment| segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')))
        .map(|name| name.trim_start_matches('*').to_owned())
        .collect()
}

fn is_json_media_type(value: &str) -> bool {
    value.split(';').next().is_some_and(|essence| essence.trim().eq_ignore_ascii_case("application/json"))
}

fn is_length_limit(error: &axum::Error) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(e) = source {
        if e.is::<http_body_util::LengthLimitError>() {
            return true;
        }
        source = e.source();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{Bound, Tracked, bind, read_members, template_variables};
    use crate::codes::ApiFieldCode;
    use crate::problem::{ApiError, FieldError};
    use serde::{Deserialize, Serialize};
    use std::collections::BTreeSet;
    use utoipa::ToSchema;

    #[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
    #[serde(deny_unknown_fields, rename_all = "camelCase")]
    struct Probe {
        given_name: String,
        count: i64,
        ratio: f64,
        active: bool,
        id: uuid::Uuid,
        #[serde(deserialize_with = "Deserialize::deserialize")]
        nickname: Option<String>,
    }

    fn bound(json: &str, path: &[&str]) -> Bound<Probe> {
        let vars: BTreeSet<String> = path.iter().map(|s| (*s).to_owned()).collect();
        bind::<Probe>(read_members(json.as_bytes()).unwrap(), &vars)
    }

    fn entries(result: Result<(), ApiError>) -> Vec<(String, String, Option<String>)> {
        match result {
            Err(ApiError::Invalid(errors)) => errors.into_iter().map(|e| (e.pointer, e.code, e.detail)).collect(),
            other => panic!("expected validation.failed, got {other:?}"),
        }
    }

    const GOOD: &str = r#"{"givenName":"a","count":-1,"ratio":0.5,"active":true,"id":"0192f0c1-8b39-7cc4-9a41-6f8e62a4a1b2","nickname":null}"#;

    #[test]
    fn a_body_that_fits_binds() {
        let value =
            bound(GOOD, &[]).validate(|p, _| Some((p.given_name.clone(), p.count, p.nickname.clone()))).unwrap();
        assert_eq!(value, ("a".to_owned(), -1, None));
        let named = GOOD.replace(r#""nickname":null"#, r#""nickname":"b""#);
        assert_eq!(bound(&named, &[]).validate(|p, _| p.nickname.clone()).unwrap(), "b");
    }

    #[test]
    fn an_absent_option_member_is_required() {
        let absent = GOOD.replace(r#","nickname":null"#, "");
        let got = entries(bound(&absent, &[]).validate(|_, _| Some(())));
        assert_eq!(got, [("/nickname".into(), "validation.required".into(), None)]);
    }

    #[test]
    fn a_nullable_member_takes_null_or_its_type_and_nothing_else() {
        let wrong = GOOD.replace(r#""nickname":null"#, r#""nickname":5"#);
        let got = entries(bound(&wrong, &[]).validate(|_, _| Some(())));
        assert_eq!(got, [("/nickname".into(), "validation.wrong-type".into(), Some("expected string".into()))]);
        let not_bool = GOOD.replace(r#""active":true"#, r#""active":"yes""#);
        let got = entries(bound(&not_bool, &[]).validate(|_, _| Some(())));
        assert_eq!(got, [("/active".into(), "validation.wrong-type".into(), Some("expected boolean".into()))]);
    }

    #[test]
    fn every_binding_failure_is_collected_in_one_pass_sorted_by_pointer() {
        let body =
            r#"{"zeta":1,"givenName":7,"count":1.5,"ratio":"x","active":true,"slug":"s","a/b":0,"nickname":null}"#;
        let got = entries(bound(body, &["slug"]).validate(|_, _| Some(())));
        assert_eq!(
            got,
            [
                ("/a~1b".into(), "validation.unknown-field".into(), None),
                ("/count".into(), "validation.wrong-type".into(), Some("expected integer".into())),
                ("/givenName".into(), "validation.wrong-type".into(), Some("expected string".into())),
                ("/id".into(), "validation.required".into(), None),
                ("/ratio".into(), "validation.wrong-type".into(), Some("expected number".into())),
                ("/slug".into(), "validation.identifier-in-path".into(), None),
                ("/zeta".into(), "validation.unknown-field".into(), None),
            ]
        );
    }

    #[test]
    fn a_duplicate_member_and_a_malformed_uuid_are_refused() {
        let dup = r#"{"givenName":"a","givenName":"b","count":1,"ratio":1,"active":true,"id":"0192f0c1-8b39-7cc4-9a41-6f8e62a4a1b2","nickname":null}"#;
        let got = entries(bound(dup, &[]).validate(|_, _| Some(())));
        assert_eq!(got, [("/givenName".into(), "validation.duplicate-member".into(), None)]);
        let bad_id = r#"{"givenName":"a","count":1,"ratio":1,"active":true,"id":"not-a-uuid","nickname":null}"#;
        let got = entries(bound(bad_id, &[]).validate(|_, _| Some(())));
        assert_eq!(got, [("/id".into(), "validation.invalid-value".into(), None)]);
    }

    #[test]
    fn a_duplicate_member_at_any_depth_is_refused_where_it_is() {
        let body = r#"{"givenName":{"a":1,"b":[{"c":1,"c":2},{"d":1,"d":2}],"a":2},"count":1,"ratio":1,"active":true,"id":"0192f0c1-8b39-7cc4-9a41-6f8e62a4a1b2","nickname":null,"nickname":null}"#;
        let got = entries(bound(body, &[]).validate(|_, _| Some(())));
        assert_eq!(
            got,
            [
                ("/givenName".into(), "validation.wrong-type".into(), Some("expected string".into())),
                ("/givenName/a".into(), "validation.duplicate-member".into(), None),
                ("/givenName/b/0/c".into(), "validation.duplicate-member".into(), None),
                ("/givenName/b/1/d".into(), "validation.duplicate-member".into(), None),
                ("/nickname".into(), "validation.duplicate-member".into(), None),
            ]
        );
    }

    #[test]
    fn the_reader_keeps_every_value_as_sent() {
        let body =
            r#"{"a":[null,true,false,-7,18446744073709551615,0.25,"s\u00e9",{"b":{"c":[]}}],"d":{},"e":"plain"}"#;
        let members = read_members(body.as_bytes()).unwrap();
        let expected: serde_json::Value = body.parse().unwrap();
        let expected: Vec<_> = expected.as_object().unwrap().clone().into_iter().collect();
        assert_eq!(members.members, expected);
        assert!(members.nested_duplicates.is_empty());
    }

    #[test]
    fn the_value_reader_names_what_it_expects() {
        use serde::de::DeserializeSeed;
        let mut duplicates = BTreeSet::new();
        let refused = Tracked { at: "/a".to_owned(), duplicates: &mut duplicates }
            .deserialize(serde::de::value::BytesDeserializer::<serde::de::value::Error>::new(b"x"))
            .unwrap_err();
        assert_eq!(refused.to_string(), "invalid type: byte array, expected a JSON value");
    }

    #[test]
    fn a_field_error_carries_its_code_params_by_name() {
        let wrong = serde_json::to_value(
            FieldError::new("/a", ApiFieldCode::WrongType { expected: "string" }).with_detail("expected string"),
        )
        .unwrap();
        assert_eq!(
            wrong,
            serde_json::json!({
                "pointer": "/a", "code": "validation.wrong-type", "detail": "expected string",
                "params": { "expected": "string" },
            })
        );
        let bare = serde_json::to_value(FieldError::new("/a", ApiFieldCode::Required)).unwrap();
        assert_eq!(bare, serde_json::json!({ "pointer": "/a", "code": "validation.required" }));
    }

    #[test]
    fn rule_failures_follow_binding_failures_except_at_a_taken_pointer() {
        let body = r#"{"givenName":"a","count":1,"ratio":1,"active":true,"id":"0192f0c1-8b39-7cc4-9a41-6f8e62a4a1b2","nickname":null,"extra":1}"#;
        let got = entries(bound(body, &[]).validate(|_, errors| {
            errors.push(FieldError::new("/extra", ApiFieldCode::Required));
            errors.push(FieldError::new("/givenName", ApiFieldCode::Required));
            None::<()>
        }));
        assert_eq!(
            got,
            [
                ("/extra".into(), "validation.unknown-field".into(), None),
                ("/givenName".into(), "validation.required".into(), None)
            ]
        );
    }

    #[test]
    fn a_body_that_is_not_an_object_is_malformed_and_says_where() {
        for (body, detail) in [
            ("[1]", "the body is not a JSON object"),
            ("\"x\"", "the body is not a JSON object"),
            ("{\"a\":", "the body is not well-formed JSON: line 1, column 5"),
            ("{\"a\" 1}", "the body is not well-formed JSON: line 1, column 6"),
        ] {
            match read_members(body.as_bytes()) {
                Err(ApiError::Malformed(d)) => assert_eq!(d, detail, "{body}"),
                other => panic!("{body}: {other:?}"),
            }
        }
    }

    #[test]
    fn rules_that_produce_nothing_and_record_nothing_are_an_internal_error() {
        assert!(matches!(bound(GOOD, &[]).validate(|_, _| None::<()>), Err(ApiError::Internal(_))));
    }

    #[test]
    fn template_variables_are_read_from_braces() {
        let vars = template_variables("/api/{tenant}/greetings/{id}/{*rest}");
        assert_eq!(vars.into_iter().collect::<Vec<_>>(), ["id", "rest", "tenant"]);
    }
}
