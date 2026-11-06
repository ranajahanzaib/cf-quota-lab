import { createQuotaLabInstrumentation } from "../../../packages/worker/src/index";
export const app = {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === "/__quotalab_ready") return new Response("ready");
    try {
      if (url.pathname.startsWith("/api/users/") && request.method === "GET") {
        await env.CACHE.get("public-banner");
        const result = await env.DB.prepare(
          "SELECT id, name FROM users WHERE id = ?",
        )
          .bind(Number(url.pathname.split("/").pop()))
          .all();
        return Response.json({ ok: true, users: result.results });
      }
      if (url.pathname === "/api/users" && request.method === "GET") {
        const email = url.searchParams.get("email");
        const limit = Math.max(
          1,
          Math.min(Number(url.searchParams.get("limit")) || 20, 100),
        );
        const result = email
          ? await env.DB.prepare("SELECT id, name FROM users WHERE email = ?")
              .bind(email)
              .all()
          : await env.DB.prepare(
              "SELECT id, name FROM users WHERE id > ? ORDER BY id LIMIT ?",
            )
              .bind(Number(url.searchParams.get("after")) || 0, limit)
              .all();
        return Response.json({ ok: true, users: result.results });
      }
      if (url.pathname === "/api/users" && request.method === "POST") {
        const body = await request.json<{ name?: string; email?: string }>();
        if (
          typeof body.name !== "string" ||
          typeof body.email !== "string" ||
          body.name.length > 100 ||
          body.email.length > 200
        )
          return Response.json({ error: "invalid_user" }, { status: 400 });
        const result = await env.DB.prepare(
          "INSERT INTO users (name,email) VALUES (?,?)",
        )
          .bind(body.name, body.email)
          .run();
        return Response.json(
          { ok: true, id: result.meta.last_row_id },
          { status: 201 },
        );
      }
      return Response.json({ error: "not_found" }, { status: 404 });
    } catch {
      return Response.json(
        { error: "dependency_unavailable" },
        { status: 503 },
      );
    }
  },
};
export default {
  fetch(request: Request, env: Env, ctx: ExecutionContext) {
    return createQuotaLabInstrumentation({
      enabled: env.QUOTALAB_LOCAL === "true",
      localTestMode: env.QUOTALAB_LOCAL === "true",
      token: env.QUOTALAB_TOKEN,
      d1Bindings: ["DB"],
      kvBindings: ["CACHE"],
    })
      .wrap(app)
      .fetch(request, env, ctx);
  },
};
