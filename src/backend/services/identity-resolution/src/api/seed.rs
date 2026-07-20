//! Persons-seed HTTP surface + background worker.
//!
//! `POST /v1/persons-seed` enqueues an `operations` row and a job on an
//! in-process channel, returning 202; a worker (spawned once in the gear init)
//! drains the channel, runs the seed via [`run_seed`], and marks the operation
//! completed/failed. The GETs poll status. Ported from the .NET
//! `PersonsSeedEndpoints` + `PersonsSeedQueue`.
//!
//! Admin-gated like the .NET `CallerAdminCheck`: the caller is resolved from the
//! `X-Insight-Person-Id` header and must hold an active `admin` role in the
//! tenant, and is recorded as the seed author. The .NET JWT id/email-claim
//! fallbacks are deferred until gears auth carries a subject.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{Extension, Path, Query};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use toolkit_canonical_errors::CanonicalError;
use toolkit_security::SecurityContext;
use utoipa::ToSchema;
use uuid::Uuid;

use super::AppState;
use super::canonical_json::CanonicalJson;
use super::error::PersonsSeedError;
use crate::config::GearConfig;
use crate::domain::seed_service::run_seed;
use crate::infra::db::ops_repo::{self, Operation, OperationStatus};
use crate::infra::db::roles_repo;
use crate::infra::db::seed_repo::MariaDbSeedStore;
use crate::infra::identity_inputs::ClickHouseIdentityInputsReader;

const LINK_BY_EMAIL_MODE: &str = "link_by_email";

/// Header carrying the caller's `person_id`, parity with the .NET
/// `HeaderCallerContext`. JWT id/email-claim fallbacks are deferred until gears
/// auth carries a subject (the host runs auth-disabled today — no claims).
const CALLER_HEADER: &str = "X-Insight-Person-Id";

/// Upper bound on one seed run in the serial worker; a stall past this fails the
/// job rather than wedging the whole queue.
const SEED_TIMEOUT: Duration = Duration::from_mins(10);
const PERSONS_SEED_OP: &str = "persons-seed";

/// A queued persons-seed job handed from the POST handler to the worker.
#[allow(clippy::struct_field_names)] // all three fields are ids by nature
#[derive(Debug, Clone, Copy)]
pub struct PersonsSeedJob {
    pub operation_id: Uuid,
    pub tenant_id: Uuid,
    pub author_person_id: Uuid,
}

/// Body of `POST /v1/persons-seed`. `mode` defaults to `link_by_email`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PersonsSeedRequest {
    #[serde(default)]
    pub mode: Option<String>,
}
impl toolkit::api::api_dto::RequestApiDto for PersonsSeedRequest {}

/// One operation's status (POST returns the queued row; GETs return current).
#[derive(Debug, Serialize, ToSchema)]
pub struct PersonsSeedOperationResponse {
    pub operation_id: Uuid,
    pub operation_type: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}
impl toolkit::api::api_dto::ResponseApiDto for PersonsSeedOperationResponse {}

impl From<Operation> for PersonsSeedOperationResponse {
    fn from(op: Operation) -> Self {
        Self {
            operation_id: op.operation_id,
            operation_type: op.operation_type,
            status: op.status.as_db().to_owned(),
            request_json: op.request_json,
            summary_json: op.summary_json,
            error_message: op.error_message,
            started_at: op.started_at.to_string(),
            completed_at: op.completed_at.map(|t| t.to_string()),
        }
    }
}

/// List response wrapper (typed for OpenAPI).
#[derive(Debug, Serialize, ToSchema)]
pub struct PersonsSeedListResponse {
    pub items: Vec<PersonsSeedOperationResponse>,
}
impl toolkit::api::api_dto::ResponseApiDto for PersonsSeedListResponse {}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub status: Option<String>,
}

