{{ config(
    materialized='view',
    schema='insight',
    alias='ai_metric_observations',
    tags=['gold']
) }}

-- Source measure observations for the unified metrics runtime. Reads only
-- class-contract fields: activity is row existence (the class contract
-- guarantees rows exist only for real activity — see silver/ai/schema.yml),
-- display labels come from tool_label / surface_label, and conversation
-- semantics come from data presence (conversation_count is NULL for sources
-- without a conversation concept). No vendor-specific columns, tool names, or
-- label mappings may appear in this model.

WITH
ai_dev_usage_source AS (
    SELECT
        insight_tenant_id AS tenant_id,
        lower(email) AS entity_id,
        day AS metric_date,
        coalesce(nullIf(tool, ''), '__unknown__') AS tool_value,
        if(
            coalesce(nullIf(tool, ''), '__unknown__') = '__unknown__',
            'Unknown',
            coalesce(nullIf(tool_label, ''), tool)
        ) AS tool_label_value,
        conversation_count,
        lines_added,
        lines_removed,
        tool_use_offered,
        tool_use_accepted,
        cost_cents
    FROM {{ ref('class_ai_dev_usage') }}
    WHERE email IS NOT NULL
      AND email != ''
),
ai_assistant_usage_source AS (
    SELECT
        insight_tenant_id AS tenant_id,
        lower(email) AS entity_id,
        day AS metric_date,
        coalesce(nullIf(tool, ''), '__unknown__') AS tool_value,
        if(
            coalesce(nullIf(tool, ''), '__unknown__') = '__unknown__',
            'Unknown',
            coalesce(nullIf(tool_label, ''), tool)
        ) AS tool_label_value,
        coalesce(nullIf(surface, ''), '__unknown__') AS surface_value,
        if(
            coalesce(nullIf(surface, ''), '__unknown__') = '__unknown__',
            'Unknown',
            coalesce(nullIf(surface_label, ''), surface)
        ) AS surface_label_value,
        conversation_count,
        message_count,
        action_count,
        cost_cents
    FROM {{ ref('class_ai_assistant_usage') }}
    WHERE email IS NOT NULL
      AND email != ''
),
ai_dev_usage_dimensions AS (
    SELECT
        *,
        CAST(
            [tuple('tool', tool_value, tool_label_value)]
            AS Array(Tuple(key String, value String, label Nullable(String)))
        ) AS tool_dimensions
    FROM ai_dev_usage_source
),
ai_assistant_usage_dimensions AS (
    SELECT
        *,
        CAST(
            [tuple('tool', tool_value, tool_label_value)]
            AS Array(Tuple(key String, value String, label Nullable(String)))
        ) AS tool_dimensions,
        CAST(
            [
                tuple('tool', tool_value, tool_label_value),
                tuple('surface', surface_value, surface_label_value)
            ] AS Array(Tuple(key String, value String, label Nullable(String)))
        ) AS tool_surface_dimensions
    FROM ai_assistant_usage_source
),
ai_active_day_source AS (
    SELECT DISTINCT
        tenant_id,
        entity_id,
        metric_date
    FROM ai_dev_usage_source

    UNION DISTINCT

    SELECT DISTINCT
        tenant_id,
        entity_id,
        metric_date
    FROM ai_assistant_usage_source
),
measure_observations AS (
    SELECT
        tenant_id,
        entity_id,
        metric_date,
        'accepted_lines' AS measure_key,
        if(
            countIf(lines_added IS NOT NULL) > 0,
            sumIf(toFloat64(lines_added), lines_added IS NOT NULL),
            CAST(NULL AS Nullable(Float64))
        ) AS value,
        tool_dimensions AS dimensions
    FROM ai_dev_usage_dimensions
    GROUP BY tenant_id, entity_id, metric_date, tool_dimensions

    UNION ALL

    SELECT
        tenant_id,
        entity_id,
        metric_date,
        'removed_lines' AS measure_key,
        if(
            countIf(lines_removed IS NOT NULL) > 0,
            sumIf(toFloat64(lines_removed), lines_removed IS NOT NULL),
            CAST(NULL AS Nullable(Float64))
        ) AS value,
        tool_dimensions AS dimensions
    FROM ai_dev_usage_dimensions
    GROUP BY tenant_id, entity_id, metric_date, tool_dimensions

    UNION ALL

    SELECT
        tenant_id,
        entity_id,
        metric_date,
        'active_day' AS measure_key,
        toNullable(toFloat64(1)) AS value,
        CAST([] AS Array(Tuple(key String, value String, label Nullable(String)))) AS dimensions
    FROM ai_active_day_source

    UNION ALL

    SELECT
        tenant_id,
        entity_id,
        metric_date,
        'cost_usd' AS measure_key,
        if(
            countIf(cost_cents IS NOT NULL) > 0,
            sumIf(toFloat64(cost_cents), cost_cents IS NOT NULL) / 100,
            CAST(NULL AS Nullable(Float64))
        ) AS value,
        tool_dimensions AS dimensions
    FROM ai_dev_usage_dimensions
    GROUP BY tenant_id, entity_id, metric_date, tool_dimensions

    UNION ALL

    SELECT
        tenant_id,
        entity_id,
        metric_date,
        'cost_usd' AS measure_key,
        if(
            countIf(cost_cents IS NOT NULL) > 0,
            sumIf(toFloat64(cost_cents), cost_cents IS NOT NULL) / 100,
            CAST(NULL AS Nullable(Float64))
        ) AS value,
        tool_dimensions AS dimensions
    FROM ai_assistant_usage_dimensions
    GROUP BY tenant_id, entity_id, metric_date, tool_dimensions

    UNION ALL

    SELECT
        tenant_id,
        entity_id,
        metric_date,
        'accepted_edit_actions' AS measure_key,
        if(
            countIf(tool_use_accepted IS NOT NULL) > 0,
            sumIf(toFloat64(tool_use_accepted), tool_use_accepted IS NOT NULL),
            CAST(NULL AS Nullable(Float64))
        ) AS value,
        tool_dimensions AS dimensions
    FROM ai_dev_usage_dimensions
    GROUP BY tenant_id, entity_id, metric_date, tool_dimensions

    UNION ALL

    SELECT
        tenant_id,
        entity_id,
        metric_date,
        'tool_use_offered' AS measure_key,
        if(
            countIf(tool_use_offered IS NOT NULL) > 0,
            sumIf(toFloat64(tool_use_offered), tool_use_offered IS NOT NULL),
            CAST(NULL AS Nullable(Float64))
        ) AS value,
        tool_dimensions AS dimensions
    FROM ai_dev_usage_dimensions
    GROUP BY tenant_id, entity_id, metric_date, tool_dimensions

    UNION ALL

    SELECT
        tenant_id,
        entity_id,
        metric_date,
        'dev_conversations' AS measure_key,
        if(
            countIf(conversation_count IS NOT NULL) > 0,
            sumIf(toFloat64(conversation_count), conversation_count IS NOT NULL),
            CAST(NULL AS Nullable(Float64))
        ) AS value,
        tool_dimensions AS dimensions
    FROM ai_dev_usage_dimensions
    GROUP BY tenant_id, entity_id, metric_date, tool_dimensions

    UNION ALL

    SELECT
        tenant_id,
        entity_id,
        metric_date,
        'assistant_messages' AS measure_key,
        if(
            countIf(message_count IS NOT NULL) > 0,
            sumIf(toFloat64(message_count), message_count IS NOT NULL),
            CAST(NULL AS Nullable(Float64))
        ) AS value,
        tool_surface_dimensions AS dimensions
    FROM ai_assistant_usage_dimensions
    GROUP BY tenant_id, entity_id, metric_date, tool_surface_dimensions

    UNION ALL

    SELECT
        tenant_id,
        entity_id,
        metric_date,
        'assistant_actions' AS measure_key,
        if(
            countIf(action_count IS NOT NULL) > 0,
            sumIf(toFloat64(action_count), action_count IS NOT NULL),
            CAST(NULL AS Nullable(Float64))
        ) AS value,
        tool_surface_dimensions AS dimensions
    FROM ai_assistant_usage_dimensions
    GROUP BY tenant_id, entity_id, metric_date, tool_surface_dimensions

    UNION ALL

    SELECT
        tenant_id,
        entity_id,
        metric_date,
        'chat_assistant_conversations' AS measure_key,
        if(
            countIf(conversation_count IS NOT NULL) > 0,
            sumIf(toFloat64(conversation_count), conversation_count IS NOT NULL),
            CAST(NULL AS Nullable(Float64))
        ) AS value,
        tool_surface_dimensions AS dimensions
    FROM ai_assistant_usage_dimensions
    WHERE surface_value = 'chat'
    GROUP BY tenant_id, entity_id, metric_date, tool_surface_dimensions
)
SELECT
    assumeNotNull(tenant_id) AS tenant_id,
    'ai_usage' AS source_key,
    'person' AS entity_type,
    assumeNotNull(entity_id) AS entity_id,
    assumeNotNull(metric_date) AS metric_date,
    CAST(NULL AS Nullable(DateTime64(3))) AS observed_at,
    measure_key,
    value,
    CAST(NULL AS Nullable(String)) AS subject_key,
    dimensions
FROM measure_observations
WHERE tenant_id IS NOT NULL
  AND entity_id IS NOT NULL
  AND metric_date IS NOT NULL
