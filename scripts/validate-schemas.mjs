import Ajv2020 from "ajv/dist/2020.js";
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { parse } from "yaml";
const ajv = new Ajv2020({ allErrors: true, strict: false });
for (const [name, max] of [
  ["uint", Number.MAX_VALUE],
  ["uint8", 255],
  ["uint16", 65535],
  ["uint32", 4294967295],
  ["uint64", 18446744073709551615],
])
  ajv.addFormat(name, {
    type: "number",
    validate: (n) => Number.isInteger(n) && n >= 0 && n <= max,
  });
ajv.addFormat("double", { type: "number", validate: Number.isFinite });
const config = ajv.compile(
  JSON.parse(await readFile("schemas/config.schema.json", "utf8")),
);
const report = ajv.compile(
  JSON.parse(await readFile("schemas/report.schema.json", "utf8")),
);
for (const example of ["basic-worker", "budget-violations"])
  for (const name of ["quotalab.yaml", "faults.yaml"]) {
    const value = parse(await readFile(`examples/${example}/${name}`, "utf8"));
    assert(config(value), JSON.stringify(config.errors));
    assert(!config({ ...value, typo: true }));
    assert(
      !config({ ...value, workload: { ...value.workload, concurrency: 0 } }),
    );
  }
let count = 0;
for (const entry of await readdir("reports", { withFileTypes: true })) {
  if (!entry.isDirectory()) continue;
  const dir = entry.name;
  try {
    const r = JSON.parse(await readFile(`reports/${dir}/run.json`, "utf8"));
    assert(report(r), JSON.stringify(report.errors));
    count++;
  } catch (e) {
    if (e.code !== "ENOENT") throw e;
  }
}
assert(count >= 4, "run npm run test:integration before validating reports");
console.log(`Schemas validated: four configs and ${count} actual reports.`);
