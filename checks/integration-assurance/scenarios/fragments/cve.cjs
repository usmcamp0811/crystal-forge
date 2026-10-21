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
    // The first package opens automatically. Do not toggle it closed before
    // checking the finding; assert that designed default explicitly.
    await expect(pkg).toHaveAttribute("aria-expanded", "true");
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
    await completedFindings(ctx, dialog, scan, "vulnerable");
    await themedEvidence(ctx, "cve-exact-scan-details", dialog, async () => {
      const reopened = await openScanDetails(ctx, row, vulnerableScan);
      expect(reopened).toEqual(scan);
      await completedFindings(ctx, dialog, reopened, "vulnerable");
    });
    ctx.emit("assurance.cve.browserExactScanId", vulnerableScan);
    await dialog.getByRole("button", { name: "Close exact scan detail", exact: true }).click();
    if (phase === "scan") return;
    await row.getByRole("button", { name: `View CVEs for scan ${vulnerableScan}`, exact: true }).click();
    await expect(page).toHaveURL(ctx.url("/cves"));
  });

  if (phase === "scan") return;
  await ctx.step("fleet finding navigation returns to the exact Current generation", async () => {
    // The designed ungrouped view exposes the exact pair's inventory action;
    // grouped rows instead make the whole row clickable.
    await page.getByRole("button", {name:"None", exact:true}).click();
    const row = page.getByTestId("cve-row").filter({ hasText: f.cveId }).filter({
      has: page.getByRole("button", { name: `Open fleet inventory for ${f.cveId} ${f.package}`, exact: true }),
    });
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

// Additional lifecycle phases deliberately do not call run(): missing precedes
// build admission; historical/clean follow corrected activation. Each phase
// consumes only same-scenario receipts already proven by the Python producer.
async function signIn(ctx, account = ctx.facts.admin, role = "Admin") {
  const { page, expect } = ctx;
  await page.goto(ctx.url("/login"));
  // Admin-created accounts receive a server-generated username. Their authored
  // email is the stable supported login identifier, also used by the API probe.
  await page.getByPlaceholder("Enter your username", { exact: true }).fill(account.email);
  await page.getByPlaceholder("Enter your password", { exact: true }).fill(account.password);
  await page.getByRole("button", { name: "Sign In", exact: true }).click();
  await expect(page).toHaveURL(ctx.url("/"));
  expect((await ctx.whoami()).body.roles).toContain(role);
  const coach = page.getByRole("button", { name: "Close Crystal Forge Coach", exact: true });
  if (await coach.isVisible()) await coach.click();
}

function selectedRead(ctx, variant, current) {
  const key = `assurance.${variant}.`;
  const system = ctx.receipts[key + "systemId"];
  const drv = ctx.receipts[key + "derivationId"];
  const target = current ? "current" : "exact_derivation";
  const matches = (url) => url.pathname === `/api/v1/systems/${system}/cve-inventory-page` &&
    (url.searchParams.get("target") || "current") === target &&
    (current || url.searchParams.get("target_id") === String(drv));
  return {
    key, system, drv, matches,
    path: `/systems/${system}?tab=cves` + (current ? "" : `&cve_target=derivation:${drv}&cve_mode=commit`),
    response: () => ctx.page.waitForResponse((r) => r.request().method() === "GET" && matches(new URL(r.url()))),
  };
}

async function openInventory(ctx, variant, current, navigate) {
  const selection = selectedRead(ctx, variant, current);
  const [response] = await Promise.all([
    selection.response(), navigate ? navigate() : ctx.page.goto(ctx.url(selection.path)),
  ]);
  ctx.expect(response.status()).toBe(200);
  const inventory = await response.json();
  ctx.expect(inventory.system_id).toBe(selection.system);
  ctx.expect(inventory.selection).toEqual(current ? { kind: "current" } :
    { kind: "exact_derivation", derivation_id: selection.drv });
  if (current) {
    const deployment = ctx.receipts[selection.key + "deployment"];
    ctx.expect(ctx.receipts[selection.key + "currentScanId"]).toBe(ctx.receipts[selection.key + "scanId"]);
    ctx.expect(inventory.running_target.derivation_id).toBe(selection.drv);
    ctx.expect(inventory.running_target.generation).toBe(deployment.generation);
  }
  await ctx.expect(ctx.page.getByTestId("system-detail")).toBeVisible();
  return inventory;
}

function completedInventory(ctx, inventory, variant, current, scanId = ctx.receipts[`assurance.${variant}.scanId`]) {
  const { expect, facts } = ctx;
  expect(inventory.source.scan_id).toBe(scanId);
  expect(inventory.evidence_representation).toBe("schema1_observations");
  expect(inventory.authority).toBe(current ? "exact" : "legacy");
  expect(inventory.read_only).toBe(!current);
  expect(inventory.current_state ?? null).toBe(current ? "exact_current_scan" : null);
  const f = facts.cve;
  expect(inventory.vulnerabilities.map((v) => [v.cve_id, v.canonical_package_name, v.installed_version, v.cvss_score]))
    .toEqual(variant === "corrected" ? [] : [[f.cveId, f.package, f.vulnerableVersion, f.cvssV3]]);
  expect(inventory.metadata.total_findings).toBe(variant === "corrected" ? 0 : 1);
}

async function expandFixture(ctx) {
  const detail = ctx.page.getByTestId("system-detail");
  const pkg = detail.locator("button[aria-controls^='system-cve-package-']").filter({ hasText: ctx.facts.cve.package });
  await ctx.expect(pkg).toHaveCount(1);
  await ctx.expect(pkg).toHaveAttribute("aria-expanded", "true");
  await ctx.expect(detail.getByText(ctx.facts.cve.cveId, { exact: true })).toBeVisible();
  return detail;
}

// These are the authored scanner facts, not counts inferred from the API under
// test. Only package cardinality is a binding check against the exact scan API.
async function completedFindings(ctx, dialog, scan, variant) {
  const n = variant === "corrected" ? 0 : 1;
  ctx.expect(scan.status).toBe("completed");
  ctx.expect([scan.total_vulnerabilities, scan.critical_count, scan.high_count, scan.medium_count, scan.low_count])
    .toEqual([n, n, 0, 0, 0]);
  ctx.expect(Number.isInteger(scan.total_packages)).toBe(true);
  ctx.expect(scan.total_packages).toBeGreaterThanOrEqual(0);
  const details = dialog.getByRole("tabpanel", { name: "Details", exact: true });
  await ctx.expect(details.getByRole("heading", { name: "Findings", exact: true })).toBeVisible();
  await ctx.expect(details.getByText(`${n} vulnerabilities across ${scan.total_packages} packages`, { exact: true })).toBeVisible();
  for (const fact of [`${n} critical`, "0 high", "0 medium", "0 low"]) {
    await ctx.expect(details.getByText(fact, { exact: true })).toBeVisible();
  }
}

async function openScanDetails(ctx, row, id) {
  const [response] = await Promise.all([
    ctx.page.waitForResponse((r) => r.request().method() === "GET" && new URL(r.url()).pathname === `/api/v1/scanning/scans/${id}`),
    row.getByRole("button", { name: `Open details for scan ${id}`, exact: true }).click(),
  ]);
  ctx.expect(response.status()).toBe(200);
  const scan = await response.json();
  ctx.expect(scan.scan_id).toBe(id);
  await ctx.page.getByRole("dialog").getByRole("tab", { name: "Details", exact: true }).click();
  return scan;
}

// Use the real theme control and restore the original preference. Dialogs must
// close before the topbar can be used; reopen the same exact read afterward.
// Keep the pinned viewport and capture the whole affected region, not a chip.
async function themedEvidence(ctx, name, region, reopen) {
  const { page, expect } = ctx;
  const root = page.locator("html");
  await expect(root).toHaveAttribute("data-theme", /^(dark|light)$/);
  const original = await root.getAttribute("data-theme");
  const viewport = page.viewportSize();
  const closeDialog = async () => {
    const dialog = page.getByRole("dialog");
    if (await dialog.count()) await dialog.getByRole("button", { name: "Close exact scan detail", exact: true }).click();
  };
  const setTheme = async (theme) => {
    if (await root.getAttribute("data-theme") !== theme) {
      await closeDialog();
      await page.getByRole("button", { name: "Toggle theme", exact: true }).click();
      await expect(root).toHaveAttribute("data-theme", theme);
      await reopen();
    }
  };
  try {
    for (const theme of ["dark", "light"]) {
      await setTheme(theme);
      expect(page.viewportSize()).toEqual(viewport);
      await region.scrollIntoViewIfNeeded();
      await expect(region).toBeInViewport({ ratio: 1 });
      const capture = theme === "dark" ? name : `${name}-light`;
      await ctx.screenshot(capture);
      // A visible outer dialog does not prove that its scrollable metadata is
      // captured. Retain the end of that region separately at the same viewport.
      const details = page.getByRole("tabpanel", { name: "Details", exact: true });
      if (await details.count()) {
        const lastField = details.locator("dd").last();
        await lastField.scrollIntoViewIfNeeded();
        await ctx.screenshot(`${capture}-metadata-end`);
        ctx.emit(`assurance.cve.visual.${capture}`, {
          actor: ctx.facts.admin.email, url: page.url(), viewport, theme,
          metadataEndFullyVisible: await lastField.evaluate((node) => {
            const box = node.getBoundingClientRect();
            if (box.top < 0 || box.bottom > window.innerHeight) return false;
            for (let parent = node.parentElement; parent; parent = parent.parentElement) {
              if (/(auto|scroll|hidden|clip)/.test(getComputedStyle(parent).overflowY)) {
                const bounds = parent.getBoundingClientRect();
                if (box.top < bounds.top || box.bottom > bounds.bottom) return false;
              }
            }
            return true;
          }),
        });
      } else {
        const target = page.getByTestId("system-detail").getByText("Scan target", { exact: true });
        await target.scrollIntoViewIfNeeded();
        await expect(target).toBeInViewport();
        await ctx.screenshot(`${capture}-selection`);
        ctx.emit(`assurance.cve.visual.${capture}`, {
          actor: ctx.facts.admin.email, url: page.url(), viewport, theme,
        });
      }
    }
  } finally {
    await setTheme(original);
  }
}

async function historicalReadOnly(ctx) {
  const detail = await expandFixture(ctx);
  const row = detail.getByRole("row").filter({ has: ctx.page.getByRole("cell", { name: ctx.facts.cve.cveId, exact: true }) });
  const cell = row.getByRole("cell", { name: "read-only", exact: true });
  await ctx.expect(cell).toBeVisible();
  const chip = cell.getByText("read-only", { exact: true });
  await ctx.expect(chip).toHaveText("read-only");
  await ctx.expect(chip).toHaveClass(/(?:^|\s)chip(?:\s|$)/);
  await ctx.expect(chip).toHaveClass(/(?:^|\s)chip-unknown(?:\s|$)/);
  await ctx.expect(chip).toHaveAttribute("title", "This selected scan cannot authorize triage");
  await ctx.expect(detail.getByTestId("system-cve-triage-open")).toHaveCount(0);
  await ctx.expect(row.getByRole("button", { name: /triage/i })).toHaveCount(0);
  await ctx.expect(detail.getByRole("checkbox")).toHaveCount(0);
  await ctx.expect(detail.getByRole("button", { name: /triage/i })).toHaveCount(0);
  return detail.getByRole("table").filter({ has: ctx.page.getByRole("cell", { name: ctx.facts.cve.cveId, exact: true }) });
}

exports.missing = async (ctx) => {
  await signIn(ctx);
  await ctx.step("never-scanned exact target is unavailable, never clean", async () => {
    ctx.expect(ctx.receipts["assurance.missing.missingProven"]).toBe(true);
    const inventory = await openInventory(ctx, "missing", false);
    ctx.expect(inventory.authority).toBe("no_scan");
    ctx.expect(inventory.source).toBeNull();
    ctx.expect(inventory.attempt ?? null).toBeNull();
    ctx.expect(inventory.evidence_representation).toBeNull();
    ctx.expect(inventory.read_only).toBe(true);
    ctx.expect(inventory.vulnerabilities).toEqual([]);
    const detail = ctx.page.getByTestId("system-detail");
    await ctx.expect(detail.getByRole("heading", { name: "No completed CVE scan for this target", exact: true })).toBeVisible();
    await ctx.expect(detail.getByRole("heading", { name: "No findings in the selected scan", exact: true })).toHaveCount(0);
    await ctx.expect(detail.getByTestId("system-cve-triage-open")).toHaveCount(0);
    await ctx.screenshot("cve-never-scanned-not-clean");
    ctx.emit("assurance.cve.browserMissingDerivationId", ctx.receipts["assurance.missing.derivationId"]);
  });
};

exports.historical = async (ctx) => {
  await signIn(ctx);
  await ctx.step("formerly Current vulnerable evidence remains exact and read-only after activation", async () => {
    ctx.expect(ctx.receipts["assurance.vulnerable.historicalProven"]).toBe(true);
    completedInventory(ctx, await openInventory(ctx, "corrected", true), "corrected", true);
    const correctedReadOnly = async () => {
      const detail = ctx.page.getByTestId("system-detail");
      await ctx.expect(detail.getByRole("heading", { name: "No findings in the selected scan", exact: true })).toBeVisible();
      await ctx.expect(detail.getByTestId("system-cve-triage-open")).toHaveCount(0);
      await ctx.expect(detail.getByRole("button", { name: /triage/i })).toHaveCount(0);
      await ctx.expect(detail.getByRole("checkbox")).toHaveCount(0);
    };
    await correctedReadOnly();
    completedInventory(ctx, await openInventory(ctx, "corrected", true, () => ctx.page.reload()), "corrected", true);
    await correctedReadOnly();
    completedInventory(ctx, await openInventory(ctx, "vulnerable", false), "vulnerable", false);
    // SystemDetail.jsx:1825 owns the exact read-only cell/token/title contract.
    // Repeat the prohibition after reload and after leaving and returning.
    await historicalReadOnly(ctx);
    completedInventory(ctx, await openInventory(ctx, "vulnerable", false, () => ctx.page.reload()), "vulnerable", false);
    await historicalReadOnly(ctx);
    completedInventory(ctx, await openInventory(ctx, "corrected", true), "corrected", true);
    await correctedReadOnly();
    completedInventory(ctx, await openInventory(ctx, "vulnerable", false), "vulnerable", false);
    const table = await historicalReadOnly(ctx);
    await themedEvidence(ctx, "cve-historical-vulnerable-read-only", table, async () => {
      await historicalReadOnly(ctx);
    });
    ctx.emit("assurance.cve.browserHistoricalScanId", ctx.receipts["assurance.vulnerable.scanId"]);
  });
};

exports.clean = async (ctx) => {
  await signIn(ctx);
  await ctx.step("activated corrected generation supplies an actual completed empty Current source", async () => {
    completedInventory(ctx, await openInventory(ctx, "corrected", true), "corrected", true);
    const detail = ctx.page.getByTestId("system-detail");
    await ctx.expect(detail.getByRole("heading", { name: "No findings in the selected scan", exact: true })).toBeVisible();
    await ctx.expect(detail.getByText(/This scan reported no eligible findings/)).toBeVisible();
    await ctx.expect(detail.getByRole("heading", { name: "No completed CVE scan for this target", exact: true })).toHaveCount(0);
    await ctx.expect(detail.getByTestId("system-cve-triage-open")).toHaveCount(0);
    await ctx.screenshot("cve-deployed-clean-completed");
    ctx.emit("assurance.cve.browserCleanScanId", ctx.receipts["assurance.corrected.scanId"]);
  });
  await ctx.step("Scanning completed empty evidence retains the corrected exact identity", async () => {
    const id = ctx.receipts["assurance.corrected.scanId"];
    await ctx.page.goto(ctx.url("/scanning"));
    await ctx.page.getByRole("tab", { name: /^Completed/ }).click();
    await ctx.page.getByRole("textbox", { name: "Search scans by configuration, flake, revision, scan ID, or derivation ID", exact: true }).fill(id);
    const row = ctx.page.getByTestId(`scanning-record-${id}`);
    await ctx.expect(row).toBeVisible();
    const [response] = await Promise.all([
      ctx.page.waitForResponse((r) => r.request().method() === "GET" && new URL(r.url()).pathname === `/api/v1/scanning/scans/${id}`),
      row.getByRole("button", { name: `Open details for scan ${id}`, exact: true }).click(),
    ]);
    ctx.expect(response.status()).toBe(200);
    const scan = await response.json();
    ctx.expect(scan.scan_id).toBe(id);
    ctx.expect(scan.derivation_id).toBe(ctx.receipts["assurance.corrected.derivationId"]);
    ctx.expect(scan.build_job_id).toBe(ctx.receipts["assurance.corrected.jobId"]);
    ctx.expect(scan.status).toBe("completed");
    ctx.expect(scan.total_vulnerabilities).toBe(0);
    ctx.expect(scan.failure).toBeNull();
    ctx.expect(scan.events.some((e) => e.execution_id === ctx.receipts["assurance.corrected.executionId"])).toBe(true);
    const dialog = ctx.page.getByRole("dialog");
    await dialog.getByRole("tab", { name: "Details", exact: true }).click();
    await ctx.expect(dialog.getByText(id, { exact: true })).toBeVisible();
    await completedFindings(ctx, dialog, scan, "corrected");
    await themedEvidence(ctx, "cve-clean-exact-scan-details", dialog, async () => {
      const reopened = await openScanDetails(ctx, row, id);
      ctx.expect(reopened).toEqual(scan);
      await completedFindings(ctx, dialog, reopened, "corrected");
    });
  });
};

async function failedRecord(ctx) {
  const { page, expect, receipts } = ctx;
  const id = receipts["assurance.vulnerable.failedScanId"];
  expect(typeof id).toBe("string");
  await page.goto(ctx.url("/scanning"));
  await page.getByRole("tab", { name: /^Completed/ }).click();
  await page.getByRole("combobox", { name: "Filter by scan status", exact: true }).selectOption("failed");
  await page.getByRole("textbox", { name: "Search scans by configuration, flake, revision, scan ID, or derivation ID", exact: true }).fill(id);
  const row = page.getByTestId(`scanning-record-${id}`);
  await expect(row).toBeVisible();
  await expect(row.getByText("Scan failed · view log", { exact: true })).toBeVisible();
  return { id, row };
}

exports.failed = async (ctx) => {
  const { page, expect, receipts } = ctx;
  await signIn(ctx);
  await ctx.step("failed scanner attempt preserves vulnerable completed evidence", async () => {
    const inventory = await openInventory(ctx, "vulnerable", false);
    completedInventory(ctx, inventory, "vulnerable", false);
    expect(inventory.attempt.scan_id).toBe(receipts["assurance.vulnerable.failedScanId"]);
    expect(inventory.attempt.status).toBe("failed");
    await expandFixture(ctx);
    await expect(page.getByText("A newer scan failed. Showing the last completed scan for this target.", { exact: true })).toBeVisible();
    await ctx.screenshot("cve-failed-attempt-preserves-source");
  });
  await ctx.step("Scanning failure opens its exact persisted execution, never a clean result", async () => {
    const { id, row } = await failedRecord(ctx);
    const [response] = await Promise.all([
      page.waitForResponse((r) => r.request().method() === "GET" && new URL(r.url()).pathname === `/api/v1/scanning/scans/${id}`),
      row.getByRole("button", { name: `Open details for scan ${id}`, exact: true }).click(),
    ]);
    expect(response.status()).toBe(200);
    const scan = await response.json();
    expect(scan.scan_id).toBe(id);
    expect(scan.derivation_id).toBe(receipts["assurance.vulnerable.derivationId"]);
    expect(scan.build_job_id).toBe(receipts["assurance.terminalFault.jobId"]);
    expect(scan.status).toBe("failed");
    expect(scan.failure).toBe("vulnix returned an invalid CVSS score");
    expect(scan.events.some((e) => e.execution_id === receipts["assurance.vulnerable.failedExecutionId"] && e.event_type === "attempt_failed")).toBe(true);
    const dialog = page.getByRole("dialog");
    const failedDetails = async () => {
      await expect(dialog.getByRole("button", { name: "Retry scan", exact: true })).toBeVisible();
      await expect(dialog.getByRole("button", { name: "Retry scan", exact: true })).toBeEnabled();
      await expect(dialog.getByRole("alert")).toContainText(receipts["assurance.vulnerable.failedError"]);
      await dialog.getByRole("tab", { name: "Log", exact: true }).click();
      const event = scan.events.find((e) => e.execution_id === receipts["assurance.vulnerable.failedExecutionId"] && e.event_type === "attempt_failed");
      const diagnostic = dialog.getByTestId(`scan-event-${event.id}`);
      await expect(diagnostic).toContainText(`execution ${event.execution_id}`);
      await expect(diagnostic).toContainText("attempt_failed");
      await expect(diagnostic).toContainText(scan.failure);
      await dialog.getByRole("tab", { name: "Details", exact: true }).click();
      const details = dialog.getByRole("tabpanel", { name: "Details", exact: true });
      await expect(details.getByText(id, { exact: true })).toBeVisible();
      await expect(details.locator("code").filter({ hasText: new RegExp(`^${scan.derivation_id}$`) })).toBeVisible();
      await expect(details.getByText(scan.build_job_id, { exact: true })).toBeVisible();
      await expect(details.getByText("Failed", { exact: true })).toBeVisible();
      await expect(details.getByText(scan.failure, { exact: true })).toBeVisible();
      // ScanningView.jsx:480 omits Findings when no completed set exists.
      // Protect the whole section, including numeric and severity summaries.
      await expect(dialog.getByRole("heading", { name: "Findings", exact: true })).toHaveCount(0);
      await expect(dialog.getByText(/\d+\s+vulnerabilities across\s+\d+\s+packages/)).toHaveCount(0);
      await expect(dialog.getByText(/^\d+\s+(critical|high|medium|low)$/)).toHaveCount(0);
    };
    await failedDetails();
    await themedEvidence(ctx, "cve-terminal-failure-exact-execution", dialog, async () => {
      expect(await openScanDetails(ctx, row, id)).toEqual(scan);
      await failedDetails();
    });
    ctx.emit("assurance.cve.browserFailedScanId", id);
  });
};

exports.retry = async (ctx) => {
  await signIn(ctx);
  // The existing lifecycle invokes this adapter after real transient recovery,
  // before manual retry admission. Original source receipts remain immutable.
  await ctx.step("recovered execution supplies a new completed historical source while corrected stays Current", async () => {
    const id = ctx.receipts["assurance.recovery.scanId"];
    ctx.expect(typeof id).toBe("string");
    ctx.expect(id).not.toBe(ctx.receipts["assurance.vulnerable.scanId"]);
    ctx.expect(id).not.toBe(ctx.receipts["assurance.vulnerable.failedScanId"]);
    ctx.expect(ctx.receipts["assurance.recovery.executionId"]).not.toBe(ctx.receipts["assurance.vulnerable.faultExecutionId"]);
    completedInventory(ctx, await openInventory(ctx, "corrected", true), "corrected", true);
    const inventory = await openInventory(ctx, "vulnerable", false);
    completedInventory(ctx, inventory, "vulnerable", false, id);
    await historicalReadOnly(ctx);
    await ctx.screenshot("cve-recovery-completed-historical-source");
    await ctx.page.goto(ctx.url("/scanning"));
    await ctx.page.getByRole("tab", { name: /^Completed/ }).click();
    await ctx.page.getByRole("textbox", { name: "Search scans by configuration, flake, revision, scan ID, or derivation ID", exact: true }).fill(id);
    const row = ctx.page.getByTestId(`scanning-record-${id}`);
    const scan = await openScanDetails(ctx, row, id);
    ctx.expect(scan.derivation_id).toBe(ctx.receipts["assurance.vulnerable.derivationId"]);
    ctx.expect(scan.build_job_id).toBe(ctx.receipts["assurance.transientFault.jobId"]);
    // The detail API retains the prior attempt's error metadata after recovery.
    // Completed status and the exact completion event establish the new result;
    // diagnostic retention must not be mistaken for a terminal failed lifecycle.
    const completed = scan.events.find((e) => e.execution_id === ctx.receipts["assurance.recovery.executionId"] && e.event_type === "attempt_completed");
    ctx.expect(completed).toBeDefined();
    const requeued = scan.events.find((e) => e.execution_id === ctx.receipts["assurance.vulnerable.faultExecutionId"] && e.event_type === "attempt_requeued");
    ctx.expect(requeued).toBeDefined();
    ctx.expect(requeued.message).toContain("vulnix process failed");
    const dialog = ctx.page.getByRole("dialog");
    await dialog.getByRole("tab", { name: "Log", exact: true }).click();
    const event = completed;
    await ctx.expect(dialog.getByTestId(`scan-event-${event.id}`)).toContainText(`execution ${event.execution_id}`);
    await dialog.getByRole("tab", { name: "Details", exact: true }).click();
    const details = dialog.getByRole("tabpanel", { name: "Details", exact: true });
    await ctx.expect(details.getByText(id, { exact: true })).toBeVisible();
    await ctx.expect(details.locator("code").filter({ hasText: new RegExp(`^${scan.derivation_id}$`) })).toBeVisible();
    await ctx.expect(details.getByText(scan.build_job_id, { exact: true })).toBeVisible();
    await completedFindings(ctx, dialog, scan, "vulnerable");
    await ctx.screenshot("cve-recovery-exact-scan-details");
    await dialog.getByRole("button", { name: "Close exact scan detail", exact: true }).click();
    ctx.emit("assurance.cve.browserRecoveryScanId", id);
  });
  await ctx.step("Admin retry admits a new exact scan without rewriting failed evidence", async () => {
    const { id, row } = await failedRecord(ctx);
    const drv = ctx.receipts["assurance.vulnerable.derivationId"];
    const [response] = await Promise.all([
      ctx.page.waitForResponse((r) => r.request().method() === "POST" && new URL(r.url()).pathname === `/api/v1/cves/rescan/${drv}`),
      row.getByRole("button", { name: `Retry exact scan ${id}`, exact: true }).click(),
    ]);
    ctx.expect(response.status()).toBe(202);
    const queued = await response.json();
    ctx.expect(queued.derivation_id).toBe(drv);
    ctx.expect(queued.enqueued).toBe(true);
    ctx.expect(queued.scan_id).not.toBe(id);
    ctx.expect(queued.scan_id).not.toBe(ctx.receipts["assurance.vulnerable.scanId"]);
    ctx.emit("assurance.cve.browserRetryScanId", queued.scan_id);
    await ctx.screenshot("cve-admin-exact-retry");
  });
};

exports.readError = async (ctx) => {
  await signIn(ctx);
  await ctx.step("inventory transport failure is an error; Retry reads the same real target", async () => {
    const selection = selectedRead(ctx, "vulnerable", false);
    // Abort only the real inventory transport. No success body, scan result or
    // finding is supplied. Remove the route before the user's Retry interaction.
    const routePattern = (url) => selection.matches(url);
    let releaseRead;
    const heldRead = new Promise(resolve => { releaseRead = resolve; });
    const abort = async (route) => { await heldRead; await route.abort("failed"); };
    const writes = [];
    const observe = (request) => {
      if (request.method() !== "GET" && new URL(request.url()).pathname.startsWith("/api/v1/")) writes.push(request.url());
    };
    ctx.page.on("request", observe);
    try {
      await ctx.page.route(routePattern, abort);
      await ctx.page.goto(ctx.url(selection.path));
      await ctx.expect(ctx.page.getByTestId("system-cves-loading")).toBeVisible();
      await ctx.expect(ctx.page.getByRole("heading", { name: "No findings in the selected scan", exact: true })).toHaveCount(0);
      await ctx.screenshot("cve-inventory-loading-not-clean");
      releaseRead();
      const error = ctx.page.getByTestId("system-cves-error");
      await ctx.expect(error.getByRole("heading", { name: "Unable to load CVE inventory", exact: true })).toBeVisible();
      await ctx.expect(ctx.page.getByRole("heading", { name: "No findings in the selected scan", exact: true })).toHaveCount(0);
      await ctx.screenshot("cve-inventory-read-error");
      await ctx.page.unroute(routePattern, abort);
      const inventory = await openInventory(ctx, "vulnerable", false,
        () => error.getByRole("button", { name: "Retry inventory read", exact: true }).click());
      completedInventory(ctx, inventory, "vulnerable", false);
      await expandFixture(ctx);
      ctx.expect(writes).toEqual([]);
      await ctx.screenshot("cve-inventory-read-retry");
      ctx.emit("assurance.cve.browserReadRetryScanId", ctx.receipts["assurance.vulnerable.scanId"]);
    } finally {
      releaseRead();
      await ctx.page.unroute(routePattern, abort);
      ctx.page.off("request", observe);
    }
  });
};

exports.viewer = async (ctx) => {
  ctx.expect(ctx.facts.cveViewer).toBeTruthy();
  await signIn(ctx, ctx.facts.cveViewer, "Viewer");
  await ctx.step("scoped Viewer reads Current finding without mutation controls", async () => {
    completedInventory(ctx, await openInventory(ctx, "vulnerable", true), "vulnerable", true);
    const detail = await expandFixture(ctx);
    await ctx.expect(detail.getByTestId("system-cve-triage-open")).toHaveCount(0);
    await ctx.screenshot("cve-viewer-current-read-only");
    ctx.emit("assurance.cve.browserViewerScanId", ctx.receipts["assurance.vulnerable.scanId"]);
  });
};
