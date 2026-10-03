// Design-only, deterministic System Detail CVE states. The app uses its ordinary
// fixture when ?sc1= is absent. These IDs are assertions, not product UI fields.
// Open ?sc1=<key>, then dispatch cf-open-system for orion-db-02 / cves.
(function () {
  const a = {
    sha: "529d1e57", generation: 192,
    source: { scanId: "00000000-0000-4000-8000-000000000a01", completedAt: "2026-09-23 09:10 UTC", scanner: "vulnix 1.10.1" },
    rows: [
      { id: "CVE-2026-11801", pkg: "openssl", version: "3.3.1", level: "high", score: "8.1", fix: "available" },
      { id: "CVE-2026-11802", pkg: "openssl", version: "3.3.1", level: "unknown", score: null, fix: "pending" },
    ],
  };
  const b = {
    sha: "a3f8c12", generation: 193,
    source: { scanId: "00000000-0000-4000-8000-000000000b01", completedAt: "2026-09-23 10:30 UTC", scanner: "vulnix 1.10.1" },
    rows: [{ id: "CVE-2026-11999", pkg: "curl", version: "8.1", level: "critical", score: "9.4", fix: "available" }],
  };
  const old = {
    sha: "ffa2b88", generation: 191,
    source: { scanId: "00000000-0000-4000-8000-000000000901", completedAt: "2026-09-20 08:20 UTC", scanner: "vulnix 1.10.1" },
    rows: [{ id: "CVE-2026-11001", pkg: "nginx", version: "1.25", level: "medium", score: "6.2", fix: "pending" }],
  };
  const states = {
    "tracked-a": { kind: "completed", target: a, editable: true },
    "local-proved": { kind: "completed", target: a, editable: true },
    "mapped-read-only": { kind: "completed", target: a, proof: "Retained deployment proof is unavailable; this scan is for the reported running configuration. Triage and remediation are read-only." },
    "mapped-clean": { kind: "completed", target: { ...a, rows: [] }, proof: "Retained deployment proof is unavailable; this scan is for the reported running configuration. Triage and remediation are read-only." },
    "unmapped": { kind: "unmapped", target: null, description: "The reported running output does not match a known configuration. No Current scan is available. Choose a known revision to inspect its own results." },
    "ambiguous": { kind: "ambiguous", target: null, description: "More than one registered target matches the reported running output. Current scan results cannot be selected." },
    "no-report": { kind: "no-report", target: null, description: "No usable running configuration has been reported. Known revisions can still be inspected." },
    "invalid-report": { kind: "invalid-report", target: null, description: "The latest running report has conflicting or incomplete target information. Current results are unavailable." },
    "known-no-scan": { kind: "no-scan", target: { ...a, source: null, rows: [] } },
    "completed-empty": { kind: "completed", target: { ...a, rows: [] }, editable: true },
    "unknown-only": { kind: "completed", target: { ...a, rows: [a.rows[1]] }, editable: true },
    "failed-rescan": { kind: "completed", target: a, editable: true, attempt: "A newer scan failed. Showing the last completed scan for this target." },
    "queued-rescan": { kind: "completed", target: a, editable: true, attempt: "A newer scan is queued. Showing the last completed scan for this target." },
    "read-loading": { kind: "loading", target: a },
    "read-error": { kind: "error", target: a, description: "Unable to read the selected target's inventory. No scan was started." },
    "retry-pending": { kind: "loading", target: a, retry: true },
    "retry-success": { kind: "completed", target: a, editable: true },
    "retry-failed": { kind: "error", target: a, description: "The inventory read failed again. The selected target has not changed." },
    "menu-error": { kind: "completed", target: a, editable: true, menuError: true },
    "continuation-error": { kind: "completed", target: a, editable: true, partial: true, totalFindings: 5 },
    "evaluated-b": { kind: "completed", target: a, editable: true },
    "activated-b": { kind: "completed", target: b, editable: true },
    "draft-refresh": { kind: "completed", target: a, editable: true },
  };
  window.SC1_CVE_DESIGN = Object.freeze({ a, b, old, states });
  // Reuse the existing cf-open-system scenario entry. Babel loads components
  // asynchronously, so wait until the real app has attached its listener.
  if (states[new URLSearchParams(location.search).get("sc1")]) {
    const open = window.setInterval(() => {
      if (document.querySelector('[data-screen-label="SystemDetail-orion-db-02"]')) {
        window.clearInterval(open);
      } else if (document.querySelector(".app")) {
        window.dispatchEvent(new CustomEvent("cf-open-system", { detail: { hostname: "orion-db-02", tab: "cves" } }));
      }
    }, 150);
  }
})();
