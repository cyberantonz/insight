"""API endpoint contract tests.

Together these modules exercise EVERY operation in the committed OpenAPI spec
(docs/components/backend/analytics/openapi.json) through the recording client,
so the endpoint-coverage gate needs no SKIP_LIST. One module per path group,
one test per (path, method, status-code) case:

  test_catalog.py            POST /v1/catalog/get_metrics
  test_metrics.py            GET+POST /v1/metrics · GET+PUT+DELETE /v1/metrics/{id}
                             POST /v1/metrics/{id}/query · POST /v1/metrics/queries
  test_metric_thresholds.py  GET+POST /v1/metrics/{id}/thresholds
                             PUT+DELETE /v1/metrics/{id}/thresholds/{tid}
  test_admin_thresholds.py   GET+POST /v1/admin/metric-thresholds
                             GET+PUT+DELETE /v1/admin/metric-thresholds/{id}
  test_columns.py            GET /v1/columns · GET /v1/columns/{table}
  test_persons.py            GET /v1/persons/{email}
  test_metric_results.py     POST /v1/metric-results

Resources come from fixtures (`api/conftest.py`): `scratch_metric` /
`scratch_threshold` / `admin_threshold_row` create the row a case needs and
delete it afterwards, so the metric catalog (`metric_catalog`, the
metric-coverage gate's universe) is never touched, soft-deleted scratch metrics
stay invisible to `GET /v1/metrics`, and the yaml rig's batch-query path is
untouched.

Status codes: each operation's success code is exercised. The endpoint gate
blocks ONLY when a documented operation is exercised by no test (a new endpoint);
per-status-code coverage is REPORTED as a percentage, not enforced (see
lib/api_coverage.py). Reachable codes are pinned by explicit tests: the success
code, 400 validation, 404 unknown/soft-deleted, 415 wrong content-type, and — for
GET /v1/persons/{email} — 200 (a seeded email resolves through the in-process
identity stub, lib/identity_stub.py) and 404 (unknown email). The remaining
declared codes (401/403/429) are unreachable by design here — auth is disabled
and nothing rate-limits — so the gate excludes them from the coverage universe.

KNOWN BUGS pinned by strict xfails (each forces cleanup when fixed):
  #1663 — legacy threshold reads 500 once a row exists (DECIMAL value vs f64
          entity): success-path threshold cases xfail; their success codes
          (200/201/204) are carried in BLOCKED so the gate does not require them.
  #1664 — duplicate admin create answers 500 (unmapped UNIQUE violation)
          instead of the declared 409: test_create_409_duplicate xfails.

Authz notes pinned by the tests: the rig runs auth-disabled with
`X-Insight-Tenant-Id: TEST_TENANT_ID` on every request; the admin gate
(`is_tenant_admin`) is a documented stub returning true, but per-row tenant
ownership is enforced (`tenant_id == Some(caller)`), so the admin lifecycle
operates only on its own tenant-scope row — the seeded product-default rows
(tenant_id NULL) are deliberately not readable per-id.
"""
