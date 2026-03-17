# PRD — Claude Team Plan Connector

> Version 1.0 — March 2026
> Based on: `docs/CONNECTORS_REFERENCE.md` Source 14 (Claude Team Plan)

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
  - [4.1 Seat Data Collection](#41-seat-data-collection)
  - [4.2 Activity Data Collection](#42-activity-data-collection)
  - [4.3 Identity Resolution](#43-identity-resolution)
  - [4.4 Silver / Gold Pipeline](#44-silver--gold-pipeline)
- [5. Non-Functional Requirements](#5-non-functional-requirements)
- [6. Open Questions](#6-open-questions)

<!-- /toc -->

---

## 1. Overview

### 1.1 Purpose

The Claude Team Plan connector collects seat assignment and daily AI tool usage data from Anthropic's Admin API for Claude Team/Enterprise workspaces. It enables analytics teams to track AI assistant adoption across the organization's Claude Team subscription, covering web, mobile, and Claude Code CLI surfaces, and feeds both developer AI tool analytics and general AI tool adoption reporting.

### 1.2 Background / Problem Statement

Organizations running Claude Team subscriptions have no centralized visibility into who is actively using Claude, which surfaces they use (web vs. Claude Code vs. mobile), and how developer AI tool usage (Claude Code) compares to conversational usage (web/mobile). The Anthropic Admin API exposes this data, but it must be ingested, identity-resolved, and routed to two different Silver streams based on the `client` field.

Unlike the Claude API connector (programmatic access, pay-per-token), Claude Team Plan is flat per-seat — different billing model, different clients, different analytics purpose.

### 1.3 Goals

- Collect complete seat roster and daily activity data from the Claude Team workspace.
- Resolve user identity (`email`) to canonical `person_id` for cross-system analytics.
- Route `claude_code` activity to `class_ai_dev_usage` Silver stream (alongside Cursor and Windsurf).
- Route `web` and `mobile` activity to `class_ai_tool_usage` Silver stream (alongside ChatGPT Team).
- Enable Gold-level developer AI adoption metrics and general AI tool adoption metrics.

### 1.4 Glossary

| Term | Definition |
|------|------------|
| Seat | An assigned Claude Team subscription slot for a specific user |
| Activity | Daily per-user usage record: messages, conversations, tokens by model and client |
| `client` | Surface used: `web` / `claude_code` / `mobile` |
| Claude Code | CLI-based AI coding assistant — surfaces developer-style usage patterns (high `tool_use_count`, long sessions, large caches) |
| `person_id` | Canonical cross-system person identifier resolved by the Identity Manager |
| `class_ai_dev_usage` | Silver stream for developer/IDE AI tool usage (Cursor, Windsurf, Claude Code) |
| `class_ai_tool_usage` | Silver stream for conversational AI tool usage (Claude Team web/mobile + ChatGPT Team) |

---

## 2. Actors

### 2.1 Human Actors

#### Workspace Administrator

**ID**: `cpt-insightspec-actor-claude-team-admin`

**Role**: Manages the Claude Team subscription, grants/revokes seat access, monitors usage.
**Needs**: Visibility into seat utilization, inactive seats, and overall adoption trends across all surfaces.

#### Analytics Engineer

**ID**: `cpt-insightspec-actor-claude-team-analytics-eng`

**Role**: Designs and maintains the Silver/Gold pipeline that consumes Claude Team Bronze data.
**Needs**: Reliable Bronze tables with a stable `client` field to correctly route activity rows to two different Silver streams.

### 2.2 System Actors

#### Anthropic Admin API

**ID**: `cpt-insightspec-actor-claude-team-anthropic-api`

**Role**: Source of seat assignment and usage data. Provides workspace user management and usage reports for Team/Enterprise accounts.

#### Identity Manager

**ID**: `cpt-insightspec-actor-claude-team-identity-mgr`

**Role**: Resolves `email` from Bronze tables to canonical `person_id` used in Silver/Gold layers.

---

## 3. Scope

### 3.1 In Scope

- Collection of current seat assignments (who has a seat, their role, status, and activity timestamps).
- Collection of daily usage activity per user, per model, per client (`web`, `claude_code`, `mobile`).
- Connector execution logging for monitoring and observability.
- Identity resolution of `email` → `person_id` in the Silver step.
- Routing `claude_code` activity to `class_ai_dev_usage` Silver stream.
- Routing `web` / `mobile` activity to `class_ai_tool_usage` Silver stream.

### 3.2 Out of Scope

- Programmatic Claude API usage — covered by the Claude API connector (`class_ai_api_usage`).
- Real-time or sub-daily granularity — the Admin API provides daily aggregates only.
- Per-request cost attribution — under Team Plan billing, per-token cost is not meaningful.
- Versioning or history of seat assignment changes (current-state only).

---

## 4. Functional Requirements

### 4.1 Seat Data Collection

#### Collect seat roster

- [ ] `p1` - **ID**: `cpt-insightspec-fr-claude-team-seats-collect`

The connector **MUST** collect all current seat assignments from the Anthropic Admin API, capturing each user's identifier, email, role, status, and activity timestamps.

**Rationale**: Seat roster enables utilization reporting — identifying inactive seats and tracking adoption growth.
**Actors**: `cpt-insightspec-actor-claude-team-admin`, `cpt-insightspec-actor-claude-team-analytics-eng`

#### Represent seat data as current-state snapshot

- [ ] `p2` - **ID**: `cpt-insightspec-fr-claude-team-seats-snapshot`

The seat collection **MUST** represent current-state only (one row per user, no historical versioning), consistent with the source API's snapshot model.

**Rationale**: The Anthropic Admin API does not provide seat change history; the Bronze table must accurately reflect its capabilities.
**Actors**: `cpt-insightspec-actor-claude-team-analytics-eng`

### 4.2 Activity Data Collection

#### Collect daily usage activity with client dimension

- [ ] `p1` - **ID**: `cpt-insightspec-fr-claude-team-activity-collect`

The connector **MUST** collect daily usage records per user, per model, and per client, capturing message count, conversation count, and token consumption (input, output, cache read, cache write).

**Rationale**: The `client` field is the critical dimension that enables routing to two distinct Silver streams — without it, developer usage (Claude Code) cannot be separated from conversational usage (web/mobile).
**Actors**: `cpt-insightspec-actor-claude-team-admin`, `cpt-insightspec-actor-claude-team-analytics-eng`

#### Capture tool_use_count for Claude Code sessions

- [ ] `p2` - **ID**: `cpt-insightspec-fr-claude-team-tool-use-count`

The activity collection **MUST** capture `tool_use_count` (tool/function calls per session) to characterize Claude Code agent sessions.

**Rationale**: High `tool_use_count` is the primary signal distinguishing Claude Code developer sessions from conversational web/mobile usage.
**Actors**: `cpt-insightspec-actor-claude-team-analytics-eng`

#### Log connector execution

- [ ] `p1` - **ID**: `cpt-insightspec-fr-claude-team-collection-runs`

The connector **MUST** record each execution run with start/end time, status, record counts, API call count, and error count.

**Rationale**: Execution logs are required for monitoring data freshness and diagnosing pipeline failures.
**Actors**: `cpt-insightspec-actor-claude-team-analytics-eng`

### 4.3 Identity Resolution

#### Resolve email to person_id

- [ ] `p1` - **ID**: `cpt-insightspec-fr-claude-team-identity-resolve`

The Silver pipeline **MUST** resolve `email` from both seat and activity Bronze tables to a canonical `person_id` via the Identity Manager.

**Rationale**: Cross-system analytics (joining AI tool usage with HR data, task tracker activity, or Cursor/Windsurf usage) requires a stable, source-independent person identifier.
**Actors**: `cpt-insightspec-actor-claude-team-identity-mgr`, `cpt-insightspec-actor-claude-team-analytics-eng`

#### Use email as the sole identity key

- [ ] `p2` - **ID**: `cpt-insightspec-fr-claude-team-identity-key`

The connector **MUST** treat `email` as the primary identity key for resolution. The Anthropic-internal `user_id` field **MUST NOT** be used for cross-system identity resolution.

**Rationale**: `user_id` is an Anthropic-platform-internal identifier not meaningful outside the Anthropic ecosystem.
**Actors**: `cpt-insightspec-actor-claude-team-identity-mgr`

### 4.4 Silver / Gold Pipeline

#### Route claude_code activity to class_ai_dev_usage

- [ ] `p1` - **ID**: `cpt-insightspec-fr-claude-team-silver-dev-usage`

Activity rows where `client = 'claude_code'` **MUST** feed the `class_ai_dev_usage` Silver stream, unified with Cursor and Windsurf activity.

**Rationale**: Claude Code represents developer AI tool usage (agent-style sessions, file context, tool invocations) that belongs in the same analytics stream as other IDE coding assistants.
**Actors**: `cpt-insightspec-actor-claude-team-analytics-eng`

#### Route web/mobile activity to class_ai_tool_usage

- [ ] `p1` - **ID**: `cpt-insightspec-fr-claude-team-silver-tool-usage`

Activity rows where `client IN ('web', 'mobile')` **MUST** feed the `class_ai_tool_usage` Silver stream, unified with ChatGPT Team activity.

**Rationale**: Web and mobile Claude usage is conversational AI tool adoption — the same analytics category as ChatGPT Team web/mobile.
**Actors**: `cpt-insightspec-actor-claude-team-analytics-eng`

---

## 5. Non-Functional Requirements

#### Data freshness

- [ ] `p2` - **ID**: `cpt-insightspec-nfr-claude-team-freshness`

The connector **MUST** be executable on a daily schedule such that activity data for day D is available by the start of day D+2.

**Threshold**: ≤ 48 hours end-to-end latency from activity occurrence to Bronze availability.
**Rationale**: Daily AI tool adoption reports require timely data; a 48h window accommodates known Anthropic API reporting delays.

#### Schema stability

- [ ] `p2` - **ID**: `cpt-insightspec-nfr-claude-team-schema-stability`

Bronze table schemas **MUST** remain stable across connector versions. Breaking schema changes **MUST** be versioned with migration guidance.

**Threshold**: Zero unannounced breaking changes to field names or types in `claude_team_seats`, `claude_team_activity`, `claude_team_collection_runs`.
**Rationale**: Downstream Silver/Gold pipelines — including two separate routing targets — depend on stable Bronze schemas.

---

## 6. Open Questions

### OQ-CT-1: Claude Code session patterns — dev usage Silver schema

`claude_team_activity` with `client = 'claude_code'` should unify with Cursor and Windsurf in `class_ai_dev_usage`. However, the metric schemas differ:

- Cursor/Windsurf have `completions_accepted`, `lines_accepted` — Claude Code does not.
- Claude Code has `tool_use_count` — Cursor/Windsurf have `is_headless` / similar signals.

**Open**: Should `class_ai_dev_usage` use nullable columns for tool-specific metrics, or separate tables per tool category?

### OQ-CT-2: Relationship to Claude API for the same user

A developer may use both Claude Team Plan (via Claude Code) and the Claude API (programmatic calls). Both generate usage under the same `person_id`.

**Status**: Deferred. Team Plan usage (conversational/developer sessions) and API usage (programmatic/product calls) are kept in separate Silver streams. Cross-stream analysis by `person_id` is performed at Gold level.
