#!/usr/bin/env node
"use strict";

const fs = require("node:fs");

const verdictPath = process.argv[2];
if (!verdictPath) {
  console.error("usage: check-web-ui-verdict.js <check-verdict.json>");
  process.exit(2);
}

try {
  const verdict = JSON.parse(fs.readFileSync(verdictPath, "utf8"));
  for (const failure of verdict?.failedAdvisorySteps || []) {
    console.warn(`advisory browser failure: ${failure.name}: ${failure.reason}`);
  }
  if (verdict?.ok === true) process.exit(0);
  if (verdict?.ok === false) process.exit(1);
  console.error(`${verdictPath}: verdict must contain a boolean ok field`);
  process.exit(2);
} catch (error) {
  console.error(`${verdictPath}: ${error.message}`);
  process.exit(2);
}
