import { cloudflareTest } from "@cloudflare/vitest-plugin";
import { defineConfig } from "vitest/config";
export default defineConfig({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: "./examples/basic-worker/wrangler.jsonc" },
    }),
  ],
  test: { include: ["packages/worker/tests/**/*.test.ts"], testTimeout: 15000 },
});
