-- Heal the AI class tables back to a dbt-owned schema.
--
-- dbt-clickhouse incremental inserts are positional and union_by_tag is a
-- positional SELECT * UNION ALL, so physical column order must equal the
-- model's SELECT order. The retired label backfill ALTERed these columns
-- onto pre-existing tables at the TAIL while the models declared them
-- mid-SELECT, which broke syncs. Labels now derive in gold
-- (macros/ai_labels.sql) — the columns leave the contract; DROP preserves
-- the order of the remaining columns, converging every instance state.
-- conversation_count is source data and stays: ADD places it correctly
-- where absent, MODIFY reorders/normalizes the type where an earlier
-- out-of-band ADD used the tail position or Nullable(Float64).
--
-- Idempotent: this channel has no ledger and re-runs on every deploy.
-- The class tables always exist here (placeholders precede migrations).
ALTER TABLE silver.class_ai_dev_usage DROP COLUMN IF EXISTS tool_label;

ALTER TABLE silver.class_ai_dev_usage
    ADD COLUMN IF NOT EXISTS conversation_count Nullable(UInt32) AFTER session_count;

ALTER TABLE silver.class_ai_dev_usage
    MODIFY COLUMN conversation_count Nullable(UInt32) AFTER session_count;

ALTER TABLE silver.class_ai_assistant_usage DROP COLUMN IF EXISTS tool_label;

ALTER TABLE silver.class_ai_assistant_usage DROP COLUMN IF EXISTS surface_label;

