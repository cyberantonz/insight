# DESIGN — OpenAI API Connector

> Version 1.0 — March 2026
> Based on: `docs/CONNECTORS_REFERENCE.md` Source 18 (OpenAI API), OQ-3

<!-- toc -->

- [1. Overview](#1-overview)
- [2. Bronze Tables](#2-bronze-tables)
  - [`openai_api_daily_usage` — Daily token usage per API key per model](#openaiapidailyusage-daily-token-usage-per-api-key-per-model)
  - [`openai_api_requests` — Individual API request events](#openaiapirequests-individual-api-request-events)
  - [`openai_api_collection_runs` — Connector execution log](#openaiapicollectionruns-connector-execution-log)
- [3. Identity Resolution](#3-identity-resolution)
- [4. Silver / Gold Mappings](#4-silver--gold-mappings)

<!-- /toc -->

---

## 1. Overview

**API**: OpenAI Usage API (`/v1/usage`)

**Category**: AI Tool

**Authentication**: Admin API key (OpenAI Platform)

**Identity**: `user_id` in `openai_api_requests` — the value of the `user` field in the request body, set by the caller. Nullable — attribution is only possible when the calling application includes this field. `openai_api_daily_usage` has no user-level attribution.

**Field naming**: snake_case — preserved as-is at Bronze level.

**Why two tables**: Same pattern as Claude API (Source 13) — daily aggregates (always available) and per-request events (require caller instrumentation). The `user` field in OpenAI requests is a caller convention, not enforced.

**Parallel to Claude API**: This connector mirrors the Claude API connector structure. The main OpenAI-specific difference is `reasoning_tokens` — an internal thinking token count specific to reasoning models (o1, o3) that are billed but not visible in the response.

---

## 2. Bronze Tables

### `openai_api_daily_usage` — Daily token usage per API key per model

| Field | Type | Description |
|-------|------|-------------|
| `date` | Date | Usage date |
| `api_key_id` | String | API key identifier (name or last-4 alias) |
| `model` | String | Model ID, e.g. `gpt-4o`, `gpt-4o-mini`, `o1`, `o3-mini` |
| `request_count` | Float64 | Number of API requests |
| `input_tokens` | Float64 | Input (prompt) tokens consumed |
| `output_tokens` | Float64 | Output (completion) tokens generated |
| `cached_tokens` | Float64 | Tokens served from prompt cache |
| `reasoning_tokens` | Float64 | Internal reasoning tokens (o1/o3 models only; billed but not in output) |
| `total_cost_cents` | Float64 | Total cost in cents |

Granularity: one row per `(date, api_key_id, model)`. `reasoning_tokens` is zero for non-reasoning models.

---

### `openai_api_requests` — Individual API request events

Available when the caller passes a `user` field in the request body. Without this field, requests are not recorded at this level.

| Field | Type | Description |
|-------|------|-------------|
| `request_id` | String | Unique request ID from response headers |
| `timestamp` | DateTime64(3) | Request timestamp |
| `api_key_id` | String | API key used |
| `user_id` | String | Value of `user` field in the request body — caller-defined identifier (nullable) |
| `model` | String | Model ID |
| `input_tokens` | Float64 | Input tokens |
| `output_tokens` | Float64 | Output tokens |
| `cached_tokens` | Float64 | Cached tokens |
| `reasoning_tokens` | Float64 | Reasoning tokens (o1/o3 only; nullable) |
| `cost_cents` | Float64 | Request cost in cents |
| `finish_reason` | String | Why generation stopped: `stop` / `length` / `tool_calls` / `content_filter` |
| `application` | String | Internal application tag (caller-set convention) |

`reasoning_tokens` is specific to o1/o3 models — they consume internal tokens before producing a response; these are billed but not visible in `output_tokens`.

---

### `openai_api_collection_runs` — Connector execution log

| Field | Type | Description |
|-------|------|-------------|
| `run_id` | String | Unique run identifier |
| `started_at` | DateTime64(3) | Run start time |
| `completed_at` | DateTime64(3) | Run end time |
| `status` | String | `running` / `completed` / `failed` |
| `daily_usage_records_collected` | Float64 | Rows collected for `openai_api_daily_usage` |
| `request_records_collected` | Float64 | Rows collected for `openai_api_requests` |
| `api_calls` | Float64 | Admin API calls made |
| `errors` | Float64 | Errors encountered |
| `settings` | String | Collection configuration (organization, lookback period, key filter) |

Monitoring table — not an analytics source.

---

## 3. Identity Resolution

Same pattern as Claude API (Source 13):

- `openai_api_daily_usage`: **no user attribution** — usage is attributable only to `api_key_id`.
- `openai_api_requests.user_id`: nullable — present only when the calling application includes the `user` field. Mapping from `user_id` to `person_id` requires client-specific Identity Manager configuration.

---

## 4. Silver / Gold Mappings

| Bronze table | Silver target | Status |
|-------------|--------------|--------|
| `openai_api_daily_usage` | `class_ai_api_usage` | Planned — stream not yet defined |
| `openai_api_requests` | `class_ai_api_usage` | Planned — with nullable `person_id` |

**Gold**: AI API cost analytics (spend per API key, per model, per application). Parallel to Claude API Gold metrics — both feed `class_ai_api_usage` enabling cross-provider spend comparison.
