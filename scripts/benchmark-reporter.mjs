import { mkdir, writeFile } from "node:fs/promises";
export default class BenchmarkReporter {
  async onTestCaseResult(test) {
    for (const annotation of test.annotations()) {
      if (annotation.type !== "benchmark") continue;
      const result = JSON.parse(annotation.message);
      await mkdir("reports", { recursive: true });
      await writeFile(
        "reports/benchmark.json",
        JSON.stringify(result, null, 2) + "\n",
      );
      console.log("Local instrumentation benchmark:", result.samples);
    }
  }
}
