/** TASK-470: one stable Add dialog, retained drafts and real scoped creates.
 * Only uniquely named workflow records are deleted. No create is mocked.
 */
const assert = require("node:assert/strict");
const { expect } = require("@playwright/test");

async function sharedCacheModalWorkflow(page, baseUrl, apiBaseUrl, screenshot) {
  const prefix = `task470-shared-${Date.now()}`;
  const key = `fixture:${Buffer.alloc(32, 7).toString("base64")}`;
  const requests = [];
  const capture = request => { if (request.url().includes("/caches")) requests.push(request); };
  page.on("request", capture);
  const api = async (method, route, data) => {
    const cookies = await page.context().cookies(apiBaseUrl);
    const csrf = cookies.find(c => c.name.includes("csrf"));
    const result = await page.evaluate(async ({ url, method, csrf, data }) => {
      const response = await fetch(url, { method, credentials: "include", headers: { "Content-Type": "application/json", ...(csrf ? { "X-CSRF-Token": csrf } : {}) }, body: data === undefined ? undefined : JSON.stringify(data) });
      return { status: response.status, text: await response.text() };
    }, { url: `${apiBaseUrl}/api/v1${route}`, method, csrf: csrf?.value, data });
    assert(result.status >= 200 && result.status < 300, `${method} ${route}: HTTP ${result.status}`);
    return result.text ? JSON.parse(result.text) : null;
  };
  const dialog = page.getByRole("dialog", { name: "Cache destination", exact: true });
  const section = name => dialog.getByRole("button", { name, exact: true }).click();
  const type = async name => {
    await section("Destination");
    await dialog.getByRole("button", { name, exact: true }).click();
    await expect(dialog.getByRole("button", { name, exact: true })).toHaveAttribute("aria-pressed", "true");
  };
  const shell = async () => {
    await expect(dialog).toBeVisible();
    for (const name of ["Destination", "Credentials", "Environments"]) {
      const button = dialog.getByRole("navigation").getByRole("button", { name, exact: true });
      await expect(button).toBeVisible();
      assert.equal(await button.locator("svg").count(), 1, `${name} has its design icon`);
    }
    await expect(dialog.locator("header").getByRole("button", { name: "Close", exact: true })).toBeVisible();
    await expect(dialog.locator("footer").getByRole("button", { name: "Cancel", exact: true })).toBeVisible();
    await expect(dialog.locator("footer").getByRole("button", { name: "Add cache", exact: true })).toBeVisible();
    await expect(dialog.locator("footer").getByRole("button", { name: "Add cache", exact: true }).locator("svg")).toHaveCount(1, { timeout: 60000 });
  };
  const shots = async kind => {
    if (!screenshot) return;
    const viewport = page.viewportSize();
    const originalTheme = await page.locator("html").getAttribute("data-theme");
    for (const theme of ["dark", "light"]) {
      await page.evaluate(theme => document.documentElement.setAttribute("data-theme", theme), theme);
      for (const name of ["Destination", "Credentials", "Environments"]) {
        await section(name);
        await page.screenshot({ path: screenshot.replace(/\.png$/, `-${kind}-${name.toLowerCase()}-${theme}.png`), fullPage: true, animations: "disabled" });
      }
      await page.setViewportSize({ width: 390, height: 844 });
      await section("Destination");
      await page.screenshot({ path: screenshot.replace(/\.png$/, `-${kind}-mobile-${theme}.png`), fullPage: true, animations: "disabled" });
      const bounds = await dialog.boundingBox();
      assert(bounds.x >= 0 && bounds.x + bounds.width <= 390, "narrow dialog fits the viewport");
      assert(await dialog.evaluate(node => node.scrollWidth <= node.clientWidth), "narrow shell does not overflow horizontally");
      await page.setViewportSize(viewport);
    }
    await page.evaluate(theme => document.documentElement.setAttribute("data-theme", theme), originalTheme || "dark");
    await section("Destination");
  };
  try {
    await page.evaluate(() => localStorage.setItem("cf.coach.ui.v2", JSON.stringify({ panel: "dismissed", track: "setup" })));
    await page.goto(`${baseUrl}/caches`);
    const envBody = await api("GET", "/environments");
    const env = (Array.isArray(envBody) ? envBody : envBody.environments)[0];
    assert(env, "fixture contains an environment");
    await page.getByRole("button", { name: "Add cache", exact: true }).click();
    await shell();
    await expect(dialog.getByRole("heading", { name: "Add cache destination", exact: true })).toBeVisible();
    const node = await dialog.elementHandle();
    await dialog.getByLabel("Name", { exact: true }).fill(prefix);
    await type("Nix HTTPS");
    await dialog.getByLabel("URL", { exact: true }).fill("https://read.example.com");
    await section("Environments");
    await dialog.getByRole("button", { name: env.name, exact: true }).click();
    await type("Attic");
    await expect(dialog.getByLabel("Attic server URL", { exact: true })).toHaveValue("https://read.example.com");
    await dialog.getByLabel("Attic server URL", { exact: true }).fill("https://attic.example.com");
    await dialog.getByLabel("Attic cache name", { exact: true }).fill("fixture-attic");
    await dialog.getByLabel("Attic public key", { exact: true }).fill(key);
    await section("Credentials");
    await dialog.getByLabel("Attic token", { exact: true }).fill("fixture-attic-token");
    await type("Niks3");
    await dialog.getByLabel("Read / substituter URL", { exact: true }).fill("https://read.example.com");
    await dialog.getByLabel("Write / API URL", { exact: true }).fill("https://write.example.com");
    await dialog.getByLabel("Signing public keys", { exact: true }).fill(key);
    await section("Credentials");
    await dialog.getByLabel("Write token", { exact: true }).fill("fixture-niks3-token");
    await type("S3-compatible");
    for (const [label, value] of [["Destination URL", "s3://fixture-bucket/prefix"], ["S3 region", "us-east-1"], ["S3 endpoint URL", "https://s3.example.com"]]) await dialog.getByLabel(label, { exact: true }).fill(value);
    await dialog.getByLabel("Signing key path (optional)", { exact: true }).fill("/fixture/signing-key");
    await dialog.getByLabel("Compression (optional)", { exact: true }).selectOption("zstd");
    await section("Credentials");
    for (const [label, value] of [["AWS access key ID", "fixture-access"], ["AWS secret access key", "fixture-secret"], ["AWS session token (optional)", "fixture-session"], ["S3 profile (optional)", "fixture-profile"]]) await dialog.getByLabel(label, { exact: true }).fill(value);
    for (const [name, badge] of [["Attic", "Attic"], ["Niks3", "Niks3"], ["Nix HTTPS", "Nix HTTPS"], ["S3-compatible", "S3"]]) {
      await type(name);
      assert(await dialog.evaluate((current, original) => current === original, node), `same actual dialog object after ${name}`);
      await shell();
      await expect(dialog.getByLabel("Name", { exact: true })).toHaveValue(prefix);
      await expect(dialog.locator("header")).toContainText(badge);
      await expect(dialog.locator("footer")).toContainText("1 selected");
      if (name === "Attic") {
        await expect(dialog.getByLabel("Attic server URL", { exact: true })).toHaveValue("https://attic.example.com");
        await expect(dialog.getByLabel("Attic cache name", { exact: true })).toHaveValue("fixture-attic");
        await expect(dialog.getByLabel("Attic public key", { exact: true })).toHaveValue(key);
        await expect(dialog.getByLabel("Write / API URL", { exact: true })).toHaveCount(0);
        await section("Credentials");
        await expect(dialog.getByLabel("Attic token", { exact: true })).toHaveValue("fixture-attic-token");
        await expect(dialog.getByLabel("AWS access key ID", { exact: true })).toHaveCount(0);
      } else if (name === "Niks3") {
        await expect(dialog.getByLabel("Read / substituter URL", { exact: true })).toHaveValue("https://read.example.com");
        await expect(dialog.getByLabel("Write / API URL", { exact: true })).toHaveValue("https://write.example.com");
        await expect(dialog.getByLabel("Signing public keys", { exact: true })).toHaveValue(key);
        await expect(dialog.getByLabel("S3 region", { exact: true })).toHaveCount(0);
        await section("Credentials");
        await expect(dialog.getByLabel("Write token", { exact: true })).toHaveValue("fixture-niks3-token");
      } else if (name === "S3-compatible") {
        await expect(dialog.getByLabel("Destination URL", { exact: true })).toHaveValue("s3://fixture-bucket/prefix");
        await expect(dialog.getByLabel("S3 region", { exact: true })).toHaveValue("us-east-1");
        await expect(dialog.getByLabel("S3 endpoint URL", { exact: true })).toHaveValue("https://s3.example.com");
        await expect(dialog.getByLabel("Signing key path (optional)", { exact: true })).toHaveValue("/fixture/signing-key");
        await expect(dialog.getByLabel("Compression (optional)", { exact: true })).toHaveValue("zstd");
        await section("Credentials");
        await expect(dialog.getByLabel("AWS access key ID", { exact: true })).toHaveValue("fixture-access");
        await expect(dialog.getByLabel("AWS secret access key", { exact: true })).toHaveValue("fixture-secret");
        await expect(dialog.getByLabel("AWS session token (optional)", { exact: true })).toHaveValue("fixture-session");
        await expect(dialog.getByLabel("S3 profile (optional)", { exact: true })).toHaveValue("fixture-profile");
      } else {
        await expect(dialog.getByLabel("URL", { exact: true })).toHaveValue("https://read.example.com");
        await expect(dialog.getByLabel("Attic cache name", { exact: true })).toHaveCount(0);
      }
      await shots(badge.toLowerCase().replace(/ /g, "-"));
    }
    await type("Niks3");
    await section("Credentials");
    await dialog.getByLabel("Write authentication").selectOption("mtls");
    await dialog.getByLabel("Read authentication").selectOption("mtls");
    for (const plane of ["Write", "Read"]) {
      await dialog.getByLabel(`${plane} client certificate`, { exact: true }).fill(`draft-${plane}-certificate`);
      await dialog.getByLabel(`${plane} private key`, { exact: true }).fill(`draft-${plane}-private-key`);
      await dialog.getByLabel(`${plane} CA certificate (optional)`, { exact: true }).fill(`draft-${plane}-CA`);
    }
    await type("Attic");
    await type("Niks3");
    assert(await dialog.evaluate((current, original) => current === original, node), "mTLS switching retains the actual dialog object");
    await section("Credentials");
    for (const plane of ["Write", "Read"]) {
      await expect(dialog.getByLabel(`${plane} client certificate`, { exact: true })).toHaveValue(`draft-${plane}-certificate`);
      await expect(dialog.getByLabel(`${plane} private key`, { exact: true })).toHaveValue(`draft-${plane}-private-key`);
      await expect(dialog.getByLabel(`${plane} CA certificate (optional)`, { exact: true })).toHaveValue(`draft-${plane}-CA`);
    }
    await dialog.getByLabel("Write authentication").selectOption("token");
    await dialog.getByLabel("Read authentication").selectOption("none");
    await dialog.getByLabel("Write token").fill("fixture-niks3-token");
    await type("S3-compatible");
    await section("Destination");
    await dialog.getByLabel("Name", { exact: true }).fill("");
    const before = requests.filter(r => r.method() === "POST" && /\/caches$/.test(r.url())).length;
    await dialog.getByRole("button", { name: "Add cache", exact: true }).click();
    await expect(dialog.getByRole("alert")).toContainText("Enter a cache name");
    assert.equal(requests.filter(r => r.method() === "POST" && /\/caches$/.test(r.url())).length, before, "invalid draft never reaches create");
    await dialog.getByLabel("Name", { exact: true }).fill(`${prefix}-S3`);
    for (const [button, apiType] of [["S3-compatible", "S3"], ["Attic", "Attic"], ["Nix HTTPS", "Nix"], ["Niks3", "Niks3"]]) {
      // Reuse the retained-draft dialog for the first real create. Subsequent
      // records start in fresh dialogs, proving actual default-to-create paths.
      if (apiType !== "S3") {
        await page.getByRole("button", { name: "Add cache", exact: true }).click();
        await type(button);
        await dialog.getByLabel("Name", { exact: true }).fill(`${prefix}-${apiType}`);
        await dialog.getByLabel(apiType === "Niks3" ? "Read / substituter URL" : apiType === "Attic" ? "Attic server URL" : "URL", { exact: true }).fill("https://read.example.com");
        if (apiType === "Attic") {
          await dialog.getByLabel("Attic cache name", { exact: true }).fill("fixture-attic");
          await dialog.getByLabel("Attic public key", { exact: true }).fill(key);
          await section("Credentials");
          await dialog.getByLabel("Attic token", { exact: true }).fill("fixture-attic-token");
        } else if (apiType === "Niks3") {
          await dialog.getByLabel("Write / API URL", { exact: true }).fill("https://write.example.com");
          await dialog.getByLabel("Signing public keys", { exact: true }).fill(key);
          await section("Credentials");
          await dialog.getByLabel("Write token", { exact: true }).fill("fixture-niks3-token");
        }
        await section("Environments");
        await dialog.getByRole("button", { name: env.name, exact: true }).click();
      }
      const responsePromise = page.waitForResponse(r => r.request().method() === "POST" && /\/api\/v1\/caches$/.test(r.url()));
      await dialog.getByRole("button", { name: "Add cache", exact: true }).click();
      const response = await responsePromise;
      assert.equal(response.status(), 201, `${apiType} real create: ${await response.text()}`);
      const created = await response.json();
      await expect(dialog).toBeHidden();
      assert.equal(created.cache_type, apiType);
      if (apiType === "S3") {
        assert.equal(created.s3_region, "us-east-1");
        assert.equal(created.s3_profile, "fixture-profile");
        assert.equal(created.s3_endpoint_url, "https://s3.example.com");
        assert.equal(created.compression, "zstd");
        assert.equal(created.signing_key_path, "/fixture/signing-key");
      }
      assert.deepEqual(response.request().postDataJSON().environment_ids, [env.id]);
      assert.deepEqual(await api("GET", `/caches/${created.id}/environments`), [env.id]);
      console.log(`TASK-470 shared dialog real ${apiType} create passed (HTTP 201, atomic scope)`);
    }
    assert(!requests.some(r => r.method() === "PUT" && /\/environments$/.test(r.url())), "no second environment PUT for any type");
    const environmentsRoute = `${apiBaseUrl}/api/v1/environments`;
    let release;
    const gate = new Promise(resolve => { release = resolve; });
    await page.route(environmentsRoute, async route => {
      await gate;
      await route.fulfill({ status: 503, json: { error: "fixture environments unavailable" } });
    });
    try {
      await page.getByRole("button", { name: "Add cache", exact: true }).click();
      await section("Environments");
      await expect(dialog.getByText("Loading environments…", { exact: true })).toBeVisible();
      await expect(dialog.getByRole("button", { name: "Add cache", exact: true })).toBeDisabled();
      await expect(dialog.locator("footer")).toContainText("Scope not loaded");
      await expect(dialog.locator("footer")).not.toContainText("Global scope");
      if (screenshot) await page.screenshot({ path: screenshot.replace(/\.png$/, "-environment-loading.png"), animations: "disabled" });
      release();
      await expect(dialog.getByRole("alert")).toContainText("Could not load environments");
      await expect(dialog.getByRole("button", { name: "Add cache", exact: true })).toBeDisabled();
      if (screenshot) await page.screenshot({ path: screenshot.replace(/\.png$/, "-environment-error.png"), animations: "disabled" });
      await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
    } finally { release(); await page.unroute(environmentsRoute); }
    console.log("TASK-470 shared dialog identity, four-type draft retention, validation and 4 real creates passed");
  } finally {
    // Recover records even if an assertion fails immediately after creation.
    for (const cache of await api("GET", "/caches")) if (cache.name.startsWith(prefix)) await api("DELETE", `/caches/${cache.id}`);
    page.off("request", capture);
  }
}

module.exports = { sharedCacheModalWorkflow };
