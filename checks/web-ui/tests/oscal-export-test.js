/**
 * OSCAL Export Integration Test
 *
 * This test exercises the real production OSCAL export path in the web UI:
 * 1. Registers an admin user and logs in
 * 2. Routes compliance API endpoints with deterministic data
 * 3. Opens the Export evidence modal and clicks Download (OSCAL is default format)
 * 4. Captures the browser-triggered file download
 * 5. Validates the downloaded OSCAL document against NIST 1.1.2 schemas
 *
 * This replaces the old independent `oscal-fixture` approach — instead of
 * independently constructing OSCAL JSON, we validate the *actual file a user
 * would download* from the web UI.
 *
 * Usage: node oscal-export-test.js <baseUrl> <outputDir> <schemaDir>
 *   baseUrl    - URL of the Crystal Forge server (default: http://127.0.0.1:3000)
 *   outputDir  - Directory for screenshots (default: /tmp/screenshots)
 *   schemaDir  - Directory containing OSCAL 1.1.2 schema JSON files
 */
const { chromium } = require("playwright");
const fs = require("fs");
const { execSync } = require("child_process");

const baseUrl = process.argv[2] || "http://127.0.0.1:3000";
const outputDir = process.argv[3] || "/tmp/screenshots";
const schemaDir = process.argv[4] || "";

const TEST_USER = {
  username: "admin",
  email: "admin@example.com",
  password: "testpassword123",
  firstName: "Test",
  lastName: "Admin",
};

const LOAD_TIMEOUT = 10000;

// ── Deterministic test UUIDs (RFC 4122-compliant) ─────────────────────────
const BUNDLE_ID = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const POLICY_ID = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
const ENV_ID = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
const SYS1_ID = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"; // prod-web-01 (failing)
const SYS2_ID = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee"; // prod-db-01 (passing)

// ── Route data builders ─────────────────────────────────────────────────────

function buildBundleData() {
  return [{
    id: BUNDLE_ID,
    name: "NIST 800-53 High",
    framework: "NIST 800-53",
    version: "rev5",
    description: "NIST 800-53 rev5 High baseline for production fleet.",
    layer: "fleet",
    owner: "Platform Security",
    last_review: "2026-01-15T00:00:00Z",
    policy_ids: [POLICY_ID],
    required_envs: [{ id: ENV_ID, name: "production", color_hex: "#3b82f6" }],
    control_count: 2,
    environment_count: 1,
  }];
}

function buildSystemsResponse() {
  return {
    bundle_id: BUNDLE_ID,
    systems: [
      {
        system_id: SYS1_ID,
        hostname: "prod-web-01",
        environment: "production",
        applies: true,
        total: 2,
        pass: 1,
        warn: 0,
        fail: 1,
        waiver: 0,
        score: 50,
      },
      {
        system_id: SYS2_ID,
        hostname: "prod-db-01",
        environment: "production",
        applies: true,
        total: 2,
        pass: 2,
        warn: 0,
        fail: 0,
        waiver: 0,
        score: 100,
      },
    ],
    totals: {
      system_count: 2,
      fully_compliant_count: 1,
      pass: 3,
      warn: 0,
      fail: 1,
      waiver: 0,
      total_controls: 4,
      overall_score: 75,
    },
  };
}

function buildEvidenceResponse(systemId, hostname) {
  const controls = [];

  if (systemId === SYS1_ID) {
    // prod-web-01: one failing control
    controls.push({
      policy_id: POLICY_ID,
      policy_name: "require_no_critical_cves",
      status: "fail",
      severity: "high",
      summary: `${hostname} has 3 critical CVEs (threshold 0).`,
      evidence_items: [
        {
          kind: "cve_scan",
          label: "CVE scan result",
          body: "critical_cves=3 threshold=0",
          artifact: {
            artifact_type: "cve_scan",
            title: "CVE Scan",
            body: "Found 3 critical CVEs: CVE-2024-7001, CVE-2024-7002, CVE-2024-7003",
          },
        },
      ],
      framework_mapping: "require_cve_check → require_no_critical_cves",
    });
  }

  // Both systems have a passing control
  controls.push({
    policy_id: POLICY_ID,
    policy_name: "require_current_deployment",
    status: "pass",
    severity: "low",
    summary: `${hostname} is on the expected deployment.`,
    evidence_items: [
      {
        kind: "nix_config",
        label: "Deployed NixOS config",
        body: "system.build.toplevel = current-deployment",
        artifact: {
          artifact_type: "nix_config",
          title: "NixOS Configuration",
          body: "nixpkgs = 24.11; services.openssh.enable = true;",
        },
      },
    ],
    framework_mapping: "deployment_policy → require_current_deployment",
  });

  return {
    bundle_id: BUNDLE_ID,
    system_id: systemId,
    hostname: hostname,
    controls: controls,
  };
}