/// `POST /v1/persons-seed` — enqueue an async persons-seed run.
pub async fn create_persons_seed(
    Extension(state): Extension<Arc<AppState>>,
    Extension(ctx): Extension<SecurityContext>,
    headers: HeaderMap,
    CanonicalJson(req): CanonicalJson<PersonsSeedRequest>,
) -> Result<impl IntoResponse, CanonicalError> {
    let mode = req
        .mode
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .unwrap_or(LINK_BY_EMAIL_MODE);
    if mode != LINK_BY_EMAIL_MODE {
        return Err(PersonsSeedError::invalid_argument()
            .with_field_violation(
                "mode",
                "unsupported mode; only 'link_by_email' is available",
                "INVALID",
            )
            .create());
    }

    let tenant = ctx.subject_tenant_id();

    // Admin gate — parity with the .NET `CallerAdminCheck`: resolve the caller
    // from the `X-Insight-Person-Id` header, then require an active `admin` role
    // in the tenant. The resolved caller is recorded as the author of the job
    // and every seeded observation.
    let author = resolve_caller(&headers).ok_or_else(|| {
        CanonicalError::unauthenticated()
            .with_reason(format!(
                "caller not identified; send the {CALLER_HEADER} header"
            ))
            .create()
    })?;
    let is_admin = roles_repo::has_active_admin(&state.db, tenant, author)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "admin role check failed");
            CanonicalError::internal("failed to verify caller permissions").create()
        })?;
    if !is_admin {
        return Err(PersonsSeedError::permission_denied()
            .with_reason("admin role required for this operation")
            .create());
    }

    let operation_id = Uuid::now_v7();
    let request_json = serde_json::json!({ "mode": mode }).to_string();

    ops_repo::enqueue(
        &state.db,
        operation_id,
        PERSONS_SEED_OP,
        tenant,
        author,
        Some(&request_json),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "enqueue operation failed");
        CanonicalError::internal("failed to enqueue seed").create()
    })?;

    let job = PersonsSeedJob {
        operation_id,
        tenant_id: tenant,
        author_person_id: author,
    };
    if state.seed_tx.try_send(job).is_err() {
        // Channel full/closed — fail the row so it isn't a zombie.
        let _ = ops_repo::fail(&state.db, operation_id, "seed queue full; retry later").await;
        return Err(CanonicalError::internal("seed queue full; retry later").create());
    }

    let op = load_op(&state.db, tenant, operation_id).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(PersonsSeedOperationResponse::from(op)),
    ))
}

/// `GET /v1/persons-seed/{id}` — poll one operation.
pub async fn get_persons_seed(
    Extension(state): Extension<Arc<AppState>>,
    Extension(ctx): Extension<SecurityContext>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, CanonicalError> {
    let tenant = ctx.subject_tenant_id();
    let op = ops_repo::get_by_id(&state.db, tenant, id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "get operation failed");
            CanonicalError::internal("failed to read operation").create()
        })?
        .filter(|o| o.operation_type == PERSONS_SEED_OP)
        .ok_or_else(|| {
            PersonsSeedError::not_found("operation not found")
                .with_resource(id.to_string())
                .create()
        })?;
    Ok(Json(PersonsSeedOperationResponse::from(op)))
}

/// `GET /v1/persons-seed` — list persons-seed operations (optional `?status=`).
pub async fn list_persons_seed(
    Extension(state): Extension<Arc<AppState>>,
    Extension(ctx): Extension<SecurityContext>,
    Query(params): Query<ListParams>,
) -> Result<impl IntoResponse, CanonicalError> {
    let tenant = ctx.subject_tenant_id();
    let status = match params.status.as_deref() {
        None | Some("") => None,
        Some(s) => Some(parse_status(s)?),
    };
    let ops = ops_repo::list(&state.db, tenant, Some(PERSONS_SEED_OP), status)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "list operations failed");
            CanonicalError::internal("failed to list operations").create()
        })?;
    let items = ops
        .into_iter()
        .map(PersonsSeedOperationResponse::from)
        .collect();
    Ok(Json(PersonsSeedListResponse { items }))
}

async fn load_op(
    db: &DatabaseConnection,
    tenant: Uuid,
    operation_id: Uuid,
) -> Result<Operation, CanonicalError> {
    ops_repo::get_by_id(db, tenant, operation_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "read operation failed");
            CanonicalError::internal("failed to read operation").create()
        })?
        .ok_or_else(|| CanonicalError::internal("operation vanished after enqueue").create())
}

/// Resolve the caller's `person_id` from the `X-Insight-Person-Id` header —
/// the header branch of the .NET `HeaderCallerContext` (present, parseable,
/// non-nil). Returns `None` when absent/blank/malformed/nil, which the handler
/// maps to 401. The JWT id/email-claim fallbacks are intentionally not ported
/// yet (auth-disabled host → no claims to read).
fn resolve_caller(headers: &HeaderMap) -> Option<Uuid> {
    let raw = headers.get(CALLER_HEADER)?.to_str().ok()?;
    let id = Uuid::parse_str(raw.trim()).ok()?;
    (!id.is_nil()).then_some(id)
}

