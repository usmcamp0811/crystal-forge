// Scan queue rows for externally managed and reference configurations (configuration scans, no deploy).
(function () {
  if (typeof SCAN_CONFIGS === "undefined" || typeof SYSTEMS === "undefined") return;
  const map = { current: "complete", stale: "stale", failed: "failed", unavailable: "failed", missing: "awaiting" };
  SYSTEMS.filter((s) => s.assurance).forEach((s, i) => {
    const a = s.assurance, st = map[a.scan.state] || "unscanned";
    SCAN_CONFIGS.push({ id: "sc-x" + i, name: s.hostname, mode: s.mgmt, flake: s.flake, commit: a.scan.rev || a.rev.sha, freshness: "recent", status: st,
      found: st === "complete" || st === "stale" ? { crit: s.cves.critical, high: s.cves.high, med: s.cves.medium } : null,
      lastScan: a.scan.at ? (st === "failed" ? "failed " : "") + a.scan.at : "pending", trigger: a.scan.at ? "post-build" : null,
      ...(st === "awaiting" ? { awaiting: "building", awaitingDetail: "Waiting for the build of " + a.rev.sha } : {}),
      ...(a.scan.error ? { error: a.scan.error.length > 60 ? a.scan.error.slice(0, 57) + "…" : a.scan.error } : {}) });
  });
  if (typeof SCAN_STATS !== "undefined") ["queued", "awaiting", "stale", "unscanned", "failed"].forEach((k) => { SCAN_STATS[k] = SCAN_CONFIGS.filter((c) => c.status === k).length; });
})();
