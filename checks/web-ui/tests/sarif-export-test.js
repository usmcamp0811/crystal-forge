/**
 * SARIF Export Integration Test
 *
 * Exercises the real production SARIF 2.1.0 export path in the web UI:
 * 1. Ensures a local admin exists and authenticates
 * 2. Routes compliance API endpoints with deterministic data for two systems:
 *    one has failing and warning controls; the other passes all evaluated
 *    controls. Both systems have a shared waived control.
 * 3. Opens the Export evidence modal, selects SARIF 2.1.0 format
 * 4. Captures the browser-triggered file download
 * 5. Validates the downloaded file against the vendored OASIS SARIF 2.1.0
 *    Errata 01 schema using Draft4Validator + FormatChecker (so empty URI
 *    fields are caught), plus semantic checks (ruleId references, host
 *    locations, waiver suppressions)
 *
 * Usage: node sarif-export-test.js <baseUrl> <outputDir> <schemaPath>
 *   baseUrl    - URL of the Crystal Forge server (default: http://127.0.0.1:3000)
 *   outputDir  - Directory for screenshots and result JSON
 *   schemaPath - Path to sarif-schema-2.1.0.json (vendored from OASIS)
 */
const { chromium } = require("playwright");
const fs = require("fs");
const { execSync } = require("child_process");
const { ensureLocalAdmin } = require("./export-auth");

const baseUrl   = process.argv[2] || "http://127.0.0.1:3000";
const outputDir = process.argv[3] || "/tmp/screenshots";
const schemaPath = process.argv[4] || "";

const LOAD_TIMEOUT = 10000;

// ── Deterministic test UUIDs ────────────────────────────────────────────────
const BUNDLE_ID  = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const POLICY_CVE = "11111111-1111-4111-8111-111111111111"; // fail
const POLICY_PKG = "22222222-2222-4222-8222-222222222222"; // warn
const POLICY_SSH = "33333333-3333-4333-8333-333333333333"; // pass
const POLICY_FW  = "44444444-4444-4444-8444-444444444444"; // waiver
const ENV_ID     = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
const SYS1_ID    = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"; // prod-web-01
const SYS2_ID    = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"; // prod-db-01

function buildBundleData() {
  return [{
    id: BUNDLE_ID,
    name: "NIST 800-53 High",
    framework: "NIST 800-53",
    version: "rev5",
    description: "NIST 800-53 rev5 High baseline.",
    layer: "fleet",
    owner: "Platform Security",
    last_review: "2026-01-15T00:00:00Z",
    policy_ids: [POLICY_CVE, POLICY_PKG, POLICY_SSH, POLICY_FW],
    required_envs: [{ id: ENV_ID, name: "production", color_hex: "#3b82f6" }],
    control_count: 4,
    environment_count: 1,
  }];
}

function buildSystemsResponse() {
  return {
    bundle_id: BUNDLE_ID,
    systems: [
      {
        system_id: SYS1_ID, hostname: "prod-web-01", environment: "production",
        applies: true, total: 4, pass: 1, warn: 1, fail: 1, waiver: 1, score: 50,
      },
      {
        system_id: SYS2_ID, hostname: "prod-db-01", environment: "production",
        applies: true, total: 4, pass: 3, warn: 0, fail: 0, waiver: 1, score: 100,
      },
    ],
    totals: {
      system_count: 2, fully_compliant_count: 1,
      pass: 4, warn: 1, fail: 1, waiver: 2, total_controls: 8, overall_score: 75,
    },
  };
}

