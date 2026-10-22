// Runtime: append environment-forms.py after the existing 23 Access cases.
// Each environment-form-<phase> browserSpecs entry maps to this file. Python
// selects one phase by request receipt and checks SQL before advancing. Each
// invocation signs in independently; no browser session is imported.
// Authority: TASK-490.3 and approved EnvironmentsView.jsx EnvFormModal/
// DeleteEnvConfirm and AdminView.jsx AdminUserModal at source 77251b04.
// Optional-gate defaults use approved EnvironmentsView.jsx at 6e4ea520:
// gatePolicyIds starts empty; the global CF-agent invariant is not assigned.
// These are functional assertions. Screenshots do not assert visual parity.
//
// Facts interface (JSON): environmentForms: {
//   create, edit, cancel, duplicate: EnvironmentDraft,
//   cancelName: string,
//   optionalPolicy: {id: string(UUID), name: string, policy_type: string},
//   invalidPolicyIds: string(UUID)[] (legacy invariant and absent identity)
// }; EnvironmentDraft = {name: string, description: string, colorName: string,
//   color_hex: string (#RRGGBB, uppercase persisted expectation),
//   default_policy: "manual"|"auto_latest"|"pinned", auto_sync: boolean,
//   requires_approval: boolean, is_production: boolean}.
// Also requires admin:{username,password}, adminRoleWire:string,
// operator:{email,roleWire}, admin.userId:string(UUID), and
// authorization.operator.userId:string(UUID) from the same scenario.
// Receipts: environmentForms.request.<phase>:true selects a phase;
// environmentForms.browser.<phase>:true closes it. create emits
// environmentForms.environmentId:string(UUID). Mutations emit
// environmentForms.response.<phase>:{status:number,body:object|null}.
// policyRequest.{create,optional,edit}:{required_policy_ids:string(UUID)[]}
// and policyResponse.<phase>:{status:number,body:object} retain the real second
// request and response. Both also have the environmentForms. prefix.
// Phases: create, optional, edit, cancel, duplicate, membership, membership-restore,
// delete. Membership restoration MUST run before later preferences cases.
"use strict";

const phases = {
  create: createEnvironment,
  optional: assignOptionalEnvironmentPolicy,
  edit: editEnvironment,
  cancel: cancelEnvironment,
  duplicate: duplicateEnvironment,
  membership: assignEnvironmentMembership,
  "membership-restore": restoreEnvironmentMembership,
  delete: deleteEnvironment,
};
const policyLabels = { manual: "Manual", auto_latest: "Auto latest", pinned: "Pinned" };

async function signIn(ctx, path) {
  const { page, expect, facts, receipts } = ctx;
  await page.goto(ctx.url("/login"));
  await page.getByPlaceholder("Enter your username", { exact: true }).fill(facts.admin.username);
  await page.getByPlaceholder("Enter your password", { exact: true }).fill(facts.admin.password);
  await page.getByRole("button", { name: "Sign In", exact: true }).click();
  await expect(page).toHaveURL(ctx.url("/"));
  const who = await ctx.whoami();
  expect(who.status).toBe(200);
  expect(who.body.user.id).toBe(receipts["admin.userId"]);
  expect(who.body.roles).toEqual([facts.adminRoleWire]);
  await page.goto(ctx.url(path));
  await expect(page.getByRole("heading", {
    name: path === "/admin" ? "Server Management" : "Environments", exact: true,
  })).toBeVisible();
  const coach = page.getByRole("button", { name: "Close Crystal Forge Coach", exact: true });
  if (await coach.isVisible()) await coach.click();
  if (path === "/environments") await table(ctx);
  else await expect(page.getByRole("table")).toBeVisible();
}

