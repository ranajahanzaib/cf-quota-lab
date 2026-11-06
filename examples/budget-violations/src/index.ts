import { createQuotaLabInstrumentation } from "../../../packages/worker/src/index";
import { app } from "../../basic-worker/src/index";
export default {
  fetch(request: Request, env: Env, ctx: ExecutionContext) {
    const bad = {
      async fetch(request: Request, env: Env): Promise<Response> {
        if (new URL(request.url).pathname === "/__quotalab_ready")
          return new Response("ready");
        try {
          // Deliberate regression: a unique write on every request, without a useful cache purpose.
          await env.CACHE.put(`visit-${crypto.randomUUID()}`, "1");
          // Same application SQL, deliberately missing users_email in this example's fixture.
          return await app.fetch(request, env);
        } catch {
          return Response.json(
            { error: "unhandled_dependency" },
            { status: 500 },
          );
        }
      },
    };
    return createQuotaLabInstrumentation({
      enabled: env.QUOTALAB_LOCAL === "true",
      localTestMode: env.QUOTALAB_LOCAL === "true",
      token: env.QUOTALAB_TOKEN,
      d1Bindings: ["DB"],
      kvBindings: ["CACHE"],
    })
      .wrap(bad)
      .fetch(request, env, ctx);
  },
};
