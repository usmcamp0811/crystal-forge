"use strict";

/**
 * Validates browser step ownership and named profile selections.
 *
 * Required groups must partition the required manifest profile exactly once.
 * Other named groups can overlap because they define compatibility or advisory
 * checks rather than required semantic ownership.
 *
 * @param {{steps: Array<{name: string, profiles: string[]}>}} manifest coverage manifest
 * @param {{requiredProfile: string, requiredGroups: string[], groups: Object<string, {steps: string[], requiredSteps: string[], advisorySteps: string[]}>}} config group configuration
 * @returns {void}
 */
function validateCheckGroups(manifest, config) {
  const manifestNames = manifest.steps.map((step) => step.name);
  const known = new Set(manifestNames);
  if (known.size !== manifestNames.length) {
    throw new Error("coverage manifest contains duplicate step names");
  }

  const required = manifest.steps
    .filter((step) => step.profiles.includes(config.requiredProfile))
    .map((step) => step.name);
  const owners = new Map(required.map((name) => [name, []]));

  for (const groupName of config.requiredGroups) {
    const group = config.groups[groupName];
    if (!group) throw new Error(`required group "${groupName}" is not defined`);
    if (group.blocking !== true) {
      throw new Error(`required group "${groupName}" must be blocking`);
    }
    for (const name of group.steps) {
      if (!known.has(name)) {
        throw new Error(`group "${groupName}" contains unknown step "${name}"`);
      }
      if (!owners.has(name)) {
        throw new Error(
          `group "${groupName}" contains step "${name}" outside required profile "${config.requiredProfile}"`,
        );
      }
      owners.get(name).push(groupName);
    }
  }

  for (const [groupName, group] of Object.entries(config.groups)) {
    const unique = new Set(group.steps);
    if (unique.size !== group.steps.length) {
      throw new Error(`group "${groupName}" contains duplicate steps`);
    }
    for (const name of group.steps) {
      if (!known.has(name)) {
        throw new Error(`group "${groupName}" contains unknown step "${name}"`);
      }
    }
    const requiredPolicy = new Set(group.requiredSteps);
    const advisoryPolicy = new Set(group.advisorySteps);
    if (requiredPolicy.size !== group.requiredSteps.length) {
      throw new Error(`group "${groupName}" contains duplicate required steps`);
    }
    if (advisoryPolicy.size !== group.advisorySteps.length) {
      throw new Error(`group "${groupName}" contains duplicate advisory steps`);
    }
    const policyOutsideGroup = [...requiredPolicy, ...advisoryPolicy].filter(
      (name) => !unique.has(name),
    );
    if (policyOutsideGroup.length) {
      throw new Error(
        `group "${groupName}" classifies steps it does not execute: ${policyOutsideGroup.join(", ")}`,
      );
    }
    const invalidPolicy = group.steps.filter(
      (name) => requiredPolicy.has(name) === advisoryPolicy.has(name),
    );
    if (invalidPolicy.length) {
      throw new Error(
        `group "${groupName}" must classify each step as required or advisory: ${invalidPolicy.join(", ")}`,
      );
    }
  }

  const invalidOwnership = [...owners]
    .filter(([, groupOwners]) => groupOwners.length !== 1)
    .map(([name, groupOwners]) => `${name} (${groupOwners.join(", ") || "unowned"})`);
  if (invalidOwnership.length) {
    throw new Error(`required step ownership must be exactly one: ${invalidOwnership.join("; ")}`);
  }

  // INVARIANT: Group execution follows manifest order so stateful chains retain
  // the same transitions as the historical ci_fast run.
  const manifestPosition = new Map(manifestNames.map((name, index) => [name, index]));
  for (const groupName of config.requiredGroups) {
    const positions = config.groups[groupName].steps.map((name) => manifestPosition.get(name));
    if (positions.some((position, index) => index > 0 && position <= positions[index - 1])) {
      throw new Error(`required group "${groupName}" does not preserve manifest step order`);
    }
  }
}

/**
 * Returns the required step names for a selected profile.
 *
 * Named groups use their explicit ownership policy. The `ci_fast` profile uses
 * the union of the ordinary shard policies. Other manifest profiles preserve
 * the historical all-selected-steps behavior.
 *
 * @param {Array<{name: string}>} selectedSteps selected ordered steps
 * @param {{requiredProfile: string, requiredGroups: string[], groups: Object<string, {requiredSteps: string[]}>}} config group configuration
 * @param {string} profile selected profile name
 * @returns {Set<string>} required selected step names
 */
function requiredStepNames(selectedSteps, config, profile) {
  const group = config.groups[profile];
  const configured = group
    ? group.requiredSteps
    : profile === config.requiredProfile
      ? config.requiredGroups.flatMap((name) => config.groups[name].requiredSteps)
      : selectedSteps.map((step) => step.name);
  const selected = new Set(selectedSteps.map((step) => step.name));
  return new Set(configured.filter((name) => selected.has(name)));
}

/**
 * Returns the ordered steps selected by a manifest or named group profile.
 *
 * @param {Array<{name: string}>} steps executable steps in manifest order
 * @param {{steps: Array<{name: string, profiles: string[]}>}} manifest coverage manifest
 * @param {{groups: Object<string, {steps: string[]}>}} config group configuration
 * @param {string} profile requested profile
 * @returns {Array<{name: string}>}
 */
function selectProfileSteps(steps, manifest, config, profile) {
  if (profile === "full") return steps;
  const namedGroup = config.groups[profile];
  if (namedGroup) {
    const selected = new Set(namedGroup.steps);
    return steps.filter((step) => selected.has(step.name));
  }

  const knownManifestProfile = manifest.steps.some((step) => step.profiles.includes(profile));
  if (!knownManifestProfile) throw new Error(`unknown UI test profile "${profile}"`);
  const manifestByName = new Map(manifest.steps.map((step) => [step.name, step]));
  return steps.filter((step) => manifestByName.get(step.name).profiles.includes(profile));
}

/**
 * Narrows a profile to explicitly requested steps without silent exclusions.
 *
 * @param {Array<{name: string}>} allSteps all executable steps
 * @param {Array<{name: string}>} profileSteps selected profile steps
 * @param {Set<string>|null} requestedSteps explicitly requested names
 * @param {string} profile selected profile name
 * @returns {Array<{name: string}>}
 */
function selectRequestedSteps(allSteps, profileSteps, requestedSteps, profile) {
  if (!requestedSteps) return profileSteps;
  const allNames = new Set(allSteps.map((step) => step.name));
  const unknown = [...requestedSteps].filter((name) => !allNames.has(name));
  if (unknown.length) throw new Error(`unknown requested UI test steps: [${unknown.join(", ")}]`);

  const profileNames = new Set(profileSteps.map((step) => step.name));
  const excluded = [...requestedSteps].filter((name) => !profileNames.has(name));
  if (excluded.length) {
    throw new Error(
      `requested UI test steps excluded by profile "${profile}": [${excluded.join(", ")}]`,
    );
  }
  return profileSteps.filter((step) => requestedSteps.has(step.name));
}

/**
 * Returns whether a selection needs an authenticated-session preflight.
 *
 * @param {Array<{name: string}>} selectedSteps selected ordered steps
 * @returns {boolean} true when the selection omits the authentication chain
 */
function needsAuthenticationPreflight(selectedSteps) {
  const names = new Set(selectedSteps.map((step) => step.name));
  return !names.has("03-registration-submit") && !names.has("05-login-submit");
}

module.exports = {
  needsAuthenticationPreflight,
  requiredStepNames,
  selectProfileSteps,
  selectRequestedSteps,
  validateCheckGroups,
};
