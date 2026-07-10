//! The identity-resolution gear.
//!
//! Runs on the `api-gateway` system gear (the REST host) under
//! `toolkit::bootstrap::run_server`. Runtime construction (config, and — next
//! step — the MariaDB pool) happens in [`IdentityResolutionGear::init`]. No
//! domain routes yet: [`IdentityResolutionGear::register_rest`] returns the host
//! router unchanged for now.

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use toolkit::api::OpenApiRegistry;
use toolkit::{Gear, GearCtx, RestApiCapability};

use crate::config::GearConfig;

/// Shared application state. Injected into handlers once we add routes.
#[derive(Clone)]
pub struct AppState {
    /// MariaDB connection pool (SeaORM) — reads `persons` / `account_person_map`.
    #[allow(dead_code)] // consumed once the read handlers are wired
    pub db: DatabaseConnection,
    #[allow(dead_code)] // consumed once handlers need runtime config
    pub config: GearConfig,
}

/// Identity-resolution gear. Capability: `rest` (HTTP surface). Config key is
/// the gear name `identity-resolution`; env overrides are
/// `APP__gears__identity-resolution__config__*`.
#[derive(Default)]
#[toolkit::gear(name = "identity-resolution", capabilities = [rest])]
pub struct IdentityResolutionGear {
    state: OnceLock<Arc<AppState>>,
}

#[async_trait]
impl Gear for IdentityResolutionGear {
    async fn init(&self, ctx: &GearCtx) -> anyhow::Result<()> {
        let config: GearConfig = ctx.config()?;
        tracing::info!("starting identity-resolution gear");

        // Self-managed MariaDB pool (same approach as the analytics gear).
        let db = crate::infra::db::connect(&config.database_url).await?;

        let state = AppState { db, config };
        self.state
            .set(Arc::new(state))
            .map_err(|_| anyhow::anyhow!("{} gear already initialized", Self::MODULE_NAME))?;
        Ok(())
    }
}

impl RestApiCapability for IdentityResolutionGear {
    fn register_rest(
        &self,
        _ctx: &GearCtx,
        router: axum::Router,
        _openapi: &dyn OpenApiRegistry,
    ) -> anyhow::Result<axum::Router> {
        // No domain routes yet — return the host router unchanged. The next
        // steps read `self.state`, then register `POST /v1/profiles` +
        // `GET /v1/persons/{email}` via the toolkit `OperationBuilder`.
        Ok(router)
    }
}
