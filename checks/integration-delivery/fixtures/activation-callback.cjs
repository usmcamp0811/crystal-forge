// Private runtime identity preparation and a transparent response recorder.
// The production agent signs requests; the real server authenticates/persists
// them. Never retain request bodies, headers, signatures, or private key values.
"use strict";
const fs = require("node:fs");
const crypto = require("node:crypto");
const http = require("node:http");

const [mode, directory, pemPath] = process.argv.slice(2);
if (mode === "config") {
  const jwk = crypto.createPrivateKey(fs.readFileSync(pemPath)).export({ format: "jwk" });
  const seed = Buffer.from(jwk.d, "base64url");
  if (jwk.crv !== "Ed25519" || seed.length !== 32) throw new Error("Invalid callback fixture key");
  // The agent expects a base64 raw 32-byte seed, not the Node fixture's PEM.
  fs.writeFileSync(`${directory}/seed`, seed.toString("base64"), { mode: 0o600 });
  fs.writeFileSync(`${directory}/config.toml`,
    `[client]\nserver_host = "127.0.0.1"\nserver_port = 3101\nprivate_key = ${JSON.stringify(`${directory}/seed`)}\n`,
    { mode: 0o600 });
} else if (mode === "relay") {
  const responses = [];
  let requests = 0;
  const record = () => fs.writeFileSync(`${directory}/responses.json`, JSON.stringify(responses), { mode: 0o600 });
  const recordCount = () => fs.writeFileSync(`${directory}/request-count.json`, JSON.stringify(requests), { mode: 0o600 });
  record();
  recordCount();
  http.createServer((incoming, outgoing) => {
    const failureReport = incoming.method === "POST" && incoming.url === "/agent/deployment-failed";
    if (failureReport) {
      requests += 1;
      recordCount();
    }
    const request = http.request({ hostname: "127.0.0.1", port: 3000,
      method: incoming.method, path: incoming.url,
      headers: { ...incoming.headers, host: "127.0.0.1:3000" } }, response => {
      const chunks = [];
      response.on("data", chunk => chunks.push(chunk));
      response.on("end", () => {
        const body = Buffer.concat(chunks);
        if (failureReport) {
          responses.push({ status: response.statusCode, body: JSON.parse(body.toString("utf8")) });
          record();
        }
        outgoing.writeHead(response.statusCode, response.headers);
        outgoing.end(body);
      });
    });
    request.on("error", () => {
      outgoing.writeHead(502, { "Content-Type": "application/json" });
      outgoing.end('{"error":"Callback fixture upstream unavailable"}');
    });
    incoming.pipe(request);
  }).listen(3101, "127.0.0.1");
} else {
  throw new Error("Unknown callback fixture mode");
}
