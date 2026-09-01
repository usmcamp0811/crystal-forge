"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const test = require("node:test");

const repository = path.resolve(__dirname, "..");
const verdictChecker = path.join(repository, "ci/check-web-ui-verdict.js");

test("exact outer gate checker accepts true and rejects false verdicts", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "web-ui-verdict-"));
  const passing = path.join(directory, "passing.json");
  const failing = path.join(directory, "failing.json");
  fs.writeFileSync(passing, JSON.stringify({ ok: true }));
  fs.writeFileSync(failing, JSON.stringify({ ok: false }));
  assert.equal(spawnSync(verdictChecker, [passing]).status, 0);
  assert.equal(spawnSync(verdictChecker, [failing]).status, 1);
  const nixGate = fs.readFileSync(path.join(repository, "checks/web-ui/default.nix"), "utf8");
  assert.match(
    nixGate,
    /\$\{pkgs\.nodejs\}\/bin\/node \$\{gateVerdictChecker\} \$\{evidence\}\/screenshots\/check-verdict\.json/,
  );
});

test("outer gate checker exposes advisory failures without failing", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "web-ui-advisory-verdict-"));
  const verdict = path.join(directory, "verdict.json");
  fs.writeFileSync(verdict, JSON.stringify({
    ok: true,
    failedAdvisorySteps: [{ name: "12f", reason: "diagnostic fixture" }],
  }));
  const result = spawnSync(verdictChecker, [verdict], { encoding: "utf8" });
  assert.equal(result.status, 0);
  assert.match(result.stderr, /advisory browser failure: 12f: diagnostic fixture/);
});

test("Nix evidence records advisory design and optional phase outcomes", () => {
  const nixGate = fs.readFileSync(path.join(repository, "checks/web-ui/default.nix"), "utf8");
  assert.doesNotMatch(nixGate, /design-targets\.log 2>&1 \|\| true/);
  assert.doesNotMatch(nixGate, /design-parity\.log 2>&1 \|\| true/);
  assert.match(nixGate, /"missingOutputs": missing_design_outputs/);
  assert.match(nixGate, /"designParity": design_parity/);
  for (const phase of [
    "vmFixtureSetup",
    "browserSemanticExecution",
    "designParity",
    "exports",
    "evidenceFinalization",
  ]) {
    assert.match(nixGate, new RegExp(`\\"${phase}\\"`));
  }
  const exportsWrapper = fs.readFileSync(
    path.join(repository, "checks/web-ui-exports/default.nix"),
    "utf8",
  );
  assert.match(exportsWrapper, /runBrowserSemanticValidation = false/);
  assert.match(exportsWrapper, /runExportValidation = true/);
});

test("producer preserves a logical gate failure and copies evidence", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "web-ui-producer-"));
  const fakeNix = path.join(directory, "nix");
  const fakeCurl = path.join(directory, "curl");
  const fakeJq = path.join(directory, "jq");
  fs.writeFileSync(fakeNix, `#!/bin/sh
printf '%s\n' "$1" >>"$NIX_FAKE_LOG"
case "$1" in
  build) mkdir -p "$NIX_FAKE_EVIDENCE/screenshots"; printf '{"ok":false}' >"$NIX_FAKE_EVIDENCE/screenshots/check-verdict.json"; exit 23 ;;
  eval) printf '%s' "$NIX_FAKE_EVIDENCE"; exit 0 ;;
esac
`);
  fs.writeFileSync(fakeCurl, "#!/bin/sh\nprintf '{\"queued_duration\":1.25}'\n");
  fs.writeFileSync(fakeJq, "#!/bin/sh\nprintf '1250\\n'\n");
  fs.chmodSync(fakeNix, 0o755);
  fs.chmodSync(fakeCurl, 0o755);
  fs.chmodSync(fakeJq, 0o755);

  const result = spawnSync("bash", [path.join(repository, "ci/web-ui-producer.sh")], {
    cwd: directory,
    env: {
      ...process.env,
      CHECK_NAME: "web-ui-fleet",
      NIX_BIN: fakeNix,
      NIX_FAKE_EVIDENCE: path.join(directory, "store-evidence"),
      NIX_FAKE_LOG: path.join(directory, "nix.log"),
      CURL_BIN: fakeCurl,
      JQ_BIN: fakeJq,
      CI_API_V4_URL: "https://gitlab.example/api/v4",
      CI_JOB_TOKEN: "fixture-token",
    },
    encoding: "utf8",
  });
  assert.equal(result.status, 23);
  assert.equal(
    fs.existsSync(path.join(directory, "web-ui-evidence/web-ui-fleet/screenshots/check-verdict.json")),
    true,
  );
  const metadata = JSON.parse(
    fs.readFileSync(path.join(directory, "web-ui-evidence/web-ui-fleet/producer.json")),
  );
  assert.deepEqual(
    {
      status: metadata.status,
      gateStatus: metadata.gateStatus,
      evidenceLookupStatus: metadata.evidenceLookupStatus,
      evidenceCopyStatus: metadata.evidenceCopyStatus,
      verdictStatus: metadata.verdictStatus,
    },
    {
      status: "failed",
      gateStatus: 23,
      evidenceLookupStatus: 0,
      evidenceCopyStatus: 0,
      verdictStatus: 1,
    },
  );
  assert.ok(metadata.durationMilliseconds >= 0);
  assert.ok(metadata.phases.gateBuild.durationMilliseconds >= 0);
  assert.equal(metadata.cacheState, "realized-during-job");
  assert.equal(metadata.queueDurationMilliseconds, 1250);
  assert.equal(metadata.queueDurationSource, "gitlab-jobs-api");
  assert.equal(fs.readFileSync(path.join(directory, "nix.log"), "utf8"), "eval\nbuild\n");
});

