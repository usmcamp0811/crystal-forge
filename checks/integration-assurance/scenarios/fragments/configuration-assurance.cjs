// Approved interaction authority: 18b47eed, FlakeExplorer/AddSystemModal,
// EditSystemModal/SystemDetail/MgmtParts/Systems. Captures are review candidates,
// not golden baselines. Each exported phase gets a fresh cf-browser context.
"use strict";

async function login(ctx) {
  const { page, expect, facts } = ctx;
  await page.goto(ctx.url("/login"));
  await page.getByPlaceholder("Enter your username", { exact: true }).fill(facts.admin.username);
  await page.getByPlaceholder("Enter your password", { exact: true }).fill(facts.admin.password);
  await page.getByRole("button", { name: "Sign In", exact: true }).click();
  await expect(page).toHaveURL(ctx.url("/"));
  expect((await ctx.whoami()).body.user.id).toBe(ctx.receipts["delivery.adminId"]);
  const coach = page.getByRole("button", { name: "Close Crystal Forge Coach", exact: true });
  if (await coach.isVisible()) await coach.click();
}

exports.register = async ctx => {
  const { page, expect, facts, receipts } = ctx;
  await login(ctx);
  // Exercise each actual menu binding before committing one registration.
  for (const mode of ["Managed", "Reference", "External"]) {
    await page.goto(ctx.url(`/flakes?flake=${receipts["delivery.flakeId"]}&pane=systems`));
    const row = page.getByRole("row").filter({ has: page.getByText("assuranceOnly", { exact: true }) });
    await expect(row).toHaveCount(1);
    await row.getByRole("button", { name: /Add to Forge/ }).click();
    await page.getByRole("menuitem", { name: new RegExp(`^${mode}`) }).click();
    const form = page.getByRole("dialog", { name: "Register system", exact: true });
    await expect(form).toBeVisible();
    await expect(form.getByRole("radio", { name: new RegExp(`^${mode}`) })).toHaveAttribute("aria-checked", "true");
    await expect(form.getByRole("button", { name: /^Agent identity/ })).toHaveCount(mode === "Managed" ? 1 : 0);
    const assignmentSection = mode === "Managed" ? "Flake assignment" : "Configuration";
    await form.getByRole("button", { name: assignmentSection, exact: true }).click();
    await expect(form.getByLabel("Flake", { exact: true })).toHaveValue(facts.flake.name);
    await expect(form.getByLabel("Branch", { exact: true })).toHaveValue(facts.flake.branch);
    await expect(form.getByLabel("NixOS configuration *", { exact: true })).toHaveValue("assuranceOnly");
    await expect(form.getByText(/Declared output: nixosConfigurations.assuranceOnly/)).toBeVisible();
    // Chromium sends a keydown without `key` when a datalist suggestion is
    // picked. Before the fix it panicked the WASM app and froze this dialog.
    const pageErrors = [];
    const onPageError = error => pageErrors.push(String(error));
    page.on("pageerror", onPageError);
    await form.getByLabel("NixOS configuration *", { exact: true })
      .evaluate(el => el.dispatchEvent(new Event("keydown", { bubbles: true })));
    // The rail includes the selected-mode badge once the resource is named.
    await form.getByRole("navigation", { name: "Registration sections" })
      .getByRole("button", { name: /^Management(?:\s|$)/ }).click();
    await expect(form.getByRole("heading", { name: "Management mode", exact: true })).toBeVisible();
    await form.getByRole("button", { name: assignmentSection, exact: true }).click();
    page.off("pageerror", onPageError);
    expect(pageErrors).toEqual([]);
    if (mode !== "External") {
      await form.getByRole("button", { name: "Cancel", exact: true }).click();
      continue;
    }
    await form.getByRole("button", { name: "System & environment", exact: true }).click();
    await form.getByLabel("System name *", { exact: true }).fill("external-assurance");
    // The parent picker lists registered managed systems from the Systems list
    // alone: no per-system detail request and no flake lookup.
    const pickerCalls = [];
    const onPickerRequest = request => pickerCalls.push(new URL(request.url()).pathname);
    page.on("request", onPickerRequest);
    await form.getByLabel("Environment *", { exact: true }).selectOption("delivery-proof");
    const parentPicker = form.getByLabel("Parent host · optional", { exact: true });
    await expect(parentPicker.locator(`option[value="${receipts["delivery.targetId"]}"]`)).toHaveCount(1);
    page.off("request", onPickerRequest);
    expect(pickerCalls.filter(path => /^\/api\/v1\/systems\/[0-9a-f-]{36}$/.test(path))).toEqual([]);
    expect(pickerCalls.filter(path => path.startsWith("/api/v1/flakes"))).toEqual([]);
    await parentPicker.selectOption(receipts["delivery.targetId"]);
    await form.getByRole("button", { name: "Tags", exact: true }).click();
    await expect(form.getByText(/No deployment policy applies/)).toBeVisible();
    await expect(form.getByLabel("Deployment policy", { exact: true })).toHaveCount(0);
    const [response] = await Promise.all([
      page.waitForResponse(r => new URL(r.url()).pathname === "/api/v1/systems" && r.request().method() === "POST"),
      form.getByRole("button", { name: "Register system", exact: true }).click(),
    ]);
    expect(response.status()).toBe(201);
    const sent = response.request().postDataJSON();
    expect(sent).toMatchObject({ management_mode: "external", hostname: "external-assurance", system_configuration_name: "assuranceOnly",
      parent_host_id: receipts["delivery.targetId"], deployment_policy: "manual" });
    expect(sent.public_key == null).toBe(true);
    const created = await response.json();
    expect(created.public_key_fingerprint).toBeNull();
    ctx.emit("management.externalId", created.id);
    const completed = page.getByRole("dialog", { name: "Registration complete", exact: true });
    await expect(completed.getByText(/Nothing will be deployed/)).toBeVisible();
    await ctx.screenshot("configuration-keyless-registration-complete");
  }
};