async function table({ page, expect }) {
  await expect(page.getByRole("heading", { name: "Environments", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Table", exact: true }).click();
  await expect(page.getByText("Loading environments…", { exact: true })).toHaveCount(0);
}

function row(ctx, name) {
  // The production badge shares the name's text container. Keep identity
  // exact while allowing that badge, including after the production edit.
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return ctx.page.getByRole("row").filter({
    has: ctx.page.getByText(new RegExp(`^${escaped}(?:\\s*PROD)?$`)),
  });
}

// Production modal headings lack dialog roles. Scope to their immediate modal
// container so the page's identically named Add/Remove buttons cannot match.
function form(ctx, edit = false) {
  return ctx.page.getByRole("heading", {
    name: edit ? /^Edit / : "Add environment", exact: !edit,
  }).locator("../..");
}

async function openCreate(ctx) {
  await ctx.page.getByRole("button", { name: "Add environment", exact: true }).click();
  await ctx.expect(form(ctx)).toBeVisible();
  return form(ctx);
}

async function openEdit(ctx, name) {
  await ctx.expect(row(ctx, name)).toHaveCount(1);
  await row(ctx, name).getByRole("button", { name: "Edit", exact: true }).click();
  const modal = form(ctx, true);
  await ctx.expect(modal).toBeVisible();
  // Edit cannot save until authoritative assignment loading has completed.
  await ctx.expect(modal.getByRole("button", { name: "Save changes", exact: true })).toBeEnabled();
  return modal;
}

async function fillDraft(modal, draft) {
  await modal.getByPlaceholder("e.g. production", { exact: true }).fill(draft.name);
  await modal.getByPlaceholder("What this tier is for", { exact: true }).fill(draft.description);
  await modal.getByRole("button", { name: draft.colorName, exact: true }).click();
  await modal.getByRole("button", { name: policyLabels[draft.default_policy], exact: true }).click();
  await modal.getByRole("checkbox", { name: "Auto-sync flakes", exact: true }).setChecked(draft.auto_sync);
  await modal.getByRole("checkbox", { name: "Require approval before deploy", exact: true }).setChecked(draft.requires_approval);
  await modal.getByRole("checkbox", { name: /^Production environment/ }).setChecked(draft.is_production);
}

async function assertDraft(ctx, modal, draft) {
  await ctx.expect(modal.getByPlaceholder("e.g. production", { exact: true })).toHaveValue(draft.name);
  await ctx.expect(modal.getByPlaceholder("What this tier is for", { exact: true })).toHaveValue(draft.description);
  await ctx.expect(modal.getByText(new RegExp(`^${draft.color_hex}$`, "i"))).toBeVisible();
  // Deployment buttons expose no pressed state. Their active class is the
  // only selected-state marker; the table also proves the policy after reload.
  await ctx.expect(modal.getByRole("button", { name: policyLabels[draft.default_policy], exact: true })).toHaveClass(/\bactive\b/);
  await ctx.expect(modal.getByRole("checkbox", { name: "Auto-sync flakes", exact: true })).toBeChecked({ checked: draft.auto_sync });
  await ctx.expect(modal.getByRole("checkbox", { name: "Require approval before deploy", exact: true })).toBeChecked({ checked: draft.requires_approval });
  await ctx.expect(modal.getByRole("checkbox", { name: /^Production environment/ })).toBeChecked({ checked: draft.is_production });
}

async function mutate(ctx, phase, method, path, trigger, status) {
  const [response] = await Promise.all([
    ctx.page.waitForResponse((r) => new URL(r.url()).pathname === path && r.request().method() === method),
    trigger(),
  ]);
  ctx.expect(response.status()).toBe(status);
  const body = status === 204 ? null : await response.json();
  ctx.emit(`environmentForms.response.${phase}`, { status: response.status(), body });
  return body;
}

async function saveEnvironment(ctx, phase, method, path, trigger, status, ids) {
  // Observe the actual second request before triggering Save. Metadata 201/200
  // alone is not a successful form save and does not prove optional assignments.
  const policyResponse = ctx.page.waitForResponse((r) =>
    /^\/api\/v1\/environments\/[^/]+\/policies$/.test(new URL(r.url()).pathname) &&
    r.request().method() === "PATCH");
  const saved = await mutate(ctx, phase, method, path, trigger, status);
  const response = await policyResponse;
  ctx.expect(new URL(response.url()).pathname).toBe(`/api/v1/environments/${saved.id}/policies`);
  const request = response.request().postDataJSON();
  ctx.expect(request).toEqual({ required_policy_ids: ids });
  ctx.emit(`environmentForms.policyRequest.${phase}`, request);
  ctx.expect(response.status()).toBe(200);
  const body = await response.json();
  ctx.expect(body.required_policy_ids).toEqual(ids);
  ctx.emit(`environmentForms.policyResponse.${phase}`, { status: response.status(), body });
  return saved;
}

async function reloadEnvironment(ctx, draft, policyIds = [ctx.facts.environmentForms.optionalPolicy.id]) {
  await ctx.page.reload();
  await table(ctx);
  await ctx.expect(row(ctx, draft.name)).toHaveCount(1);
  await ctx.expect(row(ctx, draft.name).getByText(draft.description, { exact: true })).toBeVisible();
  await ctx.expect(row(ctx, draft.name).getByText(draft.default_policy, { exact: true })).toBeVisible();
  const modal = await openEdit(ctx, draft.name);
  await assertDraft(ctx, modal, draft);
  await ctx.expect(modal.getByRole("button", { name: "Remove gate policy", exact: true })).toHaveCount(policyIds.length);
  if (policyIds.length) {
    await ctx.expect(modal.getByRole("button", { name: "Remove gate policy", exact: true })
      .locator("..")).toHaveText(ctx.facts.environmentForms.optionalPolicy.name);
  }
  await modal.getByRole("button", { name: "Cancel", exact: true }).click();
  await ctx.expect(form(ctx, true)).toHaveCount(0);
}

function observeWrites(page) {
  const writes = [];
  const observe = (request) => {
    const path = new URL(request.url()).pathname;
    if (["POST", "PUT", "PATCH", "DELETE"].includes(request.method()) &&
        (path.startsWith("/api/v1/environments") || path.startsWith("/api/v1/admin/"))) {
      writes.push({ method: request.method(), path });
    }
  };
  page.on("request", observe);
  return { writes, stop: () => page.off("request", observe) };
}

async function createEnvironment(ctx) {
  const draft = ctx.facts.environmentForms.create;
  await signIn(ctx, "/environments");
  await ctx.expect(row(ctx, draft.name)).toHaveCount(0);
  const modal = await openCreate(ctx);
  await fillDraft(modal, draft);
  await ctx.expect(modal.getByRole("button", { name: "Remove gate policy", exact: true })).toHaveCount(0);
  const saved = await saveEnvironment(ctx, "create", "POST", "/api/v1/environments",
    () => modal.getByRole("button", { name: "Add environment", exact: true }).click(), 201, []);
  ctx.expect(saved.name).toBe(draft.name);
  ctx.expect(saved.id).toMatch(/^[0-9a-f-]{36}$/i);
  ctx.emit("environmentForms.environmentId", saved.id);
  await ctx.expect(form(ctx)).toHaveCount(0);
  await reloadEnvironment(ctx, draft, []);
}

async function assignOptionalEnvironmentPolicy(ctx) {
  const config = ctx.facts.environmentForms;
  const catalogResponse = ctx.page.waitForResponse((r) =>
    new URL(r.url()).pathname === "/api/v1/policies" && r.request().method() === "GET");
  await signIn(ctx, "/environments");
  // Verify the real catalog identity and eligibility; selection labels alone
  // cannot authorize an assignment or turn a legacy built-in into a gate.
  const catalog = await catalogResponse;
  ctx.expect(catalog.status()).toBe(200);
  const policy = (await catalog.json()).find((p) => p.id === config.optionalPolicy.id);
  ctx.expect(policy).toMatchObject({ ...config.optionalPolicy, enabled: true });
  ctx.expect(policy.policy_type).not.toBe("require_cf_agent");
  const modal = await openEdit(ctx, config.create.name);
  await ctx.expect(modal.getByRole("button", { name: "Remove gate policy", exact: true })).toHaveCount(0);
  await modal.getByRole("button", { name: config.optionalPolicy.name, exact: true }).click();
  await saveEnvironment(ctx, "optional", "PATCH", `/api/v1/environments/${ctx.receipts["environmentForms.environmentId"]}`,
    () => modal.getByRole("button", { name: "Save changes", exact: true }).click(), 200, [config.optionalPolicy.id]);
  await ctx.expect(form(ctx, true)).toHaveCount(0);
  await reloadEnvironment(ctx, config.create);
}

async function editEnvironment(ctx) {
  const config = ctx.facts.environmentForms;
  await signIn(ctx, "/environments");
  const modal = await openEdit(ctx, config.create.name);
  await fillDraft(modal, config.edit);
  await ctx.expect(modal.getByRole("button", { name: "Remove gate policy", exact: true })).toHaveCount(1);
  const saved = await saveEnvironment(ctx, "edit", "PATCH", `/api/v1/environments/${ctx.receipts["environmentForms.environmentId"]}`,
    () => modal.getByRole("button", { name: "Save changes", exact: true }).click(), 200, [config.optionalPolicy.id]);
  ctx.expect(saved.id).toBe(ctx.receipts["environmentForms.environmentId"]);
  ctx.expect(saved.name).toBe(config.edit.name);
  await ctx.expect(form(ctx, true)).toHaveCount(0);
  await reloadEnvironment(ctx, config.edit);
  await ctx.expect(row(ctx, config.create.name)).toHaveCount(0);
}

async function cancelEnvironment(ctx) {
  const config = ctx.facts.environmentForms;
  await signIn(ctx, "/environments");
  const observed = observeWrites(ctx.page);
  const createModal = await openCreate(ctx);
  await fillDraft(createModal, { ...config.cancel, name: config.cancelName });
  await createModal.getByRole("button", { name: "Cancel", exact: true }).click();
  await ctx.expect(form(ctx)).toHaveCount(0);
  const editModal = await openEdit(ctx, config.edit.name);
  await fillDraft(editModal, config.cancel);
  await editModal.getByRole("button", { name: "Cancel", exact: true }).click();
  await ctx.expect(form(ctx, true)).toHaveCount(0);
  await reloadEnvironment(ctx, config.edit);
  await ctx.expect(row(ctx, config.cancelName)).toHaveCount(0);
  await ctx.expect(row(ctx, config.cancel.name)).toHaveCount(0);
  ctx.expect(observed.writes).toEqual([]);
  observed.stop();
  ctx.emit("environmentForms.cancelWrites", observed.writes);
}

async function duplicateEnvironment(ctx) {
  const config = ctx.facts.environmentForms;
  await signIn(ctx, "/environments");
  await ctx.expect(row(ctx, config.edit.name)).toHaveCount(1);
  const observed = observeWrites(ctx.page);
  const modal = await openCreate(ctx);
  await fillDraft(modal, config.duplicate);
  await modal.getByRole("button", { name: "Add environment", exact: true }).click();
  // This is duplicate-name validation, not an invented server-write error.
  await ctx.expect(modal.getByText("Environment name already exists.", { exact: true })).toBeVisible();
  await assertDraft(ctx, modal, config.duplicate);
  await ctx.screenshot("environment-form-duplicate-draft-retained");
  await modal.getByRole("button", { name: "Cancel", exact: true }).click();
  await ctx.expect(form(ctx)).toHaveCount(0);
  await reloadEnvironment(ctx, config.edit);
  ctx.expect(observed.writes).toEqual([]);
  observed.stop();
  ctx.emit("environmentForms.duplicateWrites", observed.writes);
}

async function membership(ctx, restore) {
  await signIn(ctx, "/admin");
  const userRow = () => ctx.page.getByRole("row").filter({ hasText: ctx.facts.operator.email });
  await ctx.expect(userRow()).toHaveCount(1);
  await userRow().getByRole("button", { name: "Edit", exact: true }).click();
  // The Admin heading sits one container deeper than environment headings.
  const modal = ctx.page.getByRole("heading", { name: "Edit user", exact: true }).locator("../../..");
  await ctx.expect(modal).toBeVisible();
  await modal.getByRole("button", { name: "all", exact: true }).click();
  if (!restore) await modal.getByRole("button", { name: ctx.facts.environmentForms.edit.name, exact: true }).click();
  const saved = await mutate(ctx, restore ? "membership-restore" : "membership", "PATCH",
    `/api/v1/admin/users/${ctx.receipts["authorization.operator.userId"]}`,
    () => modal.getByRole("button", { name: "Save changes", exact: true }).click(), 200);
  ctx.expect(saved.id).toBe(ctx.receipts["authorization.operator.userId"]);
  ctx.expect(saved.role).toBe(ctx.facts.operator.roleWire);
  ctx.expect(saved.enabled).toBe(true);
  ctx.expect(saved.environments).toEqual(restore ? [] : [ctx.facts.environmentForms.edit.name]);
  await ctx.expect(modal).toHaveCount(0);
  await ctx.page.reload();
  await ctx.expect(userRow()).toHaveCount(1);
  await ctx.expect(userRow().getByText(ctx.facts.operator.roleWire, { exact: true })).toBeVisible();
  await ctx.expect(userRow().getByText("active", { exact: true })).toBeVisible();
  await ctx.expect(userRow().getByText(restore ? "all" : ctx.facts.environmentForms.edit.name, { exact: true })).toBeVisible();
  await ctx.expect(userRow().getByText(restore ? ctx.facts.environmentForms.edit.name : "all", { exact: true })).toHaveCount(0);
}

async function assignEnvironmentMembership(ctx) { await membership(ctx, false); }
async function restoreEnvironmentMembership(ctx) { await membership(ctx, true); }

async function deleteEnvironment(ctx) {
  const name = ctx.facts.environmentForms.edit.name;
  await signIn(ctx, "/environments");
  const modal = await openEdit(ctx, name);
  await modal.getByRole("button", { name: "Remove environment", exact: true }).click();
  const confirmation = ctx.page.getByRole("heading", { name: "Remove environment", exact: true }).locator("../..");
  await ctx.expect(confirmation).toBeVisible();
  const remove = confirmation.getByRole("button", { name: "Remove environment", exact: true });
  await ctx.expect(remove).toBeDisabled();
  const typed = confirmation.getByPlaceholder(name, { exact: true });
  await ctx.expect(typed).toBeEnabled();
  await typed.fill(name + "-wrong");
  await ctx.expect(remove).toBeDisabled();
  await typed.fill(name);
  await ctx.expect(remove).toBeEnabled();
  await mutate(ctx, "delete", "DELETE", `/api/v1/environments/${ctx.receipts["environmentForms.environmentId"]}`,
    () => remove.click(), 204);
  await ctx.expect(confirmation).toHaveCount(0);
  await ctx.expect(form(ctx, true)).toHaveCount(0);
  await ctx.page.reload();
  await table(ctx);
  await ctx.expect(row(ctx, name)).toHaveCount(0);
  await ctx.expect(row(ctx, ctx.facts.environmentForms.create.name)).toHaveCount(0);
}

// Named phase exports let the coordinator map actions without another wrapper.
Object.assign(exports, { createEnvironment, editEnvironment, cancelEnvironment,
  assignOptionalEnvironmentPolicy,
  duplicateEnvironment, assignEnvironmentMembership, restoreEnvironmentMembership,
  deleteEnvironment });

exports.run = async (ctx) => {
  const pending = Object.keys(phases).filter((phase) =>
    ctx.receipts[`environmentForms.request.${phase}`] && !ctx.receipts[`environmentForms.browser.${phase}`]);
  ctx.expect(pending).toHaveLength(1);
  const phase = pending[0];
  await ctx.step(`${phase}: production environment form and reload`, async () => {
    await phases[phase](ctx);
    await ctx.screenshot(`environment-form-${phase}-reloaded`);
    ctx.emit(`environmentForms.browser.${phase}`, true);
  });
};
