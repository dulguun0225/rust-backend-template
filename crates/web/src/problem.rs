//! RFC 9457 problem documents. Every error response is one, and every one carries a `code` from a catalog.
//! Handlers return [`ApiError`]; the edge ([`crate::edge`]) turns every other error response into a problem
//! too, so no response of status 400 or above leaves the service without a code.

use std::collections::BTreeMap;

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use platform::catalog::{FieldCode, ParamValue, WireError};
use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};
use utoipa::openapi::schema::{ObjectBuilder, OneOfBuilder, Type};
use utoipa::openapi::{RefOr, Schema};
use utoipa::{PartialSchema, ToSchema};

use crate::codes::ApiErrorCode;

/// The problem media type.
pub const PROBLEM_JSON: &str = "application/problem+json";

/// The most entries one `validation.failed` records; the rest are counted in `errorsOmitted`, never held.
pub const MAX_ENTRIES: usize = 100;

/// An RFC 9457 problem document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    /// Always `about:blank`: `code` carries the machine meaning.
    #[serde(rename = "type")]
    pub kind: String,
    /// The status's reason phrase.
    pub title: String,
    /// The HTTP status.
    pub status: u16,
    /// Stable machine code from one of the catalogs.
    pub code: String,
    /// What is allowed, by name, e.g. `{"max": 65536}` on request.too-large: exactly the params the response
    /// code declares in the error catalog, absent when it declares none. Never the value sent.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    #[schema(inline)]
    pub params: BTreeMap<String, FieldParam>,
    /// Caller-safe text, never the value sent. On validation.malformed-body: where the JSON stopped being
    /// well formed, or that the body is missing or not an object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Present only on validation.failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<FieldError>>,
    /// Present only on validation.failed, when more inputs failed than the 100 entries `errors` holds: how
    /// many more.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors_omitted: Option<u64>,
    /// Present only on platform.internal (500): the request's correlation id, which the server log carries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incident_id: Option<String>,
}

/// Where an input other than a body member travels, as OpenAPI's parameter `in` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Location {
    /// A path template variable.
    Path,
    /// A query parameter.
    Query,
    /// A request header.
    Header,
}

impl Location {
    /// The wire name, OpenAPI's `in`.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Path => "path",
            Self::Query => "query",
            Self::Header => "header",
        }
    }
}

/// The input an entry names: a body member by its RFC 6901 pointer, or any other input by its location and
/// the name the document declares for it. Never both.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Target {
    /// A body member; `""` is the whole body.
    Pointer(String),
    /// A path variable, query parameter or header.
    Parameter(Location, String),
}

// Built only by `FieldError::new` and `FieldError::at`, from a catalog code carrying its declared params: the
// fields are private to this crate, so no other crate writes a code or a param list by hand.
/// One entry of a validation.failed problem.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FieldError {
    pub(crate) target: Target,
    pub(crate) code: String,
    pub(crate) detail: Option<String>,
    pub(crate) params: BTreeMap<String, FieldParam>,
}

/// One param of a code: a JSON integer, a JSON string, or a JSON array of strings.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(untagged)]
pub enum FieldParam {
    /// An integer, such as a maximum length.
    Integer(i64),
    /// A string fixed in the source, such as an expected JSON type.
    Text(String),
    /// Strings fixed in the source, sorted, such as the members a type declares.
    List(Vec<String>),
}

impl From<ParamValue> for FieldParam {
    fn from(value: ParamValue) -> Self {
        match value {
            ParamValue::Integer(n) => Self::Integer(n),
            ParamValue::Text(t) => Self::Text(t.to_owned()),
            ParamValue::List(items) => {
                let mut sorted: Vec<String> = items.iter().map(|s| (*s).to_owned()).collect();
                sorted.sort();
                Self::List(sorted)
            }
        }
    }
}

fn wire_params(params: Vec<(&'static str, ParamValue)>) -> BTreeMap<String, FieldParam> {
    params.into_iter().map(|(name, value)| (name.to_owned(), value.into())).collect()
}

impl FieldError {
    /// An entry for the body member at `pointer` (already an RFC 6901 pointer) with a catalog code and the
    /// params it carries.
    #[must_use]
    pub fn new(pointer: impl Into<String>, code: impl FieldCode) -> Self {
        Self::of(Target::Pointer(pointer.into()), code)
    }

    /// An entry for the path variable, query parameter or header `name`, the name the document declares.
    #[must_use]
    pub fn at(location: Location, name: impl Into<String>, code: impl FieldCode) -> Self {
        Self::of(Target::Parameter(location, name.into()), code)
    }

    fn of(target: Target, code: impl FieldCode) -> Self {
        Self { target, code: code.wire().to_owned(), detail: None, params: wire_params(code.params()) }
    }

    /// The same entry with a static detail: the detail is never built from the value sent.
    #[must_use]
    pub fn with_detail(self, detail: &'static str) -> Self {
        Self { detail: Some(detail.to_owned()), ..self }
    }

    /// The same entry with the detail `expected <word>`, the word one the readers' fixed vocabulary holds.
    #[must_use]
    pub(crate) fn expecting(self, word: &'static str) -> Self {
        Self { detail: Some(format!("expected {word}")), ..self }
    }
}

impl Serialize for FieldError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        match &self.target {
            Target::Pointer(pointer) => map.serialize_entry("pointer", pointer)?,
            Target::Parameter(location, name) => {
                map.serialize_entry("in", location.wire())?;
                map.serialize_entry("name", name)?;
            }
        }
        map.serialize_entry("code", &self.code)?;
        if !self.params.is_empty() {
            map.serialize_entry("params", &self.params)?;
        }
        if let Some(detail) = &self.detail {
            map.serialize_entry("detail", detail)?;
        }
        map.end()
    }
}

