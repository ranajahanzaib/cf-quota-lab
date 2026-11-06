# Limitations and security

Local workerd execution is not production execution. HTTP duration is not CPU time. Local D1 row metadata, duration and SQLite plans can differ from production behavior. QuotaLab cannot precisely reproduce platform CPU enforcement or infer account-wide resource usage from one application's workload.

Some D1 methods expose no row metadata; those measurements stay unknown. Sessions returned by `withSession()` are not instrumented and mark coverage incomplete. Operations through undeclared bindings or direct native references bypass wrappers. Outbound fetch coverage excludes hidden redirects and uninstrumented calls. No remote resource-metric collector is implemented.

Header telemetry is a bounded response-time snapshot. `waitUntil()` marks the snapshot incomplete because background operations may finish later. Unawaited application promises can still evade detection. WebSocket upgrades and opaque responses cannot safely carry reconstructed header telemetry and are returned unchanged. Exception behavior is preserved; an uncaught exception does not provide a successful telemetry envelope.

A representative workload is an assumption. Tiny samples, cache state, warmups, data mutations, retries and traffic skew affect estimates. The tool samples each configured scenario equally and applies separate traffic weights. It does not generate an unbounded load test or predict maximum throughput. Existing account usage must be supplied explicitly; it is otherwise unknown.

SQL plans depend on data and indexes. SQL literals and comments are normalized, and bound values are never captured. Quoted identifiers and schema names remain structural information; avoid embedding personal data in identifiers and disable `collectSql` for sensitive schemas. Fingerprints are grouping identifiers, not cryptographic anonymization. Reports deliberately omit response bodies, request payloads, configured headers and control tokens.

Local controls require both explicit test configuration and a token; never set `QUOTALAB_LOCAL=true` in deployed environments. Local mode is an operator assertion, not something proven from Host headers. The package cannot independently verify where a Worker was deployed. Token possession grants access to local telemetry and fault controls: keep token files private, bind only to loopback and do not expose the development server through a tunnel. Managed mode uses an ephemeral private file and does not log the token.

Remote targets require explicit opt-in; write methods require a second opt-in. GET endpoints may still have application side effects: review the configured workload. Redirects are never followed. Remote fault injection is forbidden. Managed mode uses local bindings and rejects explicit remote binding configuration. JSON/JSONC Wrangler files are supported by the managed runner; TOML detection in `init` is informational.

Miniflare/workerd are development tooling, not a security boundary for untrusted code. Only run trusted application source and trusted fixture SQL. Managed subprocess cleanup uses Unix process groups on the primary macOS/Linux platforms; Windows process-tree cleanup is not covered. Abrupt SIGKILL or machine failure cannot run cleanup handlers. Temporary persistence may need OS cleanup after such termination.

The optional Analytics Engine exporter writes aggregate records via the native binding. It has no historical query client, delivery guarantee or remote validation, and never changes local measurements into `measured_remote`. Do not enable a remote binding in a local test expecting all traffic to remain local. Instrumentation overhead depends on application behavior and has no claimed negligible-cost guarantee.
