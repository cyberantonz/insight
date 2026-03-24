# PRD — BambooHR Connector

> Version 1.0 — March 2026
> Based on: HR Directory domain (`docs/components/connectors/hr-directory/README.md`)

<!-- toc -->

- [1. Overview](#1-overview)
  - [1.1 Purpose](#11-purpose)
  - [1.2 Background / Problem Statement](#12-background--problem-statement)
  - [1.3 Goals (Business Outcomes)](#13-goals-business-outcomes)
  - [1.4 Glossary](#14-glossary)
- [2. Actors](#2-actors)
  - [2.1 Human Actors](#21-human-actors)
  - [2.2 System Actors](#22-system-actors)
- [3. Operational Concept & Environment](#3-operational-concept--environment)
  - [3.1 Module-Specific Environment Constraints](#31-module-specific-environment-constraints)
- [4. Scope](#4-scope)
  - [4.1 In Scope](#41-in-scope)
  - [4.2 Out of Scope](#42-out-of-scope)
- [5. Functional Requirements](#5-functional-requirements)
  - [5.1 Employee Data Collection](#51-employee-data-collection)
  - [5.2 Leave Request Collection](#52-leave-request-collection)
  - [5.3 Field Metadata Collection](#53-field-metadata-collection)
  - [5.4 Deduplication](#54-deduplication)
  - [5.5 Identity Key](#55-identity-key)
  - [5.6 Incremental Collection](#56-incremental-collection)
  - [5.7 Fault Tolerance](#57-fault-tolerance)
  - [5.8 Collection Runs](#58-collection-runs)
- [6. Non-Functional Requirements](#6-non-functional-requirements)
  - [6.1 NFR Inclusions](#61-nfr-inclusions)
  - [6.2 NFR Exclusions](#62-nfr-exclusions)
- [7. Public Library Interfaces](#7-public-library-interfaces)
  - [7.1 Public API Surface](#71-public-api-surface)
  - [7.2 External Integration Contracts](#72-external-integration-contracts)
- [8. Use Cases](#8-use-cases)
- [9. Acceptance Criteria](#9-acceptance-criteria)
- [10. Dependencies](#10-dependencies)
- [11. Assumptions](#11-assumptions)
- [12. Risks](#12-risks)
- [13. Open Questions](#13-open-questions)

<!-- /toc -->

---

## 1. Overview

### 1.1 Purpose

The BambooHR connector extracts HR directory data — employee records, time-off requests, and field metadata — from the BambooHR REST API v1 into the Insight platform's Bronze layer. This data feeds identity resolution (canonical `person_id` via email), org hierarchy construction, and leave analytics.

### 1.2 Background / Problem Statement

BambooHR is a widely-used HR information system (HRIS) for small-to-medium businesses. The Insight platform requires HR directory data for:

1. **Identity resolution** — mapping source-system user identifiers (GitHub login, Jira account ID, etc.) to real people via work email as the canonical identity anchor.
2. **Org hierarchy** — enabling team-level aggregation of engineering metrics (throughput, cycle time) by department, division, and manager chain.
3. **Leave analytics** — time-off patterns feed burnout risk signals and availability forecasting.

BambooHR returns **current-state records only** — no effective dating or versioning. Historical org changes cannot be reconstructed from BambooHR alone; the Silver layer (SCD Type 2) must snapshot Bronze records over time.

### 1.3 Goals (Business Outcomes)

1. Enable identity resolution for all Insight workspaces using BambooHR as their HR system.
2. Provide org hierarchy data for team-level metric scoping in dashboards.
3. Collect leave request history for availability and burnout risk analytics.
4. Discover custom HR fields per workspace to populate `class_people.custom_str_attrs` and `class_people.custom_num_attrs` at the Silver layer.

### 1.4 Glossary

| Term | Definition |
|------|-----------|
| **Company domain** | The BambooHR subdomain (e.g., `acme` from `acme.bamboohr.com`) identifying the customer account |
| **Custom report** | BambooHR's `POST /reports/custom` endpoint — the primary bulk data extraction mechanism; accepts a field list and returns all matching employee records |
| **Current-state** | BambooHR returns only the latest version of each record; no effective dating or historical snapshots |
| **Identity key** | The field used for cross-system person resolution — `workEmail` for BambooHR |

---

## 2. Actors

### 2.1 Human Actors

- [ ] `p1` - **ID**: `cpt-insightspec-actor-bhr-platform-engineer`

**Platform Engineer** — configures BambooHR connections (API key, company domain, field selection), monitors collection runs, and troubleshoots extraction failures.

- [ ] `p2` - **ID**: `cpt-insightspec-actor-bhr-data-analyst`

**Data Analyst** — consumes BambooHR Bronze data through Silver/Gold layers for org hierarchy analysis, headcount reporting, and leave pattern analytics.

### 2.2 System Actors

- [ ] `p1` - **ID**: `cpt-insightspec-actor-bhr-orchestrator`

**Orchestrator** — triggers BambooHR connector runs on schedule, provides state (cursor) from previous runs, and routes output to the destination.

- [ ] `p1` - **ID**: `cpt-insightspec-actor-bhr-identity-manager`

**Identity Manager** — consumes `workEmail` from `bamboohr_employees` to maintain the canonical `person_id` mapping used by all Silver streams.

- [ ] `p1` - **ID**: `cpt-insightspec-actor-bhr-destination`

**Destination (ClickHouse)** — receives extracted records and writes them to Bronze tables.

---

## 3. Operational Concept & Environment

### 3.1 Module-Specific Environment Constraints

- BambooHR API access requires a valid API key with read permissions for the company account.
- The API uses HTTP Basic Authentication with the API key as username and any string (e.g., `x`) as password.
- BambooHR does not publish numeric rate limits; the API returns `503 Service Unavailable` with an optional `Retry-After` header when request frequency is too high.
- The custom report endpoint (`POST /reports/custom`) returns all matching records in a single response — no pagination. Response sizes are bounded by the number of employees in the BambooHR account (typically < 10,000 for SMB customers).
- All API requests must use HTTPS with UTF-8 encoding.

---

## 4. Scope

### 4.1 In Scope

- Extraction of employee directory data via custom reports (insights-relevant fields only — identity, org hierarchy, job, location, dates, employment type).
- Extraction of time-off (leave) requests with date-range filtering.
- Extraction of field metadata (standard + custom field definitions) for schema discovery.
- Extraction of table metadata and list metadata (department values, field options).
- Full refresh sync on all streams (no incremental — BambooHR is current-state only).
- Error handling with retry on transient failures (503, 500) and rate limiting.
- `tenant_id` injection on all records (required platform invariant for tenant isolation).
- Metadata enrichment (`_source`, `_extracted_at`) on all records.

### 4.2 Out of Scope

- Silver/Gold layer transformations (handled by HR Silver ETL Job).
- Identity resolution logic (handled by Identity Manager).
- BambooHR OAuth 2.0 authentication (API key is sufficient for read-only extraction).
- Write operations (employee creation, update, time-off approval).
- Department hierarchy as a separate stream (department data is inline in employee records and `meta/lists`; hierarchy construction is a Silver concern).
- Custom field value extraction as a separate stream (custom fields are per-deployment; the `meta_fields` stream enables discovery).
- Tabular data endpoints (job info history, compensation history) — deferred to future iteration.
- Fields without analytics value (phone numbers, social profiles, photos, address details, sensitive demographics).

---

## 5. Functional Requirements

### 5.1 Employee Data Collection

- [ ] `p1` - **ID**: `cpt-insightspec-fr-bhr-collect-employees`

The system **MUST** extract employee records from the BambooHR custom report endpoint (`POST /reports/custom`), collecting only fields with clear analytics/insight value: employee ID, display name, first name, last name, work email, employee number, department, division, job title, employment status, supervisor (name, ID, email), location, country, city, hire date, original hire date, termination date, last changed timestamp, pay type, and standard hours per week.

**Rationale**: Employee data is the foundation for identity resolution, org hierarchy, and all person-level analytics in the Insight platform.

**Actors**: `cpt-insightspec-actor-bhr-orchestrator`, `cpt-insightspec-actor-bhr-destination`

### 5.2 Leave Request Collection

- [ ] `p1` - **ID**: `cpt-insightspec-fr-bhr-collect-leave-requests`

The system **MUST** extract time-off requests from `GET /time_off/requests` using date-range parameters (`start`, `end`), collecting: request ID, employee ID, status, leave type, start date, end date, amount, creation date, and notes (excluding note text — metadata only).

**Rationale**: Leave request data feeds burnout risk signals, availability forecasting, and team capacity analytics.

**Actors**: `cpt-insightspec-actor-bhr-orchestrator`, `cpt-insightspec-actor-bhr-destination`

### 5.3 Field Metadata Collection

- [ ] `p2` - **ID**: `cpt-insightspec-fr-bhr-collect-meta-fields`

The system **MUST** extract field metadata from `GET /meta/fields`, collecting field IDs, display names, types, and aliases for both standard and custom fields.

**Rationale**: Field metadata enables discovery of custom HR fields per BambooHR account, which feeds the Custom Attributes Normalizer at the Silver layer to populate `class_people.custom_str_attrs` and `class_people.custom_num_attrs`.

**Actors**: `cpt-insightspec-actor-bhr-orchestrator`, `cpt-insightspec-actor-bhr-destination`

### 5.4 Deduplication

- [ ] `p1` - **ID**: `cpt-insightspec-fr-bhr-deduplication`

The system **MUST** define primary keys for each stream to enable deduplication at the destination:
- `bamboohr_employees`: `id` (BambooHR employee ID)
- `bamboohr_leave_requests`: `id` (BambooHR request ID)
- `bamboohr_meta_fields`: `id` (BambooHR field ID)

**Rationale**: Primary keys enable the destination to perform upsert operations, preventing duplicate records across collection runs.

**Actors**: `cpt-insightspec-actor-bhr-destination`

### 5.5 Identity Key

- [ ] `p1` - **ID**: `cpt-insightspec-fr-bhr-identity-key`

The system **MUST** collect `workEmail` for every employee record. This field serves as the primary identity anchor for cross-system person resolution via the Identity Manager.

**Rationale**: Work email is the most reliable cross-system identifier for HR-to-engineering-tool person matching. Without it, the Insight platform cannot attribute engineering metrics to real people.

**Actors**: `cpt-insightspec-actor-bhr-identity-manager`

### 5.6 Incremental Collection

- [ ] `p1` - **ID**: `cpt-insightspec-fr-bhr-incremental-sync`

All streams use **full refresh** sync mode. BambooHR returns current-state records only — there is no reliable server-side incremental mechanism. The custom report endpoint returns all employees in a single response. Leave requests use a fixed date range (`2020-01-01` to current date). Meta endpoints return complete metadata.

**Rationale**: BambooHR is designed for SMB customers (typically < 10,000 employees). Full refresh is simple, reliable, and within the API's response size limits. The `lastChanged` field is retained in the employee schema to enable future client-side incremental sync if needed.

**Actors**: `cpt-insightspec-actor-bhr-orchestrator`

### 5.7 Fault Tolerance

- [ ] `p1` - **ID**: `cpt-insightspec-fr-bhr-fault-tolerance`

The system **MUST** handle transient API failures:
- Retry on HTTP 500, 502, 503, 504 with exponential backoff (base factor 5, max 5 retries).
- Honour `Retry-After` header on HTTP 503 (BambooHR's rate-limiting signal).
- Retry on HTTP 429 with `Retry-After` header.
- Fail the stream on HTTP 401/403 (authentication/authorization errors) without retry.

**Rationale**: BambooHR does not publish numeric rate limits and uses 503 responses for throttling. Robust retry with backoff ensures collection completes under normal API load.

**Actors**: `cpt-insightspec-actor-bhr-orchestrator`

### 5.8 Collection Runs

- [ ] `p1` - **ID**: `cpt-insightspec-fr-bhr-collection-runs`

The system **MUST** emit collection run metadata (start time, end time, status, record counts per stream, error count) to the `bamboohr_collection_runs` monitoring table.

**Rationale**: Collection run tracking enables operational monitoring and alerting on extraction failures or anomalies (e.g., sudden drop in employee count).

**Actors**: `cpt-insightspec-actor-bhr-platform-engineer`

---

## 6. Non-Functional Requirements

### 6.1 NFR Inclusions

- [ ] `p1` - **ID**: `cpt-insightspec-nfr-bhr-auth-flexibility`

The connector **MUST** support API key authentication via HTTP Basic Auth. The API key and company domain **MUST** be configurable via the source connection specification (not hardcoded).

- [ ] `p1` - **ID**: `cpt-insightspec-nfr-bhr-rate-limit-compliance`

The connector **MUST** comply with BambooHR's rate limiting by honouring `Retry-After` headers and implementing exponential backoff on 503/429 responses.

- [ ] `p1` - **ID**: `cpt-insightspec-nfr-bhr-schema-compliance`

All Bronze records **MUST** use source-native field names (BambooHR camelCase) with no field renaming. Schema transformations occur at the Silver layer.

- [ ] `p1` - **ID**: `cpt-insightspec-nfr-bhr-idempotent-writes`

Re-running the connector with the same cursor state **MUST** produce identical Bronze records. The connector does not perform writes — idempotency is ensured by deterministic API responses and primary key-based deduplication at the destination.

### 6.2 NFR Exclusions

- **Performance SLAs**: Not applicable — BambooHR API response times depend on the customer's account size and BambooHR's infrastructure. No latency guarantees.
- **High availability**: The connector runs as a scheduled batch job; no real-time availability requirement.
- **Data encryption at rest**: Handled by the destination (ClickHouse) infrastructure, not the connector.

---

## 7. Public Library Interfaces

### 7.1 Public API Surface

Not applicable. The BambooHR connector is a declarative manifest (YAML) executed by the Airbyte Declarative Connector framework. It does not expose a public API.

### 7.2 External Integration Contracts

- [ ] `p1` - **ID**: `cpt-insightspec-contract-bhr-api-v1`

**BambooHR REST API v1**

| Aspect | Detail |
|--------|--------|
| Base URL | `https://api.bamboohr.com/api/gateway.php/{companyDomain}/v1` |
| Auth | HTTP Basic — API key as username, `x` as password |
| Protocol | HTTPS, UTF-8, JSON responses |
| Rate limits | Undocumented; 503 with optional `Retry-After` on throttling |
| Endpoints consumed | `POST /reports/custom`, `GET /time_off/requests`, `GET /meta/fields` |

---

## 8. Use Cases

- [ ] `p1` - **ID**: `cpt-insightspec-usecase-bhr-initial-full-sync`

**UC-1: Initial Full Sync**

**Trigger**: Platform engineer creates a new BambooHR connection with API key, company domain, and start date.

**Flow**:
1. Orchestrator triggers the connector with no prior state.
2. Connector fetches all employees via custom report (no `lastChanged` filter on first run — all records emitted).
3. Connector fetches all leave requests from start date to current date using date-range windows.
4. Connector fetches field metadata (full sync).
5. All records are written to Bronze tables via the destination.
6. Collection run metadata is emitted.

**Postcondition**: All three Bronze streams are populated. Cursor state is stored for subsequent incremental runs.

---

- [ ] `p1` - **ID**: `cpt-insightspec-usecase-bhr-incremental-sync`

**UC-2: Incremental Sync**

**Trigger**: Orchestrator triggers a scheduled run with stored cursor state.

**Flow**:
1. Connector fetches all employees via custom report. Client-side filtering emits only records with `lastChanged` after the stored cursor.
2. Connector fetches leave requests for the date range since the last sync (with lookback window).
3. Connector fetches field metadata (full sync).
4. New/updated records are written to Bronze tables.
5. Cursor state is updated.

**Postcondition**: Bronze tables reflect changes since the last run. Destination deduplicates on primary keys.

---

- [ ] `p2` - **ID**: `cpt-insightspec-usecase-bhr-identity-feed`

**UC-3: Identity Manager Feed**

**Trigger**: Fresh employee records land in `bamboohr_employees` Bronze table.

**Flow**:
1. Identity Manager reads new/updated employee records from `bamboohr_employees`.
2. For each record, resolves `workEmail` to canonical `person_id`.
3. Updates the identity mapping table.

**Postcondition**: All BambooHR employees have a canonical `person_id` usable by all Silver streams.

---

## 9. Acceptance Criteria

1. The connector successfully extracts employee records from a BambooHR test account and writes them to the destination.
2. The connector successfully extracts leave requests within a specified date range.
3. Incremental sync for employees correctly filters by `lastChanged` cursor.
4. Date-range incremental sync for leave requests correctly bounds requests to the sync window with lookback.
5. The connector retries on 503 responses and respects `Retry-After` headers.
6. The connector fails gracefully on 401/403 with a clear error message.
7. All records include `tenant_id` (from config), `_source` = `bamboohr`, and `_extracted_at` timestamp.
8. Inline schemas match the DESIGN §3.7 table definitions.

---

## 10. Dependencies

| Dependency | Type | Purpose |
|-----------|------|---------|
| BambooHR REST API v1 | External | Source system API |
| Airbyte Declarative Connector framework (CDK v6.44+) | Runtime | Connector execution engine |
| ClickHouse destination connector | Runtime | Bronze table writes |
| Identity Manager | Downstream | Consumes `workEmail` for person resolution |

---

## 11. Assumptions

1. The BambooHR API key has read access to the custom report, time-off requests, and meta/fields endpoints.
2. BambooHR employee counts are < 10,000 per account (SMB focus), so full-dataset responses from the custom report endpoint are manageable without pagination.
3. The `lastChanged` field is updated by BambooHR whenever any employee field changes.
4. BambooHR's `Retry-After` header provides a reasonable wait time (seconds) on 503 responses.

---

## 12. Risks

| Risk | Impact | Mitigation |
|------|--------|-----------|
| BambooHR deprecates the gateway API URL pattern | Connector breaks | Monitor BambooHR API changelog; update `url_base` if gateway URL is deprecated |
| Custom report response exceeds memory for very large accounts | OOM failure | Set `onlyCurrent=true` to limit to active+terminated employees; monitor response sizes |
| `lastChanged` field not updated for all field types | Missed incremental updates | Periodically trigger full sync; document known `lastChanged` coverage gaps |
| BambooHR throttles aggressively for some accounts | Slow or failed collection | Exponential backoff with `Retry-After` compliance; configurable retry limits |

---

## 13. Open Questions

### OQ-BHR-1: Current-state records — historical org change tracking

BambooHR returns only current-state records. When a person moves departments, the previous department is lost at Bronze level.

- Should the connector snapshot employees daily (creating versioned records with `_extracted_at`)?
- Or is current-state sufficient, with SCD Type 2 tracking at the Silver layer providing the historical dimension?

### OQ-BHR-2: Leave type normalisation across HR systems

`bamboohr_leave_requests` leave types are freeform and client-configured. Normalisation to a canonical enum (`vacation` / `sick` / `parental` / `other`) is a Silver/Gold concern.

- Should the connector extract leave type metadata (policy names, categories) to assist Silver normalisation?
- Or is raw leave type sufficient for Bronze?

### OQ-BHR-3: Custom field inclusion in employee report

The custom report endpoint accepts an arbitrary field list. Currently the connector requests a fixed set of standard fields.

- Should custom field IDs be configurable via the source connection specification?
- Or should the connector always request all available fields (discovered via `GET /meta/fields`)?

### OQ-BHR-4: Tabular data endpoints

BambooHR provides tabular data (`GET /employees/{id}/tables/{tableName}`) for job info history, compensation history, and employment status history. These endpoints provide effective-dated records that BambooHR's current-state employee endpoint does not.

- Should job info / compensation / employment status history be added as separate Bronze streams?
- This would require parent-child stream design (employee IDs → table rows per employee).
