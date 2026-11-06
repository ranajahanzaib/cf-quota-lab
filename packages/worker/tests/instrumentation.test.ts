import { createExecutionContext } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { beforeAll, describe, expect, it } from "vitest";
import {
  createQuotaLabInstrumentation,
  normalizeSql,
  type Options,
  type Telemetry,
} from "../src/index";
const token = "0123456789abcdef0123456789abcdef";
const bindings = env as Env;
async function invoke(
  fn: (env: Env) => Promise<Response>,
  options: Options = {},
  fault?: object,
  id = "r-1",
) {
  const wrapped = createQuotaLabInstrumentation({
    enabled: true,
    localTestMode: true,
    token,
    d1Bindings: ["DB"],
    kvBindings: ["CACHE"],
    ...options,
  }).wrap({ fetch: (_request: Request, e: Env) => fn(e) });
  const request = new Request("http://localhost/test", {
    headers: {
      "x-quotalab-token": token,
      "x-quotalab-run": "test-run",
      "x-quotalab-scenario": "test",
      "x-quotalab-request": id,
      ...(fault ? { "x-quotalab-fault": JSON.stringify(fault) } : {}),
    },
  });
  const response = await wrapped.fetch(
    request,
    bindings,
    createExecutionContext(),
  );
  const header = response.headers.get("x-quotalab-telemetry");
  return {
    response,
    t: header ? (JSON.parse(atob(header)) as Telemetry) : null,
  };
}
beforeAll(async () => {
  await bindings.DB.exec(
    "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, email TEXT, name TEXT); INSERT OR IGNORE INTO users VALUES (1, 'one@example.test', 'One');",
  );
});
describe("D1 native binding interception", () => {
  it("preserves native this, chaining, metadata and parameter privacy", async () => {
    const { response, t } = await invoke(async (e) => {
      const result = await e.DB.prepare("SELECT * FROM users WHERE email = ?")
        .bind("one@example.test")
        .all();
      return Response.json(result.results);
    });
    expect(response.status).toBe(200);
    expect((await response.json()) as object).toHaveLength(1);
    expect(t?.counters.d1_queries).toBe(1);
    expect(t?.counters.d1_rows_read).toBeGreaterThan(0);
    expect(JSON.stringify(t)).not.toContain("one@example.test");
  });
  it("batch unwraps statements, preserves ordering, accounts metadata once and one invocation query", async () => {
    const { t } = await invoke(async (e) => {
      const result = await e.DB.batch([
        e.DB.prepare("SELECT 1 AS n"),
        e.DB.prepare("SELECT 2 AS n"),
      ]);
      expect(result[0].results).toEqual([{ n: 1 }]);
      expect(result[1].results).toEqual([{ n: 2 }]);
      return new Response("ok");
    });
    expect(t?.queries).toHaveLength(2);
    expect(t?.counters.d1_queries).toBe(1);
  });
  it("first and raw preserve values and report unknown row counts", async () => {
    const { t } = await invoke(async (e) => {
      expect(
        await e.DB.prepare("SELECT name FROM users WHERE id=1").first("name"),
      ).toBe("One");
      expect(
        await e.DB.prepare("SELECT name FROM users WHERE id=1").raw(),
      ).toEqual([["One"]]);
      return new Response("ok");
    });
    expect(t?.counters.d1_rows_read).toBeNull();
    expect(t?.queries).toHaveLength(2);
  });
  it("exec remains native and marks query and row counts unknown", async () => {
    const { t } = await invoke(async (e) => {
      const result = await e.DB.exec("SELECT 1;");
      expect(result.count).toBe(1);
      return new Response("ok");
    });
    expect(t?.counters.d1_queries).toBeNull();
    expect(t?.counters.d1_rows_read).toBeNull();
  });
  it("preserves underlying exceptions", async () => {
    const { t } = await invoke(async (e) => {
      await expect(
        e.DB.prepare("SELECT * FROM absent_table").all(),
      ).rejects.toThrow();
      return new Response("caught");
    });
    expect(t?.queries[0].success).toBe(false);
    expect(t?.queries[0].rows_read).toBeNull();
  });
});
describe("KV and invocation isolation", () => {
  it("tracks operations, units, successes and preserves get type", async () => {
    const { t } = await invoke(async (e) => {
      await e.CACHE.put("a", "hello");
      expect(await e.CACHE.get("a")).toBe("hello");
      await e.CACHE.getWithMetadata("a");
      await e.CACHE.list();
      await e.CACHE.delete("a");
      return new Response("ok");
    });
    expect(t?.counters).toMatchObject({
      kv_reads: 2,
      kv_writes: 1,
      kv_lists: 1,
      kv_deletes: 1,
    });
    expect(t?.operations.put).toEqual({
      attempts: 1,
      successes: 1,
      failures: 0,
      units: 1,
    });
  });
  it("counts bulk keys separately from API calls", async () => {
    const { t } = await invoke(async (e) => {
      const result = await e.CACHE.get(["a", "b"]);
      expect(result).toBeInstanceOf(Map);
      return new Response("ok");
    });
    expect(t?.counters.kv_reads).toBe(2);
    expect(t?.operations.get.attempts).toBe(1);
  });
  it("isolates concurrent counters", async () => {
    const result = await Promise.all(
      Array.from({ length: 8 }, (_, i) =>
        invoke(
          async (e) => {
            for (let n = 0; n <= i; n++) await e.CACHE.get("x");
            return new Response("ok");
          },
          {},
          undefined,
          `r-${i}`,
        ),
      ),
    );
    expect(result.map((x) => x.t?.counters.kv_reads)).toEqual([
      1, 2, 3, 4, 5, 6, 7, 8,
    ]);
  });
  it("fault schedule is deterministic and avoids actual writes", async () => {
    const run = () =>
      invoke(
        async (e) => {
          await e.CACHE.put("fault", "first");
          await expect(e.CACHE.put("fault", "second")).rejects.toThrow(
            "Simulated kv exhausted",
          );
          return new Response("fallback", { status: 503 });
        },
        {},
        { service: "kv", kind: "exhausted", after: 2, delay_ms: 0 },
      );
    const a = await run(),
      b = await run();
    expect(a.t?.faults).toEqual(b.t?.faults);
    expect(a.t?.operations.put).toEqual({
      attempts: 2,
      successes: 1,
      failures: 1,
      units: 2,
    });
    expect(await bindings.CACHE.get("fault")).toBe("first");
    expect(a.response.status).toBe(503);
  });
});
describe("transport boundaries", () => {
  it("disabled and production modes expose no telemetry", async () => {
    for (const options of [{ enabled: false }, { localTestMode: false }]) {
      const { response } = await invoke(
        async () => new Response("ok"),
        options,
      );
      expect(response.headers.has("x-quotalab-telemetry")).toBe(false);
    }
  });
  it("rejects invalid controls", async () => {
    const { response } = await invoke(
      async () => new Response("ok"),
      {},
      { service: "kv", kind: "exhausted", after: -1, delay_ms: 0 },
    );
    expect(response.status).toBe(400);
  });
  it("bounds telemetry and exposes dropped observations", async () => {
    const { t, response } = await invoke(
      async (e) => {
        for (let i = 0; i < 20; i++) await e.DB.prepare("SELECT 1").all();
        return new Response("ok");
      },
      { maxQueries: 2, maxTelemetryBytes: 2048 },
    );
    expect(t?.dropped).toBeGreaterThan(0);
    expect(t?.complete).toBe(false);
    expect(
      response.headers.get("x-quotalab-telemetry")!.length,
    ).toBeLessThanOrEqual(2048);
  });
  it("can omit SQL entirely", async () => {
    const { t } = await invoke(
      async (e) => {
        await e.DB.prepare("SELECT 'secret' AS n").all();
        return new Response("ok");
      },
      { collectSql: false },
    );
    expect(t?.queries[0].sql).toBeNull();
    expect(JSON.stringify(t)).not.toContain("secret");
  });
  it("fingerprints do not include literals or comments", () => {
    expect(
      normalizeSql(
        "SELECT * FROM users WHERE email='secret' AND id=42 -- token",
      ),
    ).toBe("SELECT * FROM users WHERE email=? AND id=?");
  });
});
describe("control and background coverage", () => {
  it("does not authorize telemetry or faults with a Host header", async () => {
    const wrapper = createQuotaLabInstrumentation({
      enabled: true,
      localTestMode: true,
      token,
      d1Bindings: ["DB"],
    }).wrap({
      fetch: async (_r: Request, e: Env) => {
        await e.DB.prepare("SELECT 1").all();
        return new Response("ok");
      },
    });
    const response = await wrapper.fetch(
      new Request("https://example.com", {
        headers: {
          host: "127.0.0.1",
          "x-quotalab-token": "wrong-token",
          "x-quotalab-fault": "invalid",
        },
      }),
      bindings,
      createExecutionContext(),
    );
    expect(response.status).toBe(200);
    expect(response.headers.has("x-quotalab-telemetry")).toBe(false);
  });
  it("normalizes comment markers inside literals without exposing fragments", () => {
    expect(
      normalizeSql(
        "SELECT 'secret--private', 'secret/*private*/value' -- note",
      ),
    ).toBe("SELECT ?, ?");
  });
  it("marks waitUntil work incomplete", async () => {
    const wrapped = createQuotaLabInstrumentation({
      enabled: true,
      localTestMode: true,
      token,
    }).wrap({
      fetch: (_r: Request, _e: Env, ctx: ExecutionContext) => {
        ctx.waitUntil(Promise.resolve());
        return new Response("ok");
      },
    });
    const response = await wrapped.fetch(
      new Request("http://localhost", {
        headers: { "x-quotalab-token": token },
      }),
      bindings,
      createExecutionContext(),
    );
    const t = JSON.parse(
      atob(response.headers.get("x-quotalab-telemetry")!),
    ) as Telemetry;
    expect(t.complete).toBe(false);
  });
  it("filters faults by operation and preserves response headers", async () => {
    const { t, response } = await invoke(
      async (e) => {
        await e.CACHE.get("x");
        await expect(e.CACHE.put("x", "y")).rejects.toThrow("Simulated");
        return new Response("fallback", {
          status: 503,
          headers: { "x-existing": "preserved" },
        });
      },
      {},
      {
        service: "kv",
        operation: "put",
        kind: "unavailable",
        after: 1,
        delay_ms: 0,
      },
    );
    expect(t?.operations.get.successes).toBe(1);
    expect(t?.operations.put.failures).toBe(1);
    expect(response.headers.get("x-existing")).toBe("preserved");
  });
});
it("exports compact Analytics Engine records without relabeling local provenance", async () => {
  const points: AnalyticsEngineDataPoint[] = [];
  const { t } = await invoke(
    async (e) => {
      await e.CACHE.get("public");
      return new Response("ok");
    },
    {
      analytics: {
        writeDataPoint(point) {
          if (point) points.push(point);
        },
      },
    },
  );
  expect(points).toHaveLength(1);
  expect(points[0].blobs).toEqual(["quotalab-v1", "test", "measured_local"]);
  expect(t?.provenance).toBe("measured_local");
});
