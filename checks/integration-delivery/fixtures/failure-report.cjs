// Controlled agent-report transport, not an execution of the Nix failure wrapper.
// Keys and signatures stay in the controller VM and never enter driver receipts.
"use strict";
const fs = require("node:fs");
const crypto = require("node:crypto");

// Same-run identities use the existing Admin create-user/login APIs and the
// harness's authored local-user password. No session or password enters receipts.
const actors = ["Operator", "Viewer"].map(role => ({
  role, email: `failure-${role.toLowerCase()}@example.test`, name: `Failure ${role}`,
}));
exports.actors = actors;

async function main() {
  const [mode, keyPath, hostname, bodyPath] = process.argv.slice(2);
  if (mode === "actors") {
    process.stdout.write(JSON.stringify(actors));
    return;
  }
  if (mode === "keygen") {
    const { privateKey, publicKey } = crypto.generateKeyPairSync("ed25519");
    fs.writeFileSync(keyPath, privateKey.export({ type: "pkcs8", format: "pem" }), { mode: 0o600 });
    // CF registrations use the raw 32-byte Ed25519 public key, base64 encoded.
    process.stdout.write(publicKey.export({ type: "spki", format: "der" }).subarray(-32).toString("base64"));
    return;
  }
  if (mode !== "report") throw new Error("Unknown failure-report fixture mode");
  const body = fs.readFileSync(bodyPath);
  const signature = crypto.sign(null, body, fs.readFileSync(keyPath));
  // Agent authentication signs the exact body, without the builder protocol's
  // method/path/timestamp envelope. X-Key-ID is the registered hostname.
  const response = await fetch("http://localhost:3000/agent/deployment-failed", {
    method: "POST",
    headers: { "Content-Type": "application/json", "X-Key-ID": hostname,
      "X-Signature": signature.toString("base64") },
    body,
  });
  process.stdout.write(JSON.stringify({ status: response.status, body: await response.json() }));
}
if (require.main === module) {
  main().catch(() => { console.error("Failure-report fixture failed"); process.exitCode = 1; });
}
