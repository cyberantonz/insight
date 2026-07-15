{# Display labels for the AI `tool` and `surface` dimension codes — static
   product vocabulary, same rationale as git_source_label: computed at gold
   read time, never denormalized into silver rows. An unmapped code falls
   through as itself. #}

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