test("producer identifies evidence infrastructure failure", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "web-ui-producer-"));
  const fakeNix = path.join(directory, "nix");
  fs.writeFileSync(fakeNix, `#!/bin/sh
case "$1" in build) exit 0 ;; eval) printf '%s' "$NIX_FAKE_EVIDENCE"; exit 0 ;; esac
`);
  fs.chmodSync(fakeNix, 0o755);
  const result = spawnSync("bash", [path.join(repository, "ci/web-ui-producer.sh")], {
    cwd: directory,
    env: {
      ...process.env,
      CHECK_NAME: "web-ui",
      NIX_BIN: fakeNix,
      NIX_FAKE_EVIDENCE: path.join(directory, "missing-evidence"),
    },
  });
  assert.equal(result.status, 70);
  const metadata = JSON.parse(
    fs.readFileSync(path.join(directory, "web-ui-evidence/web-ui/producer.json")),
  );
  assert.equal(metadata.status, "infrastructure-evidence-lookup-failure");
});

test("producer writes metadata and classifies an evidence copy failure", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "web-ui-copy-failure-"));
  const fakeNix = path.join(directory, "nix");
  const fakeCopy = path.join(directory, "copy");
  fs.writeFileSync(fakeNix, `#!/bin/sh
case "$1" in
  build) mkdir -p "$NIX_FAKE_EVIDENCE/screenshots"; printf '{"ok":true}' >"$NIX_FAKE_EVIDENCE/screenshots/check-verdict.json"; exit 0 ;;
  eval) printf '%s' "$NIX_FAKE_EVIDENCE"; exit 0 ;;
esac
`);
  fs.writeFileSync(fakeCopy, "#!/bin/sh\nexit 17\n");
  fs.chmodSync(fakeNix, 0o755);
  fs.chmodSync(fakeCopy, 0o755);
  const result = spawnSync("bash", [path.join(repository, "ci/web-ui-producer.sh")], {
    cwd: directory,
    env: {
      ...process.env,
      CHECK_NAME: "web-ui",
      NIX_BIN: fakeNix,
      COPY_BIN: fakeCopy,
      NIX_FAKE_EVIDENCE: path.join(directory, "store-evidence"),
    },
  });
  assert.equal(result.status, 70);
  const metadata = JSON.parse(
    fs.readFileSync(path.join(directory, "web-ui-evidence/web-ui/producer.json")),
  );
  assert.equal(metadata.status, "infrastructure-evidence-copy-failure");
  assert.equal(metadata.evidenceCopyStatus, 17);
});

test("advisory producer reports a false verdict without failing its outer job", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "web-ui-advisory-"));
  const fakeNix = path.join(directory, "nix");
  fs.writeFileSync(fakeNix, `#!/bin/sh
case "$1" in
  build) mkdir -p "$NIX_FAKE_EVIDENCE/screenshots"; printf '{"ok":false}' >"$NIX_FAKE_EVIDENCE/screenshots/check-verdict.json"; exit 0 ;;
  eval) printf '%s' "$NIX_FAKE_EVIDENCE"; exit 0 ;;
esac
`);
  fs.chmodSync(fakeNix, 0o755);
  const result = spawnSync("bash", [path.join(repository, "ci/web-ui-producer.sh")], {
    cwd: directory,
    env: {
      ...process.env,
      CHECK_NAME: "web-ui-design-parity",
      WEB_UI_BLOCKING: "false",
      NIX_BIN: fakeNix,
      NIX_FAKE_EVIDENCE: path.join(directory, "store-evidence"),
    },
  });
  assert.equal(result.status, 0);
  const metadata = JSON.parse(
    fs.readFileSync(path.join(directory, "web-ui-evidence/web-ui-design-parity/producer.json")),
  );
  assert.equal(metadata.status, "advisory-failed");
  assert.equal(metadata.verdictStatus, 1);
});

