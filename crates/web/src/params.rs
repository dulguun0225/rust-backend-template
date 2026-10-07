//! The strict readers of every input outside the body: [`StrictPath`], [`StrictQuery`] and [`StrictHeaders`].
//! `clippy.toml` bans `axum::extract::Path`, `Query`, `RawQuery` and `RawPathParams` everywhere but here, and
//! `Query` and `RawQuery` here too: the query is decoded here, since `Query` replaces text that is not UTF-8 once
//! decoded rather than refusing it.
//!
//! Each reads a type `T` that derives `Deserialize` and utoipa's `IntoParams`, the same type the operation's
//! `#[utoipa::path(params(T))]` documents, so the reader and the committed document read one declaration. Each
//! input is read as text against its declared schema, and every failure is an entry of one `validation.failed`
//! naming the input by `in` and the name the document declares:
//!
//! - a required input whose name is not in the request: `validation.required`;
//! - text that does not parse as the declared type or format, empty and blank included, a UUID in any form
//!   but its 36-character one, or text that is not UTF-8 (a header that is not visible ASCII):
//!   `validation.invalid-value`, param `expected`, the schema's `format`, else its `type`;
//! - a value outside a declared enumeration: `validation.unknown-value`, param `allowed`;
//! - a query parameter, or a header, given twice: `validation.duplicate-member` (every parameter here is
//!   single-valued);
//! - a query parameter the operation does not declare: `validation.unknown-field`, param `allowed`, the
//!   declared query parameters. Every route takes a `StrictQuery`, [`NoQuery`] when it declares none, so an
//!   undeclared parameter is refused on every route.
//!
//! An undeclared header is never refused: RFC 9110 §5.1 has a recipient ignore a header it does not recognise,
//! and intermediaries add them. `Accept`, `Content-Type` and `Authorization` are never declared as parameters
//! (OpenAPI ignores them as such) and keep their own refusals. Nothing is trimmed and no value is chosen from
//! two. At most 100 entries are recorded; the rest are counted in `errorsOmitted`.

use std::collections::BTreeSet;

use axum::extract::{FromRequestParts, RawPathParams};
use axum::http::request::Parts;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use utoipa::IntoParams;
use utoipa::openapi::path::ParameterIn;

use crate::codes::ApiFieldCode;
use crate::declared;
use crate::problem::{ApiError, Failures, FieldError, Location};
use crate::scalar;

/// The path variables `T` declares, read strictly.
#[derive(Debug)]
pub struct StrictPath<T>(pub T);

/// The query parameters `T` declares, read strictly; any other query parameter is refused.
#[derive(Debug)]
pub struct StrictQuery<T>(pub T);

/// The headers `T` declares, read strictly; any other header is ignored.
#[derive(Debug)]
pub struct StrictHeaders<T>(pub T);

/// The query of an operation that declares no query parameter: `StrictQuery<NoQuery>` refuses every one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
pub struct NoQuery {}

impl<S: Send + Sync, T: DeserializeOwned + IntoParams> FromRequestParts<S> for StrictPath<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let sent: Vec<(String, Option<String>)> = match RawPathParams::from_request_parts(parts, state).await {
            Ok(params) => params.iter().map(|(name, text)| (name.to_owned(), Some(text.to_owned()))).collect(),
            // axum names a variable whose text is not UTF-8 only in its message, "Invalid UTF-8 in `id`"; a
            // rejection that names no declared variable is an internal error.
            Err(rejection) => {
                let message = rejection.body_text();
                let name = message.strip_prefix("Invalid UTF-8 in `").and_then(|rest| rest.strip_suffix('`'));
                let declared = declared_params::<T>(Location::Path)?;
                return match declared.iter().find(|parameter| Some(parameter.name.as_str()) == name) {
                    Some(parameter) => Err(ApiError::invalid(invalid_value(Location::Path, parameter))),
                    None => Err(ApiError::internal(message)),
                };
            }
        };
        read(Location::Path, &sent, false).map(Self)
    }
}

impl<S: Send + Sync, T: DeserializeOwned + IntoParams> FromRequestParts<S> for StrictQuery<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        read(Location::Query, &query_pairs(parts.uri.query().unwrap_or_default()), true).map(Self)
    }
}

/// The query's parameters in the order sent, read as `application/x-www-form-urlencoded`: split on `&` and the
/// first `=`, `+` read as a space, `%` and two hexadecimal digits read as that byte. Text that is not UTF-8 once
/// decoded is `None`, refused as it stands rather than replaced; a name that is not UTF-8 matches no declared
/// one either way.
fn query_pairs(query: &str) -> Vec<(String, Option<String>)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, text) = pair.split_once('=').unwrap_or((pair, ""));
            (String::from_utf8_lossy(&decoded(name)).into_owned(), String::from_utf8(decoded(text)).ok())
        })
        .collect()
}

