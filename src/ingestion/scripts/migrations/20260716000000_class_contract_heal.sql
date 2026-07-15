-- Heal the AI class tables back to a dbt-owned schema (ADR-0010).
--
-- Display labels (tool_label / surface_label) were denormalized into the
-- class contract and patched onto pre-existing tables by out-of-band
-- ALTERs that APPEND at the table tail, while the models declared them
-- mid-SELECT. dbt-clickhouse incremental inserts map columns positionally
-- and union_by_tag compiles to positional `SELECT * UNION ALL`, so the
-- divergent physical orders broke syncs (CANNOT_PARSE_TEXT on staging
-- inserts, NO_COMMON_TYPE on the class union). Labels now derive in the
-- gold view from the tool/surface discriminator codes
-- (macros/ai_labels.sql); the columns leave the contract entirely.
--
-- DROP COLUMN is the order-preserving direction of out-of-band DDL: the
-- remaining columns keep their relative order, so this single statement
-- converges every instance state — tail-patched, refreshed-to-model-order,
-- and fresh — to one uniform physical schema.
--
-- conversation_count is source DATA (not derivable in gold) and stays in
-- the contract. Its position and type are pinned to the model's SELECT:
-- ADD ... AFTER places it correctly where absent; MODIFY ... AFTER is a
-- metadata-only reorder where it sits at the tail, and normalizes the
-- Nullable(Float64) the earlier out-of-band ADD used on some instances to
-- the Nullable(UInt32) the models emit (a lossless narrowing: every value
-- originates from a UInt32 staging column).
--
-- Idempotent: re-runs are no-ops (this channel has no ledger and re-runs
-- on every deploy). The class tables always exist when this runs — the
-- placeholder step precedes migrations on fresh clusters.

ALTER TABLE silver.class_ai_dev_usage DROP COLUMN IF EXISTS tool_label;

ALTER TABLE silver.class_ai_dev_usage
    ADD COLUMN IF NOT EXISTS conversation_count Nullable(UInt32) AFTER session_count;

ALTER TABLE silver.class_ai_dev_usage
    MODIFY COLUMN conversation_count Nullable(UInt32) AFTER session_count;

ALTER TABLE silver.class_ai_assistant_usage DROP COLUMN IF EXISTS tool_label;

ALTER TABLE silver.class_ai_assistant_usage DROP COLUMN IF EXISTS surface_label;

