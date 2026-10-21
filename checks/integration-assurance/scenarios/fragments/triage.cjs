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
  let poamId;
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
    poamId = result.poam_id;
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
  // The first create legitimately writes. Capture only after it completes;
  // these authenticated API reads prove exposed detail/baseline preservation,
  // not a before/after SQL snapshot of the browser's reopen sequence.
  const readPoam = async () => {
    const response = await ctx.context.request.get(ctx.url(`/api/v1/poams/${poamId}`));
    expect(response.status()).toBe(200);
    return response.json();
  };
  const beforeReopen = await readPoam();
  expect(beforeReopen.id).toBe(poamId);
  expect(beforeReopen.cve_findings).toHaveLength(1);
  const finding = beforeReopen.cve_findings[0];
  expect([finding.system_id, finding.canonical_cve_id, finding.canonical_package_name])
    .toEqual([system, f.cveId, f.package]);
  expect(finding.baseline_scan_id).toBe(r["assurance.vulnerable.scanId"]);
  expect(finding.baseline_generation).toBe(r["assurance.vulnerable.deployment"].generation);
  expect(finding.baseline_target_store_path).toBe(r["assurance.vulnerable.outputPath"]);
  expect(finding.baseline_occurrence_derivation_path).toBe(r["assurance.vulnerable.packageDerivationPath"]);
  expect(finding.baseline_observed_package_version).toBe(f.vulnerableVersion);
  const writes = [];
  const monitorWrite = (request) => {
    const path = new URL(request.url()).pathname;
    // Navigation preferences may legitimately write. Watch only triage and
    // remediation mutations, including failed requests and unexpected plans.
    const remediation = /^\/api\/v1\/(poams|acceptances|finding-waivers)(\/|$)/.test(path) ||
      (path.startsWith("/api/v1/") && path.includes("/cves/") &&
        /\/(triage|batch-triage|justifications?)(\/|$)/.test(path));
    if (["POST", "PATCH", "DELETE"].includes(request.method()) && remediation) {
      writes.push({ method: request.method(), path });
    }
  };
  page.on("request", monitorWrite);
  await ctx.step("host schedule survives System CVEs reload", async () => {
    await page.goto(ctx.url(`/systems/${system}?tab=cves`));
    const [inventory] = await Promise.all([inventoryRead(), page.reload()]);
    await assertInventory(inventory);
    const row = page.getByTestId("system-detail").getByRole("row").filter({ hasText: f.cveId });
    await expect(row.getByTestId("system-cve-triage-state")).toHaveText("Scheduled");
    await ctx.screenshot("triage-host-scheduled-reloaded");
  });
  await ctx.step("reopening and cancelling the exact host schedule preserves hydrated metadata", async () => {
    let firstDetail;
    for (let read = 0; read < 2; read += 1) {
      const row = page.getByTestId("system-detail").getByRole("row").filter({ hasText: f.cveId });
      await expect(row).toHaveCount(1);
      await expect(row.getByTestId("system-cve-triage-state")).toHaveText("Scheduled");
      const [response] = await Promise.all([
        page.waitForResponse((res) => {
          const url = new URL(res.url());
          return res.request().method() === "GET" && url.pathname === triagePath &&
            url.searchParams.get("package") === f.package;
        }),
        row.getByTestId("system-cve-triage-open").click(),
      ]);
      expect(response.status()).toBe(200);
      const detail = await response.json();
      expect(detail.canonical_cve_id).toBe(f.cveId);
      expect(detail.canonical_package_name).toBe(f.package);
      expect(detail.scope.selected_system_id).toBe(system);
      expect(detail.effective_source).toBe("host");
      expect(detail.environment_disposition).toBeNull();
      expect(detail.host_disposition.state).toBe("scheduled");
      expect(detail.host_disposition.poam_id).toBe(poamId);
      expect(detail.host_disposition.actor.user_id).toBe(r["delivery.adminId"]);
      expect(detail.host_disposition.poam.id).toBe(poamId);
      expect(detail.host_disposition.poam.title).toBe(plan.title);
      expect(detail.host_disposition.poam.plan).toBe(plan.initialPlan);
      expect(detail.host_disposition.poam.target_date).toBe("2027-02-01");
      expect(detail.host_disposition.poam.assignee.kind).toBe("user");
      expect(detail.host_disposition.poam.assignee.user_id).toBe(r["delivery.adminId"]);
      expect(detail.effective_disposition).toEqual(detail.host_disposition);
      if (read === 0) firstDetail = detail;
      else expect(detail).toEqual(firstDetail);
      const dialog = page.getByRole("dialog", { name: `Triage ${f.cveId} ${f.package}`, exact: true });
      await expect(dialog.getByTestId("cve-triage-scope-host")).toHaveAttribute("aria-checked", "true");
      await expect(dialog.getByTestId("cve-triage-scope-host"))
        .toHaveText(`${detail.scope.selected_system_hostname} only`);
      await expect(dialog.getByRole("button", { name: "Schedule patch", exact: true }))
        .toHaveAttribute("aria-pressed", "true");
      await expect(dialog.getByTestId("cve-poam-assignee")).toHaveValue(`user:${r["delivery.adminId"]}`);
      await expect(dialog.getByTestId("cve-poam-assignee")).toBeDisabled();
      await expect(dialog.getByTestId("cve-poam-target")).toHaveValue("2027-02-01");
      await expect(dialog.getByTestId("cve-poam-target")).toHaveJSProperty("readOnly", true);
      await expect(dialog.getByTestId("cve-poam-plan")).toHaveValue(plan.initialPlan);
      await expect(dialog.getByTestId("cve-poam-plan")).toHaveJSProperty("readOnly", true);
      await expect(dialog.getByText("This schedule will reuse the existing compatible POA&M. Its metadata and milestones are not changed.", { exact: true })).toBeVisible();
      await ctx.screenshot(`triage-host-scheduled-reopened-${read + 1}`);
      await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
      await expect(dialog).toHaveCount(0);
      await expect(row.getByTestId("system-cve-triage-state")).toHaveText("Scheduled");
    }
    expect(await readPoam()).toEqual(beforeReopen);
    expect(writes).toEqual([]);
    ctx.emit("assurance.remediation.browserReopenedPoamDetail", beforeReopen);
  });
  page.off("request", monitorWrite);
};
