# CI integration

The repository CI builds Rust and TypeScript, checks formatting/lint/types, runs workerd unit tests, runs the entire managed local integration suite, validates JSON Schemas, writes Markdown summaries and uploads reports. Cloudflare credentials are not needed. macOS and Ubuntu are covered by the workflow matrix.

The integration script expects exit 2 for the inefficient Worker and failing fallback behavior; those assertions do not make normal CI red. A healthy run unexpectedly failing, or an inefficient run unexpectedly passing, fails the suite.

For an application using QuotaLab, adapt `.github/workflows/quotalab-example.yml`. Run `quotalab test --managed` with the application's Wrangler config and disposable seed SQL. Preserve the CLI exit code. Write `$GITHUB_STEP_SUMMARY` and upload `reports/` using `if: always()` so failure evidence remains available.

Use `policy.missing_measurements: fail` when complete measurement is required. The examples use `warn` because their outbound subrequest count is unmeasured; do not copy that choice blindly for resource paths where coverage is mandatory. Explicit CPU budgets necessarily lack local production CPU evidence and can fail with exit 4.

`analyze` can rerun budget/traffic evaluation on a saved observation report, without issuing requests. Keep scenario/fixture semantics consistent; historical response bodies are not retained. `report` regenerates views without re-execution or policy changes.

Report schema version 1 is defined in `schemas/report.schema.json`, generated from the Rust models. Run `quotalab schemas` after changing those models and validate actual integration artifacts with `pnpm check:schemas`. Operational diagnostics go to stderr so `--format json` stdout can be parsed directly.

The workflows are executable configuration. This repository's local checks verify the underlying commands; a GitHub-hosted workflow run is only verified after pushing and observing Actions.
