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
    await form.getByRole("button", { name: "Configuration", exact: true }).click();
    await expect(form.getByLabel("Flake", { exact: true })).toHaveValue(facts.flake.name);
    await expect(form.getByLabel("Branch", { exact: true })).toHaveValue(facts.flake.branch);
    await expect(form.getByLabel("NixOS configuration *", { exact: true })).toHaveValue("assuranceOnly");
    await expect(form.getByText(/Declared output: nixosConfigurations.assuranceOnly/)).toBeVisible();
    if (mode !== "External") {
      await form.getByRole("button", { name: "Cancel", exact: true }).click();
      continue;
    }
    await form.getByRole("button", { name: "System & environment", exact: true }).click();
    await form.getByLabel("System name *", { exact: true }).fill("external-assurance");
    await form.getByLabel("Environment *", { exact: true }).selectOption("delivery-proof");
    await form.getByLabel("Parent host · optional", { exact: true }).selectOption(receipts["delivery.targetId"]);
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
          await detail.getByRole("tab", { name: tab, exact: true }).click();
          await expect(detail.getByRole("tab", { name: tab, exact: true })).toHaveAttribute("aria-selected", "true");
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
  await editor.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(editor).toHaveCount(0);
  const unchanged = await ctx.context.request.get(ctx.url(`/api/v1/systems/${external}`));
  expect(unchanged.status()).toBe(200);
  expect(await unchanged.json()).toMatchObject({ management_mode: "external", parent_host_id: receipts["delivery.targetId"] });
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
  await page.getByLabel("Filter by management mode", { exact: true }).selectOption("reference");
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
