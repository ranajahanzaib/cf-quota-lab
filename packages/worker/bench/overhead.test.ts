import { createExecutionContext } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { expect, it } from "vitest";
import { createQuotaLabInstrumentation } from "../src/index";
it("reports handler + local D1 wall-clock overhead without a performance guarantee", async ({
  annotate,
}) => {
  const token = "0123456789abcdef0123456789abcdef";
  const request = new Request("http://localhost/benchmark", {
    headers: { "x-quotalab-token": token },
  });
  const app = {
    async fetch(_r: Request, e: Env) {
      await e.DB.prepare("SELECT 1 AS n").all();
      return new Response("ok");
    },
  };
  const disabled = createQuotaLabInstrumentation().wrap(app);
  const enabled = createQuotaLabInstrumentation({
    enabled: true,
    localTestMode: true,
    token,
    d1Bindings: ["DB"],
  }).wrap(app);
  const samples: { disabled_ms: number[]; enabled_ms: number[] } = {
    disabled_ms: [],
    enabled_ms: [],
  };
  for (let round = 0; round < 3; round++)
    for (const [name, handler] of (round % 2
      ? [
          ["enabled_ms", enabled],
          ["disabled_ms", disabled],
        ]
      : [
          ["disabled_ms", disabled],
          ["enabled_ms", enabled],
        ]) as [keyof typeof samples, typeof enabled][]) {
      for (let i = 0; i < 10; i++)
        await handler.fetch(request, env as Env, createExecutionContext());
      const start = performance.now();
      for (let i = 0; i < 50; i++) {
        const response = await handler.fetch(
          request,
          env as Env,
          createExecutionContext(),
        );
        expect(response.status).toBe(200);
        await response.text();
      }
      samples[name].push((performance.now() - start) / 50);
    }
  await annotate(
    JSON.stringify({
      provenance: "measured_local",
      unit: "handler + local D1 milliseconds per invocation",
      calls_per_mode: 150,
      samples,
      note: "Includes digest authorization, wrappers and telemetry serialization; excludes HTTP transport. Not CPU time or production overhead.",
    }),
    "benchmark",
  );
});