exports.evidence = async ctx => {
  const { page, expect, receipts } = ctx;
  await login(ctx);
  const external = receipts["management.externalId"];
  const reference = receipts["management.referenceId"];
  const artifact = receipts["management.artifact"];
  const scanId = receipts["management.scanId"];
  const root = page.locator("html");
  await expect(root).toHaveAttribute("data-theme", /^(dark|light)$/);
  const originalTheme = await root.getAttribute("data-theme");
  const setTheme = async theme => {
    if (await root.getAttribute("data-theme") !== theme) {
      const [saved] = await Promise.all([
        page.waitForResponse(r => new URL(r.url()).pathname === "/api/v1/user/preferences" && r.request().method() === "PATCH"),
        page.getByRole("button", { name: "Toggle theme", exact: true }).click(),
      ]);
      expect(saved.status()).toBe(200);
      await expect(root).toHaveAttribute("data-theme", theme);
    }
  };
  try {
    for (const theme of ["dark", "light"]) {
      await setTheme(theme);
      for (const [id, mode] of [[external, "external"], [reference, "reference"]]) {
        await page.goto(ctx.url(`/systems/${id}`));
        await expect(root).toHaveAttribute("data-theme", theme);
        const detail = page.getByTestId("system-detail");
        await expect(detail.getByRole("heading", { name: "Configuration evidence", exact: true })).toBeVisible();
        await expect(detail.getByText(artifact.commit, { exact: false })).not.toHaveCount(0);
        // Later manual/periodic cases seal newer scans of the same artifact, so
        // the displayed selected scan is the server's current selection. It
        // must still be a sealed scan of this exact artifact.
        const evidenceRead = await ctx.context.request.get(ctx.url(`/api/v1/systems/${id}/configuration-assurance`));
        expect(evidenceRead.status()).toBe(200);
        const selected = (await evidenceRead.json()).selected_scan;
        expect(selected.derivation_id).toBe(artifact.id);
        expect(selected.cves).not.toBeNull();
        await expect(detail.getByText(selected.scan_id, { exact: false })).not.toHaveCount(0);
        expect(scanId).toBeTruthy();
        await expect(detail.getByRole("heading", { name: "Runtime observations", exact: true })).toHaveCount(mode === "external" ? 1 : 0);
        if (mode === "external") {
          // The parent is named by hostname, in the control card and in the
          // header of the observation that this parent reported.
          const parentRead = await ctx.context.request.get(ctx.url(`/api/v1/systems/${receipts["delivery.targetId"]}`));
          expect(parentRead.status()).toBe(200);
          const parentHostname = (await parentRead.json()).hostname;
          await expect(detail.getByRole("link", { name: parentHostname, exact: true })).toHaveAttribute("href", new RegExp(`^/systems/${receipts["delivery.targetId"]}(\\?|$)`));
          await expect(detail.getByText(`Reported by host ${parentHostname} ·`, { exact: false })).toBeVisible();
          await expect(detail.getByText(`Reported by host ${receipts["delivery.targetId"]}`, { exact: false })).toHaveCount(0);
          await expect(detail.getByText("microvm@external-assurance.service", { exact: true })).toBeVisible();
          await expect(detail.getByText("not comparable", { exact: true })).toBeVisible();
          await expect(detail.getByText("Verified guest state: not available", { exact: true })).toBeVisible();
        }
        await ctx.screenshot(`configuration-${mode}-overview-${theme}`);
        for (const [tab, title] of [["Deploy", mode === "external" ? "Deployment is controlled outside Crystal Forge" : "Reference configurations are not deployed"], ["Logs", "No runtime logs"]]) {
          await detail.getByRole("tab", { name: tab, exact: true }).click();
          await expect(detail.getByRole("heading", { name: title, exact: true })).toBeVisible();
          await expect(detail.getByRole("button", { name: /^(Deploy |Roll back|Restart)/ })).toHaveCount(0);
          await ctx.screenshot(`configuration-${mode}-${tab.toLowerCase()}-${theme}`);
        }
        for (const tab of ["Evidence history", "Config", "CVEs", "Hardening", "Compliance"]) {
          // The CVEs tab carries the critical count of the last good scan as part
          // of its accessible name ("CVEs 2"), so match the name with an optional count.
          const tabName = new RegExp(`^${tab}( \\d+)?$`);
          await detail.getByRole("tab", { name: tabName }).click();
          await expect(detail.getByRole("tab", { name: tabName })).toHaveAttribute("aria-selected", "true");
          if (tab === "CVEs") {
            // The expected count comes from the API read above, not from the page.
            const badge = detail.getByRole("tab", { name: tabName }).locator(".sd-tab-badge");
            if (selected.cves.critical > 0) await expect(badge).toHaveText(String(selected.cves.critical));
            else await expect(badge).toHaveCount(0);
          }
          if (tab === "Evidence history") {
            for (const kind of ["Eval", "Build", "Scan", "All"]) {
              await detail.getByRole("button", { name: kind, exact: true }).click();
              await expect(detail.getByRole("button", { name: kind, exact: true })).toHaveAttribute("aria-pressed", "true");
            }
            await expect(detail.getByText(artifact.commit, { exact: false })).not.toHaveCount(0);
          }
          if (tab === "Config") {
            await expect(detail.getByText("Commits only · configuration evidence", { exact: true })).toBeVisible();
            await expect(detail.getByRole("button", { name: "Generations", exact: true })).toHaveCount(0);
          }
          if (["CVEs", "Hardening"].includes(tab)) {
            await expect(detail.getByText(/Commits only · read-only/)).toBeVisible();
            await expect(detail.getByText(/does not establish Current runtime authority or permit POA&M closure/)).toBeVisible();
            await expect(detail.getByRole("button", { name: /^(Triage|Schedule patch|Accept risk|Verify now|Authoritative close)/ })).toHaveCount(0);
          }
          if (tab === "CVEs") await expect(detail.getByText(ctx.facts.cve.cveId, { exact: true })).not.toHaveCount(0);
          if (tab === "Hardening") await expect(detail.getByRole("heading", { name: "Hardening evidence unavailable", exact: true })).toBeVisible();
          if (["Hardening", "Compliance"].includes(tab)) {
            await expect(detail.getByText("Configuration assessments, not runtime assurance", { exact: true })).toBeVisible();
            await expect(detail.getByText(/CVE scan status does not establish hardening or compliance results/)).toBeVisible();
          }
          await ctx.screenshot(`configuration-${mode}-${tab.toLowerCase().replaceAll(" ", "-")}-${theme}`);
        }
      }
    }
  } finally {
    await setTheme(originalTheme);
  }
  await page.goto(ctx.url(`/systems/${external}`));
  await page.getByRole("button", { name: "Edit", exact: true }).click();
  const editor = page.getByTestId("edit-system-modal");
  await expect(editor.getByRole("button", { name: "Configuration", exact: true })).toBeVisible();
  await expect(editor.getByRole("button", { name: "Security", exact: true })).toHaveCount(0);
  await editor.getByRole("button", { name: "Management", exact: true }).click();
  await editor.getByRole("radio", { name: /^Managed/ }).click();
  await expect(editor.getByLabel("New agent public key *", { exact: true })).toBeVisible();
  await expect(editor.getByRole("button", { name: "Save changes", exact: true })).toBeDisabled();
  // Approved rail: Security appears for the managed form mode. Before the
  // conversion is saved there is no enrolled key to show or rotate.
  await editor.getByRole("button", { name: "Security", exact: true }).click();
  await expect(editor.getByText("No agent key is enrolled yet", { exact: true })).toBeVisible();
  await expect(editor.getByRole("button", { name: /Rotate key/ })).toHaveCount(0);
  await expect(editor.getByRole("button", { name: "Danger zone", exact: true })).toBeVisible();
  await editor.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(editor).toHaveCount(0);
  const unchanged = await ctx.context.request.get(ctx.url(`/api/v1/systems/${external}`));
  expect(unchanged.status()).toBe(200);
  expect(await unchanged.json()).toMatchObject({ management_mode: "external", parent_host_id: receipts["delivery.targetId"] });
  // A parent edit must invalidate both the card and the metric without a page
  // reload. Retained reports from the former association are not current.
  await page.getByRole("button", { name: "Edit", exact: true }).click();
  await editor.getByRole("button", { name: "Configuration", exact: true }).click();
  await editor.getByLabel("Parent host · optional", { exact: true }).selectOption("");
  const [detached] = await Promise.all([
    page.waitForResponse(r => new URL(r.url()).pathname === `/api/v1/systems/${external}` && r.request().method() === "PATCH"),
    editor.getByRole("button", { name: "Save changes", exact: true }).click(),
  ]);
  expect(detached.status()).toBe(200);
  expect((await detached.json()).parent_host_id).toBeNull();
  const runtimeMetric = page.locator('[data-metric="Runtime"]');
  await expect(runtimeMetric).toContainText("Unknown");
  await expect(runtimeMetric).toContainText("no parent host");
  await expect(runtimeMetric).not.toContainText("Host-observed");
  await expect(page.getByText("Associate a parent host in Edit to receive host-reported observations.", { exact: true })).toBeVisible();
  // Restore the association through the same authorized edit workflow.
  await page.getByRole("button", { name: "Edit", exact: true }).click();
  await editor.getByRole("button", { name: "Configuration", exact: true }).click();
  await editor.getByLabel("Parent host · optional", { exact: true }).selectOption(receipts["delivery.targetId"]);
  const [attached] = await Promise.all([
    page.waitForResponse(r => new URL(r.url()).pathname === `/api/v1/systems/${external}` && r.request().method() === "PATCH"),
    editor.getByRole("button", { name: "Save changes", exact: true }).click(),
  ]);
  expect(attached.status()).toBe(200);
  expect((await attached.json()).parent_host_id).toBe(receipts["delivery.targetId"]);
  await page.getByRole("button", { name: "Edit", exact: true }).click();
  await editor.getByRole("button", { name: "Management", exact: true }).click();
  await editor.getByRole("radio", { name: /^Reference/ }).click();
  await expect(editor.getByLabel("New agent public key *", { exact: true })).toHaveCount(0);
  await expect(editor.getByRole("button", { name: "Save changes", exact: true })).toBeDisabled();
  await editor.getByLabel("I authorize this management-mode change when Save changes succeeds.", { exact: true }).check();
  const [changed] = await Promise.all([
    page.waitForResponse(r => new URL(r.url()).pathname === `/api/v1/systems/${external}` && r.request().method() === "PATCH"),
    editor.getByRole("button", { name: "Save changes", exact: true }).click(),
  ]);
  expect(changed.status()).toBe(200);
  expect((await changed.json()).management_mode).toBe("reference");
  await page.reload();
  await expect(page.getByRole("heading", { name: "Runtime observations", exact: true })).toHaveCount(0);
  await page.goto(ctx.url("/systems"));
  // Design 2d519abc: each Systems filter is a checkbox multi-select.
  const modeFilter = page.getByRole("button", { name: "Management filter", exact: true });
  await modeFilter.click();
  await page.getByRole("listbox", { name: "Management", exact: true })
    .getByRole("checkbox", { name: /^Reference configuration/ }).check();
  await page.keyboard.press("Escape");
  await expect(modeFilter).toHaveText(/Management:\s*Reference configuration/);
  await expect(page.getByText("Golden workstation / ISO", { exact: true })).not.toHaveCount(0);
  await expect(page.getByText("targetA", { exact: true })).toHaveCount(0);
  await page.setViewportSize({ width: 375, height: 812 });
  await page.goto(ctx.url(`/systems/${reference}?tab=deploy`));
  await expect(page.getByRole("heading", { name: "Reference configurations are not deployed", exact: true })).toBeVisible();
  await ctx.screenshot("configuration-reference-narrow-unavailable");
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto(ctx.url(`/systems/${reference}`));
  const [requested] = await Promise.all([
    page.waitForResponse(r => new URL(r.url()).pathname === `/api/v1/systems/${reference}/cve-scan` && r.request().method() === "POST"),
    page.getByRole("button", { name: "Re-scan", exact: true }).click(),
  ]);
  expect(requested.status()).toBe(202);
  expect(new URL(requested.url()).searchParams.get("expected_derivation_id")).toBe(String(artifact.id));
  const result = await requested.json();
  expect(result.derivation_id).toBe(artifact.id);
  ctx.emit("management.requestedScan", result.scan_id);
  await expect(page.getByRole("status").filter({ hasText: result.scan_id })).not.toHaveCount(0);
  await ctx.screenshot("configuration-real-scan-request-receipt");
  ctx.emit("management.visualAuthority", { revision: "18b47eed", components: ["SystemDetail.jsx", "MgmtParts.jsx", "AddSystemModal.jsx", "EditSystemModal.jsx", "FlakeExplorer.jsx"],
    disposition: "candidate captures; independent equivalent-state reference comparison pending" });
};

