// browserSpecs["poam-maintain"] = this module. Python owns API maintenance;
// this browser phase proves those authored values survive a production reload.
"use strict";

exports.run = async (ctx) => {
  const { page, expect, facts, receipts: r } = ctx;
  const id = r["assurance.remediation.poamId"];
  const system = r["assurance.vulnerable.systemId"];
  const f = facts.remediation;
  await page.goto(ctx.url("/login"));
  await page.getByPlaceholder("Enter your username", { exact: true }).fill(facts.admin.username);
  await page.getByPlaceholder("Enter your password", { exact: true }).fill(facts.admin.password);
  await page.getByRole("button", { name: "Sign In", exact: true }).click();
  await expect(page).toHaveURL(ctx.url("/"));
  expect((await ctx.whoami()).body.roles).toContain("Admin");
  const coach = page.getByRole("button", { name: "Close Crystal Forge Coach", exact: true });
  if (await coach.isVisible()) await coach.click();
  await ctx.step("maintained real CVE plan reload retains exact identity and authored values", async () => {
    await page.goto(ctx.url(`/systems/${system}?tab=cves&poam=${id}`));
    await expect(page.getByTestId("poam-detail")).toHaveAttribute("data-poam-id", id);
    const [response] = await Promise.all([
      page.waitForResponse((res) => res.request().method() === "GET" &&
        new URL(res.url()).pathname === `/api/v1/poams/${id}`),
      page.reload(),
    ]);
    expect(response.status()).toBe(200);
    const saved = await response.json();
    expect(saved.id).toBe(id);
    expect(saved.status).toBe("in_progress");
    expect(saved.plan).toBe(f.updatedPlan);
    expect(saved.cve_findings.map((finding) => [finding.id, finding.link_id, finding.current_scan_id]))
      .toEqual([[r["assurance.remediation.findingId"], r["assurance.remediation.linkId"], r["assurance.vulnerable.scanId"]]]);
    const tray = page.getByTestId("poam-detail");
    await expect(tray).toHaveAttribute("data-poam-id", id);
    await expect(tray.getByLabel("Title", { exact: true })).toHaveValue(f.title);
    await expect(tray.getByTestId("poam-assignee-select")).toHaveValue(`user:${r["delivery.adminId"]}`);
    await expect(tray.getByLabel("Target completion", { exact: true })).toHaveValue(f.targetDate);
    await expect(tray.getByPlaceholder("What will change, where, and how it will be verified")).toHaveValue(f.updatedPlan);
    await expect(tray.getByText(`Added note: ${f.note}`, { exact: true })).toBeVisible();
    const milestone = tray.getByTestId("poam-milestone").filter({ hasText: f.milestone });
    await expect(milestone).toHaveCount(1);
    await expect(milestone).toHaveAttribute("data-milestone-id", r["assurance.remediation.milestoneId"]);
    const group = tray.getByTestId("poam-cve-group");
    await expect(group).toHaveAttribute("data-cve-id", facts.cve.cveId);
    await expect(group).toHaveAttribute("data-cve-package", facts.cve.package);
    const finding = group.getByTestId("poam-linked-vulnerability");
    await expect(finding).toHaveAttribute("data-cve-finding-id", r["assurance.remediation.findingId"]);
    await expect(finding.getByText("FAIL", { exact: true })).toBeVisible();
    await expect(finding.getByTitle(r["assurance.vulnerable.scanId"], { exact: true })).toBeVisible();
    await expect(tray.getByRole("button", { name: "Verify now", exact: true })).toHaveCount(0);
    await expect(tray.getByRole("button", { name: "Authoritative close", exact: true })).toHaveCount(0);
    await ctx.screenshot("poam-maintained-real-finding-reloaded");
    ctx.emit("assurance.remediation.browserMaintainedPoamId", id);
  });
};
