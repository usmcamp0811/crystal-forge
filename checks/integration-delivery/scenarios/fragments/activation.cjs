// Composed by owner-local Nix wrappers using the pinned cf-browser runtime.
// Each deploy/inspect wrapper must receive a fresh cf-browser context. No login
// state crosses runs. Requires facts.admin/target, delivery.adminId/targetId,
// delivery.<phase> build receipts, and (for inspect) independently collected
// delivery.<phase>.observed target/SQL receipts. Rollback uses delivery.first
// as its intended artifact. A 202 receipt proves acceptance, never activation.
"use strict";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function receipt(ctx, key) {
  const value = ctx.receipts[key];
  if (value === undefined || value === null) throw new Error(`Missing prerequisite receipt: ${key}`);
  return value;
}

function artifact(ctx, phase) {
  if (!["first", "second", "rollback"].includes(phase)) {
    throw new Error(`Unknown delivery phase: ${phase}`);
  }
  const value = receipt(ctx, `delivery.${phase === "rollback" ? "first" : phase}`);
  ctx.expect(value.commit, `${phase}: intended full commit`).toMatch(/^[0-9a-f]{40}$/);
  ctx.expect(value.outputPath, `${phase}: built output path`).toMatch(/^\/nix\/store\//);
  ctx.expect(value.message, `${phase}: authored commit message`).toEqual(ctx.expect.any(String));
  ctx.expect(value.message.length).toBeGreaterThan(0);
  return value;
}

/** Signs in the same-run Admin through production fields in a fresh runner context. */
async function login(ctx) {
  const { page, expect, facts } = ctx;
  const adminId = receipt(ctx, "delivery.adminId");
  await ctx.step("Delivery: fresh-context authorized Admin sign-in", async () => {
    await page.goto(ctx.url("/login"));
    const before = await ctx.whoami();
    expect(before.status).toBe(200);
    expect(before.body.is_authenticated, "wrapper must supply a fresh browser context").toBe(false);
    await page.getByPlaceholder("Enter your username", { exact: true }).fill(facts.admin.username);
    await page.getByPlaceholder("Enter your password", { exact: true }).fill(facts.admin.password);
    await page.getByRole("button", { name: "Sign In", exact: true }).click();
    await expect(page).toHaveURL(ctx.url("/"));
    const who = await ctx.whoami();
    expect(who.status).toBe(200);
    expect(who.body.is_authenticated).toBe(true);
    expect(who.body.user.id).toBe(adminId);
    // Admin supplies the authorized operator capability for these actions.
    expect(who.body.roles).toEqual(["Admin"]);
    await expect(page.getByRole("link", { name: "Systems", exact: true })).toBeVisible();
    const coach = page.getByRole("button", { name: "Close Crystal Forge Coach", exact: true });
    if (await coach.isVisible()) await coach.click();
  });
}

/** Requests the exact authored commit and records the real browser POST/202 pair. */
async function deploy(ctx, phase) {
  const { page, expect, facts } = ctx;
  const intended = artifact(ctx, phase);
  const targetId = receipt(ctx, "delivery.targetId");
  const short = intended.commit.slice(0, 7);
  await login(ctx);
  await ctx.step(`${phase}: select authored commit and verify deployment plan`, async () => {
    await page.goto(ctx.url(`/systems/${targetId}?tab=deploy`));
    await expect(page.getByRole("heading", { name: facts.target, exact: true })).toBeVisible();
    await page.getByRole("button", { name: "New commit", exact: true }).click();
    const targets = page.locator("section").filter({
      has: page.getByRole("heading", { name: "Select target", exact: true }),
    });
    // Click the outer button. Its SHA span opens Flakes and stops propagation.
    const commit = targets.getByRole("button").filter({
      has: page.getByText(intended.message, { exact: true }),
    }).filter({ has: page.getByText(short, { exact: true }) });
    await expect(commit).toHaveCount(1);
    await commit.click();
    const plan = page.locator("section").filter({
      has: page.getByRole("heading", { name: "Deployment plan", exact: true }),
    });
    // The plan uses dt/dd pairs without accessible labels on their values.
    const value = (label) => plan.locator("dt").filter({ hasText: new RegExp(`^${label}$`) })
      .locator("xpath=following-sibling::dd[1]");
    await expect(value("Target")).toHaveText(facts.target);
    await expect(value("To")).toHaveText(short);
    await expect(value("Strategy")).toHaveText("immediate_persist");
    await expect(value("Policy")).toHaveText("manual");
    const submit = plan.getByRole("button", { name: `Deploy ${short}`, exact: true });
    await expect(submit).toBeEnabled();
    await submit.scrollIntoViewIfNeeded();
    await ctx.screenshot(`delivery-${phase}-deployment-plan`);
    const path = `/api/v1/systems/${targetId}/deploy`;
    const [response] = await Promise.all([
      page.waitForResponse((r) => new URL(r.url()).pathname === path && r.request().method() === "POST"),
      submit.click(),
    ]);
    const request = response.request().postDataJSON();
    expect(request, `${phase}: exact browser deployment intent`).toEqual({
      commit_sha: intended.commit, action: "deploy", request_id: expect.stringMatching(UUID),
    });
    expect(response.status(), `${phase}: deployment acceptance HTTP status`).toBe(202);
    const body = await response.json();
    expect(body).toMatchObject({
      status: "accepted", policy: "manual", conversion: "not_requested",
      deployment: "queued", deployment_id: expect.stringMatching(UUID),
      message: expect.any(String),
    });
    ctx.emit(`delivery.${phase}.request`, { ...body, request });
  });
}

/** Verifies observed activation and durable production history, including reload. */
async function inspect(ctx, phase) {
  const { page, expect, facts } = ctx;
  const intended = artifact(ctx, phase);
  const observed = receipt(ctx, `delivery.${phase}.observed`);
  const targetId = receipt(ctx, "delivery.targetId");
  expect(observed.generation, `${phase}: independently observed generation`).toEqual(expect.any(Number));
  expect(observed.storePath, `${phase}: actual target runs intended output`).toBe(intended.outputPath);
  expect(observed.deploymentId, `${phase}: independently correlated deployment`).toMatch(UUID);
  if (phase !== "rollback") {
    expect(observed.deploymentId).toBe(receipt(ctx, `delivery.${phase}.request`).deployment_id);
  }
  await login(ctx);
  const api = `/api/v1/systems/${targetId}`;
  const read = (suffix) => page.waitForResponse((r) =>
    new URL(r.url()).pathname === `${api}/${suffix}` && r.request().method() === "GET");
  let successEventId;

  // Observe the UI's own production reads on both loads. Never seed history or
  // synthesize completion; the owner must collect real activation before this.
  for (const reload of [false, true]) {
    await ctx.step(`${phase}: ${reload ? "reload preserves" : "read"} observed activation and success history`, async () => {
      const [commitsResponse, generationsResponse, historyResponse] = await Promise.all([
        read("commits"), read("generations"), read("history"),
        reload ? page.reload() : page.goto(ctx.url(`/systems/${targetId}?tab=history`)),
      ]);
      for (const response of [commitsResponse, generationsResponse, historyResponse]) {
        expect(response.status(), `production GET ${new URL(response.url()).pathname}`).toBe(200);
      }
      const commits = await commitsResponse.json();
      const generations = await generationsResponse.json();
      const history = await historyResponse.json();
      expect(commits.current_commit, `${phase}: observed full current commit`).toBe(intended.commit);
      expect(generations.current_generation, `${phase}: observed current generation`).toBe(observed.generation);
      expect(generations.generations).toEqual(expect.arrayContaining([expect.objectContaining({
        generation: observed.generation, store_path: observed.storePath,
        commit_hash: intended.commit, is_current: true,
      })]));
      expect(Array.isArray(history), "production history must be an array").toBe(true);
      expect(history.length, `${phase}: real history is nonempty`).toBeGreaterThan(0);
      const successes = history.filter((event) => event.event_type === "cf_deployment_succeeded"
        && event.deployment_id === observed.deploymentId);
      expect(successes, `${phase}: exact deployment has one durable success event`).toHaveLength(1);
      expect(successes[0]).toMatchObject({
        new_store_path: observed.storePath, generation: observed.generation,
        new_generation: observed.generation, commit_hash: intended.commit, outcome: "succeeded",
      });
      expect(successes[0].id, `${phase}: durable success event identity`).toMatch(UUID);
      expect(successes[0].id, `${phase}: independently persisted history event`).toBe(observed.historyEventId);
      if (reload) expect(successes[0].id, `${phase}: same success event survives reload`).toBe(successEventId);
      else successEventId = successes[0].id;
      await expect(page.getByRole("heading", { name: facts.target, exact: true })).toBeVisible();
      // The metric has no role or test ID. Narrow by its exact direct label.
      const metric = page.getByText("Generation", { exact: true }).locator("..");
      await expect(metric.getByText(`#${observed.generation}`, { exact: true })).toBeVisible();
      await expect(page.getByRole("heading", { name: "Deployment history", exact: true })).toBeVisible();
      // DeployRow has no card role or event ID. Its Deployed label is inside
      // the header, whose parent is the card. Narrow before choosing a card;
      // rollback can legitimately repeat a previous commit/generation pair.
      const cards = page.locator("#sd-history-scroll").getByText("Deployed", { exact: true })
        .locator("..").locator("..")
        .filter({ has: page.getByText(intended.commit.slice(0, 7), { exact: true }) })
        .filter({ has: page.locator("strong").filter({ hasText: new RegExp(`^#${observed.generation}$`) }) })
        .filter({ has: page.getByText("success", { exact: true }) });
      const exactCards = observed.previousGeneration == null ? cards : cards.filter({
        has: page.getByText(`#${observed.previousGeneration}`, { exact: true }),
      });
      await expect(exactCards, `${phase}: exact previous/current generation transition`).toHaveCount(1);
      await expect(exactCards).toBeVisible();
    });
  }
  await ctx.screenshot(`delivery-${phase}-history-after-reload`);
  ctx.emit(`delivery.${phase}.browserVerified`, true);
}

module.exports = { login, deploy, inspect };
