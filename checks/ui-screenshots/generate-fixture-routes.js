/**
 * Generate fixture routes — extracts the route response bodies from
 * capture.js's buildRoutes() and writes them to a JSON file.
 *
 * Usage:
 *   node generate-fixture-routes.js <fixturesJson> <outputRoutesJson>
 *
 * The output is a JSON object mapping URL paths to response bodies,
 * e.g. { "/api/v1/dashboard/summary": { ... }, ... }
 *
 * This runs at build time (Nix derivation) so the Rust server can
 * load the pre-computed responses without needing JS transformation.
 */
"use strict";

const fs   = require("fs");

// Import buildRoutes — same function used by capture.js
const { buildRoutes } = require("./routes.js");

const fixturesPath = process.argv[2];
const outputPath   = process.argv[3] || "/tmp/fixture-routes.json";

if (!fixturesPath) {
  console.error("usage: node generate-fixture-routes.js <fixturesJson> <outputRoutesJson>");
  process.exit(2);
}

const fixtures = JSON.parse(fs.readFileSync(fixturesPath, "utf8"));
const routes   = buildRoutes(fixtures);

// Build a flat path→body map.  For function predicates we include
// multiple entries (the predicate _label tells us the canonical path).
const routeMap = {};
for (const r of routes) {
  let path;
  if (typeof r.pattern === "string") {
    path = r.pattern;
  } else if (typeof r.pattern === "function") {
    path = r.pattern._label || "fn";
  } else {
    path = r.pattern.source;
  }
  // Strip trailing * from matchPrefix labels
  path = path.replace(/\*$/, "");
  routeMap[path] = r.body;
}

fs.writeFileSync(outputPath, JSON.stringify(routeMap, null, 2));
console.log(`Wrote ${Object.keys(routeMap).length} fixture routes to ${outputPath}`);
