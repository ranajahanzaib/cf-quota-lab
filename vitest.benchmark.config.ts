import { cloudflareTest } from "@cloudflare/vitest-plugin";
import { defineConfig } from "vitest/config";
export default defineConfig({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: "./examples/basic-worker/wrangler.jsonc" },
    }),
  ],
  test: {
    reporters: ["default", "./scripts/benchmark-reporter.mjs"],
    include: ["packages/worker/bench/**/*.test.ts"],
    testTimeout: 30000,
  },
});
