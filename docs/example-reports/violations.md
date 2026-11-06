# QuotaLab — Resource Budget Report

Project: budget-violations · Profile: workers-free · Mode: local

Requests: 30 · Failed: 0 · Seed: 42

CPU: unavailable (production CPU cannot be inferred locally)

| Resource | Per request | Projected daily | Budget | Utilization |
|---|---:|---:|---:|---:|
| worker_requests | 1.00 | 20000.00 (estimated) | 100000 | 20.0% |
| d1_rows_read | 1203.21 | 24064200.00 (estimated) | 4000000 | 601.6% |
| d1_rows_written | 0.01 | 200.00 (estimated) | 100000 | 0.2% |
| kv_reads | 0.39 | 7800.00 (estimated) | 100000 | 7.8% |
| kv_writes | 1.00 | 20000.00 (estimated) | 800 | 2500.0% |
| kv_deletes | 0.00 | 0.00 (estimated) | 1000 | 0.0% |
| kv_lists | 0.00 | 0.00 (estimated) | 1000 | 0.0% |

Per-request observations: measured_local; wrapper coverage only. Unknown totals retain an observed subtotal in JSON.

- Query `Q-58dc6396`: 10 executions, peak 2009 rows read (measured_local), estimated daily rows 24056400.00.
- Query `Q-f749682f`: 10 executions, peak 1 rows read (measured_local), estimated daily rows 7800.00.
- Query `Q-a6884f2d`: 10 executions, peak 0 rows read (measured_local), estimated daily rows 0.00.

Findings:

- Critical [daily_budget]: d1_rows_read: projected 24064200.00/day exceeds budget 4000000
- Critical [daily_budget]: kv_writes: projected 20000.00/day exceeds budget 800
- Critical [invocation_budget]: d1_rows_read: peak 2009 exceeds 500
- Warning [expensive_query]: Q-58dc6396: peak 2009 locally observed rows read; inspect EXPLAIN QUERY PLAN and relevant indexes
- Warning [missing_measurement]: external_subrequests: required complete measurement unavailable

Simulated faults triggered: 0; HTTP behavior assertions are evaluated separately.

- Local binding metadata is not authoritative production consumption.
- Traffic is hypothetical and uniform; account estimates are partial unless all other usage is configured.
- Wrapper bypasses, redirects and background work cannot be fully observed; failed operations are attempts, not guaranteed billed units.
- Fixed fixtures and seed reproduce request selection and per-invocation faults; concurrency, data mutations and latency can vary.

RESULT: FAILED (exit 2)
