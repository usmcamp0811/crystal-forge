// Owner access-token lifecycle through the production Profile UI (TASK-534.3).
// Actor: the local Operator (never Admin). Each browserSpecs entry
// access-token-{create,rotate,revoke} maps to this file; Python selects one
// phase by the write-once receipt accessTokens.request.<phase>, the phase emits
// accessTokens.browser.<phase>, and SQL is checked between phases. Every phase
// signs in independently; no cookie is imported.
//
// Requires facts: operator {username,password,roleWire}, accessTokens {environment,
// otherEnvironment, operatorToken, browserToken, lifetimeDays, readScopes,
// writeScopes, unavailableScopes}; receipts: authorization.operator.userId; the create phase
// needs the API-created Operator token to be the only existing row; rotate needs
// accessTokens.browser.createdId; revoke needs accessTokens.browser.rotate.newId.
//
// Secret handling. The reveal secret is read from the real "Token" field
// after "Show token". Only its SHA-256 hex fingerprint leaves this process, in
// accessTokens.browser.<phase>.secretSha256. Python compares it with the stored
// hash, which proves the displayed secret is the stored credential. The secret
// is never emitted, logged, put in an assertion message (assertions compare
// booleans) or captured in a screenshot: screenshots are taken while the field
// is masked, and Show is undone before the acknowledgement. A failing run keeps
// a Playwright trace that can contain the throwaway credential of a disposable VM.
//
// Screenshots are the images for design comparison. The approved design is
// docs/design/CrystalForge/crystal-forge.html (AccessTokensCard,
// CfatCreateModal, CfatReveal, CfatDetails, CfatConfirm). The maintainer
// compares the screenshots side by side on the dev server. This script makes
// semantic assertions only and claims no automated pixel baseline.
//
// Accessible names below come from packages/web-ui/src/components/
// access_tokens/{mod,create,manage}.rs. Row and drawer buttons are named
// "Details for <name>", "Rotate <name>" and "Revoke <name>". Those names are
// substrings of other accessible names, so each lookup is exact and scoped to
// its container.
"use strict";

const crypto = require("crypto");

const phases = ["create", "rotate", "revoke"];
const SECRET_FORMAT = /^cfat_v1_[0-9a-f]{32}_[A-Za-z0-9_-]{43}$/;
const ACKNOWLEDGEMENT = "I have saved the token in my agent's secret store.";
const LIST_PATH = "/api/v1/user/access-tokens";

const escapeRegExp = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const tokenPath = (response) => new URL(response.url()).pathname;
const isCall = (method, path) => (response) =>
  response.request().method() === method && tokenPath(response) === path;

async function dismissCoach(page) {
  const close = page.getByRole("button", { name: "Close Crystal Forge Coach", exact: true });
  if (await close.isVisible()) await close.click();
}

async function signInOperator(ctx, phase) {
  const { page, expect, facts, receipts } = ctx;
  await page.goto(ctx.url("/login"));
  await page.getByPlaceholder("Enter your username", { exact: true }).fill(facts.operator.username);
  await page.getByPlaceholder("Enter your password", { exact: true }).fill(facts.operator.password);
  await page.getByRole("button", { name: "Sign In", exact: true }).click();
  await expect(page).toHaveURL(ctx.url("/"));
  const who = await ctx.whoami();
  expect(who.status).toBe(200);
  expect(who.body.is_authenticated).toBe(true);
  expect(who.body.user.id).toBe(receipts["authorization.operator.userId"]);
  expect(who.body.roles).toEqual([facts.operator.roleWire]);
  ctx.emit(`accessTokens.browser.${phase}.userId`, who.body.user.id);
}

// Opens Profile as the Operator and returns the Access tokens card. The
// Operator has no administration entry anywhere in the shell.
async function openProfile(ctx) {
  const { page, expect } = ctx;
  await page.goto(ctx.url("/profile"));
  await expect(page.getByRole("heading", { name: "Profile & Preferences", exact: true })).toBeVisible();
  await dismissCoach(page);
  await expect(page.getByRole("link", { name: "Systems", exact: true })).toBeVisible();
  await expect(page.locator('a[href="/admin"]')).toHaveCount(0);
  const card = page.getByTestId("access-tokens-card");
  await expect(card).toBeVisible();
  await expect(card.getByRole("heading", { name: "Access tokens" })).toBeVisible();
  await card.scrollIntoViewIfNeeded();
  return card;
}

const rows = (card) => card.getByTestId("access-token-row");
const named = (card, name) => rows(card).filter({ hasText: name });

