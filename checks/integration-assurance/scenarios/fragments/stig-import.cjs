// Named phases only; coordinator wraps each export as browserSpecs[name].run.
// Authority: 6e4ea520 ImportStigModal review/done and PolicyEditor Compliance,
// with PoliciesView InlineMappingEditor. No draft-interchange trust assertions.
// Requires facts.stigImport from stig-import-facts.py, controller-installed XML,
// same-run delivery.adminId, and Python-verified import/mappingTarget receipts.
"use strict";

const fs = require("fs");
const crypto = require("crypto");

async function signIn(ctx) {
  const { page, expect, facts } = ctx;
  await page.goto(ctx.url("/login"));
  await page.getByPlaceholder("Enter your username", { exact: true }).fill(facts.admin.username);
  await page.getByPlaceholder("Enter your password", { exact: true }).fill(facts.admin.password);
  await page.getByRole("button", { name: "Sign In", exact: true }).click();
  await expect(page).toHaveURL(ctx.url("/"));
  const who = await ctx.whoami();
  expect(who.status).toBe(200);
  expect(who.body.is_authenticated).toBe(true);
  expect(who.body.roles).toContain("Admin");
  const coach = page.getByRole("button", { name: "Close Crystal Forge Coach", exact: true });
  if (await coach.isVisible()) await coach.click();
}

function inputBytes(ctx, fact) {
  const bytes = fs.readFileSync(fact.controllerPath);
  ctx.expect(bytes.length).toBe(fact.byteLength);
  ctx.expect(crypto.createHash("sha256").update(bytes).digest("hex")).toBe(fact.sha256);
  return bytes;
}

function responseFor(ctx, endpoint) {
  return ctx.page.waitForResponse(response => response.request().method() === "POST" &&
    new URL(response.url()).pathname === `/api/v1/compliance/xccdf/${endpoint}`);
}

async function openUpload(ctx) {
  await ctx.page.goto(ctx.url("/compliance"));
  await ctx.page.getByRole("button", { name: "Import / Export", exact: true }).click();
  await ctx.page.getByRole("menuitem", { name: "Import STIG or XCCDF (.xml/.zip)", exact: true }).click();
  await ctx.expect(ctx.page.getByRole("heading", { name: "Import STIG / XCCDF", exact: true })).toBeVisible();
}

async function preview(ctx, release) {
  const { page, expect } = ctx;
  const f = ctx.facts.stigImport.releases[release];
  inputBytes(ctx, f);
  await openUpload(ctx);
  // Observe the real loading state without delaying or replacing a CF response.
  // MutationObserver retains a transient spinner even on a fast local server.
  await page.evaluate(() => {
    window.stigPreviewLoadingSeen = false;
    window.stigPreviewObserver = new MutationObserver(() => {
      if (document.body.textContent.includes("Parsing XCCDF document…")) {
        window.stigPreviewLoadingSeen = true;
      }
    });
    window.stigPreviewObserver.observe(document.body, { childList: true, subtree: true, characterData: true });
  });
  const [response] = await Promise.all([
    responseFor(ctx, "preview"), page.locator('input[type="file"]').setInputFiles(f.controllerPath),
  ]);
  expect(response.status()).toBe(200);
  expect(response.request().headers()["content-type"]).toMatch(/^multipart\/form-data; boundary=/);
  const body = await response.json();
  expect(body.sha256).toBe(f.sha256);
  expect(body.benchmark).toMatchObject({ id: f.benchmarkId, title: f.title, version: f.benchmarkVersion });
  expect(body.rule_count).toBe(f.ruleCount);
  expect(body.profile_count).toBe(f.profileCount);
  expect(body.rules.map(rule => ({ id: rule.id, stableKey: rule.group_id,
    title: rule.title, version: rule.version, severity: rule.severity }))
    .sort((a, b) => a.id.localeCompare(b.id))).toEqual(f.rules);
  await expect(page.getByRole("heading", { name: "Review imported controls", exact: true })).toBeVisible();
  const list = page.getByTestId("xccdf-review-control-list");
  await expect(list.getByRole("button")).toHaveCount(f.ruleCount);
  await expect(page.getByText(`· ${f.ruleCount} of ${f.ruleCount} selected`, { exact: true })).toBeVisible();
  for (const stableKey of ctx.facts.stigImport.stableSubset) {
    const rule = f.rules.find(item => item.stableKey === stableKey);
    const control = list.getByRole("button").filter({ hasText: rule.title });
    await expect(control).toHaveCount(1);
    await expect(control).toContainText(stableKey);
  }
  expect(await page.evaluate(() => {
    window.stigPreviewObserver.disconnect();
    return window.stigPreviewLoadingSeen;
  })).toBe(true);
  await ctx.screenshot(`stig-${release}-preview`);
  ctx.emit(`assurance.stig.${release}.preview`, { sha256: body.sha256, ruleCount: body.rule_count });
  return f;
}

