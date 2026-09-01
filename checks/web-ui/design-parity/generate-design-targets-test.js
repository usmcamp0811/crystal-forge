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
  "policy-editor",
  "compliance",
  "compliance-evidence",
  "system-detail",
  "notifications",
  "setup-coach",
  "poam-detail",
]) {
  assert(names.includes(required), `missing affected target ${required}`);
}

const policyEditor = manifest.views.find((view) => view.name === "policy-editor");
assert.strictEqual(policyEditor.designRef, "docs/design/CrystalForge/components/PolicyEditor.jsx");
assert.strictEqual(policyEditor.designMarker.selector, ".pe-shell");
assert.strictEqual(policyEditor.dioxusMarker.selector, "[data-testid='policy-editor-modal']");
assert.deepStrictEqual(policyEditor.designActions, [
  { type: "click", selector: ".nav-item", text: "Policies", waitFor: ".content[data-screen-label='policies']" },
  { type: "click", selector: "[data-coach-target='policy']", text: "New custom policy", waitFor: ".pe-shell" },
]);
assert.deepStrictEqual(policyEditor.dioxusActions, [
  { type: "click", selector: "button", text: "New custom policy", waitFor: "[data-testid='policy-editor-modal']" },
]);

const poamDetail = manifest.views.find((view) => view.name === "poam-detail");
assert.strictEqual(poamDetail.designRef, "docs/design/CrystalForge/components/PoamViews.jsx");
assert.strictEqual(poamDetail.designMarker.selector, ".poam-tray");
assert.strictEqual(poamDetail.dioxusMarker.selector, "[data-testid='poam-detail']");
assert.deepStrictEqual(poamDetail.designActions, [
  {
    type: "click",
    selector: ".dash-widget:has(.dash-w-title:has-text('POA&M Watchlist')) .dash-w-body > div",
    text: "POAM-",
    waitFor: ".poam-tray",
  },
]);
assert.deepStrictEqual(poamDetail.dioxusActions, [
  {
    type: "click",
    selector: ".poam-watchlist-row",
    text: "TASK-433 parity POA&M detail",
    waitFor: "[data-testid='poam-detail']",
  },
]);

assert.throws(
  () => validateManifest({ views: [{ name: "dashboard", route: "/" }] }),
  /designMarker\.selector.*dioxusMarker\.selector/,
  "identity markers must be mandatory",
);

console.log("design-parity manifest contracts OK");