fn decoded(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let Some((&first, tail)) = rest.split_first() {
        let escaped = match (first, tail) {
            (b'%', [high, low, after @ ..]) => hex_byte(*high, *low).map(|byte| (byte, after)),
            _ => None,
        };
        match escaped {
            Some((byte, after)) => {
                out.push(byte);
                rest = after;
            }
            None => {
                out.push(if first == b'+' { b' ' } else { first });
                rest = tail;
            }
        }
    }
    out
}

fn hex_byte(high: u8, low: u8) -> Option<u8> {
    let digit = |b: u8| char::from(b).to_digit(16);
    u8::try_from(digit(high)?.checked_mul(16)?.checked_add(digit(low)?)?).ok()
}

impl<S: Send + Sync, T: DeserializeOwned + IntoParams> FromRequestParts<S> for StrictHeaders<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let mut sent = Vec::new();
        for parameter in declared_params::<T>(Location::Header)? {
            for value in parts.headers.get_all(parameter.name.as_str()) {
                sent.push((parameter.name.clone(), value.to_str().ok().map(str::to_owned)));
            }
        }
        read(Location::Header, &sent, false).map(Self)
    }
}

/// One parameter as `T` declares it: its name, whether it is required, and its schema.
#[derive(Debug)]
struct Declared {
    name: String,
    required: bool,
    schema: Value,
}

/// The parameters `T` declares, each of which must be declared at `location`.
fn declared_params<T: IntoParams>(location: Location) -> Result<Vec<Declared>, ApiError> {
    let parameter_in = match location {
        Location::Path => ParameterIn::Path,
        Location::Query => ParameterIn::Query,
        Location::Header => ParameterIn::Header,
    };
    T::into_params(|| Some(parameter_in.clone()))
        .into_iter()
        .map(|parameter| {
            let parameter = serde_json::to_value(parameter).unwrap_or(Value::Null);
            match parameter.get("name").and_then(Value::as_str) {
                Some(name) if parameter.get("in").and_then(Value::as_str) == Some(location.wire()) => Ok(Declared {
                    name: name.to_owned(),
                    required: parameter.get("required") == Some(&Value::Bool(true)),
                    schema: parameter.get("schema").cloned().unwrap_or(Value::Null),
                }),
                _ => Err(ApiError::internal("a parameter is declared at another location than its reader's")),
            }
        })
        .collect()
}

/// Reads `T` from the inputs `sent` at `location`, each a name and its text (`None` when it is not UTF-8), in
/// the order sent; `refuse_undeclared` refuses a name `T` does not declare.
fn read<T: DeserializeOwned + IntoParams>(
    location: Location,
    sent: &[(String, Option<String>)],
    refuse_undeclared: bool,
) -> Result<T, ApiError> {
    let declared = declared_params::<T>(location)?;
    let mut failures = Failures::default();
    let mut undeclared = BTreeSet::new();
    for (name, _) in sent {
        if refuse_undeclared && !declared.iter().any(|parameter| parameter.name == *name) && undeclared.insert(name) {
            let allowed = declared::members::<T>();
            failures.push(FieldError::at(location, name, ApiFieldCode::UnknownField { allowed }));
        }
    }
    let mut object = Map::new();
    for parameter in &declared {
        let mut texts = sent.iter().filter(|(name, _)| *name == parameter.name).map(|(_, text)| text.as_deref());
        match (texts.next(), texts.next()) {
            (None, _) if parameter.required => {
                failures.push(FieldError::at(location, &parameter.name, ApiFieldCode::Required));
            }
            (None, _) => {
                object.insert(parameter.name.clone(), Value::Null);
            }
            (Some(_), Some(_)) => {
                failures.push(FieldError::at(location, &parameter.name, ApiFieldCode::DuplicateMember))
            }
            (Some(text), None) => match parse::<T>(text, parameter) {
                Some(value) => {
                    object.insert(parameter.name.clone(), value);
                }
                None => failures.push(refusal::<T>(location, parameter)),
            },
        }
    }
    if !failures.is_empty() {
        failures.sort();
        return Err(ApiError::Invalid(failures));
    }
    // A value of the declared JSON type that `T` still refuses, such as an integer past its format's range.
    serde_path_to_error::deserialize::<_, T>(Value::Object(object)).map_err(|e| {
        let member = e.path().iter().next().map(ToString::to_string);
        match declared.iter().find(|parameter| Some(&parameter.name) == member.as_ref()) {
            Some(parameter) => ApiError::invalid(refusal::<T>(location, parameter)),
            None => ApiError::internal(e.into_inner()),
        }
    })
}

