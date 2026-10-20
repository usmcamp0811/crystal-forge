// Register as browserSpecs.cve; run after the real corrected scan completes,
// while the vulnerable generation is still deployed. cve.py documents the
// same-run input receipts. The existing browser driver owns auth context,
// screenshots, diagnostics and emitted receipts; no network responses are mocked.
// This case signs in through the real UI using facts.admin. Expected finding
// membership comes only from facts.cve, imported from vuln-data/facts.json.
"use strict";

// scan runs immediately after the first real CF completion, without deployment
// or corrected-build prerequisites. current adds the observed Current journey.
// run retains the full vulnerable-Current / built-clean discrimination journey.
async function run(ctx, phase) {
  const { page, expect, facts, receipts } = ctx;
  const f = facts.cve;
  const systemId = receipts["assurance.vulnerable.systemId"];
  const vulnerableScan = receipts["assurance.vulnerable.scanId"];
  const correctedScan = receipts["assurance.corrected.scanId"];
  const vulnerableDrv = receipts["assurance.vulnerable.derivationId"];
  const correctedDrv = receipts["assurance.corrected.derivationId"];
  const deployed = receipts["assurance.vulnerable.deployment"];
  if (phase !== "scan") {
    expect(deployed.systemId).toBe(systemId);
    expect(deployed.derivationId).toBe(vulnerableDrv);
    expect(deployed.outputPath).toBe(receipts["assurance.vulnerable.outputPath"]);
    expect(deployed.generation).toBeGreaterThan(0);
    expect(receipts["assurance.vulnerable.currentScanId"]).toBe(vulnerableScan);
  }
  if (phase === "full") {
    expect(receipts["assurance.corrected.builtNotDeployedProven"]).toBe(true);
    expect(receipts["assurance.corrected.systemId"]).toBe(systemId);
    expect(correctedScan).not.toBe(vulnerableScan);
  }

  await ctx.step("sign in as the same-scenario administrator", async () => {
    await page.goto(ctx.url("/login"));
    await page.getByPlaceholder("Enter your username", { exact: true }).fill(facts.admin.username);
    await page.getByPlaceholder("Enter your password", { exact: true }).fill(facts.admin.password);
    await page.getByRole("button", { name: "Sign In", exact: true }).click();
    await expect(page).toHaveURL(ctx.url("/"));
    expect((await ctx.whoami()).body.roles).toContain("Admin");
    const closeCoach = page.getByRole("button", { name: "Close Crystal Forge Coach", exact: true });
    if (await closeCoach.isVisible()) await closeCoach.click();
  });

  // Observe the actual UI read. A request-only probe could pass while the page
  // rendered another target, so both response identity and visible state matter.
  const inventoryResponse = (target = "current", targetId = null) => page.waitForResponse((response) => {
    const url = new URL(response.url());
    return response.request().method() === "GET" &&
      url.pathname === `/api/v1/systems/${systemId}/cve-inventory-page` &&
      (url.searchParams.get("target") || "current") === target &&
      (targetId === null || url.searchParams.get("target_id") === String(targetId));
  });

  if (phase !== "scan") await ctx.step("System CVEs renders the exact deployed vulnerable scan", async () => {
    const [response] = await Promise.all([
      inventoryResponse(), page.goto(ctx.url(`/systems/${systemId}?tab=cves`)),
    ]);
    expect(response.status()).toBe(200);
    const inventory = await response.json();
    expect(inventory.system_id).toBe(systemId);
    expect(inventory.selection).toEqual({ kind: "current" });
    expect(inventory.current_state).toBe("exact_current_scan");
    expect(inventory.source.scan_id).toBe(vulnerableScan);
    expect(inventory.running_target.derivation_id).toBe(vulnerableDrv);
    expect(inventory.running_target.generation).toBe(deployed.generation);
    expect(inventory.vulnerabilities.map((v) => [v.cve_id, v.canonical_package_name, v.installed_version]))
      .toEqual([[f.cveId, f.package, f.vulnerableVersion]]);
    expect(inventory.vulnerabilities.map((v) => v.cvss_score)).toEqual([f.cvssV3]);
    const detail = page.getByTestId("system-detail");
    const pkg = detail.locator("button[aria-controls^='system-cve-package-']").filter({ hasText: f.package });
    await expect(pkg).toHaveCount(1);
    await pkg.click();
    await expect(detail.getByText(f.cveId, { exact: true })).toBeVisible();
    await expect(detail.getByTestId("system-cve-triage-open")).toBeVisible();
    await ctx.screenshot("cve-current-vulnerable");
  });

  if (phase === "full") await ctx.step("built clean output is an explicit read-only selection", async () => {
    const [response] = await Promise.all([
      inventoryResponse("exact_derivation", correctedDrv),
      page.goto(ctx.url(`/systems/${systemId}?tab=cves&cve_target=derivation:${correctedDrv}&cve_mode=commit`)),
    ]);
    expect(response.status()).toBe(200);
    const inventory = await response.json();
    expect(inventory.system_id).toBe(systemId);
    expect(inventory.selection).toEqual({ kind: "exact_derivation", derivation_id: correctedDrv });
    expect(inventory.authority).toBe("legacy");
    expect(inventory.read_only).toBe(true);
    expect(inventory.current_state ?? null).toBeNull();
    expect(inventory.source.scan_id).toBe(correctedScan);
    expect(inventory.evidence_representation).toBe("schema1_observations");
    expect(inventory.vulnerabilities).toEqual([]);
    const detail = page.getByTestId("system-detail");
    await expect(detail.getByRole("heading", { name: "No findings in the selected scan", exact: true })).toBeVisible();
    await expect(detail.getByTestId("system-cve-triage-open")).toHaveCount(0);
    await ctx.screenshot("cve-built-clean-read-only");
  });

  await ctx.step("Scanning opens the exact persisted scanner execution", async () => {
    await page.goto(ctx.url("/scanning"));
    await page.getByRole("tab", { name: /^Completed/ }).click();
    await page.getByRole("textbox", { name: "Search scans by configuration, flake, revision, scan ID, or derivation ID", exact: true })
      .fill(vulnerableScan);
    const row = page.getByTestId(`scanning-record-${vulnerableScan}`);
    await expect(row).toBeVisible();
    const [response] = await Promise.all([
      page.waitForResponse((r) => new URL(r.url()).pathname === `/api/v1/scanning/scans/${vulnerableScan}`),
      row.getByRole("button", { name: `Open details for scan ${vulnerableScan}`, exact: true }).click(),
    ]);
    expect(response.status()).toBe(200);
    const scan = await response.json();
    expect(scan.scan_id).toBe(vulnerableScan);
    expect(scan.derivation_id).toBe(vulnerableDrv);
    expect(scan.build_job_id).toBe(receipts["assurance.vulnerable.jobId"]);
    expect(scan.status).toBe("completed");
    expect(scan.total_vulnerabilities).toBe(1);
    expect(scan.scanner_version).toBe(f.scannerVersion);
    expect(scan.events.some((event) => event.execution_id === receipts["assurance.vulnerable.executionId"]))
      .toBe(true);
    const dialog = page.getByRole("dialog");
    await dialog.getByRole("tab", { name: "Details", exact: true }).click();
    await expect(dialog.getByText(vulnerableScan, { exact: true })).toBeVisible();
    await expect(dialog.getByText(receipts["assurance.vulnerable.jobId"], { exact: true })).toBeVisible();
    await ctx.screenshot("cve-exact-scan-details");
    ctx.emit("assurance.cve.browserExactScanId", vulnerableScan);
    await dialog.getByRole("button", { name: "Close exact scan detail", exact: true }).click();
    if (phase === "scan") return;
    await row.getByRole("button", { name: `View CVEs for scan ${vulnerableScan}`, exact: true }).click();
    await expect(page).toHaveURL(ctx.url("/cves"));
  });

  if (phase === "scan") return;
  await ctx.step("fleet finding navigation returns to the exact Current generation", async () => {
    const row = page.getByTestId("cve-row").filter({ hasText: f.cveId }).filter({ hasText: f.package });
    await expect(row).toHaveCount(1);
    await row.getByRole("button", { name: `Open fleet inventory for ${f.cveId} ${f.package}`, exact: true }).click();
    const drawer = page.getByTestId("cve-fleet-drawer");
    await expect(drawer).toBeVisible();
    // The text host link and icon host link share an href. Select the text
    // link by non-empty text rather than its presentation class.
    const host = drawer.locator(`a[href^='/systems/${systemId}?']`).filter({ hasText: /\S/ });
    await expect(host).toHaveCount(1);
    const [response] = await Promise.all([inventoryResponse(), host.click()]);
    expect(response.status()).toBe(200);
    const inventory = await response.json();
    expect(inventory.system_id).toBe(systemId);
    expect(inventory.selection).toEqual({ kind: "current" });
    expect(inventory.source.scan_id).toBe(vulnerableScan);
    expect(inventory.running_target.derivation_id).toBe(vulnerableDrv);
    expect(inventory.running_target.generation).toBe(deployed.generation);
    expect(inventory.vulnerabilities.map((v) => [v.cve_id, v.canonical_package_name]))
      .toEqual([[f.cveId, f.package]]);
    await expect(page.getByTestId("system-detail")).toBeVisible();
    await ctx.screenshot("cve-finding-navigation-current");
    ctx.emit("assurance.cve.browserCurrentScanId", vulnerableScan);
    if (phase === "full") ctx.emit("assurance.cve.browserBuiltNotDeployedScanId", correctedScan);
  });
}

exports.scan = (ctx) => run(ctx, "scan");
exports.current = (ctx) => run(ctx, "current");
exports.run = (ctx) => run(ctx, "full");
