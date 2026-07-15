#!/usr/bin/env bash
# Apply the ClickHouse gold-view migrations against an EXTERNAL ClickHouse.
#
# This is the in-cluster, network-mode counterpart to the ClickHouse half
# of scripts/init.sh. init.sh `kubectl exec`s into a bundled CH StatefulSet
# (retired in #1428 when the umbrella stopped bundling L2 infra), so it
# cannot reach an external CH. This script talks to CH over its HTTP
# interface via lib/ch-exec.sh (selected by CLICKHOUSE_URL) and is invoked
# by the clickhouse-migrate Helm Hook Job (post-install,post-upgrade).
#
# Steps (same order and contract as init.sh):
#   1. Create the core databases (staging, silver, app db).
#   2. Run create-bronze-placeholders.sh — minimum-viable bronze/silver
#      stubs so gold-view CREATE VIEW type-checks on a fresh cluster
#      (CH validates referenced tables at parse time). See ADR-0007.
#   3. Apply migrations/*.sql in lexicographic order.
#   4. Build the dbt gold models (tag:gold) so dbt-owned views exist at
#      deploy time instead of after the first connector sync.
#
# Bookkeeping: none — every migration is re-run on every invocation and
# MUST stay idempotent/re-runnable (CREATE OR REPLACE / IF NOT EXISTS).
# This matches the existing init.sh contract (see ingestion DESIGN §migrations).
#
# Required env (set by the Hook Job from chart values + insight-db-creds):
#   CLICKHOUSE_URL       e.g. http://ch-host:8123  (selects the HTTP backend)
#   CLICKHOUSE_USER, CLICKHOUSE_PASSWORD
#   CLICKHOUSE_DATABASE  the Insight app database
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"

: "${CLICKHOUSE_URL:?CLICKHOUSE_URL must be set (e.g. http://ch-host:8123)}"
: "${CLICKHOUSE_DATABASE:?CLICKHOUSE_DATABASE must be set (the Insight app database)}"

source "$SCRIPT_DIR/lib/ch-exec.sh"

echo "=== Creating core databases (staging, silver, ${CLICKHOUSE_DATABASE}) ==="
run_ch <<SQL
CREATE DATABASE IF NOT EXISTS staging;
CREATE DATABASE IF NOT EXISTS silver;
CREATE DATABASE IF NOT EXISTS ${CLICKHOUSE_DATABASE};
SQL

echo "=== Creating bronze/silver placeholders (ADR-0007) ==="
bash "$SCRIPT_DIR/create-bronze-placeholders.sh"

