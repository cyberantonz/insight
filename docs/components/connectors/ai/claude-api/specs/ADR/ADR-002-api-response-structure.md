# ADR-002: API Response Structure — Nested Records and Field Mapping

- **ID**: `cpt-insightspec-adr-claude-api-002`
- **Status**: Accepted
- **Date**: 2026-03-31

## Context

During implementation, the actual Anthropic Admin API response structure was found to differ significantly from what was assumed in the PRD:

### 1. Nested response structure (messages_usage and cost_report)

The usage_report endpoints return a **nested** response:

```json
{
  "data": [
    {
      "starting_at": "2025-12-27T00:00:00Z",
      "ending_at": "2025-12-28T00:00:00Z",
      "results": [
        { "model": "claude-haiku-4-5", "api_key_id": "...", "uncached_input_tokens": 67047, ... }
      ]
    }
  ]
}
```

Individual usage records are inside `data[].results[]`, NOT at the top level of `data[]`. The `date` field does not exist in individual records — it is derived from the parent bucket's `starting_at`.

### 2. Field name differences (messages_usage)

| PRD expected | API actual | Notes |
|---|---|---|
| `cache_read_tokens` | `cache_read_input_tokens` | Different name |
| `cache_creation_5m_tokens` | `cache_creation.ephemeral_5m_input_tokens` | Nested object |
| `cache_creation_1h_tokens` | `cache_creation.ephemeral_1h_input_tokens` | Nested object |
| `web_search_requests` | `server_tool_use.web_search_requests` | Nested object |
| `date` | *(not present in results)* | Derived from bucket `starting_at` |

### 3. Cost report has richer structure than PRD specified

PRD specified: `date`, `workspace_id`, `description`, `amount_cents`

API returns per cost line: `workspace_id`, `description`, `amount` (string, USD), `currency`, `cost_type`, `model`, `service_tier`, `context_window`, `token_type`, `inference_geo`

The reference implementation (`additional-claude-platform/apps/claude-platform/src/cost-report-sync.ts`) already stores these additional fields: `model`, `cost_type`, `token_type`, `service_tier`, `context_window`, `inference_geo`.

## Decision

### Extraction pattern

Use `DpathExtractor` with `field_path: ["data", "0", "results"]` and `DatetimeBasedCursor` with `step: P1D` (one day per request). This ensures `data[0]` contains exactly one date bucket. The `date` field is injected from `stream_interval['start_time'][:10]`.

### Field mapping (messages_usage)

Use `AddFields` transformations to map API field names to schema field names:

- `record.cache_read_input_tokens` → `cache_read_tokens`
- `record.cache_creation.ephemeral_5m_input_tokens` → `cache_creation_5m_tokens`
- `record.cache_creation.ephemeral_1h_input_tokens` → `cache_creation_1h_tokens`
- `record.server_tool_use.web_search_requests` → `web_search_requests`

### Cost report schema expansion

The cost_report Bronze schema is expanded to include all fields returned by the API. The `amount` field (string, USD) replaces `amount_cents` (number). Additional dimension fields (`cost_type`, `model`, `service_tier`, `context_window`, `token_type`, `inference_geo`) are added as nullable strings.

The composite unique key remains `(date, workspace_id, description)` — matching both the PRD and the reference implementation.

## Alternatives Considered

1. **P31D step with flattening** — Rejected: Airbyte's `DpathExtractor` does not support wildcard `*` on array indices (`dpath.util.get` is used, not `dpath.util.values`), preventing extraction of `data[*].results[*]`.

2. **Store date buckets as JSON blobs** — Rejected: would require dbt JSON flattening and fundamentally change the Bronze data model.

3. **Switch to Python CDK connector** — Rejected: maintains nocode approach per project conventions.

## References

- Reference implementation: `additional-claude-platform/apps/claude-platform/src/messages-usage-sync.ts`, `cost-report-sync.ts`
- Anthropic Admin API actual response structure (verified 2026-03-27)
- PRD requirements: `cpt-insightspec-fr-claude-api-messages-usage`, `cpt-insightspec-fr-claude-api-cost-report`
