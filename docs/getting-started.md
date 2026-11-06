# Getting started

From the repository root, install dependencies and build:

```sh
corepack pnpm install --frozen-lockfile
pnpm build
cargo build --workspace
cargo install --path crates/quotalab-cli --locked
```

Run the healthy example with disposable state:

```sh
quotalab test --config examples/basic-worker/quotalab.yaml \
  --managed --wrangler-config examples/basic-worker/wrangler.jsonc \
  --seed-sql examples/basic-worker/migrations/0001_users.sql \
  --output reports/healthy
```

The reports include locally measured D1 metadata and KV attempts, estimated daily consumption, coverage warnings and policy results. No secrets, HTTP bodies, bound SQL parameters or authorization headers are stored. A missing external-fetch measurement is expected for this example because it does not use the explicit fetch wrapper; it is not silently replaced with zero.

For an already running Worker, enable local-test instrumentation in private local environment settings, set the same 32+ character `QUOTALAB_TOKEN` in the Worker and CLI environment, and omit `--managed`. Do not commit the token or place it in a command argument. Use `quotalab doctor` to probe connectivity and header telemetry. Black-box HTTP tests need no token but cannot measure binding resources.

Managed persistence is temporary and deleted at shutdown. Existing-runner mode uses whichever state the developer started: use a separate `--persist-to` directory and reset fixtures before each run. POST fixtures create records; concurrency can affect database state. Never reset shared or production data using the example SQL.

`pnpm test:integration` demonstrates all local success/failure journeys and writes artifacts under `reports/`. `quotalab analyze` applies a new budget/traffic configuration to saved observations; scenario definitions and seed should remain consistent with the original run.

If Rust doctests report incompatible compiler versions, check `rustc --version` and `rustdoc --version`; both must come from the same toolchain. With rustup, `RUSTDOC="$(rustup which rustdoc)" cargo test --workspace` selects the matching documentation compiler without changing the project.
