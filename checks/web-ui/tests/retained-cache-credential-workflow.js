/** TASK-470 native stored-ID tests. All cache API requests remain unmocked.
 * CF_CACHE_CREDENTIAL_FIXTURE is the owner-managed mode-0600 runtime JSON.
 * Request native rows at entry to this workflow, then wait for seed_complete.
 * Runtime credentials are used only in password-masked replacement dialogs and
 * request bodies. Do not print fixture data, response bodies or credential values.
 */
const assert = require("node:assert/strict");
const fs = require("node:fs");
const { expect } = require("@playwright/test");

async function retainedCacheCredentialWorkflow(page, baseUrl, apiBaseUrl, captureState) {
  const filename = process.env.CF_CACHE_CREDENTIAL_FIXTURE;
  assert(filename, "CF_CACHE_CREDENTIAL_FIXTURE must identify the native cache fixture JSON file");
  const prepared = JSON.parse(fs.readFileSync(filename, "utf8"));
  assert.equal(prepared.version, 1, "native credential fixture version");
  if (prepared.seed_complete !== true) {
    // The driver may warm providers early, but earlier workflows must not see
    // these seven cache rows. Signal only after step25's original Add/security
    // workflows finish; this marker carries no credential or database values.
    const temporary = `${prepared.seed_request_path}.pending`;
    fs.writeFileSync(temporary, JSON.stringify({ version: 1, step: "25-caches-modal-attic", workflow: "retained-cache-credentials" }), { mode: 0o600, flag: "wx" });
    fs.renameSync(temporary, prepared.seed_request_path);
  }
  await expect.poll(() => {
    try { return JSON.parse(fs.readFileSync(filename, "utf8")).seed_complete === true; }
    catch { return false; }
  }, { timeout: 120000, message: "native fixture seed_complete" }).toBe(true);
  const fixture = JSON.parse(fs.readFileSync(filename, "utf8"));
  assert.equal(fixture.version, 1, "native credential fixture version");
  let checkpointSequence = 0;
  const checkpoint = async (phase, ids) => {
    const sequence = ++checkpointSequence;
    const temporary = `${fixture.checkpoint.request_path}.${sequence}`;
    // This Node-local message contains only phase names and IDs. The private
    // verifier owns raw rows/hashes and acknowledges only completed DB checks.
    fs.writeFileSync(temporary, JSON.stringify({ sequence, phase, ids }), { mode: 0o600, flag: "wx" });
    fs.renameSync(temporary, fixture.checkpoint.request_path);
    await expect.poll(() => {
      try {
        const ack = JSON.parse(fs.readFileSync(fixture.checkpoint.ack_path, "utf8"));
        return ack.sequence === sequence && ack.phase === phase && ack.ok === true && ack.id_count === ids.length;
      } catch { return false; }
    }, { timeout: 60000, message: `private database checkpoint ${phase}` }).toBe(true);
  };
  const kinds = ["attic", "s3", "niks3", "nix", "nix_basic", "http_basic", "legacy_query"];
  const secrets = ["s3_access_key_id", "attic_token", "s3_secret_access_key", "s3_session_token",
    "niks3_auth_token", "niks3_write_client_key", "niks3_read_client_key", "niks3_read_basic_username", "niks3_read_basic_password"];
  const created = [];
  const atticIds = new Set([fixture.attic.id, ...Object.values(fixture.legacy_attic).map(value => value.id)]);
  const mutationRequests = [];
  const observe = request => {
    if (/\/api\/v1\/caches(?:\/|$)/.test(request.url()) &&
      ["POST", "PUT", "DELETE", "PATCH"].includes(request.method()) &&
      !request.url().endsWith("/test-credentials") && !request.url().endsWith("/niks3/discover")) mutationRequests.push(request);
  };
  const api = async (method, route, data) => {
    const cookies = await page.context().cookies(apiBaseUrl);
    const csrf = cookies.find(c => c.name.includes("csrf"));
    const reply = await page.evaluate(async ({ url, method, csrf, data }) => {
      const response = await fetch(url, { method, credentials: "include", headers: {
        "Content-Type": "application/json", ...(csrf ? { "X-CSRF-Token": csrf } : {}),
      }, body: data === undefined ? undefined : JSON.stringify(data) });
      return { status: response.status, data: response.status === 204 ? null : await response.json() };
    }, { url: `${apiBaseUrl}/api/v1${route}`, method, csrf: csrf?.value, data });
    assert(reply.status >= 200 && reply.status < 300, `${method} ${route}: HTTP ${reply.status}`);
    return reply.data;
  };
  const read = async id => {
    const value = await api("GET", `/caches/${id}`);
    for (const field of secrets) assert(value[field] == null, `GET must redact ${field}`);
    if (value.push_to && /^https?:/.test(value.push_to)) {
      const url = new URL(value.push_to);
      assert(!url.username && !url.password, "GET must redact URI userinfo");
      assert(!url.searchParams.has(fixture.legacy_query.query_parameter), "GET must redact legacy credential query");
    }
    return value;
  };
  const unchanged = (before, after, kind) => assert(JSON.stringify(before) === JSON.stringify(after), `${kind} probe/cancel must preserve configuration and timestamps`);
  const outer = page.getByRole("dialog", { name: "Cache destination", exact: true, includeHidden: true });
  let panel, edit;
  const open = async (value, beforeEdit, saveReady = true) => {
    await page.goto(`${baseUrl}/caches`);
    await page.getByText(value.name, { exact: true }).click();
    panel = page.getByRole("dialog").filter({ has: page.getByRole("button", { name: "Edit cache", exact: true }) });
    edit = panel.getByRole("button", { name: "Edit cache", exact: true });
    await expect(edit).toBeVisible();
    if (beforeEdit) await beforeEdit();
    await edit.click();
    await expect(outer).toBeVisible();
    if (saveReady) await expect(outer.getByRole("button", { name: "Save changes", exact: true })).toBeEnabled();
    else await expect(outer.getByRole("button", { name: "Save changes", exact: true })).toBeDisabled();
    await outer.getByRole("button", { name: value.cache_type === "Niks3" ? "Write / API" : "Credentials", exact: true }).click();
  };
  const close = async () => {
    await outer.getByRole("button", { name: "Cancel", exact: true }).click();
    await expect(outer).toBeHidden();
    await expect(edit).toBeFocused();
    await panel.getByRole("button", { name: "Close", exact: true }).click();
  };
  const probe = async (id, ok = true, status = 200, errorMessage = "Legacy credential queries require migration", plane = "write") => {
    const attic = atticIds.has(id);
    const niks3 = id === fixture.niks3.id;
    if (niks3) await outer.getByRole("button", { name: plane === "write" ? "Write / API" : "Read / Pull", exact: true }).click();
    if (attic) await checkpoint("attic-probe-before", [id]);
    const url = `${apiBaseUrl}/api/v1/caches/${id}/test-credentials`;
    const responsePromise = page.waitForResponse(r => r.url() === url && r.request().method() === "POST");
    await outer.getByRole("button", { name: niks3 ? plane === "write" ? "Test write API" : "Test read endpoint" : "Test connection", exact: true }).click();
    const response = await responsePromise;
    assert.equal(response.status(), status, "real stored-ID probe HTTP status");
    const body = response.request().postDataJSON();
    assert(!Object.hasOwn(body, "override"), "probe uses an unwrapped Update patch");
    if (status === 200) {
      const result = await response.json();
      assert.equal(result.ok, ok, "native stored-ID credential result");
      if (attic) {
        assert.equal(result.probe_kind, "attic_cache_config", "native Attic cache-specific probe");
        assert.equal(result.stage, ok ? "complete" : "authentication", "safe native Attic probe stage");
        assert.equal(result.cache_access_valid, ok, "private cache read access");
        assert.equal(result.write_auth_valid, null, "cache-config GET cannot establish push permission");
        if (ok) assert.equal(result.token_auth_valid, true, "private native success authenticated the token");
        else assert.notEqual(result.token_auth_valid, true, "denied replacement cannot claim token authorization");
        await expect(outer).toContainText("Write authorization: Untested");
      }
      if (niks3) {
        assert.equal(body.probe_scope, plane, "Niks3 Test explicitly scopes one plane");
        assert.equal(result.probe_scope, plane);
        assert.equal(result.write_authorization_valid, null, "read-only metadata cannot establish uploads");
        if (plane === "write") {
          assert.equal(result.write_api_reachable, true);
          assert.equal(result.read_access_valid, null);
          assert(body.push_to == null && body.niks3_read_client_key == null && body.niks3_read_basic_password == null);
          await expect(outer.getByText("Write authorization", { exact: true }).locator("..")).toContainText("Untested");
        } else {
          assert.equal(result.read_access_valid, true);
          assert.equal(result.signing_keys_valid, true);
          assert.equal(result.write_api_reachable, null);
          assert(body.niks3_server_url == null && body.niks3_write_client_key == null && body.niks3_auth_token == null);
        }
      } else if (result.server_reachable != null) {
        assert.equal(result.write_auth_valid, null, "read/discovery cannot prove write authorization");
        await expect(outer.getByTestId("niks3-test-result")).toContainText("Write authorization: Untested");
      }
    } else await expect(outer.getByRole("alert")).toContainText(errorMessage);
    if (attic) await checkpoint(status === 400 ? "attic-probe-no-network" : ok ? "attic-probe-after" : "attic-probe-authentication", [id]);
    return body;
  };
  const screenshots = async kind => {
    if (!captureState) return;
    // Only the outer dialog is captured; no replacement secret inputs exist here.
    await captureState(`retained-${kind}-credentials`);
    const viewport = page.viewportSize();
    try {
      await page.setViewportSize({ width: 390, height: 844 });
      await captureState(`retained-${kind}-credentials-390`);
      assert(await outer.evaluate(node => node.scrollWidth <= node.clientWidth), "retained shell fits narrow viewport");
    } finally { await page.setViewportSize(viewport); }
  };
  const fillReplacement = async (kind, original = false) => {
    await outer.getByRole("button", { name: /^(Add|Edit|Replace) credential$/ }).click();
    const nested = page.getByRole("dialog", { name: "Add credential", exact: true });
    await expect(nested).toBeVisible();
    await expect(nested.getByLabel(kind === "attic" ? "Token" : "Secret access key", { exact: true })).toHaveValue("");
    if (captureState) await captureState(`retained-${kind}-replacement-blank`);
    await nested.getByLabel("Name", { exact: true }).fill("Native replacement draft");
    const source = fixture[kind];
    if (kind === "attic") await nested.getByLabel("Token", { exact: true }).fill(original ? source.token : source.replacement_token);
    else {
      await nested.getByLabel("Access key ID", { exact: true }).fill(original ? source.access_key_id : source.replacement_access_key_id);
      await nested.getByLabel("Secret access key", { exact: true }).fill(original ? source.secret_access_key : source.replacement_secret_access_key);
      await nested.getByLabel("AWS session token (optional)", { exact: true }).fill("");
    }
    await nested.getByRole("button", { name: "Save credential", exact: true }).click();
    await expect(nested).toBeHidden();
  };
  const save = async id => {
    const responsePromise = page.waitForResponse(r => r.url() === `${apiBaseUrl}/api/v1/caches/${id}` && r.request().method() === "PUT");
    await outer.getByRole("button", { name: "Save changes", exact: true }).click();
    const response = await responsePromise;
    assert.equal(response.status(), 200, "real native credential Save");
    await expect(outer).toBeHidden();
    await expect(edit).toBeFocused();
    const body = response.request().postDataJSON();
    await panel.getByRole("button", { name: "Close", exact: true }).click();
    return body;
  };
  const discoveryRows = () => [...kinds.map(kind => fixture[kind].id),
    ...Object.values(fixture.legacy_attic).map(value => value.id), fixture.production_write_mtls.id];
  const serverCaField = nested => nested.getByLabel(/^(?:Write )?Server CA bundle(?: \(optional\))?$/i);
  const writeIdentity = async (source, replacement = false, wrong = false) => {
    await outer.getByRole("button", { name: "Write / API", exact: true }).click();
    const mode = outer.getByRole("group", { name: "Write authentication", exact: true }).getByRole("button", { name: "mTLS", exact: true });
    if ((await mode.getAttribute("aria-pressed")) !== "true") await mode.click();
    const opener = outer.getByRole("button", { name: /^(Add credential|Replace|Edit replacement)$/ });
    await opener.click();
    const nested = page.getByRole("dialog", { name: "Write credential", exact: true });
    await expect(nested).toBeVisible();
    await expect(outer).toHaveAttribute("inert", "");
    // Private material is entered only inside the existing identity modal.
    // Capture outer dialogs after closing it; never screenshot filled keys.
    await nested.getByLabel("Client certificate", { exact: true }).fill(wrong ? source.wrong_client_cert : replacement ? source.replacement_write_client_cert : source.write_client_cert);
    await nested.getByLabel("Private key", { exact: true }).fill(wrong ? source.wrong_client_key : replacement ? source.replacement_write_client_key : source.write_client_key);
    await serverCaField(nested).fill(source.server_ca_bundle);
    await nested.getByRole("button", { name: "Use for this cache", exact: true }).click();
    await expect(nested).toBeHidden();
    await expect(opener).toBeFocused();
    await expect(outer.getByLabel("Private key", { exact: true })).toHaveCount(0);
  };
  const discovery = async (id, source, { retained = false, replacement = false, reject = false, token = false } = {}) => {
    await outer.getByRole("button", { name: "Write / API", exact: true }).click();
    const url = `${apiBaseUrl}/api/v1/caches/${id == null ? "niks3/discover" : `${id}/niks3/discover`}`;
    await checkpoint("discovery-before", discoveryRows());
    const waited = page.waitForResponse(r => r.url() === url && r.request().method() === "POST");
    await outer.getByRole("button", { name: "Discover", exact: true }).click();
    const response = await waited;
    const patch = response.request().postDataJSON();
    assert(!Object.hasOwn(patch, "override"), "stored discovery uses an unwrapped Save patch");
    for (const field of ["token", "auth_token", "niks3_auth_token", "niks3_read_client_cert", "niks3_read_client_key", "niks3_read_ca_cert", "read_client_cert", "read_client_key", "read_ca_cert", "s3_access_key_id", "s3_secret_access_key", "s3_session_token"])
      assert(patch[field] == null, `Discovery must not send ${field}`);
    if (retained) {
      for (const field of ["niks3_write_client_cert", "niks3_write_client_key", "niks3_write_ca_cert"])
        assert(patch[field] == null, "retained Discovery resolves stored write identity on the server");
    } else if (replacement) {
      assert(patch.niks3_write_client_key === source.replacement_write_client_key.trim(), "replacement Discovery uses only the confirmed write key");
      assert.equal(patch.niks3_write_ca_cert, source.server_ca_bundle.trim());
    } else if (id == null) {
      assert.equal(patch.server_url, source.server_url);
      assert.equal(patch.niks3_write_auth_mode, "mtls", "unsaved Discovery explicitly selects write mTLS");
      assert.equal(patch.niks3_write_ca_cert, source.server_ca_bundle.trim(), "unsaved Discovery includes server roots A+B");
      assert(patch.niks3_write_client_key === (reject ? source.wrong_client_key : source.write_client_key).trim(), "unsaved Discovery includes only its explicit write draft");
    }
    if (reject) {
      assert(response.status() >= 400 && response.status() < 600, "wrong client certificate must fail real Discovery");
      await expect(outer.getByRole("alert")).toContainText(/discovery|certificate|configuration/i);
    } else {
      assert.equal(response.status(), 200, "real native Discovery HTTP status");
      const reply = await response.json();
      assert.equal(new URL(reply.server_url).href.replace(/\/$/, ""), new URL(source.server_url).href.replace(/\/$/, ""));
      assert.equal(reply.substituter_url, source.substituter_url);
      assert.deepEqual(reply.public_keys, source.public_keys);
      assert(reply.write_auth_valid == null, "Discovery cannot prove write authorization");
      await expect(outer).toContainText("Discovery successful");
      await outer.getByRole("button", { name: "Read / Pull", exact: true }).click();
      await expect(outer.getByLabel("Read / substituter URL", { exact: true })).toHaveValue(source.substituter_url);
      await outer.getByRole("button", { name: "Trust", exact: true }).click();
      assert.deepEqual(await outer.getByRole("textbox", { name: /^Signing public key / }).evaluateAll(nodes => nodes.map(node => node.value)), source.public_keys);
      await outer.getByRole("button", { name: "Write / API", exact: true }).click();
      await expect(outer.getByText("Write authorization", { exact: true }).locator("..")).toContainText("Untested");
    }
    await checkpoint(reject ? "discovery-rejected" : token ? "discovery-token-after" : "discovery-after", discoveryRows());
    return patch;
  };
  const cancelDiscovery = async (existing) => {
    await checkpoint("discovery-before", discoveryRows());
    if (existing) await close();
    else {
      await outer.getByRole("button", { name: "Cancel", exact: true }).click();
      await expect(outer).toBeHidden();
    }
    await checkpoint("discovery-cancel", discoveryRows());
  };
  const discoveryWorkflow = async () => {
    const source = fixture.production_write_mtls;
    const count = mutationRequests.length;
    // Add remains unsaved. The client-issuer CA C never becomes server trust;
    // the explicit server bundle is roots A+B and read credentials stay absent.
    await page.goto(`${baseUrl}/caches`);
    await page.getByRole("button", { name: "Add cache", exact: true }).click();
    await outer.getByRole("button", { name: "Destination", exact: true }).click();
    await outer.getByRole("button", { name: "Niks3", exact: true }).click();
    await outer.getByLabel("Name", { exact: true }).fill("Unsaved native production mTLS discovery");
    await outer.getByRole("button", { name: "Write / API", exact: true }).click();
    await outer.getByLabel("Write / API URL", { exact: true }).fill(source.server_url);
    await writeIdentity(source);
    const submitted = await discovery(null, source);
    if (captureState) await captureState("niks3-unsaved-production-mtls-discovery");
    // Exercise the real server's missing-identity and wrong-trust rejection,
    // independently of any client validation that prevents an empty draft.
    const csrf = (await page.context().cookies(apiBaseUrl)).find(cookie => cookie.name.includes("csrf"));
    const missingIdentity = { ...submitted };
    delete missingIdentity.niks3_write_client_cert;
    delete missingIdentity.niks3_write_client_key;
    for (const body of [missingIdentity, { ...submitted, niks3_write_ca_cert: fixture.niks3.ca_cert.trim() }]) {
      await checkpoint("discovery-before", discoveryRows());
      const status = await page.evaluate(async ({ url, body, csrf }) => (await fetch(url, {
        method: "POST", credentials: "include", headers: { "Content-Type": "application/json", ...(csrf ? { "X-CSRF-Token": csrf } : {}) }, body: JSON.stringify(body),
      })).status, { url: `${apiBaseUrl}/api/v1/caches/niks3/discover`, body, csrf: csrf?.value });
      assert(status >= 400 && status < 600, "missing write identity or wrong server trust must fail actual Discovery");
      await checkpoint("discovery-rejected", discoveryRows());
    }
    await writeIdentity(source, false, true);
    await discovery(null, source, { reject: true });
    await cancelDiscovery(false);

    const before = await read(source.id);
    assert.equal(before.niks3_write_auth_mode, "mtls");
    assert.equal(before.niks3_read_auth_mode, "none");
    assert.equal(before.niks3_write_mtls_configured, true);
    await open(before);
    await expect(outer.getByTestId("niks3-write-credential-state")).toContainText("Current configured credential");
    await discovery(source.id, source, { retained: true });
    if (captureState) await captureState("niks3-retained-production-mtls-discovery");
    await cancelDiscovery(true);
    unchanged(before, await read(source.id), "stored-mTLS Discovery/Cancel");
    for (const mode of ["false", "absent"]) {
      const routePath = new RegExp(`/api/v1/caches/${source.id}$`);
      let projected = 0;
      const hint = async route => {
        if (route.request().method() !== "GET" || projected) return route.continue();
        const response = await route.fetch();
        const value = { ...(await response.json()) };
        assert(value.niks3_write_client_key == null);
        if (mode === "false") value.niks3_write_mtls_configured = false;
        else delete value.niks3_write_mtls_configured;
        projected++;
        return route.fulfill({ response, json: value });
      };
      try {
        await open(before, () => page.route(routePath, hint));
        assert.equal(projected, 1, "only fresh GET presentation hints are projected");
        await discovery(source.id, source, { retained: true });
        await cancelDiscovery(true);
      } finally { await page.unroute(routePath, hint); }
      unchanged(before, await read(source.id), "unknown stored write identity Discovery");
    }
    await open(before);
    await writeIdentity(source, true);
    await discovery(source.id, source, { replacement: true });
    if (captureState) await captureState("niks3-production-mtls-replacement-discovery");
    await cancelDiscovery(true);
    unchanged(before, await read(source.id), "replacement Discovery without Save");

    // The original token/private-read row retains its identity. Only the
    // unsaved write URL changes to an observed native public metadata ingress.
    // Neither its stored token nor its stored read identity may reach Discovery.
    const legacy = await read(fixture.niks3.id);
    await open(legacy);
    await outer.getByRole("button", { name: "Write / API", exact: true }).click();
    await outer.getByLabel("Write / API URL", { exact: true }).fill(source.token_discovery_server_url);
    await discovery(legacy.id, { server_url: source.token_discovery_server_url,
      substituter_url: fixture.niks3.substituter_url, public_keys: fixture.niks3.public_keys }, { retained: true, token: true });
    await cancelDiscovery(true);
    unchanged(legacy, await read(legacy.id), "token-mode anonymous Discovery");
    assert.equal(mutationRequests.length, count, "unsaved/retained/replacement Discovery and Cancel never save");
    await checkpoint("discovery-complete", discoveryRows());
    console.log("TASK-470 native Discovery passed: write ingress mTLS/custom server bundle, retained/unknown/replacement identity, no Bearer/read secrets/uploads, exact raw immutability");
  };
  const basicWorkflow = async () => {
    const source = fixture.production_basic;
    let id = null;
    const ids = () => id == null ? [] : [id];
    const rail = name => outer.getByRole("button", { name, exact: true }).click();
    const keys = () => outer.getByRole("textbox", { name: /^Signing public key / });
    const shots = async state => {
      if (!captureState) return;
      const viewport = page.viewportSize();
      await captureState(`niks3-five-rail-${state}`);
      try {
        await page.setViewportSize({ width: 390, height: 844 });
        assert(await outer.evaluate(node => node.scrollWidth <= node.clientWidth), "five-rail Niks3 form fits 390px");
        await captureState(`niks3-five-rail-${state}-390`);
      } finally { await page.setViewportSize(viewport); }
    };
    const basicIdentity = async (replacement = false, wrong = false) => {
      await rail("Read / Pull");
      const mode = outer.getByRole("group", { name: "Read authentication", exact: true }).getByRole("button", { name: "Basic", exact: true });
      if ((await mode.getAttribute("aria-pressed")) !== "true") await mode.click();
      const opener = outer.getByRole("button", { name: /^(Add credential|Replace|Edit replacement)$/ });
      await opener.click();
      const nested = page.getByRole("dialog", { name: "Read credential", exact: true });
      await expect(nested.getByRole("heading")).toContainText(/(?:Add|Replace) Read credential · basic/i);
      await expect(outer).toHaveAttribute("inert", "");
      await expect(nested.getByLabel("Password", { exact: true })).toHaveAttribute("type", "password");
      const prefix = replacement ? "replacement_" : "";
      await nested.getByLabel("Username", { exact: true }).fill(source[prefix + "username"]);
      await nested.getByLabel("Password", { exact: true }).fill(wrong ? "incorrect-native-password" : source[prefix + "password"]);
      assert(await nested.getByLabel("Password", { exact: true }).evaluate((node, expected) => node.value === expected,
        wrong ? "incorrect-native-password" : source[prefix + "password"]), "Basic draft retains complete password bytes");
      await nested.getByRole("button", { name: "Use for this cache", exact: true }).click();
      await expect(nested).toBeHidden();
      await expect(opener).toBeFocused();
      await expect(outer.getByLabel("Password", { exact: true })).toHaveCount(0);
      await expect(outer.getByLabel("Username", { exact: true })).toHaveCount(0);
    };
    const nativeDiscover = async (conflict = false) => {
      await rail("Write / API");
      await checkpoint("basic-before", ids());
      const url = `${apiBaseUrl}/api/v1/caches/${id == null ? "niks3/discover" : `${id}/niks3/discover`}`;
      const waited = page.waitForResponse(r => r.url() === url && r.request().method() === "POST");
      await outer.getByRole("button", { name: "Discover", exact: true }).click();
      const response = await waited;
      assert.equal(response.status(), 200);
      const result = await response.json();
      assert.equal(result.substituter_url, source.substituter_url);
      assert.deepEqual(result.public_keys, source.public_keys);
      const patch = response.request().postDataJSON();
      for (const field of ["niks3_auth_token", "niks3_read_basic_username", "niks3_read_basic_password", "niks3_read_client_key", "s3_secret_access_key"])
        assert(patch[field] == null, "Discovery sends only the selected write transport");
      if (id != null) assert(patch.niks3_write_client_key == null, "retained Edit Discovery omits the private key");
      await expect(outer).toContainText(conflict ? "Existing read URL or keys differ" : "Discovery successful");
      await checkpoint("basic-after-discovery", ids());
    };
    const planeTest = async (plane, ok = true, retained = false, redirect = false) => {
      await rail(plane === "write" ? "Write / API" : "Read / Pull");
      await checkpoint("basic-before", ids());
      const url = `${apiBaseUrl}/api/v1/caches/${id == null ? "test-credentials" : `${id}/test-credentials`}`;
      const waited = page.waitForResponse(r => r.url() === url && r.request().method() === "POST");
      await outer.getByRole("button", { name: plane === "write" ? "Test write API" : "Test read endpoint", exact: true }).click();
      const response = await waited;
      assert.equal(response.status(), 200);
      const result = await response.json();
      const patch = response.request().postDataJSON();
      assert.equal(patch.probe_scope, plane);
      assert.equal(result.probe_scope, plane);
      assert.equal(result.ok, ok);
      assert.equal(result.write_authorization_valid, null);
      if (plane === "write") {
        assert.equal(result.write_api_reachable, true);
        assert.equal(result.read_access_valid, null);
        assert(patch.push_to == null && patch.niks3_read_basic_username == null && patch.niks3_read_basic_password == null);
        assert(patch.niks3_auth_token == null);
        await expect(outer.getByText("Write authorization", { exact: true }).locator("..")).toContainText("Untested");
      } else {
        assert.equal(result.read_access_valid, ok);
        if (ok) assert.equal(result.signing_keys_valid, true);
        assert.equal(result.write_api_reachable, null);
        for (const field of ["niks3_server_url", "niks3_write_client_cert", "niks3_write_client_key", "niks3_write_ca_cert", "niks3_auth_token"])
          assert(patch[field] == null, "read-only Test excludes all write transport fields");
        await expect(outer.getByText("Read access", { exact: true }).locator("..")).toContainText(ok ? "Verified" : "Failed");
      }
      if (retained) for (const field of secrets) assert(patch[field] == null, "retained plane Test omits all stored secrets");
      await checkpoint(plane === "write" ? "basic-after-write" : redirect ? "basic-after-read-redirect" : ok ? "basic-after-read" : "basic-after-read-denied", ids());
      return patch;
    };
    const cancel = async () => {
      await checkpoint("basic-before", ids());
      await close();
      await checkpoint("basic-after-cancel", ids());
    };
    await page.goto(`${baseUrl}/caches`);
    await page.getByRole("button", { name: "Add cache", exact: true }).click();
    await rail("Destination");
    await outer.getByRole("button", { name: "Niks3", exact: true }).click();
    const name = `task470-native-basic-ui-${Date.now()}`;
    await outer.getByLabel("Name", { exact: true }).fill(name);
    const environmentIds = await api("GET", `/caches/${fixture.attic.id}/environments`);
    const environments = await api("GET", "/environments");
    const environment = (Array.isArray(environments) ? environments : environments.environments).find(value => environmentIds.includes(value.id));
    assert(environment, "native Add uses an existing fixture environment");
    await outer.getByRole("button", { name: environment.name, exact: true }).click();
    // Disabled test fixtures cannot enqueue provider work while raw-state proof
    // is running. Advanced defaults remain the accepted server defaults.
    await outer.getByRole("checkbox", { name: "Enabled", exact: true }).uncheck();
    await shots("destination");
    await rail("Write / API");
    await outer.getByLabel("Write / API URL", { exact: true }).fill(source.server_url);
    await writeIdentity(source);
    await nativeDiscover();
    await shots("write-discovered");
    await rail("Read / Pull");
    await expect(outer.getByLabel("Read / substituter URL", { exact: true })).toHaveValue(source.substituter_url);
    await outer.getByLabel("Read / substituter URL", { exact: true }).fill(fixture.nix.url.replace(/\/nix-cache-info$/, ""));
    await rail("Trust");
    const reversed = [...source.public_keys].reverse();
    for (let index = 0; index < reversed.length; index++) await keys().nth(index).fill(reversed[index]);
    await nativeDiscover(true);
    await shots("discovery-conflict-review");
    await rail("Read / Pull");
    await expect(outer.getByLabel("Read / substituter URL", { exact: true })).toHaveValue(fixture.nix.url.replace(/\/nix-cache-info$/, ""));
    await rail("Trust");
    assert.deepEqual(await keys().evaluateAll(nodes => nodes.map(node => node.value)), reversed, "Discovery cannot silently overwrite nonempty signing keys");
    await rail("Write / API");
    await outer.getByRole("button", { name: "Apply discovered metadata", exact: true }).click();
    await basicIdentity();
    await shots("read-basic");
    await rail("Trust");
    assert.deepEqual(await keys().evaluateAll(nodes => nodes.map(node => node.value)), source.public_keys);
    for (const value of source.public_keys) assert.equal(Buffer.from(value.split(":")[1], "base64").length, 32);
    await shots("trust-multiple-keys");
    await rail("Advanced");
    for (const [label, value] of [["Parallel uploads", "1"], ["Retry attempts", "3"], ["Push timeout", "3600"]])
      await expect(outer.getByLabel(label, { exact: true })).toHaveValue(value);
    await expect(outer.getByRole("checkbox", { name: "Require signatures", exact: true })).toBeChecked();
    await shots("advanced-defaults");
    await planeTest("write");
    await shots("write-scoped-evidence");
    await planeTest("read");
    await shots("read-scoped-evidence");
    const waited = page.waitForResponse(r => r.url() === `${apiBaseUrl}/api/v1/caches` && r.request().method() === "POST");
    await outer.getByRole("button", { name: "Add cache", exact: true }).click();
    const response = await waited;
    assert([200, 201].includes(response.status()));
    const saved = await response.json(); id = saved.id;
    const submitted = response.request().postDataJSON();
    assert.deepEqual(submitted.environment_ids, [environment.id], "new Save includes atomic environment scope");
    assert(submitted.niks3_read_basic_username === source.username, "Save includes the complete Basic username");
    assert(submitted.niks3_read_basic_password === source.password, "Save preserves legal password whitespace");
    assert(submitted.niks3_auth_token == null && submitted.s3_secret_access_key == null && !submitted.push_to.includes("@"));
    await expect(outer).toBeHidden();
    await checkpoint("basic-added", [id]);
    const before = await read(id);
    assert.equal(before.niks3_read_basic_configured, true);
    assert.equal(before.niks3_write_mtls_configured, true);
    await open(before);
    await expect(outer.getByTestId("niks3-write-credential-state")).toContainText("Current configured credential");
    await planeTest("write", true, true);
    await nativeDiscover();
    await rail("Read / Pull");
    await expect(outer.getByTestId("niks3-read-credential-state")).toContainText("Current configured credential");
    await planeTest("read", true, true);
    await shots("edit-retained-basic");
    // Opening then canceling the nested modal must reveal neither original
    // credential and must restore the parent and its invoking button's focus.
    const replace = outer.getByRole("button", { name: "Replace", exact: true });
    await replace.click();
    const nested = page.getByRole("dialog", { name: "Read credential", exact: true });
    await expect(nested.getByLabel("Username", { exact: true })).toHaveValue("");
    await expect(nested.getByLabel("Password", { exact: true })).toHaveValue("");
    await expect(outer).toHaveAttribute("inert", "");
    await page.keyboard.press("Escape");
    await expect(nested).toBeHidden(); await expect(replace).toBeFocused();
    await basicIdentity(true);
    await planeTest("read");
    await shots("edit-basic-replacement-unsaved");
    await outer.getByRole("button", { name: "Use current credential", exact: true }).click();
    await expect(outer.getByTestId("niks3-read-credential-state")).toContainText("Current configured credential");
    await planeTest("read", true, true);
    await basicIdentity(true, true);
    await planeTest("read", false);
    await outer.getByRole("button", { name: "Discard", exact: true }).click();
    await expect(outer.getByTestId("niks3-read-credential-state")).toContainText("Current configured credential");
    await planeTest("read", true, true);
    await outer.getByLabel("Read / substituter URL", { exact: true }).fill(source.substituter_url + "/redirect");
    await planeTest("read", false, true, true);
    await outer.getByLabel("Read / substituter URL", { exact: true }).fill(source.substituter_url);
    await planeTest("read", true, true);
    await cancel();
    unchanged(before, await read(id), "Basic replacement Test/Discover/Cancel");
    for (const hintMode of ["false", "absent"]) {
      const routePath = new RegExp(`/api/v1/caches/${id}$`);
      let projected = 0;
      const hints = async route => {
        if (route.request().method() !== "GET" || projected) return route.continue();
        const response = await route.fetch(); const value = { ...(await response.json()) };
        assert(value.niks3_read_basic_username == null && value.niks3_read_basic_password == null);
        if (hintMode === "false") value.niks3_read_basic_configured = false;
        else delete value.niks3_read_basic_configured;
        projected++;
        return route.fulfill({ response, json: value });
      };
      try {
        await open(before, () => page.route(routePath, hints));
        assert.equal(projected, 1);
        await planeTest("read", true, true);
        await cancel();
      } finally { await page.unroute(routePath, hints); }
      unchanged(before, await read(id), "Basic configured hints cannot block server-authoritative read Test");
    }
    await checkpoint("basic-complete", [id]);
    console.log("TASK-470 five-rail native Basic Add/Edit passed: real mTLS Discovery, explicit conflict review, atomic encrypted Save, scoped tests and no replacement persistence");
  };
  const metricsWorkflow = async () => {
    const records = [fixture.metrics.v16, fixture.metrics.v18];
    const names = await Promise.all(records.map(value => read(value.id)));
    await checkpoint("metrics-before", records.map(value => value.id));
    const replies = new Map();
    const collect = async response => {
      for (const value of records) if (response.request().method() === "GET" && response.url() === `${apiBaseUrl}/api/v1/caches/${value.id}/metrics`) {
        assert.equal(response.status(), 200);
        assert.equal(response.headers()["cache-control"], "no-store");
        replies.set(value.id, await response.json());
      }
    };
    page.on("response", collect);
    try {
      await page.goto(`${baseUrl}/caches`);
      await page.getByRole("button", { name: "Cards", exact: true }).click();
      await expect.poll(() => records.every(value => replies.has(value.id)), { message: "card metrics come from real scoped API responses" }).toBe(true);
      const absent = replies.get(records[0].id), available = replies.get(records[1].id);
      assert.equal(absent.status, "unavailable");
      assert.equal(absent.reason_code, "endpoint_unavailable");
      assert(absent.storage_bytes == null && absent.object_count == null && absent.path_count == null, "remote 1.6 must not display fabricated zero totals");
      assert.equal(available.status, "available");
      assert.equal(available.storage_bytes, 0); assert.equal(available.object_count, 0);
      assert.equal(available.storage_bytes_basis, "reported_logical");
      assert.equal(available.object_count_basis, "live_tracked_objects");
      assert.equal(available.path_count, null);
      if (captureState) await captureState("native-metrics-real-v16-v18-cards");
      await page.getByText(names[1].name, { exact: true }).click();
      const panel = page.getByRole("dialog").filter({ has: page.getByRole("button", { name: "Edit cache", exact: true }) });
      await expect(panel.getByText("0.00 GiB", { exact: true })).toBeVisible();
      await expect(panel.getByText("0 objects", { exact: true })).toBeVisible();
      await expect(panel).toContainText(/reported logical/i);
      await expect(panel).toContainText(/live tracked objects/i);
      if (captureState) {
        await captureState("native-metrics-real-v18-detail");
        const viewport = page.viewportSize();
        try { await page.setViewportSize({ width: 390, height: 844 }); await captureState("native-metrics-real-v18-detail-390"); }
        finally { await page.setViewportSize(viewport); }
      }
      await panel.getByRole("button", { name: "Close", exact: true }).click();
      await page.getByText(names[0].name, { exact: true }).click();
      const missing = page.getByRole("dialog").filter({ has: page.getByRole("button", { name: "Edit cache", exact: true }) });
      await expect(missing).toContainText("The metrics endpoint is unavailable");
      await expect(missing.getByText("0.00 GiB", { exact: true })).toHaveCount(0);
      await expect(missing.getByText("0 objects", { exact: true })).toHaveCount(0);
      if (captureState) await captureState("native-metrics-real-v16-unavailable-detail");
      await missing.getByRole("button", { name: "Close", exact: true }).click();
      for (let index = 0; index < records.length; index++) unchanged(names[index], await read(records[index].id), "read-only card metrics");
    } finally { page.off("response", collect); }
    await checkpoint("metrics-after", records.map(value => value.id));
    console.log("TASK-470 real native metrics cards passed: 1.6 unavailable, isolated 1.8 empty totals with accurate bases, no raw-state changes");
  };
  page.on("request", observe);
  try {
    await page.evaluate(() => localStorage.setItem("cf.coach.ui.v2", JSON.stringify({ panel: "dismissed", track: "setup" })));
    await discoveryWorkflow();
    for (const kind of kinds) {
      const id = fixture[kind]?.id;
      assert(Number.isInteger(id) && id > 0, `native ${kind} fixture ID`);
      const before = await read(id);
      if (kind === "attic") {
        assert.equal(before.push_to, fixture.attic.server_url, "primary Attic stores the production server/base shape");
        assert.equal(before.attic_cache_name, fixture.attic.cache_name);
      }
      const count = mutationRequests.length;
      await open(before);
      if (kind === "attic" || kind === "s3") {
        assert.equal(before[kind === "attic" ? "attic_token_configured" : "s3_credentials_configured"], true);
        await expect(outer.getByLabel("Credential", { exact: true })).toHaveValue("__current__");
        await expect(outer.getByLabel("Credential", { exact: true }).locator("option:checked")).toHaveText("Current configured credential");
      } else if (kind === "niks3") {
        await expect(outer.getByTestId("niks3-write-credential-state")).toContainText("Current configured credential");
        await outer.getByRole("button", { name: "Read / Pull", exact: true }).click();
        await expect(outer.getByTestId("niks3-read-credential-state")).toContainText("Current configured credential");
        await outer.getByRole("button", { name: "Write / API", exact: true }).click();
      } else {
        await expect(outer.getByRole("button", { name: "Add credential", exact: true })).toHaveCount(0);
        if (kind.endsWith("_basic")) {
          assert.equal(before.http_basic_auth_configured, true);
          await expect(outer.getByLabel("Read access").locator("option:checked")).toHaveText("Current configured credential");
          await expect(outer.getByText("Replacing Basic credentials is not available", { exact: false })).toBeVisible();
        }
        if (kind === "legacy_query") {
          assert.equal(before.legacy_query_credentials_configured, true);
          await expect(outer.locator("header")).toContainText("Migration required");
        }
      }
      for (const label of ["Attic token", "AWS secret access key", "AWS session token (optional)", "Write token", "Write private key", "Read private key"])
        await expect(outer.getByLabel(label, { exact: true })).toHaveCount(0);
      const body = await probe(id, true, kind === "legacy_query" ? 400 : 200);
      if (kind === "niks3") {
        const readBody = await probe(id, true, 200, undefined, "read");
        for (const field of secrets) assert(readBody[field] == null, "retained private read Test omits every secret");
        await outer.getByRole("button", { name: "Write / API", exact: true }).click();
      }
      for (const field of secrets) assert(!Object.hasOwn(body, field), `retained probe omits ${field}`);
      assert(body.push_to == null || kind === "niks3", "unchanged sanitized HTTP URL is not submitted");
      await screenshots(kind);
      if (kind === "attic" || kind === "s3") {
        const replace = outer.getByRole("button", { name: "Replace credential", exact: true });
        await replace.click();
        const nested = page.getByRole("dialog", { name: "Add credential", exact: true });
        await expect(nested.getByLabel("Name", { exact: true })).toBeFocused();
        const secret = nested.getByLabel(kind === "attic" ? "Token" : "Secret access key", { exact: true });
        await expect(secret).toHaveValue("");
        if (captureState) await captureState(`retained-${kind}-replacement-cancel-blank`);
        await secret.fill("fixture-canceled-replacement");
        await page.keyboard.press("Escape");
        await expect(nested).toBeHidden();
        await expect(replace).toBeFocused();
        await expect(outer.getByLabel("Credential", { exact: true })).toHaveValue("__current__");
        if (kind === "attic") {
          // A correctly signed JWT for another cache is a genuine permission
          // failure. Test the draft, then Cancel; never save a denied identity.
          await replace.click();
          const denied = page.getByRole("dialog", { name: "Add credential", exact: true });
          await denied.getByLabel("Name", { exact: true }).fill("Denied native replacement");
          await denied.getByLabel("Token", { exact: true }).fill(fixture.attic.denied_token);
          await denied.getByRole("button", { name: "Save credential", exact: true }).click();
          await probe(id, false);
          await close();
          unchanged(before, await read(id), "denied Attic replacement Cancel");
          await open(before);
          await expect(outer.getByLabel("Credential", { exact: true })).toHaveValue("__current__");
        }
        await fillReplacement(kind);
        const replacement = await probe(id);
        assert(Boolean(replacement[kind === "attic" ? "attic_token" : "s3_secret_access_key"]), "confirmed replacement is included in probe");
        if (kind === "s3") assert.equal(replacement.s3_session_token, "", "full replacement clears prior session material");
        await outer.getByRole("button", { name: "Edit credential", exact: true }).click();
        const editDraft = page.getByRole("dialog", { name: "Add credential", exact: true });
        await editDraft.getByLabel(kind === "attic" ? "Token" : "Secret access key", { exact: true }).fill("fixture-canceled-edit");
        await page.keyboard.press("Escape");
        await expect(editDraft).toBeHidden();
        const canceledEdit = await probe(id);
        assert(JSON.stringify(canceledEdit) === JSON.stringify(replacement), "canceling an edited replacement restores the confirmed draft snapshot");
      }
      if (kind.endsWith("_basic")) {
        await outer.getByRole("button", { name: "Destination", exact: true }).click();
        await outer.getByLabel("URL", { exact: true }).fill(fixture[kind].authority_change_url);
        await outer.getByRole("button", { name: "Credentials", exact: true }).click();
        await expect(outer.getByText("stored URL credentials will not be forwarded", { exact: false })).toBeVisible();
        const changed = await probe(id);
        assert.equal(changed.push_to, fixture[kind].authority_change_url, "new authority is explicit");
        // The fixture owner audits Authorization presence on this native route.
        // HTTP success proves reachability; its separate observer proves that
        // the server did not forward retained Basic auth to the new authority.
      }
      if (kind === "niks3") {
        // Separate native read replacement and write-mode conversion snapshots.
        for (const plane of ["Read", "Write"]) {
          await outer.getByRole("button", { name: plane === "Write" ? "Write / API" : "Read / Pull", exact: true }).click();
          const mode = outer.getByRole("group", { name: `${plane} authentication`, exact: true }).getByRole("button", { name: "mTLS", exact: true });
          if ((await mode.getAttribute("aria-pressed")) !== "true") await mode.click();
          await outer.getByRole("button", { name: /^(Add credential|Replace|Edit replacement)$/ }).click();
          const nested = page.getByRole("dialog", { name: `${plane} credential`, exact: true });
          await expect(nested.getByLabel("Private key", { exact: true })).toHaveValue("");
          if (captureState) await captureState(`retained-niks3-${plane.toLowerCase()}-replacement-blank`);
          await nested.getByLabel("Client certificate", { exact: true }).fill(fixture.niks3[`${plane.toLowerCase()}_client_cert`]);
          await nested.getByLabel("Private key", { exact: true }).fill(fixture.niks3[`${plane.toLowerCase()}_client_key`]);
          await serverCaField(nested).fill(fixture.niks3.ca_cert);
          await nested.getByRole("button", { name: "Use for this cache", exact: true }).click();
          await expect(nested).toBeHidden();
          await probe(id, true, 200, undefined, plane.toLowerCase());
        }
      }
      await close();
      unchanged(before, await read(id), kind);
      assert.equal(mutationRequests.length, count, "retained/replacement probes and canceled forms do not save");
      console.log(`TASK-470 real ${kind} stored-ID probe: HTTP ${kind === "legacy_query" ? 400 : 200}; no mutation`);
    }
    // Direct SQL legacy rows are separate from the seven current-API rows.
    // Metadata-only projections model stale clients; every Test POST is real.
    for (const [kind, value] of Object.entries(fixture.legacy_attic)) {
      const before = await read(value.id);
      assert.equal(before.push_to, fixture.attic.server_url, "direct legacy Attic stores the server/base shape without query markers");
      assert.equal(before.attic_cache_name, fixture.attic.cache_name);
      assert.equal(before.attic_token_configured, value.token_expected, "real legacy GET configured flag matches stored data");
      const count = mutationRequests.length;
      const collection = /\/api\/v1\/caches(?:\?.*)?$/;
      const listProjection = async route => {
        if (route.request().method() !== "GET") return route.continue();
        const response = await route.fetch();
        const body = await response.json();
        const safe = body.map(row => {
          if (row.id !== value.id) return row;
          for (const field of secrets) assert(row[field] == null, "metadata projection must remain secret-free");
          const projected = { ...row };
          if (kind === "legacy_encrypted") delete projected.attic_token_configured;
          else projected.attic_token_configured = false;
          return projected;
        });
        return route.fulfill({ response, json: safe });
      };
      await page.route(collection, listProjection);
      try {
        await open(before, undefined, value.token_expected);
        if (value.token_expected) {
          await expect(outer.getByLabel("Credential", { exact: true })).toHaveValue("__current__");
          const body = await probe(value.id);
          assert(!Object.hasOwn(body, "attic_token"), "direct legacy stored-ID Test omits token");
        } else {
          await expect(outer.getByText("Current configured credential", { exact: true })).toHaveCount(0);
          await expect(outer.getByRole("button", { name: "Test connection", exact: true })).toBeEnabled();
          const body = await probe(value.id, false, 400, /token|credential|configuration/i);
          assert(!Object.hasOwn(body, "attic_token"), "missing legacy stored-ID Test omits token");
        }
        await screenshots(kind + "-stale-list");
        await close();
      } finally { await page.unroute(collection, listProjection); }
      unchanged(before, await read(value.id), kind);
      if (value.token_expected) {
        for (const mode of ["false", "absent"]) {
          const metadata = new RegExp(`/api/v1/caches/${value.id}$`);
          let projected = 0;
          const freshProjection = async route => {
            if (route.request().method() !== "GET" || projected > 0) return route.continue();
            const response = await route.fetch();
            const body = await response.json();
            for (const field of secrets) assert(body[field] == null, "fresh metadata projection must remain secret-free");
            const safe = { ...body };
            if (mode === "false") safe.attic_token_configured = false;
            else delete safe.attic_token_configured;
            projected += 1;
            return route.fulfill({ response, json: safe });
          };
          try {
            await open(before, () => page.route(metadata, freshProjection), false);
            assert.equal(projected, 1, "only one fresh metadata GET is projected");
            await expect(outer.getByText("Current configured credential", { exact: true })).toHaveCount(0);
            await expect(outer.getByRole("button", { name: "Test connection", exact: true })).toBeEnabled();
            const body = await probe(value.id);
            assert(!Object.hasOwn(body, "attic_token"), "unknown metadata still uses real stored-ID probe without token");
            if (captureState) await captureState(`${kind}-fresh-${mode}-real-test`);
            await close();
          } finally { await page.unroute(metadata, freshProjection); }
          unchanged(before, await read(value.id), kind);
        }
      }
      assert.equal(mutationRequests.length, count, "all direct legacy Test/Cancel cases precede Save without mutation");
      console.log(`TASK-470 direct SQL ${kind}: real stored-ID Test ${value.token_expected ? 200 : 400}; no token body/no mutation`);
    }
    // Every original source finishes Test/Cancel before any intentional Save.
    // A redacted GET comparison alone cannot prove ciphertext retention, so
    // block the Save phase until the VM-private whole-row verifier acknowledges.
    assert.equal(mutationRequests.length, 0, "all source Test/Cancel phases precede every Save");
    await checkpoint("source-pre-save", kinds.map(kind => fixture[kind].id));
    await checkpoint("legacy-pre-save", Object.values(fixture.legacy_attic).map(value => value.id));
    await metricsWorkflow();
    await basicWorkflow();
    for (const kind of ["legacy_plain", "legacy_encrypted"]) {
      const value = fixture.legacy_attic[kind];
      const before = await read(value.id);
      await open(before);
      await outer.getByRole("button", { name: "Destination", exact: true }).click();
      await outer.getByLabel("Name", { exact: true }).fill(before.name + "-roundtrip");
      const patch = await save(value.id);
      assert(!Object.hasOwn(patch, "attic_token"), "unrelated legacy Save omits credential replacement");
      await checkpoint("legacy-saved", [value.id]);
      const saved = await read(value.id);
      await open(saved);
      await probe(value.id);
      await close();
      unchanged(saved, await read(value.id), kind);
      await checkpoint("legacy-retained", [value.id]);
    }
    for (const kind of ["nix_basic", "http_basic"]) {
        const id = fixture[kind].id;
        const before = await read(id);
        // Intentional Save permits name/updated_at changes and assignment
        // created_at renewal, but preserves exact membership and raw URI.
        await open(before);
        await outer.getByRole("button", { name: "Destination", exact: true }).click();
        await outer.getByLabel("Name", { exact: true }).fill(before.name + "-roundtrip");
        const saved = await save(id);
        assert(saved.push_to == null, "unrelated Basic Save omits sanitized URL");
        assert(saved.cache_type == null && saved.enabled == null, "Basic roundtrip preserves exact type and disabled status");
        const after = await read(id);
        assert.equal(after.http_basic_auth_configured, true, "Basic credentials retained after Save");
        assert.equal(after.push_to, before.push_to, "Basic public URL unchanged");
        await open(after);
        const retained = await probe(id);
        assert(retained.push_to == null, "post-Save Basic Test resolves raw URL only on the server");
        await close();
        unchanged(after, await read(id), kind);
        console.log(`TASK-470 real ${kind} unrelated Save 200 / retained Test 200; sanitized URL omitted`);
    }
    // Native replacement Test/Cancel/Save uses disposable real configurations.
    const ids = await api("GET", `/caches/${fixture.attic.id}/environments`);
    for (const kind of ["attic", "s3"]) {
      const source = fixture[kind];
      const value = await api("POST", "/caches", {
        name: `task470-native-replacement-${kind}-${Date.now()}`, cache_type: kind === "attic" ? "Attic" : "S3", enabled: false, environment_ids: ids,
        ...(kind === "attic" ? { push_to: source.server_url, attic_cache_name: source.cache_name, attic_public_key: source.public_key, attic_token: source.token }
          : { push_to: `s3://${source.bucket}`, s3_region: source.region, s3_endpoint_url: source.endpoint, s3_access_key_id: source.access_key_id, s3_secret_access_key: source.secret_access_key }),
      });
      created.push(value.id);
      if (kind === "attic") atticIds.add(value.id);
      await checkpoint("replacement-baseline", [value.id]);
      await open(value);
      await fillReplacement(kind);
      const tested = await probe(value.id);
      unchanged(value, await read(value.id), `${kind} replacement draft`);
      await checkpoint("replacement-pre-save", [value.id]);
      const saved = await save(value.id);
      assert(JSON.stringify(tested) === JSON.stringify(saved), "Test and Save use the exact same Update patch");
      await checkpoint("replacement-saved", [value.id]);
      const after = await read(value.id);
      await open(after);
      const retained = await probe(value.id);
      for (const field of secrets) assert(!Object.hasOwn(retained, field), "saved replacement is tested by ID without secret retrieval");
      await close();
      unchanged(after, await read(value.id), kind);
      await checkpoint("replacement-retained", [value.id]);
      console.log(`TASK-470 real ${kind} replacement Test 200 / Save 200 / retained Test 200`);
    }
    // Legacy queries can be saved unchanged, but cannot be replayed by Test.
    const query = await read(fixture.legacy_query.id);
    await open(query);
    await outer.getByRole("button", { name: "Destination", exact: true }).click();
    await outer.getByLabel("Name", { exact: true }).fill(query.name + "-roundtrip");
    const patch = await save(query.id);
    assert(patch.push_to == null, "unrelated query-credential Save omits sanitized URL");
    const renamed = await read(query.id);
    await open(renamed);
    await probe(query.id, false, 400);
    await close();
    unchanged(renamed, await read(query.id), "legacy query");
    console.log("TASK-470 legacy query unrelated Save 200 / retained Test 400; URI not replayed");
  } finally {
    for (const id of created) await api("DELETE", `/caches/${id}`);
    page.off("request", observe);
  }
}

module.exports = { retainedCacheCredentialWorkflow };
