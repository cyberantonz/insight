//! Route handlers.
//!
//! `/health` + `/healthz` + `/docs` are provided by the api-gateway host gear,
//! so we define no health handler.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::Extension;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use toolkit_canonical_errors::CanonicalError;
use toolkit_security::SecurityContext;
use utoipa::ToSchema;
use uuid::Uuid;

use super::error::ProfileError;
use crate::gear::AppState;
use crate::infra::db::entities::persons;
use crate::infra::db::persons_repo;

/// Body of `POST /v1/profiles`. `value_type = "email"` matches across all
/// sources for the tenant; `value_type = "id"` matches a source-native account
/// id within one source instance (needs `insight_source_type` + `insight_source_id`).
#[derive(Debug, Deserialize, ToSchema)]
pub struct ResolveProfileCommand {
    pub value_type: String,
    pub value: String,
    /// Required when `value_type = "id"` — the source instance to scope to.
    #[serde(default)]
    pub insight_source_type: Option<String>,
    /// Required when `value_type = "id"`.
    #[serde(default)]
    pub insight_source_id: Option<Uuid>,
}

/// Response body of `POST /v1/profiles` — the resolved person's profile.
/// Attributes only for now; `ids[]` and the org tree (supervisor / parent /
/// subordinates) land in follow-up steps. Null fields are omitted from JSON.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProfileResponse {
    pub person_id: Uuid,
    pub insight_tenant_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub department: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub division: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub employee_id: Option<String>,
}

// Marker traits the toolkit `OperationBuilder` requires (alongside `ToSchema`).
impl toolkit::api::api_dto::RequestApiDto for ResolveProfileCommand {}
impl toolkit::api::api_dto::ResponseApiDto for ProfileResponse {}

/// Collapse a person's observations to the current value per attribute — the
/// latest by `created_at` (per the .NET `ProfileAssembler`, ADR-0003) — and map
/// to the response DTO. `value_effective` is the DB's coalesced display value.
fn assemble_profile(
    person_id: Uuid,
    tenant_id: Uuid,
    observations: Vec<persons::Model>,
) -> ProfileResponse {
    // Keep the latest observation per value_type (max created_at).
    let mut latest: HashMap<String, persons::Model> = HashMap::new();
    for obs in observations {
        match latest.get(&obs.value_type) {
            Some(prev) if prev.created_at >= obs.created_at => {}
            _ => {
                latest.insert(obs.value_type.clone(), obs);
            }
        }
    }

    let get = |value_type: &str| -> Option<String> {
        latest
            .get(value_type)
            .and_then(|m| m.value_effective.clone())
            .filter(|s| !s.trim().is_empty())
    };

    ProfileResponse {
        person_id,
        insight_tenant_id: tenant_id,
        email: get("email"),
        display_name: get("display_name"),
        first_name: get("first_name"),
        last_name: get("last_name"),
        department: get("department"),
        division: get("division"),
        job_title: get("job_title"),
        status: get("status"),
        username: get("username"),
        employee_id: get("employee_id"),
    }
}

/// `POST /v1/profiles` — resolve one identity to a person.
///
/// email-only for now. 0 matches → 404; >1 → 409 (the .NET service used 422
/// `ambiguous_profile`; gears canonical errors have no 422, so we map to
/// `aborted`/409 — flagged for review).
pub async fn resolve_profile(
    Extension(state): Extension<Arc<AppState>>,
    Extension(ctx): Extension<SecurityContext>,
    Json(cmd): Json<ResolveProfileCommand>,
) -> Result<impl IntoResponse, CanonicalError> {
    let tenant = ctx.subject_tenant_id();

    let person_ids = match cmd.value_type.trim() {
        "email" => persons_repo::resolve_person_ids_by_email(&state.db, tenant, &cmd.value)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "resolve by email failed");
                CanonicalError::internal("profile resolution failed").create()
            })?,
        "id" => {
            let source_type = cmd.insight_source_type.as_deref().ok_or_else(|| {
                ProfileError::invalid_argument()
                    .with_field_violation(
                        "insight_source_type",
                        "insight_source_type is required for value_type='id'",
                        "REQUIRED",
                    )
                    .create()
            })?;
            let source_id = cmd.insight_source_id.ok_or_else(|| {
                ProfileError::invalid_argument()
                    .with_field_violation(
                        "insight_source_id",
                        "insight_source_id is required for value_type='id'",
                        "REQUIRED",
                    )
                    .create()
            })?;
            persons_repo::resolve_person_ids_by_source_id(
                &state.db,
                tenant,
                source_type,
                source_id,
                &cmd.value,
            )
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "resolve by source id failed");
                CanonicalError::internal("profile resolution failed").create()
            })?
        }
        _ => {
            return Err(ProfileError::invalid_argument()
                .with_field_violation(
                    "value_type",
                    "value_type must be 'email' or 'id'",
                    "INVALID",
                )
                .create());
        }
    };

    match person_ids.as_slice() {
        [] => Err(ProfileError::not_found("person not found")
            .with_resource(cmd.value)
            .create()),
        [person_id] => {
            let observations =
                persons_repo::fetch_person_observations(&state.db, tenant, *person_id)
                    .await
                    .map_err(|e| {
                        tracing::error!(error = %e, "fetch person observations failed");
                        CanonicalError::internal("profile assembly failed").create()
                    })?;
            Ok(Json(assemble_profile(*person_id, tenant, observations)))
        }
        _ => Err(ProfileError::aborted("email resolves to multiple persons")
            .with_reason("AMBIGUOUS_PROFILE")
            .create()),
    }
}
