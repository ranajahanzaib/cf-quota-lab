import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createServer as createHttpServer } from "node:http";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
const cli = resolve("target/debug/quotalab");
const tmp = await mkdtemp(join(tmpdir(), "quotalab-integration-"));
const reports = resolve("reports");
await mkdir(reports, { recursive: true });
async function port() {
  const s = createServer();
  await new Promise((r) => s.listen(0, "127.0.0.1", r));
  const p = s.address().port;
  await new Promise((r) => s.close(r));
  return p;
}
async function run(args, expected = 0, cwd = process.cwd()) {
  return new Promise((resolve, reject) => {
    const p = spawn(cli, args, {
      cwd,
      stdio: ["ignore", "pipe", "pipe"],
      env: { ...process.env, WRANGLER_SEND_METRICS: "false" },
    });
    let stdout = "",
      stderr = "";
    p.stdout.on("data", (d) => (stdout += d));
    p.stderr.on("data", (d) => (stderr += d));
    const timer = setTimeout(() => {
      p.kill("SIGINT");
      reject(new Error(`timed out: ${args.join(" ")}`));
    }, 90000);
    p.on("error", reject);
    p.on("exit", (code) => {
      clearTimeout(timer);
      try {
        assert.equal(code, expected, `${args.join(" ")}\n${stdout}\n${stderr}`);
        resolve({ stdout, stderr });
      } catch (e) {
        reject(e);
      }
    });
  });
}
async function managed(example, config, expected, name, seed) {
  const p = await port();
  await run(
    [
      "test",
      "--config",
      `examples/${example}/${config}`,
      "--url",
      `http://127.0.0.1:${p}`,
      "--managed",
      "--wrangler-config",
      `examples/${example}/wrangler.jsonc`,
      "--seed-sql",
      seed ?? `examples/${example}/migrations/0001_users.sql`,
      "--output",
      join(reports, name),
      "--format",
      "json",
    ],
    expected,
  );
  return JSON.parse(await readFile(join(reports, name, "run.json"), "utf8"));
}
try {
  await run(["init"], 0, tmp);
  await run(["init"], 3, tmp);
  await run(["init", "--force"], 0, tmp);
  const limits = JSON.parse((await run(["limits", "show"])).stdout);
  assert.equal(limits.name, "workers-free");
  assert(limits.sources.length > 0);
  const healthy = await managed("basic-worker", "quotalab.yaml", 0, "healthy");
  assert.equal(healthy.requests.length, 30);
  assert.equal(healthy.cpu_ms.value, null);
  assert.equal(healthy.cpu_ms.provenance, "unavailable");
  assert(healthy.requests.every((r) => r.telemetry));
  assert(
    healthy.projections.find((p) => p.metric === "d1_rows_read").daily.value >
      0,
  );
  const bad = await managed(
    "budget-violations",
    "quotalab.yaml",
    2,
    "violations",
  );
  assert(bad.findings.some((f) => f.code === "expensive_query"));
  assert(
    bad.findings.some(
      (f) => f.code === "daily_budget" && f.message.includes("kv_writes"),
    ),
  );
  const healthyFaults = await managed(
    "basic-worker",
    "faults.yaml",
    0,
    "healthy-faults",
  );
  const badFaults = await managed(
    "budget-violations",
    "faults.yaml",
    2,
    "violation-faults",
  );
  assert(
    healthyFaults.requests
      .filter((r) => r.scenario.endsWith("outage"))
      .every((r) => r.passed),
  );
  assert(
    badFaults.requests.some((r) => r.scenario === "kv-outage" && !r.passed),
  );
  const repeated = await managed(
    "basic-worker",
    "faults.yaml",
    0,
    "deterministic-faults",
  );
  const signature = (r) =>
    r.requests.map((q) => [q.index, q.scenario, q.telemetry?.faults]);
  assert.deepEqual(signature(healthyFaults), signature(repeated));
  // Apply the missing index to the inefficient fixture and run the corrected request handler.
  const fixedSeed = join(tmp, "fixed.sql");
  await writeFile(
    fixedSeed,
    (await readFile(
      "examples/budget-violations/migrations/0001_users.sql",
      "utf8",
    )) + "\nCREATE INDEX users_email ON users(email);\n",
  );
  const fixed = await managed(
    "basic-worker",
    "quotalab.yaml",
    0,
    "fixed",
    fixedSeed,
  );
  assert(
    fixed.projections.find((p) => p.metric === "d1_rows_read").daily.value <
      bad.projections.find((p) => p.metric === "d1_rows_read").daily.value,
  );
  await run([
    "analyze",
    "--input",
    "reports/healthy/run.json",
    "--config",
    "examples/basic-worker/quotalab.yaml",
    "--format",
    "json",
  ]);
  await run(
    [
      "report",
      "--input",
      "reports/violations/run.json",
      "--format",
      "markdown",
    ],
    2,
  );
  // Black-box, redirect and timeout tests use a genuine loopback HTTP server.
  const server = createHttpServer((req, res) => {
    if (req.url === "/slow") {
      setTimeout(() => res.end("late"), 200);
      return;
    }
    if (req.url === "/redirect") {
      res.writeHead(302, { location: "https://example.com" });
      res.end();
      return;
    }
    res.setHeader("content-type", "application/json");
    res.end('{"ok":true}');
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  const p = server.address().port;
  const original = await readFile(
    "examples/basic-worker/quotalab.yaml",
    "utf8",
  );
  const configPath = join(tmp, "test.yaml");
  let cfg = original
    .replace("http://127.0.0.1:8787", `http://127.0.0.1:${p}`)
    .replace("requests_per_scenario: 10", "requests_per_scenario: 1");
  await writeFile(configPath, cfg);
  await run([
    "doctor",
    "--config",
    configPath,
    "--wrangler-config",
    "examples/basic-worker/wrangler.jsonc",
  ]);
  await writeFile(
    configPath,
    cfg.replace("missing_measurements: warn", "missing_measurements: fail"),
  );
  await run(["test", "--config", configPath], 4);
  await writeFile(
    configPath,
    cfg.replace("path: /api/users/1", "path: /redirect"),
  );
  await run(["test", "--config", configPath], 2); // redirects are not followed
  await writeFile(
    configPath,
    cfg
      .replace("timeout_ms: 5000", "timeout_ms: 20")
      .replaceAll("path: /api/users", "path: /slow"),
  );
  await run(["test", "--config", configPath], 3);
  await new Promise((r) => server.close(r));
  await writeFile(configPath, cfg);
  await run(["test", "--config", configPath], 3); // now unreachable
  await writeFile(configPath, cfg.replace("concurrency: 3", "concurency: 3"));
  await run(["test", "--config", configPath], 3);
  await run(
    [
      "test",
      "--config",
      "examples/basic-worker/quotalab.yaml",
      "--managed",
      "--wrangler-config",
      join(tmp, "absent.jsonc"),
    ],
    3,
  );
  await writeFile(
    join(tmp, "broken.jsonc"),
    '{"name":"broken","main":"absent.ts","compatibility_date":"2026-10-30"}',
  );
  await run(
    [
      "test",
      "--config",
      "examples/basic-worker/quotalab.yaml",
      "--managed",
      "--wrangler-config",
      join(tmp, "broken.jsonc"),
    ],
    3,
  );
  await writeFile(
    configPath,
    cfg
      .replace(`http://127.0.0.1:${p}`, "https://example.com")
      .replace("mode: local", "mode: remote"),
  );
  await run(["test", "--config", configPath], 3);
  await run(["test", "--config", configPath, "--allow-remote"], 3); // mutations need separate consent
  await writeFile(
    configPath,
    (await readFile("examples/basic-worker/faults.yaml", "utf8"))
      .replace("http://127.0.0.1:8787", "https://example.com")
      .replace("mode: local", "mode: remote"),
  );
  await run(
    [
      "test",
      "--config",
      configPath,
      "--allow-remote",
      "--allow-remote-mutations",
    ],
    3,
  );
  if (process.platform !== "win32") {
    const localPort = await port();
    await writeFile(
      configPath,
      original.replace(
        "requests_per_scenario: 10",
        "requests_per_scenario: 10000",
      ),
    );
    const args = [
      "test",
      "--config",
      configPath,
      "--url",
      `http://127.0.0.1:${localPort}`,
      "--managed",
      "--wrangler-config",
      "examples/basic-worker/wrangler.jsonc",
      "--seed-sql",
      "examples/basic-worker/migrations/0001_users.sql",
    ];
    const processHandle = spawn(cli, args, {
      stdio: ["ignore", "ignore", "pipe"],
    });
    let diagnostic = "";
    processHandle.stderr.on("data", (d) => (diagnostic += d));
    const exited = new Promise((resolve) => processHandle.on("exit", resolve));
    let ready = false;
    for (let i = 0; i < 200; i++) {
      try {
        await fetch(`http://127.0.0.1:${localPort}/__quotalab_ready`, {
          signal: AbortSignal.timeout(100),
        });
        ready = true;
        break;
      } catch {}
      await new Promise((r) => setTimeout(r, 50));
    }
    assert(ready, "managed Worker failed to start for interruption check");
    processHandle.kill("SIGTERM");
    assert.equal(await exited, 3, diagnostic);
    let stopped = false;
    for (let i = 0; i < 100; i++) {
      try {
        await fetch(`http://127.0.0.1:${localPort}`, {
          signal: AbortSignal.timeout(100),
        });
      } catch {
        stopped = true;
        break;
      }
      await new Promise((r) => setTimeout(r, 25));
    }
    assert(stopped, "managed Worker remained reachable after termination");
  }
  console.log(
    "Integration passed: healthy, violations, fixes, faults, determinism, missing coverage, redirects, timeout, unreachable target, bad config, startup failure and remote safety.",
  );
} finally {
  await rm(tmp, { recursive: true, force: true });
}
