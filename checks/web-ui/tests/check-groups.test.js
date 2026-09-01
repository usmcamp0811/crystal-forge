"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const {
  needsAuthenticationPreflight,
  requiredStepNames,
  selectProfileSteps,
  selectRequestedSteps,
  validateCheckGroups,
} = require("./check-groups");

const manifest = JSON.parse(
  fs.readFileSync(path.join(__dirname, "..", "coverage-manifest.json"), "utf8"),
);
const config = JSON.parse(
  fs.readFileSync(path.join(__dirname, "..", "check-groups.json"), "utf8"),
);

test("required groups partition ci_fast exactly once", () => {
  assert.doesNotThrow(() => validateCheckGroups(manifest, config));
  const requiredCount = manifest.steps.filter((step) => step.profiles.includes("ci_fast")).length;
  const ownedCount = config.requiredGroups.reduce(
    (count, name) => count + config.groups[name].steps.length,
    0,
  );
  assert.equal(ownedCount, requiredCount);
});

test("ci_fast ownership preserves historical required steps and advisory execution", () => {
  const executable = manifest.steps
    .filter((step) => step.profiles.includes(config.requiredProfile))
    .map(({ name }) => ({ name }));
  const required = requiredStepNames(executable, config, config.requiredProfile);
  assert.deepEqual([...required].sort(), [
    "01-login-page",
    "02-registration",
    "03-registration-submit",
    "04-post-register-login",
    "05-login-submit",
    "12-systems",
    "13-flakes",
    "15j-builds-latest-per-flake-populated",
    "15k-builds-latest-combined-filters-empty-clear",
    "16-cves",
    "16b-cves-severity-filter",
    "19-policies-new-modal-fields",
    "20-policies-new-modal-rule-builder",
    "20a-policies-new-modal-pending-mappings",
    "20aa-policies-new-modal-mappings-roundtrip",
    "20ab-compliance-bundle-requirement-baseline-roundtrip",
    "20ab2-policy-editor-eight-kind-roundtrip",
    "20ac-policy-editor-category-and-imported-provenance",
    "20ac-stig-import-reconciliation-fixture",
    "20ad-stig-nixos-assertion-roundtrip",
    "20af-policy-catalog-selection-delete-regressions",
    "20b-policies-cve-gate-create-roundtrip",
    "20c-policies-multirule-create-roundtrip",
    "20d-policies-cve-gate-invalid-rejected",
    "20e-policies-multirule-rules-only-no-expression-required",
    "26c-evaluations-latest-per-flake-populated",
    "26d-evaluations-latest-combined-filters-empty-clear",
    "29g-poam-failed-evidence-create",
    "29h-poam-link-compatible-findings",
    "29i-poam-detail-edits-milestones-conflicts",
    "29k-poam-system-rollups-navigation",
    "29l-poam-bundle-rollups-batching",
    "29m-poam-assignment-relationship-immutability",
    "30a-admin-automatic-retries-defaults-reset",
    "30b-admin-automatic-retries-save-reload",
    "30c-admin-automatic-retries-failed-save-retains-draft",
    "30d-evidence-lifecycle",
    "30e-policy-card-direct-edit-preserves-evidence",
    "task433-canonical-imported-stig-refinement",
    "task433-canonical-large-catalog",
    "task433-canonical-mixed-nix-cve-evidence",
    "task433-canonical-multiline-dod",
    "task433-canonical-poam-lifecycle",
    "task433-canonical-unmapped-nix-policy",
  ].sort());
  assert.equal(executable.length - required.size, 72);
});

test("compatibility smoke requires all six selected steps", () => {
  const executable = manifest.steps.map(({ name }) => ({ name }));
  const selected = selectProfileSteps(executable, manifest, config, "compatibility");
  assert.deepEqual(
    [...requiredStepNames(selected, config, "compatibility")],
    config.groups.compatibility.steps,
  );
  assert.deepEqual(config.groups.compatibility.advisorySteps, []);
});

test("named profiles preserve manifest order and reject unknown profiles", () => {
  const executable = manifest.steps.map(({ name }) => ({ name }));
  const fleet = selectProfileSteps(executable, manifest, config, "fleet");
  assert.deepEqual(fleet.map((step) => step.name), config.groups.fleet.steps);
  assert.throws(
    () => selectProfileSteps(executable, manifest, config, "not-a-profile"),
    /unknown UI test profile/,
  );
});

