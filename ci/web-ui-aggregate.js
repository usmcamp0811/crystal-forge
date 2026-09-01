"use strict";

const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const startedAt = new Date();
const startedMonotonic = process.hrtime.bigint();
const blockingChecks = [
  "web-ui",
  "web-ui-fleet",
  "web-ui-pipeline",
  "web-ui-governance",
  "web-ui-exports",
];
const advisoryChecks = ["web-ui-design-parity"];
const semanticFiles = [
  "screenshots/results.json",
  "screenshots/verdict.json",
  "screenshots/visual-report.json",
  "screenshots/visual-summary.md",
];
const requiredFiles = Object.fromEntries(
  [...blockingChecks, ...advisoryChecks].map((check) => [
    check,
    ["producer.json", "screenshots/check-verdict.json", "screenshots/phase-timings.json"],
  ]),
);
for (const check of ["web-ui", "web-ui-fleet", "web-ui-pipeline", "web-ui-governance"]) {
  requiredFiles[check].push(...semanticFiles);
}
requiredFiles["web-ui"].push(
  "screenshots/06-dashboard--dark.png",
  "screenshots/06-dashboard--light.png",
);
requiredFiles["web-ui-fleet"].push(
  "screenshots/06-dashboard--dark.png",
  "screenshots/06-dashboard--light.png",
);
requiredFiles["web-ui-pipeline"].push(
  "screenshots/06x-pipeline-readiness-scroll--dark.png",
  "screenshots/06x-pipeline-readiness-scroll--light.png",
);
requiredFiles["web-ui-governance"].push(
  "screenshots/01-login-page--dark.png",
  "screenshots/01-login-page--light.png",
);
requiredFiles["web-ui-exports"].push(
  "screenshots/oscal-export-results.json",
  "screenshots/oscal-export-final.png",
  "screenshots/sarif-export-results.json",
  "screenshots/sarif-export-final.png",
);
requiredFiles["web-ui-design-parity"].push(
  ...semanticFiles,
  "screenshots/design-drift-report.json",
  "screenshots/design-drift-summary.md",
  "screenshots/design-parity-matrix.png",
  "screenshots/montages",
  "screenshots/design-targets",
  "screenshots/design-parity",
);

const evidenceRoot = process.env.WEB_UI_EVIDENCE_ROOT || "web-ui-evidence";
const reportDir = process.env.WEB_UI_REPORT_DIR || "web-ui-report";
const reportPath = path.join(reportDir, "report.md");
const timingPath = path.join(reportDir, "aggregation.json");
const requestedLimit = Number.parseInt(process.env.WEB_UI_INLINE_SCREENSHOT_LIMIT || "12", 10);
const inlineScreenshotLimit = Number.isFinite(requestedLimit) && requestedLimit >= 0
  ? requestedLimit
  : 12;
fs.mkdirSync(reportDir, { recursive: true });

function readJson(file) {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch {
    return null;
  }
}

function walk(directory) {
  if (!fs.existsSync(directory)) return [];
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const item = path.join(directory, entry.name);
    return entry.isDirectory() ? walk(item) : [item];
  });
}

function portablePath(file) {
  return file.split(path.sep).join("/");
}

function milliseconds(value) {
  return Number.isFinite(value) && value >= 0 ? value : null;
}

function formatDuration(value) {
  return value === null ? "unavailable" : `${(value / 1000).toFixed(1)} s`;
}

function median(values) {
  if (values.length === 0) return null;
  const ordered = [...values].sort((left, right) => left - right);
  const middle = Math.floor(ordered.length / 2);
  return ordered.length % 2 === 0
    ? (ordered[middle - 1] + ordered[middle]) / 2
    : ordered[middle];
}

const producers = [...blockingChecks, ...advisoryChecks].map((check) => {
  const directory = path.join(evidenceRoot, check);
  const metadata = readJson(path.join(directory, "producer.json"));
  const missingFiles = requiredFiles[check].filter(
    (file) => !fs.existsSync(path.join(directory, file)),
  );
  const reportedStatus = metadata?.status || "missing";
  return {
    check,
    blocking: blockingChecks.includes(check),
    directory,
    metadata,
    reportedStatus,
    status: missingFiles.length > 0 && reportedStatus === "passed"
      ? "evidence-missing"
      : reportedStatus,
    missingFiles,
    files: walk(directory),
  };
});
const producerTimings = producers.map((producer) => ({
  check: producer.check,
  blocking: producer.blocking,
  durationMilliseconds: milliseconds(producer.metadata?.durationMilliseconds),
  queueDurationMilliseconds: milliseconds(producer.metadata?.queueDurationMilliseconds),
  queueDurationSource: producer.metadata?.queueDurationSource || "unavailable",
  cacheState: producer.metadata?.cacheState || "unavailable",
  gateBuildMilliseconds: milliseconds(
    producer.metadata?.phases?.gateBuild?.durationMilliseconds,
  ),
  artifactCopyMilliseconds: milliseconds(
    producer.metadata?.phases?.evidenceCopy?.durationMilliseconds,
  ),
}));
const blockingDurations = producerTimings
  .filter((timing) => timing.blocking && timing.durationMilliseconds !== null)
  .map((timing) => timing.durationMilliseconds);