function buildEvidenceResponse(systemId, hostname) {
  const common = [
    {
      policy_id: POLICY_SSH, policy_name: "require_ssh_hardening",
      status: "pass", severity: "medium",
      summary: `${hostname} SSH configuration is hardened.`,
      evidence_items: [],
      framework_mapping: "AC-17 → require_ssh_hardening",
    },
    {
      policy_id: POLICY_FW, policy_name: "require_firewall",
      status: "waiver", severity: "high",
      summary: `${hostname} firewall waiver in effect — legacy network segment.`,
      evidence_items: [],
      framework_mapping: "SC-7 → require_firewall",
    },
  ];

  if (systemId === SYS1_ID) {
    return {
      bundle_id: BUNDLE_ID, system_id: SYS1_ID, hostname,
      controls: [
        {
          policy_id: POLICY_CVE, policy_name: "require_no_critical_cves",
          status: "fail", severity: "high",
          summary: `${hostname} has 3 critical CVEs (threshold 0).`,
          evidence_items: [{
            kind: "cve_scan", label: "CVE scan",
            body: "critical_cves=3",
            artifact: { artifact_type: "cve_scan", title: "CVE Scan", body: "CVE-2024-7001" },
          }],
          framework_mapping: "SI-2 → require_no_critical_cves",
        },
        {
          policy_id: POLICY_PKG, policy_name: "require_packages",
          status: "warn", severity: "medium",
          summary: `${hostname} package inventory incomplete — manual review needed.`,
          evidence_items: [],
          framework_mapping: "CM-7 → require_packages",
        },
        ...common,
      ],
    };
  }

  return {
    bundle_id: BUNDLE_ID, system_id: SYS2_ID, hostname,
    controls: [
      {
        policy_id: POLICY_CVE, policy_name: "require_no_critical_cves",
        status: "pass", severity: "high",
        summary: `${hostname} has no critical CVEs.`,
        evidence_items: [],
        framework_mapping: "SI-2 → require_no_critical_cves",
      },
      {
        policy_id: POLICY_PKG, policy_name: "require_packages",
        status: "pass", severity: "medium",
        summary: `${hostname} package set is compliant.`,
        evidence_items: [],
        framework_mapping: "CM-7 → require_packages",
      },
      ...common,
    ],
  };
}

async function assertVisible(locator, message, timeoutMs = 5000) {
  const visible = await locator.isVisible({ timeout: timeoutMs }).catch(() => false);
  if (!visible) throw new Error(message);
}

async function routeCompliance(page) {
  const handleRoute = async (route) => {
    const url    = route.request().url();
    const method = route.request().method();

    const evidenceMatch = url.match(/\/compliance\/bundles\/[^/]+\/systems\/([^/]+)\/evidence/);
    if (evidenceMatch) {
      const sysId = evidenceMatch[1];
      const data  = sysId === SYS2_ID
        ? buildEvidenceResponse(SYS2_ID, "prod-db-01")
        : buildEvidenceResponse(SYS1_ID, "prod-web-01");
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(data) });
      return;
    }

    const systemsMatch = url.match(/\/compliance\/bundles\/[^/]+\/systems/);
    if (systemsMatch) {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(buildSystemsResponse()) });
      return;
    }

    if (method === "GET") {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(buildBundleData()) });
      return;
    }

    await route.continue();
  };

  await page.route("**/api/v1/compliance/bundles*", handleRoute);
  await page.route(`**/api/v1/compliance/bundles/${BUNDLE_ID}/systems*`, handleRoute);
  await page.route(
    `**/api/v1/compliance/bundles/${BUNDLE_ID}/systems/*/evidence*`,
    handleRoute,
  );
}

// ── Main ────────────────────────────────────────────────────────────────────

