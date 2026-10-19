// Production Admin forms; semantic evidence, not visual parity.
// Requires facts.admin/adminRoleWire/adminFormUser/adminForms, admin.userId,
// and exactly one pending adminForms.request.<phase> receipt from admin-forms.py.
// Phases run in order: create establishes adminForms.userId for edit/cancel/
// disable; retry and classification start from restored installed configuration.
// Each registered name maps to this file but gets its own artifact directory
// and fresh browser context. Python requests one phase and checks SQL before
// requesting the next phase. No authentication state crosses browser runs.
"use strict";

const phases = ["create", "edit", "cancel", "disable", "retry", "classification-enable", "classification"];

exports.run = async (ctx) => {
  const { page, expect, facts, receipts } = ctx;
  const user = facts.adminFormUser;
  const config = facts.adminForms;
  const pending = phases.filter((p) => receipts[`adminForms.request.${p}`] && !receipts[`adminForms.browser.${p}`]);
  expect(pending).toHaveLength(1);
  const phase = pending[0];

  // These production fields have visible labels but no for/id association.
  // Select the immediate field container by its direct label, not CSS classes
  // or positional input indexes. Modal headings likewise have no dialog role.
  // A direct-label XPath is necessary because the unassociated label cannot
  // supply an accessible name. The exact label narrows to one field container.
  const inputFor = (scope, label, tag = "input") => scope.locator("label")
    .filter({ hasText: new RegExp(`^${label}$`) }).locator("..").locator(`:scope > ${tag}`);
  const userRow = () => page.getByRole("row").filter({ hasText: user.email });

  await ctx.step(`${phase}: independent Admin sign-in`, async () => {
    await page.goto(ctx.url("/login"));
    await page.getByPlaceholder("Enter your username", { exact: true }).fill(facts.admin.username);
    await page.getByPlaceholder("Enter your password", { exact: true }).fill(facts.admin.password);
    await page.getByRole("button", { name: "Sign In", exact: true }).click();
    await expect(page).toHaveURL(ctx.url("/"));
    const who = await ctx.whoami();
    expect(who.status).toBe(200);
    expect(who.body.user.id).toBe(receipts["admin.userId"]);
    expect(who.body.roles).toEqual([facts.adminRoleWire]);
    await page.goto(ctx.url("/admin"));
    await expect(page.getByRole("heading", { name: "Server Management", exact: true })).toBeVisible();
    const coach = page.getByRole("button", { name: "Close Crystal Forge Coach", exact: true });
    if (await coach.isVisible()) await coach.click();
    await expect(page.getByRole("table")).toBeVisible();
  });

  const mutate = async (method, path, trigger, status, key) => {
    const [response] = await Promise.all([
      page.waitForResponse((r) => new URL(r.url()).pathname === path && r.request().method() === method),
      trigger(),
    ]);
    expect(response.status()).toBe(status);
    const body = await response.json();
    ctx.emit(`adminForms.response.${key}`, { status: response.status(), body });
    return body;
  };
  const openEdit = async () => {
    await expect(userRow()).toHaveCount(1);
    await userRow().getByRole("button", { name: "Edit", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Edit user", exact: true })).toBeVisible();
  };
  const reloadUser = async (role, enabled) => {
    await page.reload();
    await expect(userRow()).toHaveCount(1);
    await expect(userRow().getByText(role, { exact: true })).toBeVisible();
    await expect(userRow().getByText(enabled ? "active" : "disabled", { exact: true })).toBeVisible();
    await expect(userRow().getByText("all", { exact: true })).toBeVisible();
  };
  const serverTab = () => page.getByRole("button", { name: "Server", exact: true }).click();

  await ctx.step(`${phase}: production form and reload`, async () => {
    if (phase === "create" || phase === "cancel") {
      await page.getByRole("button", { name: "Add user", exact: true }).click();
      await expect(page.getByRole("heading", { name: "Add user", exact: true })).toBeVisible();
      await inputFor(page, "Email").fill(phase === "cancel" ? config.cancelEmail : user.email);
      await inputFor(page, "Display name").fill(user.name);
      await inputFor(page, "Temporary password").fill(user.password);
      await inputFor(page, "Role", "select").selectOption(config.initialRoleWire);
      // The modal's all control is distinct from the lowercase table filter.
      const createModal = page.getByRole("heading", { name: "Add user", exact: true }).locator("../../..");
      await createModal.getByRole("button", { name: "all", exact: true }).click();
      if (phase === "create") {
        const created = await mutate("POST", "/api/v1/admin/users",
          () => page.getByRole("button", { name: "Create", exact: true }).click(), 201, phase);
        expect(created.role).toBe(config.initialRoleWire);
        expect(created.enabled).toBe(true);
        expect(created.environments).toEqual([]);
        ctx.emit("adminForms.userId", created.id);
        await expect(page.getByRole("heading", { name: "Add user", exact: true })).toHaveCount(0);
        await reloadUser(config.initialRoleWire, true);
      } else {
        const writes = [];
        const observe = (request) => {
          if (["POST", "PATCH", "PUT", "DELETE"].includes(request.method()) &&
              new URL(request.url()).pathname.startsWith("/api/v1/admin/")) writes.push(request.method());
        };
        page.on("request", observe);
        await page.getByRole("button", { name: "Cancel", exact: true }).click();
        await expect(page.getByRole("heading", { name: "Add user", exact: true })).toHaveCount(0);
        await reloadUser(config.editedRoleWire, true);
        // Cancel an existing account's draft as well as the new-account draft.
        await openEdit();
        await inputFor(page, "Role", "select").selectOption(config.initialRoleWire);
        await page.getByRole("checkbox", { name: /Enabled/ }).uncheck();
        await page.getByRole("button", { name: "Cancel", exact: true }).click();
        await expect(page.getByRole("heading", { name: "Edit user", exact: true })).toHaveCount(0);
        await reloadUser(config.editedRoleWire, true);
        expect(writes).toEqual([]);
        page.off("request", observe);
      }
    } else if (phase === "edit" || phase === "disable") {
      await openEdit();
      if (phase === "edit") await inputFor(page, "Role", "select").selectOption(config.editedRoleWire);
      else await page.getByRole("checkbox", { name: /Enabled/ }).uncheck();
      const saved = await mutate("PATCH", `/api/v1/admin/users/${receipts["adminForms.userId"]}`,
        () => page.getByRole("button", { name: "Save changes", exact: true }).click(), 200, phase);
      expect(saved.id).toBe(receipts["adminForms.userId"]);
      expect(saved.role).toBe(config.editedRoleWire);
      expect(saved.enabled).toBe(phase !== "disable");
      expect(saved.environments).toEqual([]);
      await expect(page.getByRole("heading", { name: "Edit user", exact: true })).toHaveCount(0);
      await reloadUser(config.editedRoleWire, phase !== "disable");
    } else {
      await serverTab();
      if (phase === "retry") {
        const policy = config.retryPolicy;
        // These wrapping labels include their select's option text in the
        // accessible name. Match the authored label prefix, not that suffix.
        await page.getByLabel(/^Max build retries/).selectOption(String(policy.max_build_retries));
        await page.getByLabel(/^Max eval retries/).selectOption(String(policy.max_evaluation_retries));
        await page.getByLabel(/^Backoff between attempts/).selectOption(String(policy.backoff_seconds));
        await page.getByRole("checkbox", { name: /Only retry transient failures/ }).setChecked(policy.transient_only);
        const saved = await mutate("PUT", "/api/v1/admin/automatic-retry-policy",
          () => page.getByRole("button", { name: "Save retry config", exact: true }).click(), 200, phase);
        expect(saved).toEqual(policy);
        await page.reload();
        await serverTab();
        await expect(page.getByLabel(/^Max build retries/)).toHaveValue(String(policy.max_build_retries));
        await expect(page.getByLabel(/^Max eval retries/)).toHaveValue(String(policy.max_evaluation_retries));
        await expect(page.getByLabel(/^Backoff between attempts/)).toHaveValue(String(policy.backoff_seconds));
        await expect(page.getByRole("checkbox", { name: /Only retry transient failures/ })).toBeChecked({ checked: policy.transient_only });
      } else {
        const toggle = page.getByRole("switch");
        await expect(toggle).toBeEnabled();
        if (phase === "classification-enable") {
          await expect(toggle).toHaveAttribute("aria-checked", "false");
          const saved = await mutate("PUT", "/api/v1/admin/classification-config", () => toggle.click(), 200, phase);
          expect(saved.enabled).toBe(true);
        } else {
          await expect(toggle).toHaveAttribute("aria-checked", "true");
          await inputFor(page, "Classification level", "select").selectOption(config.classification.level);
          await page.getByPlaceholder("e.g. UNCLASSIFIED//FOUO", { exact: true }).fill(config.classification.custom_text);
          const saved = await mutate("PUT", "/api/v1/admin/classification-config",
            () => page.getByRole("button", { name: "Save banner config", exact: true }).click(), 200, phase);
          expect(saved).toEqual(config.classification);
        }
        await page.reload();
        await serverTab();
        await expect(page.getByRole("switch")).toHaveAttribute("aria-checked", "true");
        if (phase === "classification") {
          await expect(inputFor(page, "Classification level", "select")).toHaveValue(config.classification.level);
          await expect(page.getByPlaceholder("e.g. UNCLASSIFIED//FOUO", { exact: true })).toHaveValue(config.classification.custom_text);
        }
      }
    }
    await ctx.screenshot(`admin-form-${phase}-reloaded`);
    ctx.emit(`adminForms.browser.${phase}`, true);
  });
};