const hasCompleteBlockingTiming = blockingDurations.length === blockingChecks.length;
const blockingCriticalPathMilliseconds = hasCompleteBlockingTiming
  ? Math.max(...blockingDurations)
  : null;
const blockingMedianJobMilliseconds = hasCompleteBlockingTiming
  ? median(blockingDurations)
  : null;
const blockingMaximumJobMilliseconds = blockingCriticalPathMilliseconds;

let report = `<!-- crystal-forge-web-ui-report:${process.env.CI_PIPELINE_ID || "local"} -->\n`;
report += "## Web UI CI evidence\n\n";
report += "| Producer | Policy | Status | Duration | Queue | Cache | Job |\n";
report += "| --- | --- | --- | ---: | ---: | --- | --- |\n";
for (const producer of producers) {
  const timing = producerTimings.find((item) => item.check === producer.check);
  const job = producer.metadata?.jobUrl
    ? `[job](${producer.metadata.jobUrl})`
    : "unavailable";
  report += `| \`${producer.check}\` | ${producer.blocking ? "blocking" : "advisory"} | **${producer.status}** | ${formatDuration(timing.durationMilliseconds)} | ${formatDuration(timing.queueDurationMilliseconds)} | ${timing.cacheState} | ${job} |\n`;
}
report += "\n### Pipeline timing\n\n";
report += `- Blocking critical path: ${formatDuration(blockingCriticalPathMilliseconds)}\n`;
report += `- Blocking job median: ${formatDuration(blockingMedianJobMilliseconds)}\n`;
report += `- Blocking job maximum: ${formatDuration(blockingMaximumJobMilliseconds)}\n\n`;

report += "\n### Failures and missing evidence\n\n";
let failureCount = 0;
for (const producer of producers) {
  const reasons = [];
  if (producer.reportedStatus === "missing") reasons.push("producer artifact is missing");
  if (producer.metadata?.evidenceLookupStatus) {
    reasons.push(`evidence lookup exited ${producer.metadata.evidenceLookupStatus}`);
  }
  if (producer.metadata?.evidenceCopyStatus) {
    reasons.push(`evidence copy exited ${producer.metadata.evidenceCopyStatus}`);
  }
  reasons.push(...producer.missingFiles.map((file) => `required evidence is missing: ${file}`));

  const browserVerdict = readJson(path.join(producer.directory, "screenshots", "verdict.json"));
  for (const failure of browserVerdict?.failedRequiredSteps || browserVerdict?.failedSteps || []) {
    reasons.push(`required: ${failure.name}: ${failure.reason}`);
  }
  for (const failure of browserVerdict?.failedAdvisorySteps || []) {
    reasons.push(`advisory: ${failure.name}: ${failure.reason}`);
  }
  if (browserVerdict?.processError) reasons.push(browserVerdict.processError);

  const checkVerdict = readJson(
    path.join(producer.directory, "screenshots", "check-verdict.json"),
  );
  for (const failure of checkVerdict?.failedAdvisorySteps || []) {
    if (!(browserVerdict?.failedAdvisorySteps || []).some((item) => item.name === failure.name)) {
      reasons.push(`advisory: ${failure.name}: ${failure.reason}`);
    }
  }
  for (const [component, ok] of Object.entries(checkVerdict?.components || {})) {
    if (ok === false) reasons.push(`${component} component failed`);
  }
  for (const output of checkVerdict?.designParity?.missingOutputs || []) {
    reasons.push(`design parity output is missing: ${output}`);
  }

  for (const filename of ["oscal-export-results.json", "sarif-export-results.json"]) {
    const results = readJson(path.join(producer.directory, "screenshots", filename));
    for (const result of Array.isArray(results) ? results : []) {
      if (!result.ok) reasons.push(`${result.name}: ${result.error || "export validation failed"}`);
    }
  }
  if (producer.reportedStatus !== "passed" && reasons.length === 0) {
    reasons.push(`producer reported ${producer.reportedStatus} without step-level details`);
  }
  if (reasons.length > 0) {
    failureCount += reasons.length;
    report += `**${producer.check}**\n\n`;
    report += reasons.map((reason) => `- ${reason}`).join("\n") + "\n\n";
  }
}
if (failureCount === 0) report += "No producer failures or missing required files were reported.\n\n";

for (const summaryName of ["visual-summary.md", "design-drift-summary.md"]) {
  const summaries = producers.flatMap((producer) =>
    producer.files.filter((file) => path.basename(file) === summaryName),
  );
  if (summaries.length > 0) {
    report += `### ${summaryName === "visual-summary.md" ? "Visual summaries" : "Design parity summary"}\n\n`;
    for (const summary of summaries) {
      report += `#### ${path.basename(path.dirname(path.dirname(summary)))}\n\n`;
      report += fs.readFileSync(summary, "utf8").trim() + "\n\n";
    }
  }
}