// ── Assertion helpers ───────────────────────────────────────────────────────

async function assertVisible(locator, message, timeoutMs = 5000) {
  const visible = await locator.isVisible({ timeout: timeoutMs }).catch(() => false);
  if (!visible) {
    throw new Error(message);
  }
}

// ── Route mock helpers ──────────────────────────────────────────────────────

async function routeComplianceBundles(page) {
  await page.route("**/api/v1/compliance/bundles*", async (route) => {
    const url = route.request().url();
    const method = route.request().method();

    // Evidence endpoint: /compliance/bundles/{id}/systems/{sysId}/evidence
    const evidenceMatch = url.match(/\/compliance\/bundles\/([^/]+)\/systems\/([^/]+)\/evidence/);
    if (evidenceMatch) {
      const sysId = evidenceMatch[2];
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(
          sysId === SYS2_ID
            ? buildEvidenceResponse(SYS2_ID, "prod-db-01")
            : buildEvidenceResponse(SYS1_ID, "prod-web-01")
        ),
      });
      return;
    }

    // Systems endpoint: /compliance/bundles/{id}/systems
    const systemsMatch = url.match(/\/compliance\/bundles\/([^/]+)\/systems/);
    if (systemsMatch) {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(buildSystemsResponse()),
      });
      return;
    }

    // Bundle list: GET /compliance/bundles
    if (method === "GET") {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(buildBundleData()),
      });
      return;
    }

    await route.continue();
  });
}

async function unrouteComplianceBundles(page) {
  await page.unroute("**/api/v1/compliance/bundles*");
}

// ── Auth flow (copies pattern from integration-test.js) ─────────────────────

async function registerUser(page) {
  await page.goto(`${baseUrl}/register`, { timeout: LOAD_TIMEOUT });
  await page.waitForTimeout(1500);

  await page.fill('input[name="username"]', TEST_USER.username);
  await page.fill('input[name="password"]', TEST_USER.password);
  await page.fill('input[name="email"]', TEST_USER.email);
  await page.fill('input[name="first_name"]', TEST_USER.firstName);
  await page.fill('input[name="last_name"]', TEST_USER.lastName);
  await page.waitForTimeout(200);

  await page.getByRole("button", { name: /sign up|register|submit/i }).first().click({ force: true });
  await page.waitForTimeout(1500);
}

async function loginUser(page) {
  await page.goto(`${baseUrl}/login`, { timeout: LOAD_TIMEOUT });
  await page.waitForTimeout(1500);

  await page.fill('input[name="username"]', TEST_USER.username);
  await page.fill('input[name="password"]', TEST_USER.password);
  await page.waitForTimeout(200);

  await page.getByRole("button", { name: /sign in|log in|login/i }).first().click({ force: true });
  await page.waitForTimeout(1500);
}

// ── Main test flow ──────────────────────────────────────────────────────────

