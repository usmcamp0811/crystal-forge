// Same delivery lifecycle, fresh browser context. The initial accepted managed
// request is a discriminating positive control, not a seeded pending fixture.
"use strict";
exports.run = async ctx => {
  const { page, expect, facts, receipts } = ctx;
  await page.goto(ctx.url("/login"));
  await page.getByPlaceholder("Enter your username", { exact: true }).fill(facts.admin.username);
  await page.getByPlaceholder("Enter your password", { exact: true }).fill(facts.admin.password);
  await page.getByRole("button", { name: "Sign In", exact: true }).click();
  await expect(page).toHaveURL(ctx.url("/"));
  const who = await ctx.whoami();
  expect(who.body.user.id).toBe(receipts["delivery.adminId"]);
  const token = (await ctx.context.cookies()).find(c => c.name === "__Host-cf-csrf");
  expect(token).toBeDefined();
  const headers = { "x-csrf-token": token.value };
  const path = `/api/v1/systems/${receipts["management.conversion.systemId"]}`;
  const intent = { commit_sha: receipts["delivery.first"].commit, action: "deploy", request_id: require("node:crypto").randomUUID() };
  const request = () => ctx.context.request.post(ctx.url(`${path}/deploy`), { headers, data: intent });
  expect((await request()).status()).toBe(202);
  // Both legal serializations must converge on revoked authority. This race
  // does not claim a measured row-lock interleaving; deterministic lower-level
  // locking regressions remain required separately.
  const [converted, replay, fresh] = await Promise.all([
    ctx.context.request.patch(ctx.url(path), { headers, data: {
      hostname: "conversion-probe", management_mode: "external", deployment_policy: "manual",
      environment: "delivery-proof", flake_name: facts.flake.name, system_configuration_name: facts.target,
    } }), request(), ctx.context.request.post(ctx.url(`${path}/deploy`), { headers,
      data: { ...intent, request_id: require("node:crypto").randomUUID() } }),
  ]);
  expect(converted.status()).toBe(200);
  expect([202, 400, 409]).toContain(replay.status());
  expect([202, 400, 409]).toContain(fresh.status());
  for (const endpoint of ["deploy", "rollback", "rollback-generation"]) {
    const denied = await ctx.context.request.post(ctx.url(`${path}/${endpoint}`), {
      headers, data: endpoint === "deploy" ? intent : endpoint === "rollback" ? { target_commit: intent.commit_sha } : { generation: 1 },
    });
    expect(denied.status(), `${endpoint}: nonmanaged authority denied`).toBe(400);
    expect((await denied.json()).message).toBe("Deployment requires an active managed system");
  }
  ctx.emit("management.conversion.raceStatuses", [converted.status(), replay.status(), fresh.status()]);
};