(async () => {
  console.log("Starting SARIF Export Integration Test");
  console.log(`  Base URL:    ${baseUrl}`);
  console.log(`  Output:      ${outputDir}`);
  console.log(`  Schema path: ${schemaPath}`);
  console.log("");

  const results = [];
  let stepOk    = true;
  let stepError = null;
  let currentStep = {
    name: "sarif-export",
    description: "SARIF export end-to-end",
  };

  const browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page    = await context.newPage();

  const record = (name, description, ok, error) => {
    results.push({ name, description, ok, error: error || null });
    console.log(`  [${ok ? "OK" : "FAIL"}] ${name}${error ? " - " + error : ""}`);
  };

  try {
    // ── Step 1: Authenticate ───────────────────────────────────────────────
    currentStep = { name: "authenticate", description: "Authenticate local admin" };
    console.log("Step: authenticate");
    await ensureLocalAdmin(page, baseUrl);
    record("authenticate", "Authenticate local admin", true);

    // ── Step 2: Navigate to compliance with mocked data ───────────────────
    currentStep = {
      name: "compliance-navigate",
      description: "Navigate to compliance with mocked data",
    };
    console.log("Step: compliance-navigate");
    await routeCompliance(page);
    await page.goto(`${baseUrl}/compliance`, { timeout: LOAD_TIMEOUT });
    await page.waitForTimeout(2000);

    await assertVisible(
      page.getByRole("heading", { name: /^Compliance$/i }).first(),
      "Expected Compliance page heading",
    );
    await assertVisible(
      page.getByText("NIST 800-53 High").first(),
      "Expected bundle in catalog",
    );

    await page.getByText("NIST 800-53 High").first().click({ force: true });
    await page.waitForTimeout(1500);

    await assertVisible(
      page.getByText("prod-web-01").first(),
      "Expected system hostname in matrix",
    );
    await page.getByTestId("compliance-drawer-close").click();
    record("compliance-navigate", "Navigate to compliance with mocked data", true);

    // ── Step 3: Open export modal ─────────────────────────────────────────
    currentStep = { name: "export-open", description: "Open export modal" };
    console.log("Step: export-open");
    await page.getByRole("button", { name: /Import \/ Export/i }).click();
    await page.getByText(/Export evidence report/i).click();
    await page.waitForTimeout(800);

    await assertVisible(
      page.getByRole("heading", { name: /Export evidence/i }).first(),
      "Expected export modal heading",
    );
    record("export-open", "Open export modal", true);

    // ── Step 4: Select SARIF format ───────────────────────────────────────
    currentStep = { name: "select-sarif", description: "Select SARIF 2.1.0 format" };
    console.log("Step: select-sarif");
    await page.getByText(/SARIF 2\.1\.0/i).first().click({ force: true });
    await page.waitForTimeout(400);

    // Confirm the button label changed to "Download SARIF 2.1.0"
    await assertVisible(
      page.getByRole("button", { name: /Download SARIF/i }).first(),
      "Expected Download button to show SARIF format name",
    );
    record("select-sarif", "Select SARIF 2.1.0 format", true);

    // ── Step 5: Trigger download ──────────────────────────────────────────
    currentStep = { name: "export-download", description: "Trigger SARIF download" };
    console.log("Step: export-download");

    const downloadPromise = page.waitForEvent("download", { timeout: 30000 });
    await page.getByRole("button", { name: /Download SARIF/i }).first().click({ force: true });
    const download = await downloadPromise;
    const downloadPath = await download.path();

    if (!downloadPath) throw new Error("Download path is null");

    const size = fs.statSync(downloadPath).size;
    if (size === 0) throw new Error("Downloaded SARIF file is empty");
    console.log(`  Downloaded: ${downloadPath} (${size} bytes)`);
    console.log(`  Suggested filename: ${download.suggestedFilename()}`);

    record("export-download", "Trigger SARIF download", true);

    // ── Step 6: Schema + semantic validation ──────────────────────────────
    currentStep = {
      name: "export-validate",
      description: "Validate SARIF against OASIS schema + semantic checks",
    };
    console.log("Step: export-validate");

    if (!schemaPath) throw new Error("Schema path not provided — pass as third argument");

    const validateScript = `${__dirname}/validate-sarif.py`;
    if (!fs.existsSync(validateScript)) {
      throw new Error(`validate-sarif.py not found at ${validateScript}`);
    }

    const cmd = [
      "python3", validateScript,
      "--sarif",  downloadPath,
      "--schema", schemaPath,
    ].join(" ");

    const output = execSync(cmd, { encoding: "utf8", timeout: 30000 });
    console.log(output);

    record("export-validate", "Validate SARIF against OASIS schema + semantic checks", true);

  } catch (err) {
    stepOk    = false;
    stepError = err.message;
    if (!results.some((result) => result.name === currentStep.name)) {
      record(currentStep.name, currentStep.description, false, stepError);
    }
    console.error(`  FAIL: ${err.message}`);
  } finally {
    if (results.length === 0) {
      results.push({ name: "sarif-export", description: "SARIF export end-to-end", ok: stepOk, error: stepError });
    }

    fs.writeFileSync(`${outputDir}/sarif-export-results.json`, JSON.stringify(results, null, 2));

    try { await page.screenshot({ path: `${outputDir}/sarif-export-final.png` }); } catch (_) {}

    await context.close();
    await browser.close();
  }

  const okCount   = results.filter(r => r.ok).length;
  const failCount = results.filter(r => !r.ok).length;
  console.log("");
  console.log("=== SARIF Export Test Summary ===");
  console.log(`  Passed: ${okCount}/${results.length}`);
  console.log(`  Failed: ${failCount}/${results.length}`);

  if (failCount > 0) {
    for (const r of results.filter(r => !r.ok)) {
      console.log(`  - ${r.name}: ${r.error}`);
    }
    process.exit(1);
  }

  console.log("\nSARIF export test: ALL CHECKS PASSED");
})().catch(err => {
  console.error(`Fatal error: ${err.message}`);
  console.error(err.stack);
  process.exit(1);
});
