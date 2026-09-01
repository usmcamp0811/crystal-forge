"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { validateCheckGroups } = require("./check-groups");

const manifest = JSON.parse(
  fs.readFileSync(path.join(__dirname, "..", "coverage-manifest.json"), "utf8"),
);
const config = JSON.parse(
  fs.readFileSync(path.join(__dirname, "..", "check-groups.json"), "utf8"),
);

validateCheckGroups(manifest, config);
const requiredCount = manifest.steps.filter((step) =>
  step.profiles.includes(config.requiredProfile),
).length;
const blockingCount = config.requiredGroups.reduce(
  (count, group) => count + config.groups[group].requiredSteps.length,
  0,
);
console.log(
  `Web UI ownership valid: ${requiredCount} ${config.requiredProfile} steps across ` +
    `${config.requiredGroups.join(", ")} (${blockingCount} required, ${requiredCount - blockingCount} advisory)`,
);