fn parse_status(s: &str) -> Result<OperationStatus, CanonicalError> {
    match s {
        "queued" => Ok(OperationStatus::Queued),
        "running" => Ok(OperationStatus::Running),
        "completed" => Ok(OperationStatus::Completed),
        "failed" => Ok(OperationStatus::Failed),
        _ => Err(PersonsSeedError::invalid_argument()
            .with_field_violation(
                "status",
                "status must be one of queued|running|completed|failed",
                "INVALID",
            )
            .create()),
    }
}

/// Background worker: drain the queue and run each seed to completion, updating
/// the `operations` row. Spawned once from the gear `init`; ends when the
/// channel closes (all senders dropped).
pub async fn run_worker(
    mut rx: mpsc::Receiver<PersonsSeedJob>,
    db: DatabaseConnection,
    config: GearConfig,
) {
    let reader = ClickHouseIdentityInputsReader::connect(
        &config.clickhouse_url,
        &config.clickhouse_database,
        &config.clickhouse_user,
        &config.clickhouse_password,
    );
    let store = MariaDbSeedStore::new(&db);

    while let Some(job) = rx.recv().await {
        // Only the worker that wins queued→running proceeds (no double-run).
        match ops_repo::try_start(&db, job.operation_id).await {
            Ok(true) => {}
            Ok(false) => continue,
            Err(e) => {
                // A transient DB blip on the queued→running transition must not
                // strand the (already-consumed) job as a zombie `queued` row —
                // mark it failed so it isn't stuck forever, like the queue-full
                // path in `create_persons_seed`.
                tracing::error!(error = %e, operation_id = %job.operation_id, "try_start failed");
                let _ =
                    ops_repo::fail(&db, job.operation_id, "try_start failed; retry later").await;
                continue;
            }
        }

        // Bound each run: the worker is single-threaded and serial, so a hung
        // ClickHouse/MariaDB call would otherwise block every subsequent job for
        // every tenant until the process restarts. Generous ceiling — a healthy
        // large-tenant seed is seconds; this only trips on a real stall.
        let seed = run_seed(
            &reader,
            &store,
            job.tenant_id,
            job.author_person_id,
            Uuid::now_v7,
        );
        let result = tokio::time::timeout(SEED_TIMEOUT, seed)
            .await
            .unwrap_or_else(|_| Err(anyhow::anyhow!("persons-seed timed out")));

        match result {
            Ok(summary) => {
                let summary_json =
                    serde_json::to_string(&summary).unwrap_or_else(|_| "{}".to_owned());
                if let Err(e) = ops_repo::complete(&db, job.operation_id, &summary_json).await {
                    tracing::error!(error = %e, operation_id = %job.operation_id, "complete failed");
                }
            }
            Err(e) => {
                // Log the real error server-side, but persist only a generic
                // message: `error_message` is returned verbatim by the GET/list
                // endpoints, so raw driver/anyhow text must not leak to callers.
                tracing::error!(error = %e, operation_id = %job.operation_id, "persons-seed failed");
                if let Err(e2) = ops_repo::fail(
                    &db,
                    job.operation_id,
                    "persons-seed failed; see server logs",
                )
                .await
                {
                    tracing::error!(error = %e2, operation_id = %job.operation_id, "fail update failed");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with(value: &str) -> anyhow::Result<HeaderMap> {
        let mut h = HeaderMap::new();
        h.insert(CALLER_HEADER, value.parse()?);
        Ok(h)
    }

    #[test]
    fn resolve_caller_reads_valid_person_header() -> anyhow::Result<()> {
        let id = Uuid::from_u128(0x1234_5678_9abc_def0);
        assert_eq!(resolve_caller(&headers_with(&id.to_string())?), Some(id));
        // Surrounding whitespace is tolerated.
        assert_eq!(
            resolve_caller(&headers_with(&format!("  {id}  "))?),
            Some(id)
        );
        Ok(())
    }

    #[test]
    fn resolve_caller_rejects_missing_blank_nil_and_malformed() -> anyhow::Result<()> {
        assert_eq!(resolve_caller(&HeaderMap::new()), None, "absent header");
        assert_eq!(resolve_caller(&headers_with("")?), None, "blank");
        assert_eq!(
            resolve_caller(&headers_with("not-a-uuid")?),
            None,
            "malformed"
        );
        assert_eq!(
            resolve_caller(&headers_with(&Uuid::nil().to_string())?),
            None,
            "nil uuid is not a caller"
        );
        Ok(())
    }
}
