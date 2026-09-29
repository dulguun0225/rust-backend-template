//! The service's HTTP surface. One module per feature, shaped like [`greeting`]: its handlers, its request
//! and response types, its catalogs. [`routes`] registers every handler through `OpenApiRouter::routes`, the
//! only registration `clippy.toml` allows, so the committed document (`openapi/v1.json`) lists every route
//! the service serves.

pub mod greeting;

use std::sync::Arc;

use platform::clock::Clock;
use utoipa::OpenApi as _;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// The service's name: the document's title and the binary's name. `scripts/init.mjs` sets it.
pub const SERVICE_NAME: &str = "starter";

/// What every handler may reach: the transaction seam and the clock.
#[derive(Debug, Clone)]
pub struct AppState {
    /// The one transaction seam.
    pub tx: db::Tx,
    /// The injected clock.
    pub clock: Arc<dyn Clock>,
}

#[derive(utoipa::OpenApi)]
#[openapi(components(schemas(web::problem::Problem, web::problem::FieldError)))]
struct Components;

/// Every feature's routes, with the document's shared parts.
pub fn routes() -> OpenApiRouter<AppState> {
    let mut document = Components::openapi();
    document.info = utoipa::openapi::Info::new(SERVICE_NAME, "v1");
    OpenApiRouter::with_openapi(document)
        .routes(routes!(greeting::create_greeting))
        .routes(routes!(greeting::get_greeting))
}

/// The served router, every edge layer applied.
pub fn app(state: AppState) -> axum::Router {
    web::edge::finish(routes(), state).0
}

/// The OpenAPI document of exactly what [`app`] serves.
#[must_use]
pub fn openapi() -> utoipa::openapi::OpenApi {
    routes().split_for_parts().1
}
