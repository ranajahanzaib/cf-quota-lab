# QuotaLab — Resource Budget Report

Project: basic-worker · Profile: workers-free · Mode: local

Requests: 30 · Failed: 0 · Seed: 42

CPU: unavailable (production CPU cannot be inferred locally)

| Resource | Per request | Projected daily | Budget | Utilization |
|---|---:|---:|---:|---:|
| worker_requests | 1.00 | 20000.00 (estimated) | 100000 | 20.0% |
| d1_rows_read | 1.59 | 31800.00 (estimated) | 4000000 | 0.8% |
| d1_rows_written | 0.02 | 400.00 (estimated) | 100000 | 0.4% |
| kv_reads | 0.39 | 7800.00 (estimated) | 100000 | 7.8% |
| kv_writes | 0.00 | 0.00 (estimated) | 800 | 0.0% |
| kv_deletes | 0.00 | 0.00 (estimated) | 1000 | 0.0% |
| kv_lists | 0.00 | 0.00 (estimated) | 1000 | 0.0% |

Per-request observations: measured_local; wrapper coverage only. Unknown totals retain an observed subtotal in JSON.

- Query `Q-58dc6396`: 10 executions, peak 2 rows read (measured_local), estimated daily rows 24000.00.
- Query `Q-f749682f`: 10 executions, peak 1 rows read (measured_local), estimated daily rows 7800.00.
- Query `Q-a6884f2d`: 10 executions, peak 0 rows read (measured_local), estimated daily rows 0.00.

Findings:

- Warning [missing_measurement]: external_subrequests: required complete measurement unavailable

Simulated faults triggered: 0; HTTP behavior assertions are evaluated separately.

- Local binding metadata is not authoritative production consumption.
- Traffic is hypothetical and uniform; account estimates are partial unless all other usage is configured.
- Wrapper bypasses, redirects and background work cannot be fully observed; failed operations are attempts, not guaranteed billed units.
- Fixed fixtures and seed reproduce request selection and per-invocation faults; concurrency, data mutations and latency can vary.

RESULT: PASSED (exit 0)
