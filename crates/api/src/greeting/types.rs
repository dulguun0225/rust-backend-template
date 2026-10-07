//! The greeting feature's wire types.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// The longest name, in characters; the migration's check constraint says the same.
pub const NAME_MAX_CHARS: u16 = 100;

/// Creates one greeting. Only the fields this operation writes; the id is assigned by the service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CreateGreetingRequest {
    /// Who to greet, 1 to 100 characters after trimming.
    pub name: String,
}

/// The path of one greeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Path)]
pub struct GreetingPath {
    /// The greeting's id
    pub id: Uuid,
}

/// A greeting as the service returns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GreetingView {
    /// The greeting's opaque id.
    pub id: Uuid,
    /// Who is greeted.
    pub name: String,
    /// The greeting's text.
    pub message: String,
    /// When it was created.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}
