//! The HTTP platform tier: what every feature's handlers use and what no feature owns.
//!
//! - [`codes`]: the cross-cutting catalogs, [`codes::ApiErrorCode`] and [`codes::ApiFieldCode`].
//! - [`problem`]: RFC 9457 problem documents and [`problem::ApiError`], the one error type handlers return.
//! - [`body`]: [`body::StrictJson`], the one request-body reader, and [`body::Bound`], through which alone a
//!   handler reaches the value.
//! - [`params`]: [`params::StrictPath`], [`params::StrictQuery`] and [`params::StrictHeaders`], the readers of
//!   every input outside the body.
//! - [`declared`]: the members and enumeration values a request type declares, as the lists refusals carry.
//! - [`respond`]: JSON success responses.
//! - [`edge`]: [`edge::finish`], which turns the feature routes into the served router with every layer on
//!   it — correlation, the problem edge, panic capture and the body limit — and [`edge::serve`].

pub mod body;
pub mod codes;
pub mod declared;
pub mod edge;
pub mod params;
pub mod problem;
pub mod respond;
mod scalar;
