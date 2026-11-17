// Source-aware Users modal regression (TASK-529.2 review P1).
// Requires admin/operator/adminSourceGrants facts and same-run admin.userId /
// authorization.operator.userId receipts. Python supplies a pure-IdP Operator
// and production membership, then checks SQL/audits and restores the fixture.
// This is real UI/HTTP coverage, not a signed IdP callback or reconciliation.
"use strict";

exports.run = async (ctx) => {
  const { page, expect, facts, receipts } = ctx;
  const config = facts.adminSourceGrants;
  const id = receipts["authorization.operator.userId"];
  const path = `/api/v1/admin/users/${id}`;
  const expectedSources = {
    local_roles: [], local_environments: [], oidc_roles: ["Operator"],
    oidc_environments: [config.environment], legacy_roles: [], legacy_environments: [],
  };

  await ctx.step("Admin opens the pure-IdP Operator's Users modal", async () => {
    await page.goto(ctx.url("/login"));
    await page.getByPlaceholder("Enter your username", { exact: true }).fill(facts.admin.username);
    await page.getByPlaceholder("Enter your password", { exact: true }).fill(facts.admin.password);
    await page.getByRole("button", { name: "Sign In", exact: true }).click();
    await expect(page).toHaveURL(ctx.url("/"));
    const who = await ctx.whoami();
    expect(who.status).toBe(200);
    expect(who.body.user.id).toBe(receipts["admin.userId"]);
    expect(who.body.roles).toEqual(["Admin"]);
    await page.goto(ctx.url("/admin"));
    await expect(page.getByRole("heading", { name: "Server Management", exact: true })).toBeVisible();
    const coach = page.getByRole("button", { name: "Close Crystal Forge Coach", exact: true });
    if (await coach.isVisible()) await coach.click();
  });

  const open = async () => {
    const row = page.getByTestId(`admin-user-${id}`);
    await expect(row).toContainText(facts.operator.email);
    await row.getByRole("button", { name: `Edit user ${facts.operator.username} (${facts.operator.email})`, exact: true }).click();
    const modal = page.getByTestId("edit-user-modal");
    await expect(modal).toBeVisible();
    await expect(modal.getByTestId("effective-role")).toHaveText("Effective role (read-only): Operator");
    await expect(modal.getByTestId("effective-environments")).toHaveText(`Assigned environment memberships (read-only): ${config.environment}`);
    await expect(modal.getByTestId("role-sources")).toContainText("Identity provider roles: Operator");
    await expect(modal.getByTestId("role-sources")).toContainText("Local roles: None");
    await expect(modal.getByTestId("role-sources")).toContainText("Unresolved legacy roles: None");
    await expect(modal.getByTestId("environment-sources")).toContainText(`Identity provider environments: ${config.environment}`);
    await expect(modal.getByTestId("environment-sources")).toContainText("Local environments: None");
    await expect(modal.getByTestId("environment-sources")).toContainText("Unresolved legacy environments: None");
    await expect(modal.getByTestId("edit-local-role")).not.toBeChecked();
    await expect(modal.getByRole("combobox", { name: "Local role override", exact: true })).toHaveValue("");
    await expect(modal.getByTestId("local-role")).toBeDisabled();
    await expect(modal.getByTestId("edit-local-environments")).not.toBeChecked();
    const environment = modal.getByTestId("local-environments").getByRole("button", { name: config.environment, exact: true });
    await expect(environment).toHaveAttribute("aria-pressed", "false");
    await expect(environment).toBeDisabled();
    await expect(modal.getByTestId("confirm-legacy-role")).toHaveCount(0);
    await expect(modal.getByTestId("confirm-legacy-environments")).toHaveCount(0);
    return modal;
  };

  await ctx.step("unrelated password/status save omits every grant field", async () => {
    const modal = await open();
    // Password inputs have no implicit textbox role; use their accessible label.
    await modal.getByLabel("Password reset", { exact: true }).fill(config.resetPassword);
    await modal.getByTestId("user-enabled").uncheck();
    const [response] = await Promise.all([
      page.waitForResponse((r) => new URL(r.url()).pathname === path && r.request().method() === "PATCH"),
      modal.getByRole("button", { name: "Save changes", exact: true }).click(),
    ]);
    const payload = response.request().postDataJSON();
    expect(payload).toEqual({ password: config.resetPassword, enabled: false });
    expect(response.status()).toBe(200);
    const body = await response.json();
    expect(body.id).toBe(id);
    expect(body.enabled).toBe(false);
    expect(body.role).toBe("Operator");
    expect(body.environments).toEqual([config.environment]);
    expect(body.grant_sources).toEqual(expectedSources);
    ctx.emit("adminSourceGrants.response", { status: response.status(), payload, body });
    await expect(modal).toHaveCount(0);
  });

  await ctx.step("fresh reload retains IdP-only authority and empty local drafts", async () => {
    const [response] = await Promise.all([
      page.waitForResponse((r) => new URL(r.url()).pathname === "/api/v1/admin/users" && r.request().method() === "GET"),
      page.reload(),
    ]);
    expect(response.status()).toBe(200);
    const users = await response.json();
    const saved = users.filter((u) => u.id === id);
    expect(saved).toHaveLength(1);
    expect(saved[0].grant_sources).toEqual(expectedSources);
    const modal = await open();
    await expect(modal.getByTestId("user-enabled")).not.toBeChecked();
    await expect(modal.getByLabel("Password reset", { exact: true })).toHaveValue("");
    await ctx.screenshot("admin-source-unrelated-save-reloaded");
    await modal.getByRole("button", { name: "Cancel", exact: true }).click();
    ctx.emit("adminSourceGrants.browserComplete", true);
  });
};
