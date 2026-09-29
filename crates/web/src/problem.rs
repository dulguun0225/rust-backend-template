//! RFC 9457 problem documents. Every error response is one, and every one carries a `code` from a catalog.
//! Handlers return [`ApiError`]; the edge ([`crate::edge`]) turns every other error response into a problem
//! too, so no response of status 400 or above leaves the service without a code.

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use platform::catalog::{FieldCode, WireError};
use serde::Serialize;
use utoipa::ToSchema;

use crate::codes::ApiErrorCode;

/// The problem media type.
pub const PROBLEM_JSON: &str = "application/problem+json";

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
    /// Caller-safe text, never the value sent. On validation.malformed-body: where the JSON stopped being
    /// well formed, or that the body is missing or not an object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Present only on validation.failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<FieldError>>,
    /// Present only on platform.internal (500): the request's correlation id, which the server log carries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incident_id: Option<String>,
}

/// One entry of a validation.failed problem.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, ToSchema)]
pub struct FieldError {
    /// RFC 6901 JSON pointer to the member, e.g. /name.
    pub pointer: String,
    /// A field code, e.g. validation.required.
    pub code: String,
    /// Caller-safe text naming what was expected, e.g. `expected boolean`; never the value sent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl FieldError {
    /// An entry at `pointer` (already an RFC 6901 pointer) with a catalog code.
    #[must_use]
    pub fn new(pointer: impl Into<String>, code: impl FieldCode) -> Self {
        Self { pointer: pointer.into(), code: code.wire().to_owned(), detail: None }
    }

    /// The same entry with a static detail: the detail is never built from the value sent.
    #[must_use]
    pub fn with_detail(self, detail: &'static str) -> Self {
        Self { detail: Some(detail.to_owned()), ..self }
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
    /// A catalog outcome: its code and status, nothing else.
    Rejected {
        /// The status.
        status: u16,
        /// The wire code.
        code: &'static str,
    },
    /// validation.malformed-body with its detail.
    Malformed(String),
    /// validation.failed with its entries.
    Invalid(Vec<FieldError>),
    /// An unexpected failure. The cause goes to the server log; the response carries only the incident id.
    Internal(String),
}

impl ApiError {
    /// A catalog outcome.
    #[must_use]
    pub fn rejected(code: impl WireError) -> Self {
        Self::Rejected { status: code.status(), code: code.wire() }
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
            Self::Rejected { status, code } => problem_response(status, code, None, None, None),
            Self::Malformed(detail) => problem_response(
                ApiErrorCode::MalformedBody.status(),
                ApiErrorCode::MalformedBody.wire(),
                Some(detail),
                None,
                None,
            ),
            Self::Invalid(errors) => problem_response(
                ApiErrorCode::ValidationFailed.status(),
                ApiErrorCode::ValidationFailed.wire(),
                None,
                Some(errors),
                None,
            ),
            Self::Internal(cause) => {
                let mut response = StatusCode::INTERNAL_SERVER_ERROR.into_response();
                response.extensions_mut().insert(Unhandled(cause));
                response
            }
        }
    }
}

/// A problem response. `status` comes from a catalog, which holds every status to 400..=599.
#[must_use]
pub fn problem_response(
    status: u16,
    code: &str,
    detail: Option<String>,
    errors: Option<Vec<FieldError>>,
    incident_id: Option<String>,
) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let problem = Problem {
        kind: "about:blank".to_owned(),
        title: status.canonical_reason().unwrap_or("Error").to_owned(),
        status: status.as_u16(),
        code: code.to_owned(),
        detail,
        errors,
        incident_id,
    };
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
    use super::pointer;

    #[test]
    fn a_pointer_escapes_tilde_and_slash() {
        assert_eq!(pointer("a/b~c"), "/a~1b~0c");
        assert_eq!(pointer(""), "/");
    }
}
