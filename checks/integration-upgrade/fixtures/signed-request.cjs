// Fault probe for the released, sessionless builder protocol. The same key
// belongs to an actual interrupted release claim. Never log its seed/signature.
// A fresh timestamp ensures rejection proves fencing rather than clock expiry.
"use strict";
const fs = require("node:fs");
const crypto = require("node:crypto");
const [id, path, bodyFile] = process.argv.slice(2);
const seed = Buffer.from(fs.readFileSync("/var/lib/crystal-forge/builder-api.key", "utf8").trim(), "base64");
if (seed.length !== 32) throw new Error("Expected the release's 32-byte Ed25519 seed");
const key = crypto.createPrivateKey({key: Buffer.concat([
  Buffer.from("302e020100300506032b657004220420", "hex"), seed,
]), format: "der", type: "pkcs8"});
const timestamp = new Date().toISOString();
const body = fs.readFileSync(bodyFile);
const signature = crypto.sign(null, Buffer.concat([Buffer.from(`POST\n${path}\n${timestamp}\n`), body]), key);
fs.writeFileSync("/run/upgrade-probe-headers", [
  `X-Builder-ID: ${id}`, `X-Timestamp: ${timestamp}`,
  `X-Signature: ${signature.toString("base64")}`, "Content-Type: application/json", "",
].join("\n"), {mode: 0o600});
