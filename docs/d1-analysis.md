# D1 regression analysis

Run both examples using the commands in the README. Both execute the same parameterized `WHERE email = ?` lookup against 2,000 seeded users. The healthy fixture has `users_email`; the inefficient fixture does not. The difference is observed through local D1 row metadata and can also be explained using SQLite plans.

The inefficient Worker additionally calls KV `put()` once on every request. At 20,000 projected requests/day it attempts 20,000 writes, compared with the developer budget of 800. This is independent of the D1 scan issue. Its outer KV failure path returns 500 rather than the declared 503 dependency response.

Minimal fixes:

1. Add `CREATE INDEX users_email ON users(email);` to the fixture migration, then rerun with reset state.
2. Remove the per-request visit-key write, as in the healthy handler. Do not replace it with a shared key written at request frequency; KV has a same-key write rate limit.
3. Catch dependency failures at the handler boundary and return a controlled response matching the declared contract.

The integration suite applies the index to the inefficient SQL fixture, uses the corrected handler, and asserts lower projected D1 row consumption plus passing policies. Rust tests separately compare `SCAN users` with indexed `SEARCH` plans on the same explicitly created SQLite database.

To inspect a retained report against a selected local database:

```sh
quotalab explain --database ./fixtures.sqlite --input reports/violations/run.json
```

Create the fixture database from the corresponding migration using SQLite tooling first. `explain` opens exactly that file read-only, enables `query_only`, binds NULL to captured normalized placeholders, and outputs the plan plus contextual findings. It never selects a production database implicitly or applies an index automatically.

A full scan is flagged when its referenced table has at least 500 rows or observed peak row reads provide equivalent evidence. Indexed scans are not automatically called missing-index problems. Temporary sorting and correlated plans are review suggestions. Planner choices depend on fixture cardinality, statistics, placeholder values, SQLite versions and indexing; EXPLAIN findings are structural evidence, not measured production runtime.

JSON fingerprint summaries include execution counts, total and peak rows read, rows written and weighted daily contribution when available. Missing row metadata remains unknown. Summaries rank by expected daily rows read, then stable fingerprint order. SQL literals and bound parameters do not enter fingerprints; SQL collection can be disabled entirely.