/// The parameter's text as the JSON value its schema declares, or `None` when it is not one.
fn parse<T: DeserializeOwned>(text: Option<&str>, parameter: &Declared) -> Option<Value> {
    let text = text?;
    if let Some(allowed) = declared::values::<T>(&parameter.name) {
        return allowed.contains(&text).then(|| Value::from(text));
    }
    match scalar::json_type(&parameter.schema) {
        Some("integer") => scalar::number(text).filter(|n| n.is_i64() || n.is_u64()).map(Value::Number),
        Some("number") => scalar::number(text).map(Value::Number),
        Some("boolean") => match text {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
        _ if parameter.schema.get("format").and_then(Value::as_str) == Some("uuid") => {
            scalar::is_uuid_text(text).then(|| Value::from(text))
        }
        _ => Some(Value::from(text)),
    }
}

/// The entry refusing the parameter's text: `validation.unknown-value` when its type is an enumeration, else
/// `validation.invalid-value`.
fn refusal<T: DeserializeOwned>(location: Location, parameter: &Declared) -> FieldError {
    match declared::values::<T>(&parameter.name) {
        Some(allowed) => FieldError::at(location, &parameter.name, ApiFieldCode::UnknownValue { allowed }),
        None => invalid_value(location, parameter),
    }
}

fn invalid_value(location: Location, parameter: &Declared) -> FieldError {
    let expected = scalar::expected(&parameter.schema);
    FieldError::at(location, &parameter.name, ApiFieldCode::InvalidValue { expected }).expecting(expected)
}

#[cfg(test)]
mod tests {
    use super::{NoQuery, StrictPath, query_pairs, read};
    use crate::problem::{ApiError, Location};
    use axum::extract::FromRequestParts as _;
    use serde::Deserialize;
    use serde_json::{Value, json};
    use utoipa::{IntoParams, ToSchema};

    #[derive(Debug, Deserialize, IntoParams)]
    #[serde(deny_unknown_fields, rename_all = "camelCase")]
    #[into_params(parameter_in = Query)]
    struct Probe {
        count: i32,
        ratio: f64,
        active: bool,
        id: uuid::Uuid,
        label: String,
        kind: Kind,
        #[serde(deserialize_with = "Deserialize::deserialize")]
        maybe: Option<i64>,
    }

    #[derive(Debug, PartialEq, Eq, Deserialize, ToSchema)]
    #[serde(deny_unknown_fields, rename_all = "lowercase")]
    enum Kind {
        Plain,
        Fancy,
    }

    #[derive(Debug, Deserialize, IntoParams)]
    #[serde(deny_unknown_fields)]
    #[into_params(parameter_in = Header)]
    struct Elsewhere {
        #[serde(rename = "X-Id")]
        id: uuid::Uuid,
    }

    const ID: &str = "0192f0c1-8b39-7cc4-9a41-6f8e62a4a1b2";

    fn sent(pairs: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
        pairs.iter().map(|(name, text)| ((*name).to_owned(), text.map(str::to_owned))).collect()
    }

    fn good(replace: &str, with: Option<Option<&str>>) -> Vec<(String, Option<String>)> {
        let all = [
            ("count", Some("-7")),
            ("ratio", Some("0.5")),
            ("active", Some("false")),
            ("id", Some(ID)),
            ("label", Some("")),
            ("kind", Some("fancy")),
        ];
        let kept = all.into_iter().filter(|(name, _)| *name != replace);
        sent(&kept.chain(with.map(|text| (replace, text))).collect::<Vec<_>>())
    }

    fn refused(sent: &[(String, Option<String>)], refuse_undeclared: bool) -> Value {
        match read::<Probe>(Location::Query, sent, refuse_undeclared) {
            Err(ApiError::Invalid(failures)) => json!({
                "errors": serde_json::to_value(failures.entries()).unwrap(),
                "omitted": failures.omitted(),
            }),
            other => panic!("expected validation.failed, got {other:?}"),
        }
    }

    fn one(sent: &[(String, Option<String>)]) -> Value {
        let got = refused(sent, true);
        assert_eq!(got["omitted"], 0);
        assert_eq!(got["errors"].as_array().map(Vec::len), Some(1), "{got}");
        got["errors"][0].clone()
    }

    #[test]
    fn every_declared_input_reads_as_its_declared_type() {
        let probe = read::<Probe>(Location::Query, &good("active", Some(Some("true"))), true).unwrap();
        assert_eq!((probe.count, probe.active, probe.label.as_str(), probe.kind), (-7, true, "", Kind::Fancy));
        assert_eq!(
            (probe.id.to_string(), probe.ratio.to_string(), probe.maybe),
            (ID.to_owned(), "0.5".to_owned(), None)
        );
        let given = read::<Probe>(Location::Query, &good("maybe", Some(Some("18"))), true).unwrap();
        assert_eq!((given.maybe, given.active), (Some(18), false));
        assert!(read::<NoQuery>(Location::Query, &[], true).is_ok());
    }

    #[test]
    fn text_that_does_not_parse_is_an_invalid_value_naming_what_was_expected() {
        for (name, text, expected) in [
            ("count", "", "int32"),
            ("count", " 1", "int32"),
            ("count", "1.5", "int32"),
            ("count", "+1", "int32"),
            ("count", "3000000000", "int32"),
            ("ratio", "x", "double"),
            ("active", "yes", "boolean"),
            ("active", "", "boolean"),
            ("id", "0192f0c18b397cc49a416f8e62a4a1b2", "uuid"),
            ("id", " ", "uuid"),
        ] {
            let entry = one(&good(name, Some(Some(text))));
            assert_eq!(
                entry,
                json!({ "in": "query", "name": name, "code": "validation.invalid-value",
                        "params": { "expected": expected }, "detail": format!("expected {expected}") }),
                "{name}={text:?}"
            );
        }
        let entry = one(&good("label", Some(None)));
        assert_eq!(entry["code"], "validation.invalid-value");
        assert_eq!(entry["params"], json!({ "expected": "string" }));
    }

    #[test]
    fn a_value_outside_its_enumeration_is_an_unknown_value_listing_the_values() {
        for text in [Some("Fancy"), Some(""), None] {
            assert_eq!(
                one(&good("kind", Some(text))),
                json!({ "in": "query", "name": "kind", "code": "validation.unknown-value",
                        "params": { "allowed": ["fancy", "plain"] } })
            );
        }
    }

    #[test]
    fn an_absent_required_input_a_repeated_one_and_an_undeclared_one_are_each_named() {
        assert_eq!(one(&good("count", None)), json!({ "in": "query", "name": "count", "code": "validation.required" }));
        let mut twice = good("count", Some(Some("1")));
        twice.extend(sent(&[("count", Some("1"))]));
        assert_eq!(one(&twice), json!({ "in": "query", "name": "count", "code": "validation.duplicate-member" }));
        let mut undeclared = good("", None);
        undeclared.extend(sent(&[("nmae", Some("x")), ("nmae", Some("y"))]));
        assert_eq!(
            one(&undeclared),
            json!({ "in": "query", "name": "nmae", "code": "validation.unknown-field",
                    "params": { "allowed": ["active", "count", "id", "kind", "label", "maybe", "ratio"] } })
        );
        assert!(read::<Probe>(Location::Query, &undeclared, false).is_ok());
    }

    #[test]
    fn every_failure_is_one_entry_sorted_and_the_cap_counts_the_rest() {
        let got = refused(&sent(&[("label", Some("a")), ("label", Some("b")), ("kind", Some("x"))]), true);
        let names: Vec<&str> = got["errors"].as_array().unwrap().iter().filter_map(|e| e["name"].as_str()).collect();
        assert_eq!(names, ["active", "count", "id", "kind", "label", "ratio"]);
        let many: Vec<(String, Option<String>)> = (0..101).map(|n| (format!("u{n:03}"), Some(String::new()))).collect();
        let got = refused(&[good("", None), many].concat(), true);
        assert_eq!((got["errors"].as_array().map(Vec::len), got["omitted"].as_u64()), (Some(100), Some(1)));
    }

    #[test]
    fn a_parameter_declared_at_another_location_is_an_internal_error() {
        assert!(matches!(read::<Elsewhere>(Location::Query, &[], true), Err(ApiError::Internal(_))));
        let header = read::<Elsewhere>(Location::Header, &sent(&[("X-Id", Some(ID))]), false).unwrap();
        assert_eq!(header.id.to_string(), ID);
    }

    #[test]
    fn the_query_is_decoded_as_sent_and_text_that_is_not_utf8_is_not_replaced() {
        let pairs = query_pairs("a=1&&b=x+y%2B%2b%41%zz%4&c&d=%E2%82%AC&e=%FF&%FF=1&f=a=b&=v");
        let expected: Vec<(String, Option<String>)> = [
            ("a", Some("1")),
            ("b", Some("x y++A%zz%4")),
            ("c", Some("")),
            ("d", Some("\u{20ac}")),
            ("e", None),
            ("\u{fffd}", Some("1")),
            ("f", Some("a=b")),
            ("", Some("v")),
        ]
        .into_iter()
        .map(|(name, text)| (name.to_owned(), text.map(str::to_owned)))
        .collect();
        assert_eq!(pairs, expected);
        assert_eq!(query_pairs(""), []);
    }

    #[tokio::test]
    async fn a_route_with_no_path_variables_is_an_internal_error() {
        let (mut parts, ()) = axum::http::Request::new(()).into_parts();
        let got = StrictPath::<NoQuery>::from_request_parts(&mut parts, &()).await;
        assert!(matches!(got, Err(ApiError::Internal(_))));
    }
}
