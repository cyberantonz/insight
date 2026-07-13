//! HTTP API layer — route table + handlers.

pub mod error;
mod handlers;

use std::sync::Arc;

use axum::Extension;
use axum::Router;
use axum::http::StatusCode;
use axum::middleware::from_fn;
use toolkit::api::{OpenApiRegistry, OperationBuilder};

use crate::auth;
use crate::gear::AppState;

/// Mount the identity-resolution routes onto the host's router.
///
/// Builds our endpoints on a fresh sub-router (so the tenant middleware + the
/// `AppState` extension scope to our routes, not the host's `/health`/`/docs`),
/// then merges it into the host router.
pub fn register_routes(
    host_router: Router,
    openapi: &dyn OpenApiRegistry,
    state: Arc<AppState>,
) -> Router {
    let api = build_operations(Router::new(), openapi)
        .layer(from_fn(auth::tenant_middleware))
        .layer(Extension(state));

    host_router.merge(api)
}

/// Declare each operation via the toolkit `OperationBuilder` (records the route
/// + its OpenAPI spec + auth/error metadata).
fn build_operations(router: Router, openapi: &dyn OpenApiRegistry) -> Router {
    OperationBuilder::post("/v1/profiles")
        .operation_id("identity_resolution.profiles.resolve")
        .summary("Resolve a profile by email or source-native id")
        .authenticated()
        .no_license_required()
        .json_request::<handlers::ResolveProfileCommand>(openapi, "Identity to resolve")
        .json_response_with_schema::<handlers::ProfileResponse>(
            openapi,
            StatusCode::OK,
            "Resolved person",
        )
        .standard_errors(openapi)
        .handler(handlers::resolve_profile)
        .register(router, openapi)
}
