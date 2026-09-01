"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");
const { createBrowserVerdict, exitCodeForVerdict } = require("./browser-verdict");

test("all selected steps pass without gating on unselected results", () => {
  const verdict = createBrowserVerdict(
    ["login", "systems"],
    [
      { name: "login", ok: true },
      { name: "systems", ok: true },
      { name: "unselected", ok: false, error: "must not affect a focused run" },
    ],
  );

  assert.equal(verdict.ok, true);
  assert.equal(exitCodeForVerdict(verdict), 0);
  assert.deepEqual(verdict.failedRequiredSteps, []);
  assert.deepEqual(verdict.failedAdvisorySteps, []);
  assert.deepEqual(verdict.failedSteps, []);
  assert.deepEqual(
    verdict.selectedSteps.map(({ name, ok }) => ({ name, ok })),
    [
      { name: "login", ok: true },
      { name: "systems", ok: true },
    ],
  );
});

test("a failed selected step preserves its name and reason", () => {
  const verdict = createBrowserVerdict(
    ["login", "systems"],
    [
      { name: "login", ok: true },
      { name: "systems", ok: false, error: "Deploy request returned HTTP 500" },
    ],
  );

  assert.equal(verdict.ok, false);
  assert.equal(exitCodeForVerdict(verdict), 1);
  assert.deepEqual(verdict.failedSteps, [
    { name: "systems", reason: "Deploy request returned HTTP 500" },
  ]);
});

test("an advisory failure is reported without failing the blocking verdict", () => {
  const verdict = createBrowserVerdict(
    ["login", "systems"],
    [
      { name: "login", ok: true },
      { name: "systems", ok: false, error: "historical advisory failure" },
    ],
    { requiredSteps: ["login"] },
  );

  assert.equal(verdict.ok, true);
  assert.equal(exitCodeForVerdict(verdict), 0);
  assert.deepEqual(verdict.failedRequiredSteps, []);
  assert.deepEqual(verdict.failedAdvisorySteps, [
    { name: "systems", reason: "historical advisory failure" },
  ]);
  assert.deepEqual(verdict.failedSteps, []);
});

test("missing required and advisory results retain distinct policies", () => {
  const verdict = createBrowserVerdict(["login", "systems"], [], {
    requiredSteps: ["login"],
  });

  assert.equal(verdict.ok, false);
  assert.deepEqual(verdict.failedRequiredSteps, [
    { name: "login", reason: "selected step did not produce a result" },
  ]);
  assert.deepEqual(verdict.failedAdvisorySteps, [
    { name: "systems", reason: "selected step did not produce a result" },
  ]);
});

test("a missing selected result and a process error cannot pass", () => {
  const verdict = createBrowserVerdict(
    ["login", "systems"],
    [{ name: "login", ok: true }],
    { processError: "unhandled rejection: browser disconnected" },
  );

  assert.equal(verdict.ok, false);
  assert.deepEqual(verdict.failedRequiredSteps, [
    { name: "systems", reason: "selected step did not produce a result" },
  ]);
  assert.deepEqual(verdict.failedSteps, [
    { name: "systems", reason: "selected step did not produce a result" },
  ]);
  assert.equal(verdict.processError, "unhandled rejection: browser disconnected");
});
