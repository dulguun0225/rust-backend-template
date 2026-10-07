//! The greeting feature's handlers. A business outcome leaves as `ApiError::rejected` with a
//! `GreetingErrorCode`; the documented responses below name each one.

use axum::extract::State;
use platform::log::{Log, LogField};
use uuid::Uuid;
use web::body::StrictJson;
use web::params::{NoQuery, StrictPath, StrictQuery};
use web::problem::{ApiError, FieldError};
use web::respond::{Created, JsonBody};

use super::codes::{GreetingErrorCode, GreetingFieldCode};
use super::types::{CreateGreetingRequest, GreetingPath, GreetingView, NAME_MAX_CHARS};
use crate::AppState;

static LOG: Log = Log::new(module_path!());

/// Create one greeting.
#[utoipa::path(
    post,
    path = "/api/greetings",
    operation_id = "createGreeting",
    tag = "greeting",
    request_body = CreateGreetingRequest,
    responses(
        (status = 201, description = "Created. `Location` is the new greeting's URL.", body = GreetingView),
        (status = 400, description = "validation.failed (a field rule, an undeclared member, a wrong JSON type, an undeclared query parameter), validation.malformed-body", body = web::problem::Problem, content_type = "application/problem+json"),
        (status = 413, description = "request.too-large, `params.max` the most bytes a body may have", body = web::problem::Problem, content_type = "application/problem+json"),
    )
)]
pub async fn create_greeting(
    State(state): State<AppState>,
    StrictQuery(NoQuery {}): StrictQuery<NoQuery>,
    StrictJson(body): StrictJson<CreateGreetingRequest>,
) -> Result<Created<GreetingView>, ApiError> {
    let name = body.validate(valid_name)?;
    let id = platform::ids::new_id();
    let now = state.clock.now();
    state.tx.write(async |w| store::greeting::insert(w, id, &name, now).await).await.map_err(ApiError::internal)?;
    LOG.info("greeting created", &[LogField::id("greeting_id", id)]);
    Ok(Created { location: format!("/api/greetings/{id}"), body: view(id, name, now) })
}

/// One greeting by its opaque id.
#[utoipa::path(
    get,
    path = "/api/greetings/{id}",
    operation_id = "getGreeting",
    tag = "greeting",
    params(GreetingPath),
    responses(
        (status = 200, description = "OK", body = GreetingView),
        (status = 400, description = "validation.failed (an id that is not a UUID in its 36-character form, an undeclared query parameter)", body = web::problem::Problem, content_type = "application/problem+json"),
        (status = 404, description = "not-found", body = web::problem::Problem, content_type = "application/problem+json"),
    )
)]
pub async fn get_greeting(
    State(state): State<AppState>,
    StrictPath(GreetingPath { id }): StrictPath<GreetingPath>,
    StrictQuery(NoQuery {}): StrictQuery<NoQuery>,
) -> Result<JsonBody<GreetingView>, ApiError> {
    let row = state.tx.read(async |r| store::greeting::find(r, id).await).await.map_err(ApiError::internal)?;
    let row = row.ok_or_else(|| ApiError::rejected(GreetingErrorCode::NotFound))?;
    Ok(JsonBody(view(row.id, row.name, row.created_at)))
}

fn view(id: Uuid, name: String, created_at: time::OffsetDateTime) -> GreetingView {
    GreetingView { id, message: format!("Hello, {name}!"), name, created_at }
}

/// The field rules: each failure is recorded, and no value is produced when any is.
fn valid_name(request: &CreateGreetingRequest, errors: &mut Vec<FieldError>) -> Option<String> {
    let name = request.name.trim();
    if name.is_empty() {
        errors.push(FieldError::new("/name", GreetingFieldCode::Required));
        return None;
    }
    if name.chars().count() > usize::from(NAME_MAX_CHARS) {
        errors.push(FieldError::new("/name", GreetingFieldCode::TooLong { max: NAME_MAX_CHARS }));
        return None;
    }
    Some(name.to_owned())
}