// Proves a secret is nowhere the browser could persist it. Only booleans
// reach an assertion message.
async function assertSecretAbsent(ctx, secret) {
  const { page, context, expect } = ctx;
  const found = await page.evaluate((value) => {
    const dump = (store) => {
      const out = [];
      for (let i = 0; i < store.length; i += 1) {
        const key = store.key(i);
        out.push(key, store.getItem(key));
      }
      return out.join("\n");
    };
    return {
      pageText: document.body.innerText.includes(value),
      pageHtml: document.documentElement.outerHTML.includes(value),
      fieldValues: [...document.querySelectorAll("input, textarea")].some((e) => e.value.includes(value)),
      localStorage: dump(window.localStorage).includes(value),
      sessionStorage: dump(window.sessionStorage).includes(value),
      url: window.location.href.includes(value),
    };
  }, secret);
  expect(found).toEqual({
    pageText: false, pageHtml: false, fieldValues: false,
    localStorage: false, sessionStorage: false, url: false,
  });
  expect((await context.cookies()).some((cookie) => cookie.value.includes(secret))).toBe(false);
}

// Drives the one-time reveal view (create or rotate). Returns after the
// acknowledgement is ticked and Done is enabled, with the field masked again.
async function reveal(ctx, phase, title) {
  const { page, expect } = ctx;
  const dialog = page.getByRole("dialog", { name: title });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("heading", { name: title })).toBeVisible();
  await expect(dialog.getByText("This is the only time the secret will be displayed")).toBeVisible();
  const done = dialog.getByRole("button", { name: "Done", exact: true });
  await expect(done).toBeDisabled();
  const field = dialog.getByLabel("Token", { exact: true });
  await expect(field).toHaveValue(/•/);
  await ctx.screenshot(`access-token-${phase}-reveal-masked`);

  await dialog.getByRole("button", { name: "Show token", exact: true }).click();
  await expect(dialog.getByRole("button", { name: "Hide token", exact: true })).toBeVisible();
  const secret = await field.inputValue();
  expect(SECRET_FORMAT.test(secret)).toBe(true);
  const fingerprint = crypto.createHash("sha256").update(secret, "utf8").digest("hex");
  ctx.emit(`accessTokens.browser.${phase}.secretSha256`, fingerprint);
  await dialog.getByRole("button", { name: "Hide token", exact: true }).click();
  await expect(field).toHaveValue(/•/);

  // Done stays disabled until the acknowledgement is ticked.
  await expect(done).toBeDisabled();
  await dialog.getByRole("checkbox", { name: ACKNOWLEDGEMENT, exact: true }).check();
  await expect(done).toBeEnabled();
  return { dialog, done, secret };
}

async function finishReveal(ctx, { dialog, done, secret }) {
  await done.click();
  await ctx.expect(ctx.page.getByRole("dialog")).toHaveCount(0);
  await assertSecretAbsent(ctx, secret);
}

async function expectNoStore(response) {
  // Every token response carries the secret or concerns it; none may be cached.
  const cacheControl = response.headers()["cache-control"] || "";
  if (!cacheControl.includes("no-store")) {
    throw new Error(`${response.request().method()} ${tokenPath(response)} lacks Cache-Control: no-store`);
  }
}

