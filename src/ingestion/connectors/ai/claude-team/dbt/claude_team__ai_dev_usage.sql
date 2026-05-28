-- depends_on: {{ ref('claude_team__bronze_promoted') }}
-- Bronze → Silver step 1: Claude Team per-user per-day CC usage → class_ai_dev_usage
--
-- Source: bronze_claude_team.claude_team_code_metrics — daily aggregate stream
-- pulled via the customer-deployed claude-team-proxy from the claude.ai
-- web API (/api/organizations/{org_id}/claude_code/metrics). One row per
-- (email, metric_date) — metrics already aggregated to daily grain by the API.
--
-- Filter: email IS NOT NULL AND trim(email) != ''.
-- Rows without an email cannot be attributed to a user and are dropped.
--
-- session_count semantics: mapped directly from `total_sessions` (the API
-- exposes actual session counts, unlike Cursor).
--
-- lines_added semantics: `total_lines_accepted` — lines accepted from
-- AI suggestions. Claude Team does not surface total keystrokes (only
-- AI-accepted lines), so total_lines_added / total_lines_removed are NULL.
--
-- prs_with_cc_count: `prs_with_cc` — PRs where Claude Code was active
-- at least once. Paired with `pull_requests_count` (total_prs) to compute
-- the prs_with_cc_percentage Gold metric.
{{ config(
    materialized='incremental',
    incremental_strategy='append',
    unique_key='unique_key',
    engine='ReplacingMergeTree(_version)',
    order_by=['unique_key'],
    on_schema_change='append_new_columns',
    settings={'allow_nullable_key': 1},
    schema='staging',
    tags=['claude-team', 'silver:class_ai_dev_usage']
) }}

SELECT
    tenant_id                                           AS insight_tenant_id,
    source_id,
    -- Unique key format: tenant-source-email-day (consistent with claude_admin pattern)
    CAST(concat(
        coalesce(tenant_id, ''), '-',
        coalesce(source_id, ''), '-',
        lower(trim(coalesce(email, ''))), '-',
        coalesce(metric_date, '')
    ) AS String)                                        AS unique_key,
    lower(trim(email))                                  AS email,
    -- Claude Team uses session-based auth (operator sessionKey cookie);
    -- individual users are identified by email, not API keys.
    CAST(NULL AS Nullable(String))                      AS api_key_id,
    toDate(metric_date)                                 AS day,
    'claude_code'                                       AS tool,
    toUInt32(coalesce(total_sessions, 0))               AS session_count,
    toUInt32(coalesce(total_lines_accepted, 0))         AS lines_added,
    -- Claude Team does not expose AI-removed lines — zero, not NULL,
    -- because class_ai_dev_usage.lines_removed is NOT NULL.
    toUInt32(0)                                         AS lines_removed,
    -- Total keystrokes (AI + manual) are not available from the web API.
    CAST(NULL AS Nullable(UInt32))                      AS total_lines_added,
    CAST(NULL AS Nullable(UInt32))                      AS total_lines_removed,
    -- Inline-completion offered/accepted counters are not surfaced by the
    -- claude.ai team metrics endpoint.
    CAST(NULL AS Nullable(UInt32))                      AS tool_use_offered,
    CAST(NULL AS Nullable(UInt32))                      AS tool_use_accepted,
    CAST(NULL AS Nullable(UInt32))                      AS agent_sessions,
    CAST(NULL AS Nullable(UInt32))                      AS chat_requests,
    CAST(NULL AS Nullable(UInt32))                      AS cost_cents,
    -- Git-level attribution: commits not exposed; PRs available.
    CAST(NULL AS Nullable(UInt32))                      AS commits_count,
    toUInt32OrNull(toString(total_prs))                 AS pull_requests_count,
    -- PRs where Claude Code was active at least once (source: prs_with_cc).
    -- Used in Gold to compute prs_with_cc_percentage.
    toUInt32OrNull(toString(prs_with_cc))               AS prs_with_cc_count,
    CAST(NULL AS Nullable(String))                      AS tool_action_breakdown_json,
    -- `claude_playwright` — this connector scrapes the claude.ai web API via
    -- a customer-hosted Playwright/Chromium proxy (no official REST API exists).
    'claude_playwright'                                 AS source,
    data_source,
    CAST(_airbyte_extracted_at AS Nullable(DateTime64(3))) AS collected_at,
    toUnixTimestamp64Milli(_airbyte_extracted_at)          AS _version
FROM {{ source('bronze_claude_team', 'claude_team_code_metrics') }}
WHERE email IS NOT NULL
  AND trim(email) != ''
  -- Guard against NULL metric_date: toDate(NULL) → 1970-01-01 which
  -- silently corrupts the incremental boundary (same pattern as cursor__ai_dev_usage).
  AND metric_date IS NOT NULL
{% if is_incremental() %}
  AND toDate(metric_date) > (
      SELECT coalesce(max(day), toDate('1970-01-01')) - INTERVAL 3 DAY
      FROM {{ this }}
  )
{% endif %}