exports.previewV1R1 = async ctx => {
  await ctx.step("V1R1 real multipart preview and input-derived controls", async () => {
    await signIn(ctx);
    const writes = [];
    ctx.page.on("request", request => {
      if (request.method() === "POST" && new URL(request.url()).pathname === "/api/v1/compliance/xccdf/import") {
        writes.push(request.url());
      }
    });
    await preview(ctx, "V1R1");
    await ctx.page.getByRole("button", { name: "Discard draft", exact: true }).click();
    await ctx.page.getByRole("button", { name: "Cancel", exact: true }).click();
    ctx.expect(writes).toEqual([]);
  });
};

async function importRelease(ctx, release) {
  await ctx.step(`${release} real browser file import`, async () => {
    await signIn(ctx);
    const f = await preview(ctx, release);
    await ctx.page.getByText("Bundle name", { exact: true }).locator("..").getByRole("textbox").fill(f.bundleName);
    const [response] = await Promise.all([
      responseFor(ctx, "import"),
      ctx.page.getByRole("button", { name: "Skip & create all", exact: true }).click(),
    ]);
    ctx.expect(response.request().headers()["content-type"]).toMatch(/^multipart\/form-data; boundary=/);
    ctx.expect(response.status()).toBe(201);
    const body = await response.json();
    ctx.expect(body.source_sha256).toBe(f.sha256);
    ctx.expect(body.created_policy_count).toBe(f.ruleCount);
    ctx.expect(body.reused_policy_versions).toBe(0);
    ctx.expect(body.excluded_rule_count).toBe(0);
    ctx.expect(body.created_policy_version_ids).toHaveLength(f.ruleCount);
    await ctx.expect(ctx.page.getByRole("heading", { name: "Import complete", exact: true })).toBeVisible();
    await ctx.screenshot(`stig-${release}-import-complete`);
    ctx.emit(`assurance.stig.${release}.import`, body);
    await ctx.page.getByRole("button", { name: "Close", exact: true }).click();
  });
}

exports.importV1R1 = ctx => importRelease(ctx, "V1R1");
exports.importV1R2 = ctx => importRelease(ctx, "V1R2");

async function rejectInvalid(ctx, name) {
  await ctx.step(`${name} visibly rejected and commit endpoint rejects identical bytes`, async () => {
    await signIn(ctx);
    const f = ctx.facts.stigImport.invalid[name];
    const bytes = inputBytes(ctx, f);
    await openUpload(ctx);
    const [response] = await Promise.all([
      responseFor(ctx, "preview"), ctx.page.locator('input[type="file"]').setInputFiles(f.controllerPath),
    ]);
    ctx.expect(response.status()).toBe(422);
    await ctx.expect(ctx.page.getByText(/^Preview failed:/)).toBeVisible();
    await ctx.expect(ctx.page.getByRole("heading", { name: "Review imported controls", exact: true })).toHaveCount(0);
    await ctx.screenshot(`stig-${name}-rejected`);
    // Invalid files have no UI commit action. Exercise the real commit boundary
    // with a syntactically valid plan and this Admin context's actual CSRF cookie.
    const csrf = (await ctx.context.cookies()).find(cookie => cookie.name === "__Host-cf-csrf");
    ctx.expect(Boolean(csrf)).toBe(true);
    const commit = await ctx.context.request.post(ctx.url("/api/v1/compliance/xccdf/import"), {
      headers: { "x-csrf-token": csrf.value },
      multipart: {
        file: { name: f.filename, mimeType: "application/xml", buffer: bytes },
        plan: JSON.stringify({ expected_sha256: f.sha256, selected_profile_id: null,
          selected_rule_ids: [], rule_actions: [], bundle: { name: `Rejected ${name}`,
            framework: "xccdf", version: "1", environment_ids: [] } }),
      },
    });
    ctx.expect(commit.status()).toBe(422);
    const error = await commit.json();
    ctx.expect(error.error).not.toBe("IMPORT_PLAN_INVALID");
    ctx.emit(`assurance.stig.invalid.${name}`, { previewStatus: response.status(), importStatus: commit.status() });
  });
}

exports.invalidTruncated = ctx => rejectInvalid(ctx, "truncated");
exports.invalidNonXccdf = ctx => rejectInvalid(ctx, "non-xccdf");