test("aggregator reports every producer and preserves detailed evidence", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "web-ui-aggregate-"));
  for (const check of ["web-ui", "web-ui-fleet", "web-ui-pipeline", "web-ui-exports", "web-ui-design-parity"]) {
    const screenshots = path.join(directory, "web-ui-evidence", check, "screenshots");
    fs.mkdirSync(screenshots, { recursive: true });
    fs.writeFileSync(path.join(directory, "web-ui-evidence", check, "producer.json"), JSON.stringify({
      check,
      status: check === "web-ui-fleet" ? "failed" : check === "web-ui-design-parity" ? "advisory-failed" : "passed",
      gateStatus: check === "web-ui-fleet" ? 1 : 0,
      evidenceLookupStatus: 0,
      evidenceCopyStatus: 0,
      durationMilliseconds: check === "web-ui-fleet" ? 12000 : 6000,
      queueDurationMilliseconds: 1500,
      queueDurationSource: "gitlab-jobs-api",
      cacheState: "substituted",
      phases: {
        gateBuild: { durationMilliseconds: 5000 },
        evidenceCopy: { durationMilliseconds: 500 },
      },
      jobUrl: `https://gitlab.example/jobs/${check}`,
    }));
  }
  fs.writeFileSync(
    path.join(directory, "web-ui-evidence/web-ui-fleet/screenshots/verdict.json"),
    JSON.stringify({
      failedRequiredSteps: [{ name: "fleet-step", reason: "HTTP 500" }],
      failedAdvisorySteps: [{ name: "fleet-advisory", reason: "historical noncritical" }],
    }),
  );
  fs.writeFileSync(
    path.join(directory, "web-ui-evidence/web-ui-design-parity/screenshots/design-drift-summary.md"),
    "Design similarity: 92%",
  );
  fs.writeFileSync(
    path.join(directory, "web-ui-evidence/web-ui/screenshots/dashboard.png"),
    "fixture",
  );
  fs.writeFileSync(
    path.join(directory, "web-ui-evidence/web-ui-exports/screenshots/oscal-export-results.json"),
    JSON.stringify([{ name: "oscal-download", ok: true }]),
  );

  const result = spawnSync("node", [path.join(repository, "ci/web-ui-aggregate.js")], {
    cwd: directory,
    env: { ...process.env, CI_MERGE_REQUEST_IID: "", CI_PIPELINE_ID: "123" },
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  const report = fs.readFileSync(path.join(directory, "web-ui-report/report.md"), "utf8");
  for (const check of ["web-ui", "web-ui-fleet", "web-ui-pipeline", "web-ui-governance", "web-ui-exports", "web-ui-design-parity"]) {
    assert.match(report, new RegExp(check));
  }
  assert.match(report, /web-ui-governance.*missing/);
  assert.match(report, /required: fleet-step: HTTP 500/);
  assert.match(report, /advisory: fleet-advisory: historical noncritical/);
  assert.match(report, /Design similarity: 92%/);
  assert.match(report, /dashboard\.png/);
  assert.match(report, /oscal-export-results\.json/);
  assert.match(report, /required evidence is missing/);
  assert.match(report, /Blocking critical path: unavailable/);
  assert.match(report, /substituted/);
  assert.equal(fs.existsSync(path.join(directory, "web-ui-report/aggregation.json")), true);
  let timing = JSON.parse(
    fs.readFileSync(path.join(directory, "web-ui-report/aggregation.json")),
  );
  assert.equal(timing.blockingCriticalPathMilliseconds, null);
  assert.equal(timing.hasCompleteBlockingTiming, false);
  assert.equal(timing.producerTimings.length, 6);

  const governance = path.join(directory, "web-ui-evidence/web-ui-governance");
  fs.mkdirSync(path.join(governance, "screenshots"), { recursive: true });
  fs.writeFileSync(path.join(governance, "producer.json"), JSON.stringify({
    check: "web-ui-governance",
    status: "passed",
    durationMilliseconds: 8000,
    queueDurationMilliseconds: 2000,
    queueDurationSource: "gitlab-jobs-api",
    cacheState: "local-hit",
    phases: {
      gateBuild: { durationMilliseconds: 7000 },
      evidenceCopy: { durationMilliseconds: 500 },
    },
  }));
  const completeResult = spawnSync("node", [path.join(repository, "ci/web-ui-aggregate.js")], {
    cwd: directory,
    env: { ...process.env, CI_MERGE_REQUEST_IID: "", CI_PIPELINE_ID: "123" },
    encoding: "utf8",
  });
  assert.equal(completeResult.status, 0, completeResult.stderr);
  timing = JSON.parse(fs.readFileSync(path.join(directory, "web-ui-report/aggregation.json")));
  assert.equal(timing.blockingCriticalPathMilliseconds, 12000);
  assert.equal(timing.hasCompleteBlockingTiming, true);
});