async function createPhase(ctx) {
  const { page, expect, facts } = ctx;
  const t = facts.accessTokens;
  let card;

  await ctx.step("Operator opens Profile; the card lists only the API-created token", async () => {
    await signInOperator(ctx, "create");
    card = await openProfile(ctx);
    await expect(card.getByRole("table", { name: "Access tokens" })).toBeVisible();
    await expect(rows(card)).toHaveCount(1);
    await expect(rows(card).first()).toContainText(t.operatorToken);
    await expect(rows(card).first().getByText("Active", { exact: true })).toBeVisible();
    await ctx.screenshot("access-tokens-card-with-list");
  });

  let created;
  await ctx.step("create modal offers only the assigned environment and read scopes", async () => {
    await dismissCoach(page);
    // An empty list shows a second Create button in the empty state; the
    // header button comes first in document order.
    await card.getByRole("button", { name: "Create access token", exact: true }).first().click();
    const dialog = page.getByRole("dialog", { name: "Create access token" });
    await expect(dialog).toBeVisible();
    await dialog.getByLabel("Token name *", { exact: true }).fill(t.browserToken);
    // Expiration is a radiogroup; aria-checked marks the selected lifetime.
    const expiration = dialog.getByRole("radiogroup", { name: "Expiration *", exact: true });
    await expect(expiration.getByRole("radio", { name: `${t.lifetimeDays} days`, exact: true }))
      .toHaveAttribute("aria-checked", "true");
    await expect(dialog.getByRole("checkbox", { name: t.otherEnvironment, exact: true })).toHaveCount(0);
    const environment = dialog.getByRole("checkbox", { name: t.environment, exact: true });
    await expect(environment).not.toBeChecked();
    await environment.check();
    for (const scope of t.readScopes) {
      await expect(dialog.getByRole("checkbox", { name: new RegExp(escapeRegExp(scope)) })).toBeChecked();
    }
    // Write scopes are selectable for an Operator but never preselected.
    for (const scope of t.writeScopes) {
      const box = dialog.getByRole("checkbox", { name: new RegExp(escapeRegExp(scope)) });
      await expect(box).toBeEnabled();
      await expect(box).not.toBeChecked();
    }
    for (const scope of t.unavailableScopes) {
      const box = dialog.getByRole("checkbox", { name: new RegExp(escapeRegExp(scope)) });
      await expect(box).toBeDisabled();
      await expect(box).not.toBeChecked();
    }
    // The tile shows only the reason, one per unreleased scope.
    await expect(dialog.getByText("Not supported yet", { exact: true })).toHaveCount(t.unavailableScopes.length);
    await ctx.screenshot("access-token-create-modal");

    const [response] = await Promise.all([
      page.waitForResponse(isCall("POST", LIST_PATH)),
      dialog.getByRole("button", { name: "Create token", exact: true }).click(),
    ]);
    expect(response.status()).toBe(201);
    await expectNoStore(response);
    created = (await response.json()).id;
    expect(typeof created).toBe("string");
    ctx.emit("accessTokens.browser.createdId", created);
  });

  await ctx.step("reveal shows the secret once; Done needs the acknowledgement", async () => {
    const view = await reveal(ctx, "create", "Access token created");
    await finishReveal(ctx, view);
    // The embedded public id is the id the server returned.
    expect(view.secret.startsWith(`cfat_v1_${created.replace(/-/g, "")}_`)).toBe(true);
    // The row appears with its status text and the chosen grant.
    const row = named(card, t.browserToken);
    await expect(row).toHaveCount(1);
    await expect(row.getByText("Active", { exact: true })).toBeVisible();
    await expect(row).toContainText(t.environment);
    await expect(row).toContainText(`Read · ${t.readScopes.length}`);
    await expect(rows(card)).toHaveCount(2);

    await ctx.step("details drawer shows the granted and effective access", async () => {
      await row.getByRole("button", { name: `Details for ${t.browserToken}`, exact: true }).click();
      const drawer = page.getByRole("dialog", { name: t.browserToken, exact: true });
      await expect(drawer).toBeVisible();
      const section = (title) => drawer.locator("section").filter({
        has: page.getByRole("heading", { name: title, exact: true }),
      });
      // Both sections render the environment as a chip. Only the granted
      // section lists scopes; the effective section lists environments and
      // names a scope only when the role no longer permits it.
      for (const title of ["Granted at creation", "Effective now"]) {
        await expect(section(title).getByText(t.environment, { exact: true })).toBeVisible();
        for (const scope of [...t.writeScopes, ...t.unavailableScopes]) {
          await expect(section(title).getByText(scope, { exact: true })).toHaveCount(0);
        }
      }
      for (const scope of t.readScopes) {
        await expect(section("Granted at creation").getByText(scope, { exact: true })).toBeVisible();
      }
      await expect(section("Effective now").getByText(/^Unavailable:/)).toHaveCount(0);
      await expect(section("Effective now").getByText("Not permitted by your current role")).toHaveCount(0);
      await expect(section("Recent activity").getByText(/^Created by you from Profile ·/)).toHaveCount(1);
      await expect(drawer.getByRole("button", { name: `Rotate ${t.browserToken}`, exact: true })).toBeVisible();
      await expect(drawer.getByRole("button", { name: `Revoke ${t.browserToken}`, exact: true })).toBeVisible();
      await ctx.screenshot("access-token-details-drawer");
      await drawer.getByRole("button", { name: "Close details", exact: true }).click();
      await expect(page.getByRole("dialog")).toHaveCount(0);
    });

    // The secret does not return after a reload.
    await page.reload();
    card = await openProfile(ctx);
    await expect(named(card, t.browserToken)).toHaveCount(1);
    await expect(named(card, t.browserToken).getByText("Active", { exact: true })).toBeVisible();
    await assertSecretAbsent(ctx, view.secret);
  });

  ctx.emit("accessTokens.browser.create", true);
}

