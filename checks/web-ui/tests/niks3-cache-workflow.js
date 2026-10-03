/** TASK-470: isolated Niks3 form assertions shared by host and VM harnesses.
 * Discovery/probes are mocked; mutations use the selected fixture API. The
 * workflow deletes only its own uniquely named cache, never resets fixtures.
 */
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { chromium } = require("playwright");
const { expect } = require("@playwright/test");
const { generateKeyPairSync } = require("node:crypto");

async function niks3CacheWorkflow(page, baseUrl, apiBaseUrl, screenshot, captureState) {
  const name = `task470-ui-${Date.now()}`;
  const keys = ["cache-one", "cache-two"].map(label => `${label}:${Buffer.alloc(32, 7).toString("base64")}`);
  const token = "fixture-niks3-token-not-production";
  const rotatedToken = "fixture-niks3-rotated-token";
  const privateKey = generateKeyPairSync("rsa", { modulusLength: 2048, privateKeyEncoding: { type: "pkcs8", format: "pem" }, publicKeyEncoding: { type: "spki", format: "pem" } }).privateKey;
  // Frozen public X.509 fixture, shared with the server certificate tests.
  // The generated key is deliberately unrelated: these assertions exercise
  // persistence, not TLS handshakes. Real identities belong to the VM fixture.
  const cert = `-----BEGIN CERTIFICATE-----
MIIBYzCCARWgAwIBAgIUUiT5rFbIc6C8wVUsrDv3mmB3PDkwBQYDK2VwMCYxJDAi
BgNVBAMMG3Rhc2s0NzAtY2VydGlmaWNhdGUtZml4dHVyZTAgFw0yNjEwMDMwMDQ1
MTJaGA8yMTI2MDkwOTAwNDUxMlowJjEkMCIGA1UEAwwbdGFzazQ3MC1jZXJ0aWZp
Y2F0ZS1maXh0dXJlMCowBQYDK2VwAyEArmgxjwzB+VI7yxPnKKkVj9uulx0wtyrf
HC/p+1wXxpOjUzBRMB0GA1UdDgQWBBS5G+8DC/fImjFHjQtXKwSe/LTS7TAfBgNV
HSMEGDAWgBS5G+8DC/fImjFHjQtXKwSe/LTS7TAPBgNVHRMBAf8EBTADAQH/MAUG
AytlcANBANv0+9Ic9w11sJMah7/3hykwUUhs+iQqGZWTnWwQumZ7bkfKPl4Illm2
Zh/vQ6oHa2rNyo8ob+V7jS6Zzq1/6Qk=
-----END CERTIFICATE-----`;
  const privateValues = [token, rotatedToken, privateKey];
  const requests = [];
  let createdId;
  let discoveryCount = 0;
  let partialFailure = true;
  let failDiscovery = true;
  let failTest = true;
  let failEnvironmentRead = false;
  let releaseDiscovery, releaseTest, releaseAssignment;
  const discoveryGate = new Promise(resolve => { releaseDiscovery = resolve; });
  const testGate = new Promise(resolve => { releaseTest = resolve; });
  const assignmentGate = new Promise(resolve => { releaseAssignment = resolve; });
  const discoveryRoute = "**/api/v1/caches/niks3/discover";
  const testRoute = "**/api/v1/caches/test-credentials";
  const environmentsRoute = /\/api\/v1\/caches\/\d+\/environments$/;
  const capture = request => { if (request.url().includes("/caches")) requests.push(request); };
  page.on("request", capture);
  await page.route(discoveryRoute, async route => {
    const body = route.request().postDataJSON();
    assert.deepEqual(Object.keys(body), ["server_url"]);
    assert.equal(body.server_url, "https://write.example.com");
    discoveryCount++;
    if (failDiscovery) {
      failDiscovery = false;
      return route.fulfill({ status: 400, json: { error: "fixture-discovery-error" } });
    }
    await discoveryGate;
    await route.fulfill({ json: { server_url: "https://write.example.com", substituter_url: "https://read.example.com", public_keys: keys, oidc_audience: "fixture-oidc-not-offered" } });
  });
  await page.route(testRoute, async route => {
    if (failTest) { failTest = false; return route.fulfill({ status: 502, json: { error: token } }); }
    await testGate;
    await route.fulfill({ json: { success: true, message: "Public metadata/read checks passed", status_code: 200, tested_url: "https://read.example.com", server_reachable: true, discovery_valid: true, write_auth_valid: null, read_endpoint_reachable: true, signing_keys_found: true } });
  });
  await page.route(environmentsRoute, async route => {
    if (route.request().method() === "GET" && failEnvironmentRead
      && await page.getByRole("dialog", { name: "Niks3 cache destination" }).count() > 0) {
      failEnvironmentRead = false;
      return route.fulfill({ status: 503, json: { error: "fixture assignment read failure" } });
    }
    if (route.request().method() === "PUT" && partialFailure) {
      partialFailure = false;
      await assignmentGate;
      return route.fulfill({ status: 503, json: { error: "fixture assignment failure" } });
    }
    await route.continue();
  });
  const csrf = async () => {
    const cookies = await page.context().cookies(apiBaseUrl);
    const cookie = cookies.find(c => c.name.includes("csrf"));
    return cookie ? { "X-CSRF-Token": cookie.value } : {};
  };
  const api = async (method, url, data) => {
    // Browser fetch preserves the fixture's secure loopback session-cookie
    // behavior. Node's APIRequestContext does not send that cookie over HTTP.
    const headers = await csrf();
    const response = await page.evaluate(async ({ url, method, headers, data }) => {
      const result = await fetch(url, { method, credentials: "include", headers: { ...headers, "Content-Type": "application/json" }, body: data === undefined ? undefined : JSON.stringify(data) });
      return { status: result.status, text: await result.text() };
    }, { url: `${apiBaseUrl}/api/v1${url}`, method, headers, data });
    assert(response.status >= 200 && response.status < 300, `${method} ${url} status ${response.status}`);
    return { text: async () => response.text, json: async () => JSON.parse(response.text) };
  };
  const getRedacted = async () => {
    const response = await api("GET", `/caches/${createdId}`);
    const body = await response.text();
    for (const secret of privateValues) assert(!body.includes(secret), "GET must not leak private material");
    const cache = JSON.parse(body);
    for (const field of ["niks3_auth_token", "niks3_write_client_key", "niks3_read_client_key"]) assert(!cache[field], `GET redacts ${field}`);
    return cache;
  };
  const openEdit = async () => {
    await page.goto(`${baseUrl}/caches`);
    await page.getByText(name, { exact: true }).click();
    await page.getByRole("button", { name: "Edit cache", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Niks3 cache destination" });
    await expect(dialog).toBeVisible();
    return dialog;
  };
  const save = async dialog => {
    await dialog.getByRole("button", { name: "Save changes", exact: true }).click();
    await expect(dialog).toBeHidden();
  };
  try {
    await page.goto(`${baseUrl}/caches`);
    await page.getByRole("button", { name: "Add cache", exact: true }).click();
    await page.getByRole("button", { name: "Niks3", exact: true }).click();
    let dialog = page.getByRole("dialog", { name: "Niks3 cache destination" });
    await expect(dialog).toBeVisible();
    for (const section of ["Destination", "Credentials", "Environments"]) await expect(dialog.getByRole("button", { name: section, exact: true })).toBeVisible();
    await dialog.getByRole("button", { name: "Add cache", exact: true }).click();
    await expect(dialog.getByRole("alert")).toContainText("Enter a cache name");
    await dialog.getByLabel("Name", { exact: true }).fill(name);
    await dialog.getByLabel("Write / API URL", { exact: true }).fill("https://write.example.com");
    await dialog.getByRole("button", { name: "Discover configuration" }).click();
    await expect(dialog.getByRole("alert")).toContainText("Discovery failed");
    await dialog.getByRole("button", { name: "Discover configuration" }).click();
    await expect(dialog.getByRole("button", { name: "Discovering…", exact: true })).toBeDisabled();
    await expect(dialog.getByLabel("Write / API URL")).toBeDisabled();
    releaseDiscovery();
    await expect(dialog.getByLabel("Read / substituter URL")).toHaveValue("https://read.example.com");
    await expect(dialog.getByLabel("Signing public keys")).toHaveValue(keys.join("\n"));
    await expect(dialog.getByRole("status")).toContainText("Review URLs and all keys before saving");
    assert.equal(discoveryCount, 2);
    // Discovery must not persist anything or transport any credentials.
    assert(!requests.some(r => r.method() === "POST" && /\/caches$/.test(r.url())));
    await dialog.getByRole("button", { name: "Credentials", exact: true }).click();
    await dialog.getByLabel("Write token").fill(token);
    await dialog.getByRole("button", { name: "Test connection" }).click();
    await expect(dialog.getByRole("alert")).toContainText("Connection test failed");
    await expect(dialog.getByRole("alert")).not.toContainText(token);
    await dialog.getByRole("button", { name: "Test connection" }).click();
    await expect(dialog.getByRole("button", { name: "Testing…", exact: true })).toBeDisabled();
    await expect(dialog.getByLabel("Write token")).toBeDisabled();
    releaseTest();
    await expect(dialog.getByTestId("niks3-test-result")).toContainText("Write authorization: Untested");
    await expect(dialog.getByTestId("niks3-test-result")).not.toContainText("Write authorization: Verified");
    await expect(dialog.getByText("External credential providers are not offered.", { exact: false })).toBeVisible();
    if (screenshot) await page.screenshot({ path: screenshot.replace(/\.png$/, "-credentials.png"), fullPage: true, animations: "disabled" });
    if (captureState) await captureState("niks3-credentials");
    await dialog.getByRole("button", { name: "Environments", exact: true }).click();
    const envResponse = await api("GET", "/environments");
    const envBody = await envResponse.json();
    const envs = Array.isArray(envBody) ? envBody : envBody.environments;
    assert(envs.length > 0, "fixture requires an environment");
    const selected = envs[0];
    await dialog.getByRole("button", { name: selected.name, exact: true }).click();
    await dialog.getByRole("button", { name: "Add cache", exact: true }).click();
    await expect(dialog.getByRole("button", { name: "Saving…", exact: true })).toBeDisabled();
    await expect(dialog.getByRole("button", { name: selected.name, exact: true })).toBeDisabled();
    releaseAssignment();
    await expect(dialog.getByRole("alert")).toContainText("Cache saved, but environment assignment failed");
    const list = await (await api("GET", "/caches")).json();
    const own = list.filter(c => c.name === name);
    assert.equal(own.length, 1);
    createdId = own[0].id;
    await save(dialog);
    assert.deepEqual(await (await api("GET", `/caches/${createdId}/environments`)).json(), [selected.id]);
    let cache = await getRedacted();
    assert.equal(cache.niks3_write_token_configured, true);
    assert.equal(cache.niks3_read_auth_mode, "none");
    assert.deepEqual(cache.niks3_public_keys, keys);
    dialog = await openEdit();
    await dialog.getByRole("button", { name: "Credentials", exact: true }).click();
    await expect(dialog.getByLabel("Write token")).toHaveValue("");
    await expect(dialog.getByText("Token configured.", { exact: false })).toBeVisible();
    await dialog.getByLabel("Write token").fill(rotatedToken);
    await save(dialog);
    assert(requests.some(r => r.method() === "PUT" && r.postDataJSON()?.niks3_auth_token === rotatedToken), "rotation sends the replacement token");
    cache = await getRedacted();
    assert(cache.niks3_write_token_configured);
    dialog = await openEdit();
    await dialog.getByRole("button", { name: "Credentials", exact: true }).click();
    await expect(dialog.getByLabel("Write token")).toHaveValue("");
    await save(dialog);
    cache = await getRedacted();
    assert(cache.niks3_write_token_configured, "blank edit retains token");
    dialog = await openEdit();
    await dialog.getByRole("button", { name: "Credentials", exact: true }).click();
    await dialog.getByLabel("Write authentication").selectOption("mtls");
    await dialog.getByLabel("Read authentication").selectOption("mtls");
    await dialog.getByRole("button", { name: "Save changes", exact: true }).click();
    await expect(dialog.getByRole("alert")).toContainText("Write mTLS requires");
    for (const plane of ["Write", "Read"]) {
      await dialog.getByLabel(`${plane} client certificate`, { exact: true }).fill(cert);
      await dialog.getByLabel(`${plane} private key`, { exact: true }).fill(privateKey);
      await dialog.getByLabel(`${plane} CA certificate (optional)`, { exact: true }).fill(cert);
    }
    await save(dialog);
    cache = await getRedacted();
    assert.equal(cache.niks3_write_token_configured, false);
    assert.equal(cache.niks3_write_mtls_configured, true);
    assert.equal(cache.niks3_read_mtls_configured, true);
    dialog = await openEdit();
    await dialog.getByRole("button", { name: "Credentials", exact: true }).click();
    await expect(dialog.getByLabel("Write private key")).toHaveValue("");
    await expect(dialog.getByLabel("Read private key")).toHaveValue("");
    if (screenshot) await page.screenshot({ path: screenshot.replace(/\.png$/, "-mtls.png"), fullPage: true, animations: "disabled" });
    if (captureState) await captureState("niks3-mtls");
    await dialog.getByRole("button", { name: "Environments", exact: true }).click();
    await expect(dialog.getByRole("button", { name: selected.name, exact: true })).toHaveAttribute("aria-pressed", "true");
    await save(dialog); // Blank identity replacements retain configured mTLS.
    cache = await getRedacted();
    assert(cache.niks3_write_mtls_configured && cache.niks3_read_mtls_configured);
    dialog = await openEdit();
    await dialog.getByRole("button", { name: "Credentials", exact: true }).click();
    await dialog.getByRole("checkbox", { name: "Remove Write custom CA on save" }).check();
    await dialog.getByRole("checkbox", { name: "Remove Read custom CA on save" }).check();
    await save(dialog);
    cache = await getRedacted();
    assert.equal(cache.niks3_write_ca_cert, null);
    assert.equal(cache.niks3_read_ca_cert, null);
    dialog = await openEdit();
    await dialog.getByRole("button", { name: "Credentials", exact: true }).click();
    await dialog.getByLabel("Write authentication").selectOption("token");
    await dialog.getByLabel("Write token").fill(token);
    await dialog.getByLabel("Read authentication").selectOption("none");
    await save(dialog);
    cache = await getRedacted();
    assert(cache.niks3_write_token_configured);
    assert.equal(cache.niks3_write_mtls_configured, false);
    assert.equal(cache.niks3_read_mtls_configured, false);
    assert.equal(cache.niks3_write_client_cert, null);
    assert.equal(cache.niks3_read_client_cert, null);
    assert.deepEqual(await (await api("GET", `/caches/${createdId}/environments`)).json(), [selected.id]);
    failEnvironmentRead = true;
    dialog = await openEdit();
    await expect(dialog.getByRole("alert")).toContainText("Environment assignments could not be loaded");
    await expect(dialog.getByRole("button", { name: "Save changes", exact: true })).toBeDisabled();
    await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
    dialog = await openEdit();
    if (captureState) await captureState("niks3-destination");
    if (screenshot) {
      await page.screenshot({ path: screenshot, fullPage: true, animations: "disabled" });
      const viewport = page.viewportSize();
      await page.setViewportSize({ width: 390, height: 844 });
      await page.screenshot({ path: screenshot.replace(/\.png$/, "-mobile.png"), fullPage: true, animations: "disabled" });
      const bounds = await dialog.boundingBox();
      assert(bounds.x >= 0 && bounds.x + bounds.width <= 390, "mobile dialog fits viewport");
      await page.setViewportSize(viewport);
    }
    await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
    for (const request of requests) {
      const url = request.url();
      for (const secret of privateValues) assert(!url.includes(secret), "secrets never occur in URLs");
    }
    const visibleText = await page.locator("body").innerText();
    for (const secret of privateValues) assert(!visibleText.includes(secret), "secrets never occur in rendered text");
  } finally {
    releaseDiscovery(); releaseTest(); releaseAssignment();
    // Recover the owned ID if a later assertion fails immediately after create.
    if (!createdId) {
      try {
        const list = await (await api("GET", "/caches")).json();
        createdId = list.find(c => c.name === name)?.id;
      } catch { /* Preserve the original failure when login/session was lost. */ }
    }
    if (createdId) await api("DELETE", `/caches/${createdId}`);
    page.off("request", capture);
    await page.unroute(discoveryRoute);
    await page.unroute(testRoute);
    await page.unroute(environmentsRoute);
  }
}

module.exports = { niks3CacheWorkflow };

// Host feedback uses the repository web-ui-test launcher for Nix-provided
// Playwright/browser dependencies. It never starts or restarts services.
if (require.main === module) {
  (async () => {
    const [baseUrl, outputDir] = process.argv.slice(2);
    const apiBaseUrl = process.env.CF_UI_API_BASE_URL;
    assert(apiBaseUrl, "explicit fixture API origin required");
    const browser = await chromium.launch({ headless: true });
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, reducedMotion: "reduce" });
    await context.addInitScript(origin => localStorage.setItem("cf_backend_origin", origin), apiBaseUrl);
    // The dev banner can bypass the selected API origin. Permit only the
    // verified preview origins, regardless of the dynamically allocated ports.
    const previewOrigins = new Set([new URL(baseUrl).origin, new URL(apiBaseUrl).origin]);
    await context.route(/https?:\/\/(127\.0\.0\.1|localhost|\[::1\])(?::\d+)?\//, route =>
      previewOrigins.has(new URL(route.request().url()).origin) ? route.continue() : route.abort());
    const page = await context.newPage();
    let result;
    try {
      await page.goto(`${baseUrl}/login`);
      await page.getByPlaceholder("Enter your username").fill(process.env.CF_UI_TEST_USERNAME || "admin");
      await page.getByPlaceholder("Enter your password").fill(process.env.CF_UI_TEST_PASSWORD || "password");
      await page.getByRole("button", { name: "Sign In", exact: true }).click();
      await page.waitForURL(url => !url.pathname.includes("login"));
      const status = await page.request.get(`${apiBaseUrl}/status`);
      assert.equal(status.status(), 200);
      await niks3CacheWorkflow(page, baseUrl, apiBaseUrl, path.join(outputDir, "task470-niks3-form.png"));
      result = { name: "task470-niks3-cache", ok: true };
      console.log("TASK-470 Niks3 browser workflow passed against", apiBaseUrl);
    } catch (error) {
      result = { name: "task470-niks3-cache", ok: false, error: error.message };
      console.error(error);
      process.exitCode = 1;
    } finally {
      fs.writeFileSync(path.join(outputDir, "results.json"), JSON.stringify([result], null, 2));
      await context.close();
      await browser.close();
    }
  })().catch(error => { console.error(error); process.exitCode = 1; });
}