// Background work is bounded. The Systems page must not acknowledge navigation
// badges in a loop (it once sent about 60 requests per second), and a hidden
// document must stop polling and refresh as soon as it is shown again. Hidden is
// simulated: headless Chromium keeps its real visibility, so this exercises the
// application's reaction to `visibilitychange`, not the browser's tab handling.
exports.backgroundWork = async ctx => {
  const { page, expect } = ctx;
  await login(ctx);
  const calls = [];
  page.on("request", request => {
    const url = new URL(request.url());
    if (url.pathname.startsWith("/api/")) calls.push(`${request.method()} ${url.pathname}`);
  });
  const hits = call => calls.filter(entry => entry === call).length;
  const setHidden = hidden => page.evaluate(value => {
    Object.defineProperty(document, "hidden", { configurable: true, get: () => value });
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => (value ? "hidden" : "visible") });
    document.dispatchEvent(new Event("visibilitychange"));
  }, hidden);

  await page.goto(ctx.url("/systems"));
  await page.waitForTimeout(3000);
  calls.length = 0;
  await page.waitForTimeout(6000);
  expect(hits("POST /api/v1/navigation/acknowledge")).toBeLessThanOrEqual(1);

  // Visible: the Builds page keeps its live refresh.
  await page.goto(ctx.url("/builds"));
  await page.waitForTimeout(2500);
  calls.length = 0;
  await expect.poll(() => hits("GET /api/v1/build-jobs"), { timeout: 9000 }).toBeGreaterThan(0);
  // Hidden: no polling of any kind.
  await setHidden(true);
  await page.waitForTimeout(1000);
  calls.length = 0;
  await page.waitForTimeout(8000);
  expect(calls.filter(entry => entry.startsWith("GET "))).toEqual([]);
  // Shown again: the stale data is refreshed without a reload.
  await setHidden(false);
  await expect.poll(() => hits("GET /api/v1/build-jobs"), { timeout: 3000 }).toBeGreaterThan(0);
  await ctx.screenshot("ui-background-work-resumed");
};

