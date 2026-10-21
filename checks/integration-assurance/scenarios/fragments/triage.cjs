// browserSpecs["triage-create"] = this module. Requires the same-run vulnerable
// Current receipts and authored facts.remediation. No response is intercepted.
"use strict";

exports.run = async (ctx) => {
  const { page, expect, facts, receipts: r } = ctx;
  const system = r["assurance.vulnerable.systemId"];
  const f = facts.cve;
  const plan = facts.remediation;
  const triagePath = `/api/v1/systems/${system}/cves/${f.cveId}/triage`;
  const inventoryPath = `/api/v1/systems/${system}/cve-inventory-page`;
  await ctx.step("administrator signs in to this Assurance database", async () => {
    await page.goto(ctx.url("/login"));
    await page.getByPlaceholder("Enter your username", { exact: true }).fill(facts.admin.username);
    await page.getByPlaceholder("Enter your password", { exact: true }).fill(facts.admin.password);
    await page.getByRole("button", { name: "Sign In", exact: true }).click();
    await expect(page).toHaveURL(ctx.url("/"));
    expect((await ctx.whoami()).body.roles).toContain("Admin");
    const coach = page.getByRole("button", { name: "Close Crystal Forge Coach", exact: true });
    if (await coach.isVisible()) await coach.click();
  });
  const inventoryRead = () => page.waitForResponse((response) => {
    const url = new URL(response.url());
    return response.request().method() === "GET" && url.pathname === inventoryPath &&
      (url.searchParams.get("target") || "current") === "current";
  });
  const assertInventory = async (response) => {
    expect(response.status()).toBe(200);
    const inventory = await response.json();
    expect(inventory.system_id).toBe(system);
    expect(inventory.source.scan_id).toBe(r["assurance.vulnerable.scanId"]);
    expect(inventory.running_target.derivation_id).toBe(r["assurance.vulnerable.derivationId"]);
    expect(inventory.selection).toEqual({ kind: "current" });
    expect(inventory.read_only).toBe(false);
    expect(inventory.vulnerabilities.map((v) => [v.cve_id, v.canonical_package_name]))
      .toEqual([[f.cveId, f.package]]);
  };
  await ctx.step("Schedule patch uses the actual host finding and typed administrator", async () => {
    const [inventory] = await Promise.all([
      inventoryRead(), page.goto(ctx.url(`/systems/${system}?tab=cves`)),
    ]);
    await assertInventory(inventory);
    const row = page.getByTestId("system-detail").getByRole("row").filter({ hasText: f.cveId });
    await expect(row).toHaveCount(1);
    await expect(row.getByTestId("system-cve-triage-state")).toHaveText("Outstanding");
    const [detailResponse] = await Promise.all([
      page.waitForResponse((response) => response.request().method() === "GET" &&
        new URL(response.url()).pathname === triagePath),
      row.getByTestId("system-cve-triage-open").click(),
    ]);
    expect(detailResponse.status()).toBe(200);
    const detail = await detailResponse.json();
    expect(detail.canonical_cve_id).toBe(f.cveId);
    expect(detail.canonical_package_name).toBe(f.package);
    expect(detail.scope.selected_system_id).toBe(system);
    expect(detail.host_disposition).toBeNull();
    // The approved dialog generates the title. It has no editable title field.
    expect(plan.title).toBe(`${f.cveId} - patch ${f.package} on ${detail.scope.selected_system_hostname}`);
    const dialog = page.getByRole("dialog", { name: `Triage ${f.cveId} ${f.package}`, exact: true });
    await expect(dialog.getByTestId("cve-triage-scope-host")).toHaveAttribute("aria-checked", "true");
    await dialog.getByRole("button", { name: "Schedule patch", exact: true }).click();
    await expect(dialog.getByTestId("cve-poam-title")).toHaveCount(0);
    await dialog.getByTestId("cve-poam-target").fill(plan.targetDate);
    await dialog.getByTestId("cve-poam-plan").fill(plan.initialPlan);
    await dialog.getByTestId("cve-poam-assignee").selectOption(`user:${r["delivery.adminId"]}`);
    const [scheduled] = await Promise.all([
      page.waitForResponse((response) => response.request().method() === "POST" &&
        new URL(response.url()).pathname === triagePath),
      dialog.getByTestId("cve-triage-submit").click(),
    ]);
    expect(scheduled.status()).toBe(200);
    const request = scheduled.request().postDataJSON();
    expect(request.scope).toBe("host");
    expect(request.action).toBe("schedule_patch");
    expect(request.canonical_package_name).toBe(f.package);
    expect(request.poam.title).toBe(plan.title);
    expect(request.poam.plan).toBe(plan.initialPlan);
    expect(request.poam.target_date).toBe("2027-02-01");
    expect(request.poam.assignee).toEqual({ kind: "user", user_id: r["delivery.adminId"] });
    const result = await scheduled.json();
    expect(result.poam_reused).toBe(false);
    expect(typeof result.poam_id).toBe("string");
    expect(result.detail.effective_source).toBe("host");
    expect(result.detail.host_disposition.poam_id).toBe(result.poam_id);
    await expect(page).toHaveURL(new RegExp(`[?&]poam=${result.poam_id}(?:&|$)`));
    const tray = page.getByTestId("poam-detail");
    await expect(tray).toHaveAttribute("data-poam-id", result.poam_id);
    await expect(tray.getByLabel("Title", { exact: true })).toHaveValue(plan.title);
    await expect(tray.getByPlaceholder("What will change, where, and how it will be verified")).toHaveValue(plan.initialPlan);
    await ctx.screenshot("triage-created-real-host-plan");
    ctx.emit("assurance.remediation.poamId", result.poam_id);
    ctx.emit("assurance.remediation.triageRequest", request);
  });
  await ctx.step("host schedule survives System CVEs reload", async () => {
    await page.goto(ctx.url(`/systems/${system}?tab=cves`));
    const [inventory] = await Promise.all([inventoryRead(), page.reload()]);
    await assertInventory(inventory);
    const row = page.getByTestId("system-detail").getByRole("row").filter({ hasText: f.cveId });
    await expect(row.getByTestId("system-cve-triage-state")).toHaveText("Scheduled");
    await ctx.screenshot("triage-host-scheduled-reloaded");
  });
};
