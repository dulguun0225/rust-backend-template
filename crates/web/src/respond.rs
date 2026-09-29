//! JSON success responses. `axum::Json` is banned in every crate (`clippy.toml`) because it is also a
//! request-body extractor; responses use these instead.

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::problem::ApiError;

/// A 200 with a JSON body.
#[derive(Debug)]
pub struct JsonBody<T>(pub T);

/// A 201 with a JSON body and a `Location` header naming the new resource.
#[derive(Debug)]
pub struct Created<T> {
    /// The new resource's path, e.g. `/api/greetings/0192…`.
    pub location: String,
    /// The representation.
    pub body: T,
}

impl<T: Serialize> IntoResponse for JsonBody<T> {
    fn into_response(self) -> Response {
        json(StatusCode::OK, &self.0)
    }
}

impl<T: Serialize> IntoResponse for Created<T> {
    fn into_response(self) -> Response {
        let mut response = json(StatusCode::CREATED, &self.body);
        match HeaderValue::try_from(self.location) {
            Ok(location) => {
                response.headers_mut().insert(header::LOCATION, location);
                response
            }
            Err(e) => ApiError::internal(e).into_response(),
        }
    }
}

fn json<T: Serialize>(status: StatusCode, body: &T) -> Response {
    match serde_json::to_vec(body) {
        Ok(bytes) => {
            let mut response = (status, bytes).into_response();
            response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
            response
        }
        Err(e) => ApiError::internal(e).into_response(),
    }
}
