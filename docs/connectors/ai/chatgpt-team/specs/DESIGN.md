# DESIGN — ChatGPT Team Connector

> Version 1.0 — March 2026
> Based on: `docs/CONNECTORS_REFERENCE.md` Source 19 (ChatGPT Team)

<!-- toc -->

- [1. Overview](#1-overview)
- [2. Bronze Tables](#2-bronze-tables)
  - [`chatgpt_team_seats` — Seat assignment and status](#chatgptteamseats-seat-assignment-and-status)
  - [`chatgpt_team_activity` — Daily usage per user per model](#chatgptteamactivity-daily-usage-per-user-per-model)
  - [`chatgpt_team_collection_runs` — Connector execution log](#chatgptteamcollectionruns-connector-execution-log)
- [3. Identity Resolution](#3-identity-resolution)
- [4. Silver / Gold Mappings](#4-silver--gold-mappings)

<!-- /toc -->

---

## 1. Overview

**API**: OpenAI Admin API — workspace user management and usage reports for Team/Enterprise accounts

**Category**: AI Tool

**Authentication**: Admin API key (OpenAI Platform — Team/Enterprise plan)

**Identity**: `email` in both `chatgpt_team_seats` and `chatgpt_team_activity` — resolved to canonical `person_id` via Identity Manager.

**Field naming**: snake_case — preserved as-is at Bronze level.

**Why two tables**: Seat assignment (one row per user) and daily activity (many rows per user over time) are different entities — same pattern as Claude Team Plan (Source 14).

**Parallel to Claude Team Plan**: Same two-table model — seats + daily activity. ChatGPT Team covers `chatgpt.com` web interface, desktop app, and mobile. The billing model is flat per-seat (no per-request cost). Reasoning tokens appear in the activity table for o1/o3 model usage.

| Aspect | OpenAI API (Source 18) | ChatGPT Team (Source 19) |
|--------|------------------------|--------------------------|
| Billing | Pay-per-token | Fixed per-seat/month |
| Access | `api.openai.com` | `chatgpt.com` + desktop app |
| Clients | Programmatic only | `web`, `desktop`, `mobile` |

---

## 2. Bronze Tables

### `chatgpt_team_seats` — Seat assignment and status

| Field | Type | Description |
|-------|------|-------------|
| `user_id` | String | OpenAI platform user ID |
| `email` | String | User email — primary key for cross-system identity resolution |
| `role` | String | `owner` / `admin` / `member` |
| `status` | String | `active` / `inactive` / `pending` |
| `added_at` | DateTime64(3) | When the seat was assigned |
| `last_active_at` | DateTime64(3) | Last recorded activity |

One row per user. Current-state only — no versioning.

---

### `chatgpt_team_activity` — Daily usage per user per model

| Field | Type | Description |
|-------|------|-------------|
| `user_id` | String | OpenAI platform user ID |
| `email` | String | User email — identity key |
| `date` | Date | Activity date |
| `client` | String | `web` / `desktop` / `mobile` |
| `model` | String | Model used, e.g. `gpt-4o`, `o1`, `o3-mini` |
| `conversation_count` | Float64 | Number of distinct conversations |
| `message_count` | Float64 | Messages sent |
| `input_tokens` | Float64 | Input tokens consumed |
| `output_tokens` | Float64 | Output tokens generated |
| `reasoning_tokens` | Float64 | Reasoning tokens (o1/o3 models only; billed but not in output) |

No `cost_cents` — flat subscription.

`reasoning_tokens` is present for o1/o3 model usage — ChatGPT Team exposes this to workspace admins even though it is not billed separately under the flat subscription.

---

### `chatgpt_team_collection_runs` — Connector execution log

| Field | Type | Description |
|-------|------|-------------|
| `run_id` | String | Unique run identifier |
| `started_at` | DateTime64(3) | Run start time |
| `completed_at` | DateTime64(3) | Run end time |
| `status` | String | `running` / `completed` / `failed` |
| `seats_collected` | Float64 | Rows collected for `chatgpt_team_seats` |
| `activity_records_collected` | Float64 | Rows collected for `chatgpt_team_activity` |
| `api_calls` | Float64 | Admin API calls made |
| `errors` | Float64 | Errors encountered |
| `settings` | String | Collection configuration (workspace, lookback period) |

Monitoring table — not an analytics source.

---

## 3. Identity Resolution

`email` in both `chatgpt_team_seats` and `chatgpt_team_activity` is the primary identity key — resolved to canonical `person_id` via Identity Manager in Silver step 2.

`user_id` (OpenAI platform user ID) is OpenAI-internal — not used for cross-system resolution.

---

## 4. Silver / Gold Mappings

| Bronze table | Silver target | Status |
|-------------|--------------|--------|
| `chatgpt_team_seats` | *(seat roster)* | Available — no unified stream defined yet |
| `chatgpt_team_activity` | `class_ai_tool_usage` | Planned — alongside Claude Team web/mobile |

**Gold**: General AI tool adoption metrics (active users, conversation volume, model distribution, client breakdown). Alongside Claude Team Plan web/mobile activity, enables cross-provider AI tool adoption analytics.
