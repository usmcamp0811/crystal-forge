// browserSpecs["poam-verify-close"] = this module. Run after Python's explicit
// production verification/closure. Browser reads remain real and unmocked.
"use strict";

exports.run = async (ctx) => {
  const { page, expect, facts, receipts: r } = ctx;
  const id = r["assurance.remediation.poamId"];
  const system = r["assurance.corrected.systemId"];
  await page.goto(ctx.url("/login"));
  await page.getByPlaceholder("Enter your username", { exact: true }).fill(facts.admin.username);
  await page.getByPlaceholder("Enter your password", { exact: true }).fill(facts.admin.password);
  await page.getByRole("button", { name: "Sign In", exact: true }).click();
  await expect(page).toHaveURL(ctx.url("/"));
  expect((await ctx.whoami()).body.roles).toContain("Admin");
  const coach = page.getByRole("button", { name: "Close Crystal Forge Coach", exact: true });
  if (await coach.isVisible()) await coach.click();
  await ctx.step("closed real plan reload preserves the original finding and both PASS attempts", async () => {
    await page.goto(ctx.url(`/systems/${system}?tab=cves&poam=${id}`));
    await expect(page.getByTestId("poam-detail")).toHaveAttribute("data-poam-id", id);
    const [response] = await Promise.all([
      page.waitForResponse((res) => res.request().method() === "GET" &&
        new URL(res.url()).pathname === `/api/v1/poams/${id}`),
      page.reload(),
    ]);
    expect(response.status()).toBe(200);
    const detail = await response.json();
    expect(detail.id).toBe(id);
    expect(detail.status).toBe("completed");
    expect(detail.closure_attempt_id).toBe(r["assurance.remediation.closureAttemptId"]);
    expect(detail.plan).toBe(facts.remediation.updatedPlan);
    expect(detail.cve_findings.map((f) => [f.id, f.link_id, f.baseline_scan_id, f.current_scan_id]))
      .toEqual([[r["assurance.remediation.findingId"], r["assurance.remediation.linkId"],
        r["assurance.vulnerable.scanId"], r["assurance.corrected.scanId"]]]);
    for (const attemptId of [r["assurance.remediation.verifyAttemptId"], r["assurance.remediation.closureAttemptId"]]) {
      const attempt = detail.verification_attempts.find((a) => a.id === attemptId);
      expect(attempt.outcome).toBe("accepted");
      expect(attempt.attempted_by).toBe(r["delivery.adminId"]);
      expect(attempt.cve_items.map((item) => [item.cve_finding_id, item.result, item.baseline_scan_id, item.scan_id]))
        .toEqual([[r["assurance.remediation.findingId"], "pass", r["assurance.vulnerable.scanId"], r["assurance.corrected.scanId"]]]);
    }
    const tray = page.getByTestId("poam-detail");
    await expect(tray).toHaveAttribute("data-poam-id", id);
    await expect(tray.getByText("Completed", { exact: true })).toBeVisible();
    await expect(tray.getByRole("button", { name: "Verify now", exact: true })).toHaveCount(0);
    await expect(tray.getByRole("button", { name: "Reopen", exact: true })).toBeVisible();
    const history = tray.getByTestId("poam-retired-vulnerability");
    await expect(history).toHaveCount(1);
    await expect(history).toHaveAttribute("data-cve-finding-id", r["assurance.remediation.findingId"]);
    await expect(history.getByText(facts.cve.cveId, { exact: true })).toBeVisible();
    await expect(history.getByText(`Baseline ${facts.cve.vulnerableVersion} · Scan ${r["assurance.vulnerable.scanId"]}`, { exact: true })).toBeVisible();
    const verification = tray.getByTestId("poam-verification-result");
    await expect(verification).toHaveCount(2);
    for (const result of await verification.all()) {
      await expect(result.locator(`[data-cve-finding-id='${r["assurance.remediation.findingId"]}']`)
        .getByText("PASS", { exact: true })).toBeVisible();
      await expect(result.getByText(r["assurance.corrected.scanId"], { exact: true })).toBeVisible();
    }
    await ctx.screenshot("poam-closed-real-current-history-reloaded");
    ctx.emit("assurance.remediation.browserClosedPoamId", id);
  });
  await ctx.step("System CVEs Current still reads the actual corrected clean scan", async () => {
    const [response] = await Promise.all([
      page.waitForResponse((res) => {
        const url = new URL(res.url());
        return res.request().method() === "GET" && url.pathname === `/api/v1/systems/${system}/cve-inventory-page` &&
          (url.searchParams.get("target") || "current") === "current";
      }),
      page.goto(ctx.url(`/systems/${system}?tab=cves`)),
    ]);
    expect(response.status()).toBe(200);
    const inventory = await response.json();
    expect(inventory.system_id).toBe(system);
    expect(inventory.source.scan_id).toBe(r["assurance.corrected.scanId"]);
    expect(inventory.running_target.derivation_id).toBe(r["assurance.corrected.derivationId"]);
    expect(inventory.vulnerabilities).toEqual([]);
    await expect(page.getByRole("heading", { name: "No findings in the selected scan", exact: true })).toBeVisible();
    await ctx.screenshot("poam-closed-corrected-current-clean");
  });
};
