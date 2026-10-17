// Access browser case: first-administrator
// registration, sign-in, and reload through the production UI.
//
// Selectors use visible text, placeholders, and roles. The production forms
// do not associate <label> elements with inputs, so labelled fields are
// located by their placeholder or by the input that follows the label.
"use strict";

function inputAfterLabel(page, label) {
  return page.getByText(label, { exact: true }).locator("xpath=following-sibling::input[1]");
}

exports.run = async (ctx) => {
  const { page, expect, facts } = ctx;
  const admin = facts.admin;

  await ctx.step("fresh installation opens first-administrator registration", async () => {
    await page.goto(ctx.url("/"));
    await expect(page).toHaveURL(/\/register$/);
    await expect(page.getByText("Administrator Registration", { exact: true })).toBeVisible();
  });

  await ctx.step("register the first administrator", async () => {
    await page.getByPlaceholder("admin", { exact: true }).fill(admin.username);
    await page.getByPlaceholder("admin@example.com", { exact: true }).fill(admin.email);
    await inputAfterLabel(page, "First Name").fill(admin.firstName);
    await inputAfterLabel(page, "Last Name").fill(admin.lastName);
    await page.getByPlaceholder("Minimum 8 characters", { exact: true }).fill(admin.password);
    await page.getByPlaceholder("Re-enter password", { exact: true }).fill(admin.password);
    await page.getByRole("button", { name: "Create Administrator Account" }).click();
  });

  await ctx.step("registration does not create a session", async () => {
    await expect(page).toHaveURL(/\/login$/);
    const who = await ctx.whoami();
    expect(who.status).toBe(200);
    expect(who.body.is_authenticated).toBe(false);
  });

  await ctx.step("sign in as the new administrator", async () => {
    await page.getByPlaceholder("Enter your username", { exact: true }).fill(admin.username);
    await page.getByPlaceholder("Enter your password", { exact: true }).fill(admin.password);
    await page.getByRole("button", { name: "Sign In" }).click();
    await expect(page).toHaveURL(ctx.url("/"));
  });

  let signedInUserId = null;
  await ctx.step("session identifies the administrator", async () => {
    const who = await ctx.whoami();
    expect(who.status).toBe(200);
    expect(who.body.is_authenticated).toBe(true);
    expect(who.body.auth_mode).toBe(facts.authMode);
    expect(who.body.user.email).toBe(admin.email);
    expect(who.body.user.display_name).toBe(admin.name);
    expect(who.body.roles).toEqual([facts.adminRoleWire]);
    signedInUserId = ctx.emit("admin.userId", who.body.user.id);
    await expect(page.getByRole("navigation").getByText(admin.name, { exact: true }).filter({ visible: true })).toBeVisible();
  });

  await ctx.step("reload keeps the authenticated shell", async () => {
    await page.reload();
    await expect(page).toHaveURL(ctx.url("/"));
    await expect(page.getByRole("navigation").getByText(admin.name, { exact: true }).filter({ visible: true })).toBeVisible();
    const who = await ctx.whoami();
    expect(who.body.is_authenticated).toBe(true);
    expect(who.body.user.id).toBe(signedInUserId);
    await ctx.screenshot("signed-in-after-reload");
  });
};
