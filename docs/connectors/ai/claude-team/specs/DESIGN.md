# DESIGN — Claude Team Plan Connector

> Version 1.0 — March 2026
> Based on: `docs/CONNECTORS_REFERENCE.md` Source 14 (Claude Team Plan)

<!-- toc -->

- [1. Overview](#1-overview)
- [2. Bronze Tables](#2-bronze-tables)
  - [`claude_team_seats` — Seat assignment and status](#claudeteamseats-seat-assignment-and-status)
  - [`claude_team_activity` — Daily usage per user per model per client](#claudeteamactivity-daily-usage-per-user-per-model-per-client)
  - [`claude_team_collection_runs` — Connector execution log](#claudeteamcollectionruns-connector-execution-log)
- [3. Identity Resolution](#3-identity-resolution)
- [4. Silver / Gold Mappings](#4-silver--gold-mappings)

<!-- /toc -->

---

## 1. Overview

**API**: Anthropic Admin API — user management and usage endpoints for Team/Enterprise accounts

**Category**: AI Tool

**Authentication**: Admin API key (Anthropic Console — Team/Enterprise plan)

**Identity**: `email` in both `claude_team_seats` and `claude_team_activity` — resolved to canonical `person_id` via Identity Manager.

**Field naming**: snake_case — preserved as-is at Bronze level.

**Why two tables**: Seat assignment (one row per user) and daily activity (many rows per user over time) are different entities. Merging would repeat seat metadata on every activity row.

**Key difference from Claude API (Source 13)**: This connector covers usage through the web interface (`web`), mobile app (`mobile`), and Claude Code CLI (`claude_code`) — not programmatic API calls. Billing is flat per-seat/month; per-request cost is not meaningful.

| Aspect | Claude API (Source 13) | Claude Team (Source 14) |
|--------|------------------------|-------------------------|
| Billing | Pay-per-token | Fixed per-seat/month |
| Access | `api.anthropic.com` | `claude.ai` + Claude Code |
| Usage data | Token counts + costs | Token counts, no per-request cost |
| Clients | Programmatic only | `web`, `claude_code`, `mobile` |

**Claude Code** (`client = 'claude_code'`) signals developer AI tool usage — large contexts, long multi-turn sessions, heavy tool use (`stop_reason = 'tool_use'`), high `cache_write_tokens` from system prompt and file context caching.

---

## 2. Bronze Tables

### `claude_team_seats` — Seat assignment and status

| Field | Type | Description |
|-------|------|-------------|
| `user_id` | String | Anthropic platform user ID |
| `email` | String | User email — primary key for cross-system identity resolution |
| `role` | String | `owner` / `admin` / `member` |
| `status` | String | `active` / `inactive` / `pending` |
| `added_at` | DateTime64(3) | When the seat was assigned |
| `last_active_at` | DateTime64(3) | Last recorded activity across all clients |

One row per user. Current-state only — no versioning.

---

### `claude_team_activity` — Daily usage per user per model per client

| Field | Type | Description |
|-------|------|-------------|
| `user_id` | String | Anthropic platform user ID |
| `email` | String | User email — identity key |
| `date` | Date | Activity date |
| `client` | String | `web` / `claude_code` / `mobile` — which surface was used |
| `model` | String | Model ID, e.g. `claude-opus-4-6`, `claude-sonnet-4-6` |
| `message_count` | Float64 | Number of messages / turns sent |
| `conversation_count` | Float64 | Number of distinct conversations or sessions |
| `input_tokens` | Float64 | Input tokens consumed |
| `output_tokens` | Float64 | Output tokens generated |
| `cache_read_tokens` | Float64 | Tokens served from prompt cache |
| `cache_write_tokens` | Float64 | Tokens written to prompt cache |
| `tool_use_count` | Float64 | Tool/function calls made (relevant for Claude Code agent sessions) |

No `cost_cents` field — under a Team subscription the per-token cost is not meaningful; the cost is the seat fee.

**Claude Code signals** (`client = 'claude_code'`): high `tool_use_count`, long multi-turn `conversation_count`, large `cache_write_tokens` from system prompt caching.

---

### `claude_team_collection_runs` — Connector execution log

| Field | Type | Description |
|-------|------|-------------|
| `run_id` | String | Unique run identifier |
| `started_at` | DateTime64(3) | Run start time |
| `completed_at` | DateTime64(3) | Run end time |
| `status` | String | `running` / `completed` / `failed` |
| `seats_collected` | Float64 | Rows collected for `claude_team_seats` |
| `activity_records_collected` | Float64 | Rows collected for `claude_team_activity` |
| `api_calls` | Float64 | Admin API calls made |
| `errors` | Float64 | Errors encountered |
| `settings` | String | Collection configuration (workspace, lookback period) |

Monitoring table — not an analytics source.

---

## 3. Identity Resolution

`email` in both `claude_team_seats` and `claude_team_activity` is the primary identity key — resolved to canonical `person_id` via Identity Manager in Silver step 2.

`user_id` (Anthropic platform user ID) is Anthropic-internal — not used for cross-system resolution.

---

## 4. Silver / Gold Mappings

| Bronze table | Silver target | Status |
|-------------|--------------|--------|
| `claude_team_seats` | *(seat roster)* | Available — no unified stream defined yet |
| `claude_team_activity` (where `client = 'claude_code'`) | `class_ai_dev_usage` | Planned — alongside Cursor and Windsurf |
| `claude_team_activity` (where `client IN ('web', 'mobile')`) | `class_ai_tool_usage` | Planned — alongside ChatGPT Team |

**Gold**: Developer AI tool metrics (Claude Code sessions alongside Cursor/Windsurf) and general AI tool adoption metrics (web/mobile usage alongside ChatGPT Team). The `client` field enables splitting the same source into two different Silver streams.
