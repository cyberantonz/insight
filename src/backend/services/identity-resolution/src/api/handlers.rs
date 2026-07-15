//! Route handlers (thin controllers — DTOs + assembly live in `crate::domain`).
//!
//! `/health` + `/healthz` + `/docs` are provided by the api-gateway host gear,
//! so this service defines no health handler.

use std::sync::Arc;

use axum::Json;
use axum::extract::Extension;
use axum::response::IntoResponse;
use toolkit_canonical_errors::CanonicalError;
use toolkit_security::SecurityContext;
use uuid::Uuid;

use super::AppState;
use super::canonical_json::CanonicalJson;
use super::error::ProfileError;
use crate::domain::profile::{
    ParentProjection, ResolveProfileCommand, assemble_profile, latest_values,
};
use crate::infra::db::persons_repo;

/// `POST /v1/profiles` — resolve one identity (email or source-native id) to a
/// person, then assemble the profile.
///
/// 0 matches → 404; >1 → 409 (the .NET service used 422 `ambiguous_profile`;
/// gears canonical errors have no 422, so we map to `aborted`/409 — flagged for
/// review).
pub async fn resolve_profile(
    Extension(state): Extension<Arc<AppState>>,
    Extension(ctx): Extension<SecurityContext>,
    CanonicalJson(cmd): CanonicalJson<ResolveProfileCommand>,
) -> Result<impl IntoResponse, CanonicalError> {
    let tenant = ctx.subject_tenant_id();
    let person_ids = resolve_person_ids(&state, tenant, &cmd).await?;

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
            let source_ids =
                persons_repo::current_source_ids_for_person(&state.db, tenant, *person_id)
                    .await
                    .map_err(|e| {
                        tracing::error!(error = %e, "fetch source ids failed");
                        CanonicalError::internal("profile assembly failed").create()
                    })?;
            let parent = resolve_parent(&state, tenant, *person_id).await?;
            Ok(Json(assemble_profile(
                *person_id,
                tenant,
                observations,
                source_ids,
                parent,
            )))
        }
        _ => Err(ProfileError::aborted("email resolves to multiple persons")
            .with_reason("AMBIGUOUS_PROFILE")
            .create()),
    }
}

/// Validate the request and resolve it to candidate `person_id`s.
///
/// Validation mirrors the .NET `ResolveProfileCommandValidator`; resolution
/// dispatches on `value_type` ("email" across all sources, "id" scoped to one
/// source instance). Returns the (possibly empty or multi-element) match set —
/// the caller maps 0 → 404, 1 → profile, >1 → 409.
async fn resolve_person_ids(
    state: &AppState,
    tenant: Uuid,
    cmd: &ResolveProfileCommand,
) -> Result<Vec<Uuid>, CanonicalError> {
    let value_type = cmd.value_type.trim();

    if cmd.value.trim().is_empty() {
        return Err(ProfileError::invalid_argument()
            .with_field_violation("value", "value must not be empty", "INVALID")
            .create());
    }
    if cmd.value.chars().count() > 320 {
        return Err(ProfileError::invalid_argument()
            .with_field_violation("value", "value must be at most 320 characters", "INVALID")
            .create());
    }
    if value_type == "email"
        && (cmd.insight_source_type.is_some() || cmd.insight_source_id.is_some())
    {
        return Err(ProfileError::invalid_argument()
            .with_field_violation(
                "insight_source_type",
                "insight_source_type / insight_source_id must be null for value_type='email'",
                "INVALID",
            )
            .create());
    }

    let person_ids = match value_type {
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

    Ok(person_ids)
}

/// Resolve the person's parent (supervisor) edge from `org_chart`, filtered to
/// the configured source instance, into the projection the assembler writes.
/// Returns `Ok(None)` when the person has no current parent edge on that source.
async fn resolve_parent(
    state: &AppState,
    tenant: Uuid,
    child_person_id: Uuid,
) -> Result<Option<ParentProjection>, CanonicalError> {
    let source_type = &state.config.org_chart_source_type;

    let edges = persons_repo::current_parents_for_child(&state.db, tenant, child_person_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "fetch parent edges failed");
            CanonicalError::internal("profile assembly failed").create()
        })?;
    let Some(edge) = edges.into_iter().find(|e| &e.source_type == source_type) else {
        return Ok(None);
    };

    let parent_obs =
        persons_repo::fetch_person_observations(&state.db, tenant, edge.parent_person_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "fetch parent observations failed");
                CanonicalError::internal("profile assembly failed").create()
            })?;
    let parent_ids =
        persons_repo::current_source_ids_for_person(&state.db, tenant, edge.parent_person_id)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "fetch parent source ids failed");
                CanonicalError::internal("profile assembly failed").create()
            })?;

    let latest = latest_values(parent_obs);
    // Parent's source-native id on the same source instance as the edge.
    let source_native_id = parent_ids
        .into_iter()
        .find(|s| &s.source_type == source_type && s.source_id == edge.source_id)
        .map(|s| s.value);

    Ok(Some(ParentProjection {
        person_id: edge.parent_person_id,
        email: latest.get("email").cloned(),
        display_name: latest.get("display_name").cloned(),
        source_native_id,
    }))
}
