#!/usr/bin/env node
// JS side of the template-renderer parity check. Renders every case in
// tests/template_parity.json with the shared frontend engine
// (src/template_engine.js) and asserts byte-identical output with the
// fixture's `expected` strings. The Rust side (template_renderer.rs) runs the
// same fixture in cargo test, so if both sides pass, the two renderers agree.
//
// Run: node scripts/check-template-parity.mjs

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { renderTemplateOutput } from "../src/template_engine.js";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const fixture = JSON.parse(
  readFileSync(join(root, "tests/template_parity.json"), "utf8"),
);

// Parity cases must never hit an error path; make failures loud.
const formatError = (key, params) =>
  `<<error:${key}:${JSON.stringify(params ?? {})}>>`;

let failures = 0;
for (const testCase of fixture.cases) {
  const result = renderTemplateOutput(testCase.blocks, fixture.metadata, {
    username: fixture.metadata.username,
    uploadDate: fixture.metadata.upload_date,
    formatError,
  });
  const actual = result.error !== undefined ? result.error : result.output;
  if (actual === testCase.expected) {
    console.log(`ok   ${testCase.name}`);
  } else {
    failures += 1;
    console.error(`FAIL ${testCase.name}`);
    console.error(`  expected: ${JSON.stringify(testCase.expected)}`);
    console.error(`  actual:   ${JSON.stringify(actual)}`);
  }
}

if (failures > 0) {
  console.error(`\n${failures} parity case(s) failed`);
  process.exit(1);
}
console.log(`\nall ${fixture.cases.length} parity cases passed (JS)`);