async function rotatePhase(ctx) {
  const { page, expect, facts, receipts } = ctx;
  const t = facts.accessTokens;
  const original = receipts["accessTokens.browser.createdId"];
  expect(typeof original).toBe("string");
  let card;

  await ctx.step("Operator rotates the browser-created token after confirming", async () => {
    await signInOperator(ctx, "rotate");
    card = await openProfile(ctx);
    await expect(named(card, t.browserToken)).toHaveCount(1);
    await expect(named(card, t.browserToken).getByText("Active", { exact: true })).toBeVisible();
    await dismissCoach(page);
    await card.getByRole("button", { name: `Rotate ${t.browserToken}`, exact: true }).click();
    // Scope to the dialog: its confirm button is "Rotate token", not the row
    // button "Rotate <name>".
    const confirm = page.getByRole("dialog", { name: `Rotate "${t.browserToken}"?` });
    await expect(confirm).toBeVisible();
    await ctx.screenshot("access-token-rotate-confirm");
    const [response] = await Promise.all([
      page.waitForResponse(isCall("POST", `${LIST_PATH}/${original}/rotate`)),
      confirm.getByRole("button", { name: "Rotate token", exact: true }).click(),
    ]);
    expect(response.status()).toBe(201);
    await expectNoStore(response);
    const body = await response.json();
    expect(body.replaced_token_id).toBe(original);
    expect(body.id === original).toBe(false);
    expect(body.name).toBe(t.browserToken);
    ctx.emit("accessTokens.browser.rotate.newId", body.id);
  });

  await ctx.step("rotated reveal shows the replacement secret once", async () => {
    const view = await reveal(ctx, "rotate", "Access token rotated");
    await expect(view.dialog.getByText(/was revoked;/)).toBeVisible();
    await finishReveal(ctx, view);
  });

  await ctx.step("list shows the old row Revoked and the replacement Active", async () => {
    const mine = named(card, t.browserToken);
    await expect(mine).toHaveCount(2);
    await expect(mine.getByText("Revoked", { exact: true })).toHaveCount(1);
    await expect(mine.getByText("Active", { exact: true })).toHaveCount(1);
    // Only the replacement still offers Rotate and Revoke.
    await expect(page.getByRole("button", { name: `Rotate ${t.browserToken}`, exact: true })).toHaveCount(1);
    await expect(page.getByRole("button", { name: `Revoke ${t.browserToken}`, exact: true })).toHaveCount(1);
  });

  ctx.emit("accessTokens.browser.rotate", true);
}

async function revokePhase(ctx) {
  const { page, expect, facts, receipts } = ctx;
  const t = facts.accessTokens;
  const replacement = receipts["accessTokens.browser.rotate.newId"];
  expect(typeof replacement).toBe("string");
  let card;

  await ctx.step("Operator revokes the replacement after confirming", async () => {
    await signInOperator(ctx, "revoke");
    card = await openProfile(ctx);
    const mine = named(card, t.browserToken);
    await expect(mine).toHaveCount(2);
    await expect(mine.getByText("Revoked", { exact: true })).toHaveCount(1);
    await expect(mine.getByText("Active", { exact: true })).toHaveCount(1);
    await dismissCoach(page);
    await card.getByRole("button", { name: `Revoke ${t.browserToken}`, exact: true }).click();
    // Scope to the dialog: its confirm button is "Revoke token", not the row
    // button "Revoke <name>".
    const confirm = page.getByRole("dialog", { name: `Revoke "${t.browserToken}"?` });
    await expect(confirm).toBeVisible();
    await ctx.screenshot("access-token-revoke-dialog");
    const [response] = await Promise.all([
      page.waitForResponse(isCall("DELETE", `${LIST_PATH}/${replacement}`)),
      confirm.getByRole("button", { name: "Revoke token", exact: true }).click(),
    ]);
    expect(response.status()).toBe(200);
    await expectNoStore(response);
    const body = await response.json();
    expect(body.id).toBe(replacement);
    expect(body.status).toBe("revoked");
  });

  await ctx.step("both rows read Revoked, with and without a reload", async () => {
    const assertRevoked = async () => {
      const mine = named(card, t.browserToken);
      await expect(mine).toHaveCount(2);
      await expect(mine.getByText("Revoked", { exact: true })).toHaveCount(2);
      await expect(mine.getByText("Active", { exact: true })).toHaveCount(0);
      await expect(page.getByRole("button", { name: `Rotate ${t.browserToken}`, exact: true })).toHaveCount(0);
      await expect(page.getByRole("button", { name: `Revoke ${t.browserToken}`, exact: true })).toHaveCount(0);
      // Revoking one token leaves the API-created token active.
      await expect(named(card, t.operatorToken).getByText("Active", { exact: true })).toBeVisible();
    };
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await assertRevoked();
    await ctx.screenshot("access-tokens-card-after-revoke");
    await page.reload();
    card = await openProfile(ctx);
    await assertRevoked();
  });

  ctx.emit("accessTokens.browser.revoke", true);
}

const handlers = { create: createPhase, rotate: rotatePhase, revoke: revokePhase };

exports.run = async (ctx) => {
  const { expect, receipts } = ctx;
  const pending = phases.filter((phase) =>
    receipts[`accessTokens.request.${phase}`] && !receipts[`accessTokens.browser.${phase}`]);
  expect(pending).toHaveLength(1);
  await handlers[pending[0]](ctx);
};
