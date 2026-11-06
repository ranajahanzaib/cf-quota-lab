/** Request-scoped, opt-in instrumentation. Never patches global fetch. */
export type Provenance =
  | "measured_local"
  | "measured_remote"
  | "estimated"
  | "simulated"
  | "unavailable";
export interface Fault {
  service: "d1" | "kv" | "fetch";
  kind: "unavailable" | "exhausted" | "latency";
  after: number;
  delay_ms: number;
  operation?: string;
}
export interface Operation {
  attempts: number;
  successes: number;
  failures: number;
  units: number;
}
export interface Query {
  fingerprint: string;
  sql: string | null;
  method: string;
  rows_read: number | null;
  rows_written: number | null;
  duration_ms: number | null;
  success: boolean;
}
export interface Telemetry {
  version: 1;
  request_id: string;
  run_id: string;
  scenario: string;
  provenance: Provenance;
  counters: Record<string, number | null>;
  operations: Record<string, Operation>;
  queries: Query[];
  faults: string[];
  dropped: number;
  complete: boolean;
  coverage: string[];
}
export interface Options {
  enabled?: boolean;
  localTestMode?: boolean;
  token?: string;
  d1Bindings?: string[];
  kvBindings?: string[];
  collectSql?: boolean;
  maxQueries?: number;
  maxTelemetryBytes?: number;
  analytics?: AnalyticsEngineDataset;
}
export class QuotaLabSimulatedError extends Error {
  readonly simulated = true;
  constructor(
    readonly service: string,
    readonly kind: string,
  ) {
    super(`Simulated ${service} ${kind}`);
    this.name = "QuotaLabSimulatedError";
  }
}
// This lexer discards comments, string/blob literals, and numeric constants. Bound values never enter it.
export function normalizeSql(sql: string): string {
  let out = "",
    i = 0;
  while (i < sql.length) {
    if (
      sql[i] === "'" ||
      ((sql[i] === "x" || sql[i] === "X") && sql[i + 1] === "'")
    ) {
      if (sql[i] !== "'") i++;
      i++;
      while (i < sql.length) {
        if (sql[i] === "'") {
          if (sql[i + 1] === "'") {
            i += 2;
            continue;
          }
          i++;
          break;
        }
        i++;
      }
      out += "?";
      continue;
    }
    if (sql[i] === "-" && sql[i + 1] === "-") {
      while (i < sql.length && !"\r\n".includes(sql[i])) i++;
      out += " ";
      continue;
    }
    if (sql[i] === "/" && sql[i + 1] === "*") {
      i += 2;
      while (i < sql.length && !(sql[i] === "*" && sql[i + 1] === "/")) i++;
      i = Math.min(sql.length, i + 2);
      out += " ";
      continue;
    }
    // Quoted identifiers remain structural; users should disable SQL text for sensitive schemas.
    if (sql[i] === '"' || sql[i] === "`" || sql[i] === "[") {
      const close = sql[i] === "[" ? "]" : sql[i];
      out += sql[i++];
      while (i < sql.length) {
        const ch = sql[i++];
        out += ch;
        if (ch === close) {
          if (sql[i] === close) {
            out += sql[i++];
            continue;
          }
          break;
        }
      }
      continue;
    }
    out += sql[i++];
  }
  return out
    .replace(/\b(?:0x[\da-f]+|\d+(?:\.\d+)?(?:e[+-]?\d+)?)\b/gi, "?")
    .replace(/\s+/g, " ")
    .trim();
}
export function fingerprint(sql: string): string {
  let h = 2166136261;
  for (const c of sql) {
    h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  }
  return `Q-${(h >>> 0).toString(16).padStart(8, "0")}`;
}
function safeId(value: string | null): string {
  return value && /^[a-zA-Z0-9_-]{1,80}$/.test(value) ? value : "unknown";
}
function parseFault(value: string | null): Fault | undefined {
  if (!value) return;
  if (value.length > 512) throw new Error("Invalid QuotaLab fault controls");
  const f: unknown = JSON.parse(value);
  if (!f || typeof f !== "object")
    throw new Error("Invalid QuotaLab fault controls");
  const v = f as Record<string, unknown>;
  if (
    Object.keys(v).some(
      (k) => !["service", "kind", "after", "delay_ms", "operation"].includes(k),
    ) ||
    !["d1", "kv", "fetch"].includes(String(v.service)) ||
    !["unavailable", "exhausted", "latency"].includes(String(v.kind)) ||
    !Number.isSafeInteger(v.after) ||
    Number(v.after) < 1 ||
    !Number.isSafeInteger(v.delay_ms) ||
    Number(v.delay_ms) < 0 ||
    Number(v.delay_ms) > 10000 ||
    (v.operation !== undefined &&
      ![
        "get",
        "getWithMetadata",
        "put",
        "delete",
        "list",
        "run",
        "all",
        "first",
        "raw",
        "batch",
        "exec",
        "fetch",
      ].includes(String(v.operation)))
  )
    throw new Error("Invalid QuotaLab fault controls");
  return f as Fault;
}
function nativeProperty(target: object, prop: PropertyKey): unknown {
  const value: unknown = Reflect.get(target, prop, target);
  return typeof value === "function" ? value.bind(target) : value;
}
function numeric(v: unknown): number | null {
  return typeof v === "number" && Number.isSafeInteger(v) && v >= 0 ? v : null;
}
/** Access the explicit request-scoped outbound wrapper; does not patch global fetch. */
export function quotaLabFetch(env: object): typeof fetch {
  const wrapper: unknown = Reflect.get(env, "QUOTALAB_FETCH");
  return typeof wrapper === "function" ? (wrapper as typeof fetch) : fetch;
}
export function createQuotaLabInstrumentation(options: Options = {}) {
  const maxQueries = Math.max(0, Math.min(options.maxQueries ?? 32, 128));
  const maxBytes = Math.max(
    1024,
    Math.min(options.maxTelemetryBytes ?? 6144, 8192),
  );
  return {
    wrap<E extends object>(handler: {
      fetch(
        request: Request,
        env: E,
        ctx: ExecutionContext,
      ): Response | Promise<Response>;
    }) {
      return {
        async fetch(
          request: Request,
          env: E,
          ctx: ExecutionContext,
        ): Promise<Response> {
          if (
            !options.enabled ||
            !options.localTestMode ||
            !options.token ||
            options.token.length < 32
          )
            return handler.fetch(request, env, ctx);
          const supplied = request.headers.get("x-quotalab-token");
          if (!supplied || supplied.length > 256)
            return handler.fetch(request, env, ctx);
          const enc = new TextEncoder();
          const [a, b] = await Promise.all([
            crypto.subtle.digest("SHA-256", enc.encode(supplied)),
            crypto.subtle.digest("SHA-256", enc.encode(options.token)),
          ]);
          if (!crypto.subtle.timingSafeEqual(a, b))
            return handler.fetch(request, env, ctx);
          let fault: Fault | undefined;
          try {
            fault = parseFault(request.headers.get("x-quotalab-fault"));
          } catch {
            return new Response("Invalid QuotaLab controls", { status: 400 });
          }
          const t: Telemetry = {
            version: 1,
            request_id: safeId(request.headers.get("x-quotalab-request")),
            run_id: safeId(request.headers.get("x-quotalab-run")),
            scenario: safeId(request.headers.get("x-quotalab-scenario")),
            provenance: "measured_local",
            counters: {
              d1_queries: 0,
              d1_rows_read: 0,
              d1_rows_written: 0,
              kv_reads: 0,
              kv_writes: 0,
              kv_deletes: 0,
              kv_lists: 0,
              external_subrequests: null,
            },
            operations: {},
            queries: [],
            faults: [],
            dropped: 0,
            complete: true,
            coverage: [
              "configured bindings only",
              "first/raw/exec lack row metadata",
              "background operations after response may be missing",
              "fetch wrapper calls only; redirects and bypasses unobserved",
            ],
          };
          const counts: Record<string, number> = {};
          const add = (key: string, n: number | null) => {
            const old = t.counters[key];
            t.counters[key] =
              n === null || old === null ? null : (old ?? 0) + n;
          };
          async function before(
            service: string,
            operation: string,
          ): Promise<void> {
            if (
              !fault ||
              fault.service !== service ||
              (fault.operation && fault.operation !== operation)
            )
              return;
            counts[service] = (counts[service] ?? 0) + 1;
            if (counts[service] < fault.after) return;
            if (t.faults.length < 32)
              t.faults.push(`simulated:${service}:${operation}:${fault.kind}`);
            else {
              t.dropped++;
              t.complete = false;
            }
            if (fault.kind === "latency") {
              await new Promise((resolve) =>
                setTimeout(resolve, fault.delay_ms),
              );
              return;
            }
            throw new QuotaLabSimulatedError(service, fault.kind);
          }
          const statements = new WeakMap<
            object,
            { native: D1PreparedStatement; sql: string }
          >();
          const record = (
            sql: string,
            method: string,
            value: unknown,
            success: boolean,
          ) => {
            const meta =
              value && typeof value === "object" && "meta" in value
                ? (value as { meta: Record<string, unknown> }).meta
                : undefined;
            const read = numeric(meta?.rows_read),
              written = numeric(meta?.rows_written);
            add("d1_rows_read", read);
            add("d1_rows_written", written);
            if (t.queries.length >= maxQueries) {
              t.dropped++;
              t.complete = false;
              return;
            }
            const normalized = normalizeSql(sql);
            t.queries.push({
              fingerprint: fingerprint(normalized),
              sql:
                options.collectSql === false ? null : normalized.slice(0, 512),
              method,
              rows_read: read,
              rows_written: written,
              duration_ms:
                typeof meta?.duration === "number" ? meta.duration : null,
              success,
            });
          };
          const wrapStatement = (
            native: D1PreparedStatement,
            sql: string,
          ): D1PreparedStatement => {
            const proxy = new Proxy(native, {
              get(target, prop) {
                if (prop === "bind")
                  return (...args: unknown[]) =>
                    wrapStatement(
                      Reflect.apply(
                        target.bind,
                        target,
                        args,
                      ) as D1PreparedStatement,
                      sql,
                    );
                if (["run", "all", "first", "raw"].includes(String(prop)))
                  return async (...args: unknown[]) => {
                    add("d1_queries", 1);
                    try {
                      await before("d1", String(prop));
                      const value: unknown = await Reflect.apply(
                        Reflect.get(target, prop, target) as (
                          ...a: unknown[]
                        ) => unknown,
                        target,
                        args,
                      );
                      record(sql, String(prop), value, true);
                      return value;
                    } catch (e) {
                      record(sql, String(prop), undefined, false);
                      throw e;
                    }
                  };
                return nativeProperty(target, prop);
              },
            });
            statements.set(proxy, { native, sql });
            return proxy;
          };
          const wrapD1 = (db: D1Database): D1Database =>
            new Proxy(db, {
              get(target, prop) {
                if (prop === "prepare")
                  return (sql: string) =>
                    wrapStatement(target.prepare(sql), sql);
                if (prop === "batch")
                  return async (input: D1PreparedStatement[]) => {
                    const entries = input.map((s) => statements.get(s));
                    add("d1_queries", 1);
                    try {
                      await before("d1", "batch");
                      const result = await target.batch(
                        input.map((s, i) => entries[i]?.native ?? s),
                      );
                      result.forEach((v, i) =>
                        record(entries[i]?.sql ?? "", "batch", v, v.success),
                      );
                      return result;
                    } catch (e) {
                      entries.forEach((s) =>
                        record(s?.sql ?? "", "batch", undefined, false),
                      );
                      throw e;
                    }
                  };
                if (prop === "exec")
                  return async (sql: string) => {
                    // exec may contain multiple statements; its returned count is not rows-read metadata.
                    t.counters.d1_queries = null;
                    try {
                      await before("d1", "exec");
                      const value = await target.exec(sql);
                      record(sql, "exec", value, true);
                      return value;
                    } catch (e) {
                      record(sql, "exec", undefined, false);
                      throw e;
                    }
                  };
                if (prop === "withSession") {
                  t.complete = false;
                  return nativeProperty(target, prop);
                }
                return nativeProperty(target, prop);
              },
            });
          const writes = new Map<string, number>();
          const wrapKv = (kv: KVNamespace): KVNamespace =>
            new Proxy(kv, {
              get(target, prop) {
                const metric = (
                  {
                    get: "kv_reads",
                    getWithMetadata: "kv_reads",
                    put: "kv_writes",
                    delete: "kv_deletes",
                    list: "kv_lists",
                  } as Record<string, string>
                )[String(prop)];
                if (!metric) return nativeProperty(target, prop);
                return async (...args: unknown[]) => {
                  const units = Array.isArray(args[0]) ? args[0].length : 1;
                  add(metric, units);
                  const op = (t.operations[String(prop)] ??= {
                    attempts: 0,
                    successes: 0,
                    failures: 0,
                    units: 0,
                  });
                  op.attempts++;
                  op.units += units;
                  if (prop === "put" && typeof args[0] === "string") {
                    const hash = fingerprint(args[0]);
                    const now = Date.now();
                    if (writes.has(hash) && now - writes.get(hash)! < 1000)
                      t.coverage.length < 16 &&
                        t.coverage.push(
                          "repeated same-key write within 1s (request-local)",
                        );
                    if (writes.size < 128) writes.set(hash, now);
                  }
                  try {
                    await before("kv", String(prop));
                    const value: unknown = await Reflect.apply(
                      Reflect.get(target, prop, target) as (
                        ...a: unknown[]
                      ) => unknown,
                      target,
                      args,
                    );
                    op.successes++;
                    return value;
                  } catch (e) {
                    op.failures++;
                    throw e;
                  }
                };
              },
            });
          const bindings = new Map<PropertyKey, unknown>();
          const wrapped = new Proxy(env, {
            get(target, prop) {
              if (bindings.has(prop)) return bindings.get(prop);
              const value: unknown = Reflect.get(target, prop, target);
              if ((options.d1Bindings ?? []).includes(String(prop))) {
                const proxy = wrapD1(value as D1Database);
                bindings.set(prop, proxy);
                return proxy;
              }
              if ((options.kvBindings ?? []).includes(String(prop))) {
                const proxy = wrapKv(value as KVNamespace);
                bindings.set(prop, proxy);
                return proxy;
              }
              return value;
            },
          });
          const fetchWrapper: typeof fetch = async (input, init) => {
            if (t.counters.external_subrequests === null)
              t.counters.external_subrequests = 0;
            add("external_subrequests", 1);
            await before("fetch", "fetch");
            return fetch(input, init);
          };
          // Explicit accessor: application may read env.QUOTALAB_FETCH and use it for outbound calls.
          bindings.set("QUOTALAB_FETCH", fetchWrapper);
          if ((options.d1Bindings ?? []).length === 0) {
            t.counters.d1_queries = null;
            t.counters.d1_rows_read = null;
            t.counters.d1_rows_written = null;
          }
          if ((options.kvBindings ?? []).length === 0)
            for (const k of ["kv_reads", "kv_writes", "kv_deletes", "kv_lists"])
              t.counters[k] = null;
          let response: Response;
          const wrappedCtx = new Proxy(ctx, {
            get(target, prop) {
              if (prop === "waitUntil")
                return (promise: Promise<unknown>) => {
                  t.complete = false;
                  if (t.coverage.length < 16)
                    t.coverage.push(
                      "waitUntil background work may outlive telemetry snapshot",
                    );
                  target.waitUntil(promise);
                };
              return nativeProperty(target, prop);
            },
          });
          try {
            response = await handler.fetch(request, wrapped, wrappedCtx);
          } catch (e) {
            t.complete = false;
            throw e;
          }
          if (options.analytics) {
            try {
              options.analytics.writeDataPoint({
                indexes: [t.run_id],
                blobs: ["quotalab-v1", t.scenario, t.provenance],
                doubles: [
                  t.counters.d1_rows_read ?? -1,
                  t.counters.kv_writes ?? -1,
                ],
              });
            } catch {
              t.coverage.push("Analytics Engine export failed");
            }
          }
          const encode = () =>
            btoa(
              String.fromCharCode(
                ...new TextEncoder().encode(JSON.stringify(t)),
              ),
            );
          let payload = encode();
          while (payload.length > maxBytes && t.queries.length) {
            t.queries.pop();
            t.dropped++;
            t.complete = false;
            payload = encode();
          }
          if (payload.length > maxBytes) {
            t.coverage = ["telemetry truncated"];
            t.faults = t.faults.slice(0, 4);
            t.operations = {};
            t.dropped++;
            t.complete = false;
            payload = encode();
          }
          // WebSockets and opaque responses cannot be reconstructed safely.
          if (response.status === 101 || response.status === 0) return response;
          const result = new Response(response.body, response);
          result.headers.set("x-quotalab-telemetry", payload);
          return result;
        },
      };
    },
  };
}
