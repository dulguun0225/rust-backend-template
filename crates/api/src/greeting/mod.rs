//! The worked-example feature. Copy its shape for a new feature: a migration, a `store` module owning the
//! table, a module here with the handlers, the request and response types and the catalogs, a row in
//! `table-owners.toml`, the catalogs in `tests/catalog.rs`, a sample of each type in `tests/schemas.rs`.
//! Then delete this one.
//!
//! Every request body binds as `StrictJson<T>`, where `T` is a type this one operation owns; the handler
//! reaches the value only through `Bound::validate`, before its transaction. An update, when one is added, is
//! `PUT /api/greetings/{id}` binding its own `UpdateGreetingRequest`, which declares the fields that operation
//! writes and never `id`.

mod codes;
mod handlers;
mod types;

pub use codes::{GreetingErrorCode, GreetingFieldCode};
pub use handlers::{__path_create_greeting, __path_get_greeting, create_greeting, get_greeting};
pub use types::{CreateGreetingRequest, GreetingView, NAME_MAX_CHARS};
