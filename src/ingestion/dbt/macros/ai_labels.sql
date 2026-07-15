{# Display labels for the AI `tool` and `surface` dimension codes. Static
   product vocabulary, not source data — same rationale as
   git_source_label: computed in the gold view so it applies retroactively
   on every read, centralized so no vendor mapping is inlined in model SQL
   and no label is ever denormalized into silver rows (where historical
   rows would need backfills the incremental models never re-read). An
   unmapped code falls through as itself, so a new connector renders its
   raw code until the vocabulary here learns it. #}

{% macro ai_tool_label(tool_expr) %}
multiIf(
    {{ tool_expr }} = 'cursor', 'Cursor',
    {{ tool_expr }} = 'claude_code', 'Claude Code',
    {{ tool_expr }} = 'copilot', 'GitHub Copilot',
    {{ tool_expr }} = 'codex', 'Codex',
    {{ tool_expr }} = 'claude', 'Claude',
    {{ tool_expr }} = 'chatgpt', 'ChatGPT',
    {{ tool_expr }}
)
{% endmacro %}

{% macro ai_surface_label(surface_expr) %}
multiIf(
    {{ surface_expr }} = 'chat', 'Chat',
    {{ surface_expr }} = 'excel', 'Excel',
    {{ surface_expr }} = 'powerpoint', 'PowerPoint',
    {{ surface_expr }} = 'cowork', 'Cowork',
    {{ surface_expr }} = 'cross', 'Cross',
    {{ surface_expr }}
)
{% endmacro %}