// A reused head B requests its own carrier. The server scans execution source
// A; the browser must accept that response as a match for B.
exports.reuseRescan = async ctx => {
  const { page, expect, receipts } = ctx;
  await login(ctx);
  const binding = receipts["management.reuseBinding"];
  await page.goto(ctx.url(`/systems/${receipts["management.referenceId"]}`));
  const [requested] = await Promise.all([
    page.waitForResponse(r => new URL(r.url()).pathname === `/api/v1/systems/${receipts["management.referenceId"]}/cve-scan` && r.request().method() === "POST"),
    page.getByRole("button", { name: "Re-scan", exact: true }).click(),
  ]);
  const body = await requested.text();
  expect(requested.status(), `reused-head scan admission response: ${body}`).toBe(202);
  expect(new URL(requested.url()).searchParams.get("expected_derivation_id")).toBe(String(binding.id));
  const result = JSON.parse(body);
  expect(result.configuration_derivation_id).toBe(binding.id);
  expect(result.derivation_id).toBe(binding.reused_from_derivation_id);
  await expect(page.getByRole("status").filter({ hasText: result.scan_id })).not.toHaveCount(0);
  await expect(page.getByText("Scan response identity did not match the requested artifact.", { exact: false })).toHaveCount(0);
  ctx.emit("management.reuseRescan", result.scan_id);
  await ctx.screenshot("configuration-reused-head-rescan-receipt");
};

exports.removed = async ctx => {
  const { page, expect, receipts } = ctx;
  await login(ctx);
  await page.goto(ctx.url(`/systems/${receipts["management.referenceId"]}`));
  await expect(page.getByRole("heading", { name: "nixosConfigurations.assuranceOnly is missing at the selected revision", exact: true })).toBeVisible();
  await expect(page.getByText("The configuration was removed or renamed. Evidence from earlier revisions is retained.", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Re-scan", exact: true })).toBeDisabled();
  await ctx.screenshot("configuration-removed-head-retains-history");
  await page.getByRole("tab", { name: "Evidence history", exact: true }).click();
  await page.getByRole("button", { name: "Scan", exact: true }).click();
  await expect(page.getByText(receipts["management.artifact"].commit, { exact: false })).not.toHaveCount(0);
  await ctx.screenshot("configuration-removed-head-historical-scan");
};