test("validator rejects duplicate required ownership", () => {
  const invalid = structuredClone(config);
  const duplicate = invalid.groups.fleet.steps[0];
  invalid.groups.pipeline.steps.push(duplicate);
  invalid.groups.pipeline.advisorySteps.push(duplicate);
  assert.throws(() => validateCheckGroups(manifest, invalid), /exactly one/);
});

test("validator rejects missing or conflicting step policy", () => {
  const missing = structuredClone(config);
  missing.groups.fleet.advisorySteps.shift();
  assert.throws(() => validateCheckGroups(manifest, missing), /classify each step/);

  const conflicting = structuredClone(config);
  conflicting.groups.fleet.requiredSteps.push(conflicting.groups.fleet.advisorySteps[0]);
  assert.throws(() => validateCheckGroups(manifest, conflicting), /classify each step/);
});

test("requested steps cannot be unknown or excluded by a known profile", () => {
  const executable = manifest.steps.map(({ name }) => ({ name }));
  const fleet = selectProfileSteps(executable, manifest, config, "fleet");
  assert.throws(
    () => selectRequestedSteps(executable, fleet, new Set(["not-a-step"]), "fleet"),
    /unknown requested UI test steps/,
  );
  assert.throws(
    () => selectRequestedSteps(executable, fleet, new Set(["15-builds"]), "fleet"),
    /excluded by profile "fleet"/,
  );
});

test("named groups authenticate when the ordered auth chain is absent", () => {
  const executable = manifest.steps.map(({ name }) => ({ name }));
  const fleet = selectProfileSteps(executable, manifest, config, "fleet");
  const governance = selectProfileSteps(executable, manifest, config, "governance");
  assert.equal(needsAuthenticationPreflight(fleet), true);
  assert.equal(needsAuthenticationPreflight(governance), false);
});

test("ci_fast Setup Coach workflows remain advisory in the fleet group", () => {
  const coachSteps = [
    "06a-onboarding-coach-dashboard",
    "06g-onboarding-coach-minimized",
    "06h-onboarding-coach-all-configured",
  ];
  for (const step of coachSteps) {
    assert.ok(config.groups.fleet.steps.includes(step));
    assert.ok(config.groups.fleet.advisorySteps.includes(step));
    assert.ok(!config.groups.fleet.requiredSteps.includes(step));
  }
});

test("compatibility smoke retains auth, dashboard, and context-scoped coach suppression", () => {
  assert.deepEqual(config.groups.compatibility.steps, [
    "01-login-page",
    "02-registration",
    "03-registration-submit",
    "04-post-register-login",
    "05-login-submit",
    "06-dashboard",
  ]);
  const source = fs.readFileSync(path.join(__dirname, "integration-test.js"), "utf8");
  assert.match(source, /async function suppressOnboardingCoach\(pageOrContext\)/);
  assert.match(source, /await context\.addInitScript/);
  assert.ok(
    source.indexOf("await suppressOnboardingCoach(context)") <
      source.indexOf("const createStepPage = async"),
  );
});

test("failed-page route cleanup suppresses detached errors and focused 12h retains tab semantics", () => {
  const source = fs.readFileSync(path.join(__dirname, "integration-test.js"), "utf8");
  assert.match(source, /Promise\.allSettled\(\[\s*page\.unrouteAll\(\{ behavior: "ignoreErrors" \}\),\s*page\.close\(\)/);
  const workflow12h = source.slice(
    source.indexOf('name: "12h-system-detail-cves-package-workflow"'),
    source.indexOf('name: "12d-systems-api-error-no-mock-fallback"'),
  );
  assert.match(workflow12h, /getByRole\("tab", \{ name: "CVEs" \}\)/);
});

test("browser shard diagnostics publish current step and classify process timeout", () => {
  const source = fs.readFileSync(path.join(__dirname, "integration-test.js"), "utf8");
  const driver = fs.readFileSync(path.join(__dirname, "..", "default.nix"), "utf8");

  assert.match(source, /writeJsonAtomically\(currentStepPath/);
  assert.ok(
    source.indexOf("writeJsonAtomically(currentStepPath") <
      source.indexOf("await step.action(page)"),
  );
  assert.match(source, /currentStep: readCurrentStep\(\)/);
  assert.match(driver, /playwrightProcessTimeout \? 900/);
  assert.match(driver, /timeout --signal=TERM --kill-after=30s/);
  assert.match(driver, /integration\.exit\.tmp/);
  assert.match(driver, /if exit_code in \["124", "137"\]/);
  assert.match(driver, /current-step\.json/);
  assert.match(driver, /no logical verdict was produced/);
});