echo "=== Applying ClickHouse migrations ==="
shopt -s nullglob
for migration in "$SCRIPT_DIR/migrations"/*.sql; do
  echo "  $(basename "$migration")"
  run_ch < "$migration"
done

echo "=== Healing AI staging contract schemas (ADR-0010) ==="
# Display labels left the class contract — they derive in the gold view
# from the tool/surface discriminator codes (macros/ai_labels.sql).
# Pre-existing staging tables may still carry the columns, tail-appended
# by the retired label repairs or by dbt's append_new_columns. Physical
# column ORDER must equal the model's SELECT order (dbt-clickhouse
# incremental inserts map positionally; union_by_tag is a positional
# SELECT * UNION ALL), so the columns are dropped — DROP preserves the
# relative order of the remaining columns and converges every instance
# state to one uniform schema. conversation_count is contract DATA and is
# position-pinned to the model SELECT instead (ADD places it correctly
# where absent; MODIFY is a metadata-only reorder where it was
# tail-appended). Guarded per table: staging tables do not exist before
# the connector's first dbt run. Idempotent: re-runs are no-ops.
heal_ai_dev_staging() {
  local table="$1"
  ch_table_exists staging "${table}" || return 0
  echo "  staging.${table}"
  run_ch <<SQL
ALTER TABLE staging.${table} DROP COLUMN IF EXISTS tool_label;
ALTER TABLE staging.${table} ADD COLUMN IF NOT EXISTS conversation_count Nullable(UInt32) AFTER session_count;
ALTER TABLE staging.${table} MODIFY COLUMN conversation_count Nullable(UInt32) AFTER session_count;
SQL
}

heal_ai_assistant_staging() {
  local table="$1"
  ch_table_exists staging "${table}" || return 0
  echo "  staging.${table}"
  run_ch <<SQL
ALTER TABLE staging.${table} DROP COLUMN IF EXISTS tool_label;
ALTER TABLE staging.${table} DROP COLUMN IF EXISTS surface_label;
SQL
}

heal_ai_dev_staging cursor__ai_dev_usage
heal_ai_dev_staging claude_enterprise__ai_dev_usage
heal_ai_dev_staging claude_team__ai_dev_usage
heal_ai_dev_staging claude_admin__ai_dev_usage
heal_ai_dev_staging copilot__ai_dev_usage
heal_ai_dev_staging chatgpt_team__ai_dev_usage
heal_ai_assistant_staging claude_enterprise__ai_assistant_usage
heal_ai_assistant_staging chatgpt_team__ai_assistant_usage

echo "=== Healing collab-chat and CRM contract schemas (ADR-0010) ==="
# Same positional invariant as the AI heal above, two more occurrences:
#  * collab chat: `direct_and_group_messages` (#266) was added mid-SELECT
#    with append_new_columns and no connector rebuild — pre-existing member
#    tables tail-appended it (or, for the class table, never received it:
#    the class model defaults to on_schema_change='ignore').
#  * CRM: the salesforce members project the envelope `custom_fields` blob
#    after `metadata`; the hubspot members historically did not, so the
#    positional class union failed whenever both connectors were configured.
#    The hubspot models now emit a structural '{}' at the same position.
# Class tables are healed here rather than in migrations/*.sql because the
# `AFTER` anchors do not exist on the minimal gold-view placeholders — a
# real table (built by dbt from the model) always carries them, so the
# heals are guarded on "exists and is not a placeholder". Placeholder
# tables need no healing: drop_silver_placeholders_at_start replaces them
# with the real schema before the first materialization.
ch_table_is_real() {
  local db="$1" table="$2"
  ch_table_exists "$db" "$table" || return 1
  local placeholder_count
  placeholder_count="$(
    printf "SELECT count() FROM system.tables WHERE database='%s' AND name='%s' AND comment='INSIGHT_PLACEHOLDER_v1'" "$db" "$table" |
      _ch_http_query |
      tr -d '[:space:]'
  )"
  [[ "$placeholder_count" == "0" ]]
}

heal_collab_chat_table() {
  local db="$1" table="$2"
  ch_table_is_real "$db" "$table" || return 0
  echo "  ${db}.${table}"
  run_ch <<SQL
ALTER TABLE ${db}.${table} ADD COLUMN IF NOT EXISTS direct_and_group_messages Nullable(Int64) AFTER group_chat_messages;
ALTER TABLE ${db}.${table} MODIFY COLUMN direct_and_group_messages Nullable(Int64) AFTER group_chat_messages;
SQL
}

heal_crm_table() {
  local db="$1" table="$2"
  ch_table_is_real "$db" "$table" || return 0
  echo "  ${db}.${table}"
  run_ch <<SQL
ALTER TABLE ${db}.${table} ADD COLUMN IF NOT EXISTS custom_fields String DEFAULT '{}' AFTER metadata;
ALTER TABLE ${db}.${table} MODIFY COLUMN custom_fields String DEFAULT '{}' AFTER metadata;
SQL
}

heal_collab_chat_table staging m365__collab_chat_activity
heal_collab_chat_table staging slack__collab_chat_activity
heal_collab_chat_table staging zulip_proxy__collab_chat_activity
heal_collab_chat_table silver class_collab_chat_activity
heal_crm_table staging hubspot__crm_accounts
heal_crm_table staging hubspot__crm_activities
heal_crm_table staging hubspot__crm_contacts
heal_crm_table staging hubspot__crm_deals
heal_crm_table staging hubspot__crm_users
heal_crm_table silver class_crm_accounts
heal_crm_table silver class_crm_activities
heal_crm_table silver class_crm_contacts
heal_crm_table silver class_crm_deals
heal_crm_table silver class_crm_users

echo "=== Building gold models (dbt run --select tag:gold) ==="
# Gold views are dbt-owned but must exist at DEPLOY time, not first-sync
# time: the analytics service marks metric definitions schema-error while
# an observation view is missing, which blanks those metrics for every
# frontend request until the first connector sync builds the view (hours
# on a scheduled instance). The placeholders created above guarantee every
# relation the views reference exists, so this run type-checks on a fresh
# cluster — the same guarantee the scoped per-connector dbt runs rely on
# for sideways refs. Idempotent: views are create-or-replace and
# table-materialized gold models rebuild via atomic swap. Table builds are
# bounded by the models' own ClickHouse settings (memory, threads, disk
# spill — ADR-0009), so this step degrades to a slower build rather than
# failing the deploy on data volume.
#
# Profile generation mirrors the dbt-run WorkflowTemplate: python3 writes
# profiles.yml from env vars, never interpolating values into YAML text.
DBT_PROFILES_DIR="$(mktemp -d)"
export DBT_PROFILES_DIR
python3 - <<'PY'
import os
from urllib.parse import urlparse

import yaml

url = urlparse(os.environ["CLICKHOUSE_URL"])
profile = {
    "ingestion": {
        "target": "migrate",
        "outputs": {
            "migrate": {
                "type": "clickhouse",
                "host": url.hostname,
                "port": url.port or (8443 if url.scheme == "https" else 8123),
                "schema": "silver",
                "user": os.environ["CLICKHOUSE_USER"],
                "password": os.environ["CLICKHOUSE_PASSWORD"],
                "secure": url.scheme == "https",
                "send_receive_timeout": 1500,
                "query_limit": 0,
                "connect_timeout": 30,
            }
        },
    }
}
with open(os.path.join(os.environ["DBT_PROFILES_DIR"], "profiles.yml"), "w") as f:
    yaml.safe_dump(profile, f)
PY
(cd "$SCRIPT_DIR/../dbt" && dbt run --profiles-dir "$DBT_PROFILES_DIR" --log-format json --select tag:gold)
rm -rf "$DBT_PROFILES_DIR"

echo "=== ClickHouse migrations complete ==="