test("aggregator keeps its report when GitLab API calls fail", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "web-ui-api-failure-"));
  const bin = path.join(directory, "bin");
  fs.mkdirSync(bin);
  fs.writeFileSync(path.join(bin, "curl"), "#!/bin/sh\nprintf '{\"message\":\"denied\"}'\n");
  fs.chmodSync(path.join(bin, "curl"), 0o755);

  const result = spawnSync("node", [path.join(repository, "ci/web-ui-aggregate.js")], {
    cwd: directory,
    env: {
      ...process.env,
      PATH: `${bin}:${process.env.PATH}`,
      CI_MERGE_REQUEST_IID: "7",
      CI_PIPELINE_ID: "123",
      CI_API_V4_URL: "https://gitlab.example/api/v4",
      CI_PROJECT_ID: "9",
      GITLAB_TOKEN: "",
    },
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(fs.existsSync(path.join(directory, "web-ui-report/report.md")), true);
  assert.match(result.stderr, /report remains available as an artifact/);
  assert.match(result.stderr, /GITLAB_TOKEN is unavailable/);
});

test("aggregator uses PRIVATE-TOKEN and caps inline uploads", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "web-ui-api-auth-"));
  const bin = path.join(directory, "bin");
  const curlLog = path.join(directory, "curl.log");
  const screenshots = path.join(directory, "web-ui-evidence/web-ui/screenshots");
  fs.mkdirSync(screenshots, { recursive: true });
  fs.writeFileSync(path.join(screenshots, "one.png"), "one");
  fs.writeFileSync(path.join(screenshots, "two.png"), "two");
  fs.mkdirSync(bin);
  fs.writeFileSync(path.join(bin, "curl"), `#!/bin/sh
printf '%s\n' "$*" >>"${curlLog}"
case "$*" in *uploads*) printf '{"markdown":"![upload](/file.png)"}' ;; *) printf '[]' ;; esac
`);
  fs.chmodSync(path.join(bin, "curl"), 0o755);

  const result = spawnSync("node", [path.join(repository, "ci/web-ui-aggregate.js")], {
    cwd: directory,
    env: {
      ...process.env,
      PATH: `${bin}:${process.env.PATH}`,
      CI_MERGE_REQUEST_IID: "7",
      CI_PIPELINE_ID: "123",
      CI_API_V4_URL: "https://gitlab.example/api/v4",
      CI_PROJECT_ID: "9",
      GITLAB_TOKEN: "fixture-token",
      WEB_UI_INLINE_SCREENSHOT_LIMIT: "1",
    },
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  const calls = fs.readFileSync(curlLog, "utf8");
  assert.match(calls, /PRIVATE-TOKEN:/);
  assert.doesNotMatch(calls, /JOB-TOKEN:/);
  assert.equal((calls.match(/\/uploads/g) || []).length, 1);
});

test("GitLab CI declares exact blocking and advisory Web UI producers", () => {
  const yaml = fs.readFileSync(path.join(repository, ".gitlab-ci.yml"), "utf8");
  const matrix = yaml.match(/web-ui-check:[\s\S]*?web-ui-design-parity:/)?.[0] || "";
  for (const check of ["web-ui", "web-ui-fleet", "web-ui-pipeline", "web-ui-governance", "web-ui-exports"]) {
    assert.match(matrix, new RegExp(`- ${check}(?:\\n|$)`));
  }
  assert.doesNotMatch(matrix, /- web-ui-design-parity/);
  const generic = yaml.match(/flake-check:[\s\S]*?web-ui-check:/)?.[0] || "";
  assert.doesNotMatch(generic, /- web-ui(?:\n|$)/);
  const aggregator = yaml.match(/web-ui-evidence-report:[\s\S]*?cve-processing-test:/)?.[0] || "";
  assert.match(aggregator, /when: always/);
  assert.match(aggregator, /allow_failure: true/);
  assert.match(aggregator, /needs:/);
  assert.doesNotMatch(aggregator, /dependencies:/);
  assert.doesNotMatch(aggregator, /image: alpine/);
  assert.match(aggregator, /nix run \.#web-ui-evidence-report/);
  assert.match(aggregator, /- web-ui-evidence\//);
  assert.match(matrix, /nix run \.#web-ui-producer/);
});