/// The entry's schema: one of a body entry, which has `pointer`, and a parameter entry, which has `in` and
/// `name`, each with `code` and, when the code has them, `params` and `detail`. Written by hand because the
/// two shapes are one Rust type.
impl PartialSchema for FieldError {
    fn schema() -> RefOr<Schema> {
        let text = |description: &str| ObjectBuilder::new().schema_type(Type::String).description(Some(description));
        let entry = |located: ObjectBuilder| {
            located
                .property("code", text("A field code, e.g. validation.required."))
                .required("code")
                .property(
                    "params",
                    ObjectBuilder::new()
                        .schema_type(Type::Object)
                        .property_names(Some(ObjectBuilder::new().schema_type(Type::String)))
                        .additional_properties(Some(FieldParam::schema()))
                        .description(Some(
                            "What is allowed, by name, e.g. `{\"max\": 100}` on validation.too-long: exactly the params \
                             the code declares in the error catalog, absent when it declares none. Never the value sent.",
                        )),
                )
                .property(
                    "detail",
                    text("Caller-safe text naming what was expected, e.g. `expected boolean`; never the value sent."),
                )
        };
        let member = entry(
            ObjectBuilder::new()
                .schema_type(Type::Object)
                .description(Some("A body member."))
                .property("pointer", text("RFC 6901 JSON pointer to the member, e.g. /name; \"\" is the whole body."))
                .required("pointer"),
        );
        let parameter = entry(
            ObjectBuilder::new()
                .schema_type(Type::Object)
                .description(Some("A path variable, query parameter or header."))
                .property(
                    "in",
                    ObjectBuilder::new()
                        .schema_type(Type::String)
                        .enum_values(Some([Location::Path, Location::Query, Location::Header].map(Location::wire)))
                        .description(Some("Where the input travels, as OpenAPI's parameter `in` names it.")),
                )
                .required("in")
                .property("name", text("The input's name as the document declares it, never as it was sent."))
                .required("name"),
        );
        OneOfBuilder::new()
            .description(Some("One entry of a validation.failed problem: a body member, or another input."))
            .item(member)
            .item(parameter)
            .into()
    }
}

impl ToSchema for FieldError {}

/// The entries of one `validation.failed`: at most [`MAX_ENTRIES`] recorded, the rest only counted, so a
/// request cannot buy a response, or a heap, many times its own size.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Failures {
    entries: Vec<FieldError>,
    omitted: u64,
}

impl Failures {
    /// Records `failure`, or counts it once [`MAX_ENTRIES`] are recorded.
    pub fn push(&mut self, failure: FieldError) {
        if self.entries.len() < MAX_ENTRIES {
            self.entries.push(failure);
        } else {
            self.omitted = self.omitted.saturating_add(1);
        }
    }

    /// No failure recorded or counted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.omitted == 0
    }

    /// The recorded entries.
    #[must_use]
    pub fn entries(&self) -> &[FieldError] {
        &self.entries
    }

    /// How many failures were counted and not recorded.
    #[must_use]
    pub const fn omitted(&self) -> u64 {
        self.omitted
    }

    pub(crate) fn sort(&mut self) {
        self.entries.sort();
    }
}

/// The RFC 6901 pointer to a top-level member.
#[must_use]
pub fn pointer(member: &str) -> String {
    format!("/{}", member.replace('~', "~0").replace('/', "~1"))
}

/// The one error type handlers return.
#[derive(Debug)]
pub enum ApiError {
    /// A catalog outcome: its code, status and params, and a detail built from the service's own values.
    Rejected {
        /// The status.
        status: u16,
        /// The wire code.
        code: &'static str,
        /// The params the code declares, with the values this outcome carries.
        params: Vec<(&'static str, ParamValue)>,
        /// Caller-safe text, never built from the value sent.
        detail: Option<String>,
    },
    /// validation.malformed-body with its detail.
    Malformed(String),
    /// validation.failed with its entries.
    Invalid(Failures),
    /// An unexpected failure. The cause goes to the server log; the response carries only the incident id.
    Internal(String),
}

impl ApiError {
    /// A catalog outcome.
    #[must_use]
    pub fn rejected(code: impl WireError) -> Self {
        Self::Rejected { status: code.status(), code: code.wire(), params: code.params(), detail: None }
    }