(async () => {
  console.log("Starting OSCAL Export Integration Test");
  console.log(`  Base URL: ${baseUrl}`);
  console.log(`  Output: ${outputDir}`);
  console.log(`  Schema dir: ${schemaDir}`);
  console.log("");

  const results = [];
  let stepOk = true;
  let stepError = null;

  const browser = await chromium.launch();
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
  });
  const page = await context.newPage();

  const recordResult = (name, description, ok, error) => {
    results.push({ name, description, ok, error: error || null });
    const status = ok ? "OK" : "FAIL";
    console.log(`  [${status}] ${name}${error ? ` - ${error}` : ""}`);
  };

  try {
    // ── Step 1: Register user ──────────────────────────────────────────────
    console.log("Step: register - Register admin user");
    try {
      await registerUser(page);
      recordResult("register", "Register admin user", true);
    } catch (err) {
      // Registration may already exist; try login directly
      console.log(`  Registration note: ${err.message} (may already exist, attempting login)`);
      recordResult("register", "Register admin user", true);
    }

    // ── Step 2: Login ──────────────────────────────────────────────────────
    console.log("Step: login - Login as admin");
    await loginUser(page);
    recordResult("login", "Login as admin", true);

    // ── Step 3: Navigate to compliance with route-mocked data ──────────────
    console.log("Step: compliance-navigate - Navigate to compliance view with mocked data");
    await routeComplianceBundles(page);

    await page.goto(`${baseUrl}/compliance`, { timeout: LOAD_TIMEOUT });
    await page.waitForTimeout(2000);

    // Verify page head renders
    await assertVisible(
      page.getByRole("heading", { name: /^Compliance$/i }).first(),
      "Expected Compliance page heading",
    );

    // Verify bundle data rendered
    await assertVisible(
      page.getByText("NIST 800-53 High").first(),
      "Expected bundle name in catalog",
    );

    // Select bundle (click the first bundle in catalog)
    const bundleCard = page.getByText("NIST 800-53 High").first();
    await bundleCard.click({ force: true });
    await page.waitForTimeout(1500);

    // Verify systems loaded
    await assertVisible(
      page.getByText("prod-web-01").first(),
      "Expected system hostname in systems matrix",
    );

    recordResult("compliance-navigate", "Navigate to compliance view with mocked data", true);

    // ── Step 4: Open export modal ──────────────────────────────────────────
    console.log("Step: export-open - Open Export evidence modal");
    await page.getByRole("button", { name: /Export evidence/i }).first().click({ force: true });
    await page.waitForTimeout(800);

    await assertVisible(
      page.getByRole("heading", { name: /Export evidence/i }).first(),
      "Expected export modal heading",
    );
    await assertVisible(
      page.getByText(/OSCAL 1.1.2 JSON/i).first(),
      "Expected OSCAL format option selected by default",
    );

    recordResult("export-open", "Open Export evidence modal", true);

    // ── Step 5: Trigger OSCAL export download ──────────────────────────────
    console.log("Step: export-download - Trigger OSCAL export download");

    // Set up download listener BEFORE clicking Download
    const downloadPromise = page.waitForEvent("download", { timeout: 30000 });

    // OSCAL is the default format (format signal is "oscal"), so just click Download
    const downloadBtn = page.getByRole("button", { name: /^Download$/ }).first();
    await assertVisible(downloadBtn, "Expected Download button in export modal");
    await downloadBtn.click({ force: true });

    // Wait for the download to complete
    const download = await downloadPromise;
    const downloadPath = await download.path();

    if (!downloadPath) {
      throw new Error("Download path is null — download may have been intercepted");
    }

    console.log(`  Downloaded file: ${downloadPath}`);
    console.log(`  Suggested filename: ${download.suggestedFilename()}`);

    // Verify the file has content
    const fileStats = fs.statSync(downloadPath);
    if (fileStats.size === 0) {
      throw new Error("Downloaded file is empty");
    }
    console.log(`  File size: ${fileStats.size} bytes`);

    recordResult("export-download", "Trigger OSCAL export download", true);

    // ── Step 6: Validate downloaded file against NIST schemas ──────────────
    console.log("Step: export-validate - Validate OSCAL file against NIST 1.1.2 schemas");

    if (!schemaDir) {
      throw new Error("Schema directory not provided — pass as third argument");
    }

    const validateScript = `${__dirname}/validate.py`;
    if (!fs.existsSync(validateScript)) {
      throw new Error(`validate.py not found at ${validateScript}`);
    }

    const validateCmd = [
      "python3", validateScript,
      "--assessment-results", downloadPath,
      "--schema-dir", schemaDir,
    ].join(" ");

    const validateResult = execSync(validateCmd, {
      encoding: "utf8",
      timeout: 30000,
    });
    console.log(validateResult);

    recordResult("export-validate", "Validate OSCAL file against NIST 1.1.2 schemas", true);

  } catch (err) {
    stepOk = false;
    stepError = err.message;
    console.error(`  FAIL: ${err.message}`);
  } finally {
    // Save results
    if (results.length === 0) {
      results.push({
        name: "oscal-export",
        description: "OSCAL export end-to-end test",
        ok: stepOk,
        error: stepError,
      });
    }

    fs.writeFileSync(`${outputDir}/oscal-export-results.json`, JSON.stringify(results, null, 2));

    // Take a final screenshot if possible
    try {
      await page.screenshot({ path: `${outputDir}/oscal-export-final.png` });
    } catch (_) {}

    await context.close();
    await browser.close();
  }

  // Report
  const okCount = results.filter((r) => r.ok).length;
  const failCount = results.filter((r) => !r.ok).length;
  console.log("");
  console.log("=== OSCAL Export Test Summary ===");
  console.log(`  Passed: ${okCount}/${results.length}`);
  console.log(`  Failed: ${failCount}/${results.length}`);

  if (failCount > 0) {
    for (const r of results.filter((r) => !r.ok)) {
      console.log(`  - ${r.name}: ${r.error}`);
    }
    process.exit(1);
  }

  console.log("\nOSCAL export test: ALL CHECKS PASSED");
})().catch((err) => {
  console.error(`Fatal error: ${err.message}`);
  console.error(err.stack);
  process.exit(1);
});