const allArtifacts = [...new Set(producers.flatMap((producer) => producer.files))].sort();
const exportEvidence = allArtifacts.filter((file) =>
  /(?:oscal|sarif)-export/i.test(path.basename(file)),
);
report += "### Export evidence\n\n";
report += exportEvidence.length
  ? exportEvidence.map((file) => `- \`${portablePath(file)}\``).join("\n") + "\n\n"
  : "No export evidence was published.\n\n";

const screenshots = allArtifacts.filter((file) => file.endsWith(".png"));
report += `### Screenshots (${screenshots.length})\n\n`;
report += screenshots.length
  ? screenshots.map((file) => `- \`${portablePath(file)}\``).join("\n") + "\n\n"
  : "No screenshots were published.\n\n";
report += `Only the first ${Math.min(inlineScreenshotLimit, screenshots.length)} screenshots are uploaded inline; all files remain in aggregate job artifacts.\n\n`;
report += `### All producer artifacts (${allArtifacts.length})\n\n`;
report += allArtifacts.length
  ? allArtifacts.map((file) => `- \`${portablePath(file)}\``).join("\n") + "\n\n"
  : "No producer artifacts were downloaded.\n\n";
report += `Generated by pipeline ${process.env.CI_PIPELINE_ID || "local"}.\n`;

// Write a complete artifact before any network operation. API or upload
// failures must not remove the local reviewer evidence.
fs.writeFileSync(reportPath, report);

function curl(args) {
  return spawnSync("curl", ["--silent", "--show-error", ...args], {
    encoding: "utf8",
  });
}

let publicationStatus = "not-requested";
const mrIid = process.env.CI_MERGE_REQUEST_IID;
if (mrIid) {
  const api = process.env.GITLAB_API_URL || process.env.CI_API_V4_URL;
  const projectId = process.env.CI_PROJECT_ID;
  const token = process.env.GITLAB_TOKEN;
  if (!api || !projectId) {
    publicationStatus = "missing-api-configuration";
    console.warn("GitLab API variables are incomplete; report remains available as an artifact");
  } else if (!token) {
    publicationStatus = "missing-gitlab-token";
    console.warn("Masked GITLAB_TOKEN is unavailable; uploads and MR publication were skipped, and the report remains available as an artifact");
  } else {
    const auth = ["--header", `PRIVATE-TOKEN: ${token}`];
    for (const screenshot of screenshots.slice(0, inlineScreenshotLimit)) {
      const response = curl([
        ...auth,
        "--form",
        `file=@${screenshot}`,
        `${api}/projects/${projectId}/uploads`,
      ]);
      try {
        const markdown = JSON.parse(response.stdout).markdown;
        if (response.status === 0 && markdown) {
          report += `\n#### ${path.basename(screenshot, ".png")}\n\n${markdown}\n`;
        } else {
          report += `\n- Upload failed: \`${portablePath(screenshot)}\`\n`;
        }
      } catch {
        report += `\n- Upload failed: \`${portablePath(screenshot)}\`\n`;
      }
    }
    fs.writeFileSync(reportPath, report);

    const marker = `<!-- crystal-forge-web-ui-report:${process.env.CI_PIPELINE_ID || "local"} -->`;
    const notesUrl = `${api}/projects/${projectId}/merge_requests/${mrIid}/notes`;
    const notesResponse = curl([...auth, notesUrl]);
    let noteId;
    try {
      noteId = JSON.parse(notesResponse.stdout).find((note) => note.body?.includes(marker))?.id;
    } catch {
      console.warn("Could not read MR notes; report remains available as an artifact");
    }
    const payload = JSON.stringify({ body: report });
    const result = noteId
      ? curl([...auth, "--request", "PUT", "--header", "Content-Type: application/json", "--data", payload, `${notesUrl}/${noteId}`])
      : curl([...auth, "--request", "POST", "--header", "Content-Type: application/json", "--data", payload, notesUrl]);
    try {
      publicationStatus = result.status === 0 && Boolean(JSON.parse(result.stdout).id)
        ? "published"
        : "failed";
    } catch {
      publicationStatus = "failed";
    }
    if (publicationStatus !== "published") {
      console.warn("Could not publish the MR report; report remains available as an artifact");
    }
  }
}

const endedAt = new Date();
fs.writeFileSync(timingPath, JSON.stringify({
  schemaVersion: 1,
  startedAt: startedAt.toISOString(),
  endedAt: endedAt.toISOString(),
  durationMilliseconds: Number(process.hrtime.bigint() - startedMonotonic) / 1e6,
  producerCount: producers.length,
  artifactCount: allArtifacts.length,
  screenshotCount: screenshots.length,
  inlineScreenshotLimit,
  inlineScreenshotCount: Math.min(inlineScreenshotLimit, screenshots.length),
  publicationStatus,
  producerTimings,
  blockingCriticalPathMilliseconds,
  blockingMedianJobMilliseconds,
  blockingMaximumJobMilliseconds,
  hasCompleteBlockingTiming,
}, null, 2));

console.log(`Wrote ${reportPath} and ${timingPath}`);
