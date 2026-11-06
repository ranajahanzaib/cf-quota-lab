# Fault injection

Fault controls require enabled instrumentation, explicit local-test mode, a 32+ character token, and authenticated request controls. Managed mode creates the token. A Host header is never sufficient. Keep local controls disabled and the token empty in deployment configuration.

A scenario may specify:

```yaml
fault:
  service: kv # d1 | kv | fetch
  operation: put # optional; defaults to all operations for that service
  kind: exhausted # unavailable | exhausted | latency
  after: 3
  delay_ms: 0
expect:
  status: 503
  json_contains:
    error: dependency_unavailable
  fault_triggered: true
```

The third matching operation and later operations fail before reaching the binding. The trigger counts matching attempted operations within that invocation. It does not enforce an actual account quota. A batch is one matching operation. `latency` awaits `delay_ms` (at most 10,000) before performing the operation; a request timeout can characterize slow dependencies.

`QuotaLabSimulatedError` has `simulated: true`, `service`, and `kind`. Its message explicitly says simulated. It is not an authentic Cloudflare error/code. Application catches and fallbacks remain responsible for returning the promised response. Triggering an error alone is not a passing fault test.

Use `quotaLabFetch(env)` for explicit outbound instrumentation. The returned request-scoped wrapper is available during enabled invocations and otherwise falls back to ordinary `fetch`. Redirect hops and calls bypassing this accessor are unobserved. The reserved internal environment property is `QUOTALAB_FETCH`.

The seed selects request/fixture ordering; operation counters select faults. Fixed configuration, fixtures, seed and local state reproduce those choices. Concurrent write ordering and latency are not deterministic. Give normal-traffic scenarios positive weights and synthetic failure scenarios zero weight.

The healthy example handles both D1 exhaustion and KV outage with controlled 503 responses. The inefficient example fails the KV outage response assertion. `pnpm test:integration` runs each and compares repeated fault signatures.
