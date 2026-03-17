# PRD — Claude API Connector

> Version 1.0 — March 2026
> Based on: `docs/CONNECTORS_REFERENCE.md` Source 13 (Claude API), OQ-3

<!-- toc -->

- [1. Overview](#1-overview)
  - [1.1 Purpose](#11-purpose)
  - [1.2 Background / Problem Statement](#12-background--problem-statement)
  - [1.3 Goals](#13-goals)
  - [1.4 Glossary](#14-glossary)
- [2. Actors](#2-actors)
  - [2.1 Human Actors](#21-human-actors)
  - [2.2 System Actors](#22-system-actors)
- [3. Scope](#3-scope)
  - [3.1 In Scope](#31-in-scope)
  - [3.2 Out of Scope](#32-out-of-scope)
- [4. Functional Requirements](#4-functional-requirements)
  - [4.1 Daily Usage Collection](#41-daily-usage-collection)
  - [4.2 Per-Request Event Collection](#42-per-request-event-collection)
  - [4.3 Identity Resolution](#43-identity-resolution)
  - [4.4 Silver / Gold Pipeline](#44-silver--gold-pipeline)
- [5. Non-Functional Requirements](#5-non-functional-requirements)
- [6. Open Questions](#6-open-questions)

<!-- /toc -->

---

## 1. Overview

### 1.1 Purpose

The Claude API connector collects daily token usage aggregates and per-request event data from the Anthropic Admin API. It enables organizations to track programmatic AI API spend, attribute usage to teams or applications, and build cost analytics across API keys and models.

### 1.2 Background / Problem Statement

Organizations using the Anthropic Claude API for internal tooling, automations, or AI-powered product features lack centralized visibility into API spend, per-key utilization, and per-application cost attribution. The Anthropic Admin API provides two complementary data surfaces: daily aggregates (always available) and per-request events (available only when callers instrument their requests with a `X-Anthropic-User-Id` header).

Unlike Claude Team Plan (conversational, flat-seat billing), the Claude API is programmatic, pay-per-token, and not associated with individual user sessions at the API level.

### 1.3 Goals

- Collect complete daily API usage aggregates per API key and model.
- Collect per-request events where caller instrumentation permits.
- Enable cost attribution by API key, model, and application tag.
- Resolve `user_id` (when present) to canonical `person_id` for person-level analytics.
- Feed `class_ai_api_usage` Silver stream for cross-provider programmatic API cost analytics.

### 1.4 Glossary

| Term | Definition |
|------|------------|
| `api_key_id` | API key identifier (name or last-4 alias from Anthropic Console) |
| `X-Anthropic-User-Id` | Optional request header set by callers to identify the requesting user |
| `application` | Caller-set convention tag identifying which product or service made an API call |
| `person_id` | Canonical cross-system person identifier resolved by the Identity Manager |
| `class_ai_api_usage` | Silver stream for programmatic API usage (Claude API + OpenAI API) |
| Daily aggregate | One row per `(date, api_key_id, model)` — always available, no user attribution |
| Per-request event | One row per API request — available only when `X-Anthropic-User-Id` is set |

---

## 2. Actors

### 2.1 Human Actors

#### Platform Engineer / Developer

**ID**: `cpt-insightspec-actor-claude-api-developer`

**Role**: Builds internal tooling or product features that call the Anthropic Claude API.
**Needs**: Visibility into their application's API spend and token consumption.

#### Analytics Engineer

**ID**: `cpt-insightspec-actor-claude-api-analytics-eng`

**Role**: Designs and maintains the Silver/Gold pipeline that consumes Claude API Bronze data.
**Needs**: Reliable Bronze tables with stable schemas, consistent cost fields, and clear attribution conventions.

### 2.2 System Actors

#### Anthropic Admin API

**ID**: `cpt-insightspec-actor-claude-api-anthropic-api`

**Role**: Source of daily usage aggregates and per-request event data for the organization's Anthropic account.

#### Identity Manager

**ID**: `cpt-insightspec-actor-claude-api-identity-mgr`

**Role**: Maps `user_id` (caller-defined string from `X-Anthropic-User-Id`) to canonical `person_id`. Requires client-specific configuration for each application's user ID convention.

---

## 3. Scope

### 3.1 In Scope

- Collection of daily token usage aggregates per API key per model.
- Collection of per-request events when callers instrument with `X-Anthropic-User-Id`.
- Connector execution logging for monitoring and observability.
- Conditional identity resolution of `user_id` → `person_id` (only when `user_id` is present).
- Feeding `class_ai_api_usage` Silver stream for programmatic API cost analytics.

### 3.2 Out of Scope

- Conversational Claude Team Plan usage — covered by the Claude Team connector (`class_ai_tool_usage`).
- Enforcement of `X-Anthropic-User-Id` instrumentation in calling applications — this is a caller responsibility.
- Real-time or sub-daily granularity for daily aggregates — the Admin API provides daily resolution only.
- Per-prompt or per-token content — the connector collects metadata and counts, not prompt/response content.

---

## 4. Functional Requirements

### 4.1 Daily Usage Collection

#### Collect daily aggregates

- [ ] `p1` - **ID**: `cpt-insightspec-fr-claude-api-daily-collect`

The connector **MUST** collect daily token usage aggregates from the Anthropic Admin API, capturing request count, input/output/cache tokens, and cost per API key per model.

**Rationale**: Daily aggregates are always available and provide the baseline for API cost analytics regardless of per-request instrumentation coverage.
**Actors**: `cpt-insightspec-actor-claude-api-analytics-eng`

#### Log connector execution

- [ ] `p1` - **ID**: `cpt-insightspec-fr-claude-api-collection-runs`

The connector **MUST** record each execution run with start/end time, status, record counts, API call count, and error count for both daily usage and request event collections.

**Rationale**: Execution logs are required for monitoring data freshness, diagnosing failures, and detecting under-instrumented traffic.
**Actors**: `cpt-insightspec-actor-claude-api-analytics-eng`

### 4.2 Per-Request Event Collection

#### Collect per-request events when available

- [ ] `p1` - **ID**: `cpt-insightspec-fr-claude-api-requests-collect`

The connector **MUST** collect per-request event records when callers have instrumented their requests with `X-Anthropic-User-Id`. Records without this header **MUST NOT** be collected at this granularity.

**Rationale**: Per-request events enable person-level and application-level cost attribution, which is not possible from daily aggregates alone.
**Actors**: `cpt-insightspec-actor-claude-api-analytics-eng`, `cpt-insightspec-actor-claude-api-developer`

#### Treat user_id as nullable

- [ ] `p2` - **ID**: `cpt-insightspec-fr-claude-api-nullable-user`

The `user_id` field in per-request events **MUST** be treated as nullable. Rows with `user_id = NULL` are valid and represent requests where the caller did not set `X-Anthropic-User-Id`.

**Rationale**: Not all callers instrument their requests; unattributed rows must still be collected for cost completeness.
**Actors**: `cpt-insightspec-actor-claude-api-analytics-eng`

### 4.3 Identity Resolution

#### Resolve user_id to person_id conditionally

- [ ] `p1` - **ID**: `cpt-insightspec-fr-claude-api-identity-resolve`

The Silver pipeline **MUST** resolve `user_id` to `person_id` via the Identity Manager when `user_id` is non-null. Rows with `user_id = NULL` **MUST** pass through with `person_id = NULL`.

**Rationale**: Cross-system analytics require `person_id`; NULL rows remain valid for cost attribution by API key and application.
**Actors**: `cpt-insightspec-actor-claude-api-identity-mgr`, `cpt-insightspec-actor-claude-api-analytics-eng`

#### Support caller-convention mapping

- [ ] `p2` - **ID**: `cpt-insightspec-fr-claude-api-identity-convention`

The Identity Manager configuration **MUST** support per-application mapping conventions, as `user_id` may be an email, employee ID, GitHub login, or other caller-defined identifier depending on the calling application.

**Rationale**: `X-Anthropic-User-Id` is a caller convention — there is no enforced format. The Identity Manager must be configurable per application.
**Actors**: `cpt-insightspec-actor-claude-api-identity-mgr`

### 4.4 Silver / Gold Pipeline

#### Feed class_ai_api_usage

- [ ] `p1` - **ID**: `cpt-insightspec-fr-claude-api-silver-api-usage`

Both daily usage aggregates and per-request events **MUST** feed the `class_ai_api_usage` Silver stream. Rows with `person_id = NULL` are valid in this stream and **MUST NOT** be filtered out.

**Rationale**: Cost attribution by API key and application is valid even without person resolution. NULL `person_id` rows enable key-level spend analytics.
**Actors**: `cpt-insightspec-actor-claude-api-analytics-eng`

#### Keep class_ai_api_usage separate from class_ai_tool_usage

- [ ] `p1` - **ID**: `cpt-insightspec-fr-claude-api-silver-separation`

`class_ai_api_usage` (programmatic, pay-per-token) **MUST NOT** be merged with `class_ai_tool_usage` (conversational, flat-seat). There **MUST NOT** be a combined `class_ai_usage` stream.

**Rationale**: Programmatic API usage and conversational tool usage serve different analytics purposes and have incompatible billing models and schemas.
**Actors**: `cpt-insightspec-actor-claude-api-analytics-eng`

---

## 5. Non-Functional Requirements

#### Data freshness

- [ ] `p2` - **ID**: `cpt-insightspec-nfr-claude-api-freshness`

The connector **MUST** be executable on a daily schedule such that daily usage data for day D is available within 48 hours of the end of day D.

**Threshold**: ≤ 48 hours end-to-end latency from API activity to Bronze availability.
**Rationale**: Daily cost reporting requires timely data; a 48h window accommodates known Anthropic API reporting delays.

#### Instrumentation coverage monitoring

- [ ] `p3` - **ID**: `cpt-insightspec-nfr-claude-api-instrumentation-warning`

The connector **SHOULD** emit a warning when the daily aggregate request count significantly exceeds the per-request event count for the same period, indicating uninstrumented traffic.

**Threshold**: Warning when per-request event count < 50% of daily aggregate request count for the same `(date, api_key_id)`.
**Rationale**: Low instrumentation coverage degrades person-level attribution quality; visibility helps teams improve instrumentation.

---

## 6. Open Questions

### OQ-CAPI-1: Per-key user attribution — `X-Anthropic-User-Id` coverage

`claude_api_requests` requires the calling application to pass `X-Anthropic-User-Id`. This is a caller convention, not enforced by Anthropic:

- How much of the client's API traffic is instrumented with this header?
- Should the connector emit a warning when daily_usage request count significantly exceeds request records count?

See also: `CONNECTORS_REFERENCE.md` OQ-3.

### OQ-CAPI-2: `class_ai_api_usage` Silver design — nullable `person_id`

**Status**: CLOSED. `class_ai_api_usage` is the single Silver target for all Claude API usage. Rows with `person_id = NULL` are valid. There is no separate `class_ai_usage` stream. Both daily aggregates and per-request events map to this stream.
