# Architecture

`quotalab-core` owns configuration, explicit observations, versioned limits, weighted projections, policy evaluation, SQLite plans and report rendering. `quotalab` owns CLI arguments, HTTP execution and subprocess lifecycle. The TypeScript package owns request-scoped interception and bounded telemetry. These boundaries keep accounting testable without a Worker process.

Each invocation creates independent counters, a prepared-statement WeakMap, fault counters and a query buffer. Environment proxies wrap only declared bindings. Native methods execute with their native receiver. Prepared statements are unwrapped before passing to native `batch()`. Returned values and thrown exceptions are preserved; `first()` and `raw()` are never rewritten into a different execution method.

The runner sends a run/scenario/request identity and token only to literal loopback targets. Authorized local instrumentation serializes observations into one bounded response header. The runner validates identities and schema before accepting it. Header contents contain normalized SQL structure, counters and limitations, never request bodies or bound parameters. Limits apply to serialized size and retained queries; dropped events invalidate complete totals.

The Rust runner polls only `concurrency` request futures at once, uses a finite request count and explicit timeouts, and does not follow redirects. SplitMix64 chooses a deterministic ordering within each round and selects JSON fixtures independently of task completion order. Every scenario receives the configured sample count; traffic weights affect projections. Wall-clock latency and completion order can vary.

Managed mode starts Wrangler directly using Node, avoids shell command construction, uses loopback networking and disables remote bindings. A temporary environment file contains the ephemeral token, and a separate temporary directory contains D1/KV state. Unix process groups and RAII guards stop Wrangler/workerd on errors, SIGINT or SIGTERM; other platforms use child termination. Seed subprocesses use the same cleanup guard.

Observations flow into scenario summaries, per-invocation peaks, fingerprint groups and traffic projections. Missing data propagates through required totals. Fault scenarios should have zero traffic weight: their counters describe synthetic behavior and are excluded from normal traffic forecasts.

Local D1 metadata comes from workerd's local binding implementation. SQLite `EXPLAIN QUERY PLAN` is separate evidence from an explicitly selected database opened read-only. Neither is authoritative production measurement. Account projections include configured existing usage but cannot discover other Workers automatically.