    /// request.too-large: the body is longer than `max` bytes, the limit the service was built with.
    #[must_use]
    pub fn too_large(max: u32) -> Self {
        let code = ApiErrorCode::PayloadTooLarge { max };
        Self::Rejected {
            status: code.status(),
            code: code.wire(),
            params: code.params(),
            detail: Some(format!("The request body must be at most {max} bytes.")),
        }
    }

    /// validation.failed with one entry.
    #[must_use]
    pub fn invalid(failure: FieldError) -> Self {
        let mut failures = Failures::default();
        failures.push(failure);
        Self::Invalid(failures)
    }

    /// An unexpected failure, with its cause for the server log.
    #[must_use]
    pub fn internal(cause: impl core::fmt::Display) -> Self {
        Self::Internal(cause.to_string())
    }
}

/// Marks a response as an unexpected failure the edge must log and render: it carries the cause for the log.
#[derive(Debug, Clone)]
pub struct Unhandled(pub String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Rejected { status, code, params, detail } => {
                problem_response(Problem { params: wire_params(params), detail, ..problem(status, code) })
            }
            Self::Malformed(detail) => problem_response(Problem {
                detail: Some(detail),
                ..problem(ApiErrorCode::MalformedBody.status(), ApiErrorCode::MalformedBody.wire())
            }),
            Self::Invalid(Failures { entries, omitted }) => problem_response(Problem {
                errors: Some(entries),
                errors_omitted: (omitted > 0).then_some(omitted),
                ..problem(ApiErrorCode::ValidationFailed.status(), ApiErrorCode::ValidationFailed.wire())
            }),
            Self::Internal(cause) => {
                let mut response = StatusCode::INTERNAL_SERVER_ERROR.into_response();
                response.extensions_mut().insert(Unhandled(cause));
                response
            }
        }
    }
}

/// A problem with `status` and `code` and nothing else. `status` comes from a catalog, which holds every status
/// to 400..=599; anything else is answered as a 500.
#[must_use]
pub fn problem(status: u16, code: &str) -> Problem {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    Problem {
        kind: "about:blank".to_owned(),
        title: status.canonical_reason().unwrap_or("Error").to_owned(),
        status: status.as_u16(),
        code: code.to_owned(),
        params: BTreeMap::new(),
        detail: None,
        errors: None,
        errors_omitted: None,
        incident_id: None,
    }
}

/// The problem as a response, with its media type.
#[must_use]
pub fn problem_response(problem: Problem) -> Response {
    let status = StatusCode::from_u16(problem.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    match serde_json::to_vec(&problem) {
        Ok(body) => {
            let mut response = (status, body).into_response();
            response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(PROBLEM_JSON));
            response
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::{ApiError, Failures, FieldError, Location, MAX_ENTRIES, pointer};
    use crate::codes::ApiFieldCode;
    use serde_json::json;

    #[test]
    fn a_pointer_escapes_tilde_and_slash() {
        assert_eq!(pointer("a/b~c"), "/a~1b~0c");
        assert_eq!(pointer(""), "/");
    }

    #[test]
    fn an_entry_names_a_member_by_pointer_or_another_input_by_location_and_name() {
        let member =
            FieldError::new("/a", ApiFieldCode::WrongType { expected: "string" }).with_detail("expected string");
        assert_eq!(
            serde_json::to_value(member).unwrap(),
            json!({ "pointer": "/a", "code": "validation.wrong-type", "params": { "expected": "string" }, "detail": "expected string" })
        );
        for (location, wire) in [(Location::Path, "path"), (Location::Query, "query"), (Location::Header, "header")] {
            let entry = FieldError::at(location, "X-Id", ApiFieldCode::Required);
            assert_eq!(
                serde_json::to_value(entry).unwrap(),
                json!({ "in": wire, "name": "X-Id", "code": "validation.required" })
            );
        }
    }

    #[test]
    fn a_list_param_goes_on_the_wire_sorted() {
        let entry = FieldError::new("/z", ApiFieldCode::UnknownField { allowed: &["b", "c", "a"] });
        assert_eq!(serde_json::to_value(entry).unwrap()["params"], json!({ "allowed": ["a", "b", "c"] }));
    }

    #[test]
    fn failures_record_at_most_the_cap_and_count_the_rest() {
        let mut failures = Failures::default();
        assert!(failures.is_empty());
        for n in 0..=MAX_ENTRIES {
            failures.push(FieldError::new(pointer(&n.to_string()), ApiFieldCode::Required));
        }
        assert_eq!((failures.entries().len(), failures.omitted()), (MAX_ENTRIES, 1));
        assert!(
            matches!(ApiError::invalid(FieldError::new("/a", ApiFieldCode::Required)), ApiError::Invalid(f) if f.entries().len() == 1)
        );
    }
}
