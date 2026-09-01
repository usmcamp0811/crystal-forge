"use strict";

const SCHEMA_VERSION = 2;

/**
 * Creates the final verdict for a browser test process.
 *
 * Required and advisory selections both retain failures. A missing required
 * result fails the gate. A missing advisory result is an advisory failure.
 * Process-level failures, such as unhandled rejections, fail the gate because
 * they make the execution evidence incomplete.
 *
 * @param {Array<string|{name: string}>} selectedSteps selected browser steps
 * @param {Array<{name: string, ok: boolean, error?: string|null}>} results recorded results
 * @param {{completed?: boolean, processError?: string|null, requiredSteps?: Iterable<string>}} [options] process state and ownership
 * @returns {{schemaVersion: number, completed: boolean, ok: boolean, selectedSteps: Array<{name: string, required: boolean, ok: boolean, reason: string|null}>, failedRequiredSteps: Array<{name: string, reason: string}>, failedAdvisorySteps: Array<{name: string, reason: string}>, failedSteps: Array<{name: string, reason: string}>, processError: string|null}}
 */
function createBrowserVerdict(selectedSteps, results, options = {}) {
  const completed = options.completed !== false;
  const processError = options.processError || null;
  const requiredSteps = new Set(
    options.requiredSteps === undefined
      ? selectedSteps.map((step) => (typeof step === "string" ? step : step.name))
      : options.requiredSteps,
  );
  const resultsByName = new Map(results.map((result) => [result.name, result]));
  const selected = selectedSteps.map((step) => {
    const name = typeof step === "string" ? step : step.name;
    const result = resultsByName.get(name);
    if (!result) {
      return {
        name,
        required: requiredSteps.has(name),
        ok: false,
        reason: "selected step did not produce a result",
      };
    }
    return {
      name,
      required: requiredSteps.has(name),
      ok: result.ok === true,
      reason: result.ok === true ? null : result.error || "selected step failed without a reason",
    };
  });
  const failures = (required) => selected
    .filter((step) => step.required === required && !step.ok)
    .map(({ name, reason }) => ({ name, reason }));
  const failedRequiredSteps = failures(true);
  const failedAdvisorySteps = failures(false);

  return {
    schemaVersion: SCHEMA_VERSION,
    completed,
    ok: completed && !processError && failedRequiredSteps.length === 0,
    selectedSteps: selected,
    failedRequiredSteps,
    failedAdvisorySteps,
    // Compatibility alias: failedSteps contains blocking step failures only.
    failedSteps: failedRequiredSteps,
    processError,
  };
}

/**
 * Returns the process exit code for a browser verdict.
 *
 * @param {{ok: boolean}} verdict final browser verdict
 * @returns {number} zero only when the verdict passed
 */
function exitCodeForVerdict(verdict) {
  return verdict.ok ? 0 : 1;
}

module.exports = { createBrowserVerdict, exitCodeForVerdict };