async function openMappedPolicy(ctx, reload = false) {
  const t = ctx.receipts["assurance.stig.mappingTarget"];
  await ctx.page.goto(ctx.url("/deployment-policies"));
  if (reload) await ctx.page.reload();
  await ctx.page.getByRole("tab", { name: /^Security controls/ }).click();
  await ctx.page.getByRole("combobox").filter({ has: ctx.page.getByRole("option", {
    name: "Flat list (no grouping)", exact: true,
  }) }).selectOption("flat");
  await ctx.page.getByPlaceholder("Search policies…", { exact: true }).fill(t.policyName);
  const card = ctx.page.locator(`[data-policy-card][data-policy-id="${t.policyId}"]`);
  await ctx.expect(card).toHaveCount(1);
  const path = `/api/v1/policy-versions/${t.policyVersionId}/requirement-mappings`;
  const [response] = await Promise.all([
    ctx.page.waitForResponse(res => res.request().method() === "GET" && new URL(res.url()).pathname === path),
    card.getByTestId("policy-card-edit").click(),
  ]);
  ctx.expect(response.status()).toBe(200);
  const dialog = ctx.page.getByTestId("policy-editor-modal");
  await ctx.expect(dialog).toBeVisible();
  await dialog.getByTestId("policy-editor-tab-provenance").click();
  const provenance = dialog.getByTestId("policy-editor-provenance");
  const source = ctx.facts.stigImport.releases.V1R1;
  const rule = source.rules.find(item => item.stableKey === t.stableKey);
  await ctx.expect(provenance).toContainText(source.filename);
  await ctx.expect(provenance).toContainText(rule.id);
  await ctx.expect(provenance.getByRole("textbox")).toHaveCount(0);
  await dialog.getByTestId("policy-editor-tab-mappings").click();
  return dialog;
}

function mappedRow(ctx, dialog) {
  return dialog.getByTestId("policy-mapping-row").filter({ hasText: ctx.facts.stigImport.mappingRationale });
}

exports.mapping = async ctx => {
  await ctx.step("manual mapping of exact imported policy to same stable control in V1R2", async () => {
    await signIn(ctx);
    const t = ctx.receipts["assurance.stig.mappingTarget"];
    const dialog = await openMappedPolicy(ctx);
    await dialog.getByRole("button", { name: "+ Add mapping", exact: true }).click();
    await dialog.getByLabel("Framework", { exact: true }).selectOption(t.frameworkId);
    await dialog.getByLabel("Version", { exact: true }).selectOption(t.frameworkVersionId);
    await dialog.getByLabel("Requirement", { exact: true }).fill(t.externalId);
    await dialog.getByRole("button", { name: `${t.externalId} · rule · ${t.requirementTitle}`, exact: true }).click();
    await dialog.getByRole("button", { name: /^Supports The policy contributes/ }).click();
    await dialog.getByRole("button", { name: "Partial", exact: true }).click();
    await dialog.getByPlaceholder("Why this policy satisfies the requirement", { exact: true })
      .fill(ctx.facts.stigImport.mappingRationale);
    const path = `/api/v1/policy-versions/${t.policyVersionId}/requirement-mappings`;
    const [response] = await Promise.all([
      ctx.page.waitForResponse(res => res.request().method() === "POST" && new URL(res.url()).pathname === path),
      dialog.getByRole("button", { name: "Add mapping", exact: true }).click(),
    ]);
    ctx.expect(response.status()).toBe(201);
    const request = response.request().postDataJSON();
    ctx.expect(request).toMatchObject({ requirement_version_id: t.requirementVersionId,
      relationship: "supports", coverage: "partial", rationale: ctx.facts.stigImport.mappingRationale });
    await ctx.expect(mappedRow(ctx, dialog)).toHaveCount(1);
    await ctx.expect(mappedRow(ctx, dialog)).toContainText(t.externalId);
    ctx.emit("assurance.stig.mappingRequest", request);
    await ctx.screenshot("stig-manual-mapping-created");
  });
};

exports.mappingReload = async ctx => {
  await ctx.step("new browser and reload preserve the exact manual mapping", async () => {
    await signIn(ctx);
    const dialog = await openMappedPolicy(ctx, true);
    const row = mappedRow(ctx, dialog);
    await ctx.expect(row).toHaveCount(1);
    await ctx.expect(row).toContainText(ctx.receipts["assurance.stig.mappingTarget"].externalId);
    await ctx.expect(row).toContainText("Supports");
    await ctx.expect(row).toContainText("Partial");
    await ctx.expect(row.getByTestId("policy-mapping-provenance")).toHaveText("Manual mapping");
    await ctx.screenshot("stig-manual-mapping-reloaded");
    ctx.emit("assurance.stig.mappingReloaded", true);
  });
};
