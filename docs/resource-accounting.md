# Resource accounting

Every observation envelope has provenance. Request latency and throughput are local wall-clock measurements. Binding counters are local wrapper observations. Weighted traffic means and forecasts are estimates. Triggered faults are explicitly synthetic. Missing production CPU is unavailable, never zero or inferred from HTTP latency.

D1 `prepare()` and `bind()` do not count executions. `run()`/`all()` metadata supplies rows read/written and SQL duration where present. The package does not assume rows returned equal rows read. `first()`, `raw()` and `exec()` retain their native return values and contribute unknown row measurements; `exec()` also has an unknown invocation-query count because its script may contain multiple statements. A `batch()` counts once against the current invocation query limit, while its individual result metadata contributes row consumption. These semantics follow the [current D1 limits](https://developers.cloudflare.com/d1/platform/limits/); they are version-sensitive.

KV counters track attempted keys read/written/deleted and list requests. A bulk read counts its keys, while the operation record records one API attempt. Successes and failures are separate API counts. Attempts include failures and are conservative characterization inputs, not guaranteed billed consumption. Repeated same-key writes within a request produce a coverage note; cross-request or distributed write frequency is not inferred.

The explicit outbound wrapper counts calls, not hidden redirect requests. Unwrapped global fetch calls remain invisible. No global monkey-patching is performed. The report does not claim complete subrequest coverage; no wrapper calls yields unavailable, not an asserted zero.

For metric M, normal scenario i and normalized traffic weight w:

```
weighted_M = sum(w_i * observed_M_i / sample_count_i)
projected_daily_M = requests_per_day * weighted_M + existing_usage_M
utilization_percent = projected_daily_M / budget * 100
exhaustion_hours = 24 * budget / projected_daily_M
```

Exhaustion assumes uniform traffic and a fresh daily allowance; it is not a predicted wall-clock deadline. Zero budgets have no meaningful utilization percentage. Missing samples or unknown rows make complete projections unavailable; `observed_subtotal` remains available. A subtotal is not scaled into an invented complete forecast. Dropped telemetry and background work invalidate completeness.

Fault scenarios are excluded from normal resource forecasts. Set their weight to zero; a positive fault weight leaves the corresponding forecast unavailable. Behavior assertions still apply. Known existing usage adds to the estimate; account-wide usage outside the configured inputs remains unknown.

The [versioned reference profile](../profiles/workers-free.v1.json) records verification date and official sources. Storage limits are reference data, not measured storage usage. Analytics Engine reference limits are documented but not enforced by the workload policy engine. CPU enforcement can tolerate occasional excess and is not reproduced by a hard local timer.

`pnpm bench` compares disabled and enabled handlers with the same local D1 query over three interleaved batches. It reports local handler wall-clock samples, including authorization/wrapping/serialization, without an overhead threshold or production CPU claim. It excludes network transport and should be rerun on the developer's hardware and application.
