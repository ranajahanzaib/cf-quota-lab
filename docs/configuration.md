# Configuration

Use `quotalab init` or copy an example `quotalab.yaml`. The parser rejects unknown keys and negative unsigned quantities. Every top-level section is explicit. Optional defaults: no warmup, no fixtures/headers/faults, empty existing usage, fault trigger `after: 1`, and `delay_ms: 0`. Missing budget values inherit relevant profile limits; D1 per-request row budgets are opt-in.

`version` must be 1. Project names have 1–128 characters. Scenario names are unique 1–80 character alphanumeric/underscore/hyphen identifiers. HTTP methods: GET, POST, PUT, PATCH, DELETE. Paths must begin with `/` and remain on the configured origin. Host and QuotaLab control headers are reserved.

Limits: concurrency 1–64, 1–10,000 requests per scenario, at most 100 scenarios and 100,000 total measured requests, timeout 1–60,000 ms, at most 1,000 warmups per scenario and 1,000 fixtures. Response assertions read at most 1 MiB. Oversized or malformed responses fail rather than silently truncating assertions.

At least one scenario weight must be positive. Weights are normalized by their sum. Each scenario receives the same sample count, including zero-weight fault cases. Forecasts use the weighted means, not the observed test ordering. JSON fixture bodies are selected by seed and request index; they replace `request.json` when present. The tool does not substitute arbitrary templates or execute configuration code.

`expect.status` is required. `expect.json_contains` recursively matches object subsets and exact primitive/array values. `expect.fault_triggered: true` additionally requires observed injection; it never replaces HTTP assertions. Full response bodies are not retained, so reanalysis cannot apply new body assertions to historical observations.

Only the entire target URL supports environment substitution, such as `url: ${QUOTALAB_URL}`. Arbitrary header values, secrets and SQL are not interpolated. Reports omit configured headers, request payloads, target query strings and control tokens.

Daily budget keys:

- `worker.max_requests_per_day`
- `d1.max_projected_rows_read_per_day`, `max_projected_rows_written_per_day`
- `kv.max_projected_reads_per_day`, `max_projected_writes_per_day`, `max_projected_deletes_per_day`, `max_projected_lists_per_day`

Invocation budget keys:

- `worker.max_external_subrequests_per_request`, `max_cpu_ms_per_request`
- `d1.max_rows_read_per_request`, `max_rows_written_per_request`, `max_queries_per_request`

CPU remains unavailable locally; explicitly requesting a CPU budget applies the missing-measurement policy. `policy.fail_on` accepts `informational`, `warning`, or `critical`. `missing_measurements` accepts `warn`, `fail`, or `ignore`; `fail` gives exit 4. Eighty-percent utilization is a warning, while exceeding a budget is critical.

Use `traffic.existing_usage` with metric names `worker_requests`, `d1_rows_read`, `d1_rows_written`, `kv_reads`, `kv_writes`, `kv_deletes`, `kv_lists`. Values are known existing daily operations from other workloads. Unknown existing usage is not measured by this tool; estimates remain partial account projections.

A custom `profile` specifies `name` and `path` to a JSON file matching the built-in profile shape. Paths resolve relative to the config file. Include all daily limits and the external-subrequest/D1-query invocation limits, provenance sources and a verification date. Developer budgets override limits without editing a profile. `quotalab limits show --path profile.json` displays custom reference data.

Local mode requires a literal loopback IP (for example `127.0.0.1` or `[::1]`), avoiding hostname resolution ambiguity. Remote mode requires `--allow-remote`; non-GET methods additionally require `--allow-remote-mutations`. Review all endpoints, including GET endpoints with side effects. Redirects are refused in every mode. Faults are forbidden remotely, even with opt-in flags.
