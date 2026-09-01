const assert = require("assert");
const fs = require("fs");
const path = require("path");

const { validateManifest } = require("./generate-design-targets.js");

const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, "manifest.json"), "utf8"));
validateManifest(manifest);

const names = manifest.views.map((view) => view.name);
assert.strictEqual(new Set(names).size, names.length, "target names must be unique");

for (const required of [
  "dashboard",
  "policies",
  "compliance",
  "compliance-evidence",
  "system-detail",
  "notifications",
  "setup-coach",
]) {
  assert(names.includes(required), `missing affected target ${required}`);
}

assert.throws(
  () => validateManifest({ views: [{ name: "dashboard", route: "/" }] }),
  /designMarker\.selector.*dioxusMarker\.selector/,
  "identity markers must be mandatory",
);

console.log("design-parity manifest contracts OK");
