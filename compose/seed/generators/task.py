"""
task-tracking silver generator: worklogs.

Everyone except sales (light) tracks tasks. Support team gets extra
volume + the `data_source='zendesk-placeholder'` marker called out
in SEED_DATA_FORMAT §3, since there's no real Zendesk connector in
the repo yet.
"""

from __future__ import annotations

from collections.abc import Sequence
from typing import TYPE_CHECKING

from generators.base import (
    bulk_insert,
    days_window,
    deterministic_uuid,
    persona_multiplier,
    poisson,
    seeded_rng,
    truncate,
    weekday_multiplier,
)
from profiles import TEAM_PROFILES, Person

if TYPE_CHECKING:
    import clickhouse_connect.driver.client


def _task_persons(roster: Sequence[Person]) -> list[Person]:
    return [
        p for p in roster
        if p.team and (
            TEAM_PROFILES[p.team].weights.get("jira", 0) > 0
            or TEAM_PROFILES[p.team].weights.get("zendesk-placeholder", 0) > 0
        )
    ]


def seed_task_worklogs(
    client: clickhouse_connect.driver.client.Client,
    roster: Sequence[Person],
    tenant_uuid: str,
    days: int,
) -> int:
    truncate(client, "silver", "class_task_worklogs")
    cols = [
        "insight_tenant_id", "insight_source_id", "worklog_id",
        "issue_id", "author_id", "author_email", "work_date",
        "duration_seconds", "worklog_seconds", "unique_key", "_version",
    ]
    rows: list[tuple[object, ...]] = []
    version = 1
    for p in _task_persons(roster):
        persona = persona_multiplier(p.uuid)
        jira_w = TEAM_PROFILES[p.team or ""].weights.get("jira", 0)
        zendesk_w = TEAM_PROFILES[p.team or ""].weights.get("zendesk-placeholder", 0)
        # Pick the dominant data_source for the row's `insight_source_id`
        # — the support team gets zendesk-placeholder, everyone else jira.
        primary_w = max(jira_w, zendesk_w)
        if primary_w <= 0:
            continue
        for d in days_window(days):
            rng = seeded_rng(p.uuid, d, "task.worklogs")
            mean = 4 * persona * primary_w * weekday_multiplier(d)
            n_logs = min(poisson(rng, mean), 12)
            if n_logs == 0:
                continue
            # Each worklog 15min-2h. Cap total at 8h/day.
            day_cap = 8 * 3600
            spent = 0
            for i in range(n_logs):
                if spent >= day_cap:
                    break
                duration = min(rng.randint(900, 7200), day_cap - spent)
                spent += duration
                worklog_id = deterministic_uuid("task.worklog", p.uuid, d.isoformat(), str(i))
                issue_id = f"INSIGHT-{rng.randint(1000, 9999)}"
                rows.append((
                    tenant_uuid,
                    deterministic_uuid("task.source", p.uuid),
                    worklog_id, issue_id,
                    p.email, p.email, d,
                    float(duration), float(duration),
                    worklog_id, version,
                ))
    return bulk_insert(client, "silver", "class_task_worklogs", cols, rows)


def seed_task_users(
    client: clickhouse_connect.driver.client.Client,
    roster: Sequence[Person],
    tenant_uuid: str,
) -> int:
    """Required so `insight.task_worklog_seconds_per_day` (INNER JOIN
    on insight_source_id + user_id) actually emits rows."""
    truncate(client, "silver", "class_task_users")
    cols = [
        "insight_tenant_id", "insight_source_id", "user_id", "email",
        "unique_key", "_version",
    ]
    rows: list[tuple[object, ...]] = []
    version = 1
    for p in _task_persons(roster):
        src_id = deterministic_uuid("task.source", p.uuid)
        # author_id in class_task_worklogs == p.email — mirror that here so
        # the JOIN matches.
        rows.append((
            tenant_uuid, src_id, p.email, p.email,
            deterministic_uuid("task.user", p.uuid), version,
        ))
    return bulk_insert(client, "silver", "class_task_users", cols, rows)


def generate(
    client: clickhouse_connect.driver.client.Client,
    roster: Sequence[Person],
    tenant_uuid: str,
    days: int,
) -> dict[str, int]:
    return {
        "silver.class_task_worklogs": seed_task_worklogs(client, roster, tenant_uuid, days),
        "silver.class_task_users":    seed_task_users(client, roster, tenant_uuid),
    }
