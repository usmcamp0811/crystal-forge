// The Nix spec wrapper calls run(ctx, require(<immutable activation.cjs>)).
// Do not require a sibling: separately written Nix store files need not share
// a directory. Requires delivery.first, delivery.first.observed/retained,
// delivery.second.observed and the Admin/target receipts used by helpers.login.
// This fragment requests real rollback only. The owner observes target recovery
// independently, then calls activation.inspect(ctx, "rollback") in a fresh run.
"use strict";

/** Requests exact retained-generation rollback through the production History UI. */
async function run(ctx, helpers) {
  const { page, expect, facts, receipts } = ctx;
  if (typeof helpers?.login !== "function") {
    throw new Error("Recovery wrapper must pass activation helpers as run(ctx, helpers)");
  }
  for (const key of ["delivery.targetId", "delivery.first", "delivery.first.observed",
    "delivery.first.retained", "delivery.second.observed"]) {
    if (receipts[key] === undefined || receipts[key] === null) {
      throw new Error(`Missing prerequisite receipt: ${key}`);
    }
  }
  const targetId = receipts["delivery.targetId"];
  const intended = receipts["delivery.first"];
  const first = receipts["delivery.first.observed"];
  const retained = receipts["delivery.first.retained"];
  const second = receipts["delivery.second.observed"];
  expect(retained.rollback_eligible, "first generation remains eligible for rollback").toBe(true);
  expect(retained.generation_snapshot_id, "retained snapshot authority").toMatch(
    /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i,
  );
  expect(retained.generation).toBe(first.generation);
  expect(retained.store_path).toBe(first.storePath);
  expect(retained.store_path).toBe(intended.outputPath);
  expect(second.generation, "rollback starts from a distinct active generation").not.toBe(first.generation);
  await helpers.login(ctx);

  await ctx.step("rollback: retained first-generation History action opens exact plan", async () => {
    await page.goto(ctx.url(`/systems/${targetId}?tab=history`));
    await expect(page.getByRole("heading", { name: facts.target, exact: true })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Deployment history", exact: true })).toBeVisible();
    const card = page.locator(`[data-event-id="${first.historyEventId}"][data-deployment-id="${first.deploymentId}"]`);
    await expect(card, "unique successful first-generation History card").toHaveCount(1);
    await expect(card.getByText(intended.commit.slice(0,7), {exact:true})).toBeVisible();
    await expect(card.locator("strong")).toHaveText(`#${first.generation}`);
    const rollback = card.getByRole("button", { name: "Rollback to this generation", exact: true });
    await expect(rollback).toBeEnabled();
    await rollback.click();
    await expect(page).toHaveURL((url) => url.pathname === `/systems/${targetId}`
      && url.searchParams.get("tab") === "deploy"
      && url.searchParams.get("deploy_generation") === String(first.generation));
    const plan = page.locator("section").filter({
      has: page.getByRole("heading", { name: "Rollback plan", exact: true }),
    });
    const value = (label) => plan.locator("dt").filter({ hasText: new RegExp(`^${label}$`) })
      .locator("xpath=following-sibling::dd[1]");
    await expect(value("Target")).toHaveText(facts.target);
    await expect(value("From")).toContainText(`gen #${second.generation}`);
    await expect(value("To")).toHaveText(`gen #${retained.generation}`);
    await expect(value("Store Path")).toHaveText(retained.store_path);
    await expect(value("Commit")).toHaveText(intended.commit.slice(0, 7));
    await expect(value("Commit").getByRole("button")).toHaveAttribute("title", intended.commit);
    await expect(value("Strategy")).toHaveText("switch_to_generation");
    await expect(value("Policy")).toHaveText("manual");
    const submit = plan.getByRole("button", { name: `Switch to gen #${retained.generation}`, exact: true });
    await expect(submit).toBeEnabled();
    await submit.scrollIntoViewIfNeeded();
    await ctx.screenshot("delivery-rollback-plan");
    const path = `/api/v1/systems/${targetId}/rollback-generation`;
    const [response] = await Promise.all([
      page.waitForResponse((r) => new URL(r.url()).pathname === path && r.request().method() === "POST"),
      submit.click(),
    ]);
    const request = response.request().postDataJSON();
    expect(request, "browser must send exact retained snapshot selectors").toEqual({
      generation_snapshot_id: retained.generation_snapshot_id,
      generation: retained.generation, store_path: retained.store_path,
    });
    expect(response.status(), "rollback request acceptance HTTP status").toBe(202);
    const body = await response.json();
    expect(body).toMatchObject({ status: "accepted", message: expect.any(String) });
    ctx.emit("delivery.rollback.request", { ...body, request });
  });
}

module.exports = { run };
