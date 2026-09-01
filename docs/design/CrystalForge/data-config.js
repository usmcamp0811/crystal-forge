/* ───────────────────────────────────────────────────────────────────────────
   Evaluated NixOS config — data layer

   This file simulates the BACKEND contract, not a client-side dataset. The
   shape here is what a real Crystal Forge deployment would serve, because a
   naive implementation does not survive a large fleet. Four constraints drive
   the design:

   1. EXTRACTION RIDES ON THE BUILD EVAL.
      Evaluating nixosConfigurations.<host> costs ~2-20s cold / 1-5s warm and
      0.5-2 GB RAM. That eval already happens for every build, so the option
      set is extracted inside that job and written out as an artifact. It is
      never evaluated on demand for a page view.

   2. NEVER SERIALIZE THE `config` ATTRSET.
      `builtins.toJSON config` throws or explodes: it contains functions,
      derivations, and recursive references. The extractor walks the `options`
      tree instead, `tryEval`s each `.value`, caps recursion depth, and reduces
      package-typed values to store path + name. Provenance is free at that
      point — `options.<path>.files` is already computed by the module merge.

   3. STORAGE IS BASE + DELTA, CONTENT-ADDRESSED.
      A real host has 5k-15k options; a few MB of JSON each. Hundreds of hosts
      x every commit is untenable stored naively. Hosts sharing a flake rev
      share nearly all option values, so we store ONE base blob per
      (flake rev, module set) and only a per-host delta. Below, BASE_OPTIONS is
      built once and shared by every system; each host holds ~30 overlay rows.

   4. QUERIES ARE SERVER-SIDE.
      The client never receives the full option set. It asks for a page with a
      search term and filter, and gets back {rows, total, counts}. The diff
      against a previous generation is computed server-side too — shipping two
      full option sets to the browser to diff them is the thing to avoid.

   The async API below (ConfigAPI) mirrors that contract with simulated
   latency, so the UI is written against the real shape rather than against a
   local array it can synchronously filter.
   ─────────────────────────────────────────────────────────────────────────── */
(function () {
  function hash(s) { let h = 2166136261; for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); } return h >>> 0; }
  function rngFor(seed) { let x = hash(seed) || 1; return () => { x ^= x << 13; x >>>= 0; x ^= x >> 17; x ^= x << 5; x >>>= 0; return x / 4294967296; }; }
  const pick = (r, a) => a[Math.floor(r() * a.length) % a.length];

  /* ── Module registry ─────────────────────────────────────────────────────
     Modelled on a real, complex flake (gitlab:usmcamp0811/dotfiles, ~90 inputs,
     Snowfall-generated hosts) but deliberately NOT Snowfall-specific. The only
     things assumed are what every flake has: a module came from SOME input, at
     SOME revision, from SOME file path. `input` is the dimension a naive
     implementation forgets — with dozens of inputs, "set by hardening/sshd.nix"
     is not answerable without knowing WHICH input's tree that path is in.

     `kind` is presentational only:
       self     — the flake being deployed
       input    — a third-party or first-party flake input
       nixpkgs  — the channel a given host resolved against (there may be
                  several distinct channels across one fleet) */
  const MODULES = [
    { path: "nixos/modules/profiles/qemu-guest.nix", kind: "nixpkgs", input: "nixpkgs", rev: "release-26.05" },
    { path: "nixos/modules/services/networking/ssh/sshd.nix", kind: "nixpkgs", input: "nixpkgs", rev: "release-26.05" },
    { path: "nixos/modules/services/monitoring/prometheus/exporters.nix", kind: "nixpkgs", input: "nixpkgs", rev: "release-26.05" },
    { path: "nixos/modules/security/audit.nix", kind: "nixpkgs", input: "nixpkgs", rev: "release-26.05" },
    { path: "nixos/modules/config/users-groups.nix", kind: "nixpkgs", input: "nixpkgs", rev: "release-26.05" },
    { path: "nixos/modules/virtualisation/amazon-image.nix", kind: "nixpkgs", input: "unstable", rev: "nixos-unstable" },
    { path: "modules/nixos/system/default.nix", kind: "self", input: "self", rev: null },
    { path: "modules/nixos/system/networking/default.nix", kind: "self", input: "self", rev: null },
    { path: "modules/nixos/system/zfs/default.nix", kind: "self", input: "self", rev: null },
    { path: "modules/nixos/user/default.nix", kind: "self", input: "self", rev: null },
    { path: "modules/nixos/security/gpg/default.nix", kind: "self", input: "self", rev: null },
    { path: "modules/nixos/services/prometheus/default.nix", kind: "self", input: "self", rev: null },
    { path: "modules/nixos/services/grafana/default.nix", kind: "self", input: "self", rev: null },
    { path: "modules/nixos/services/k3s/default.nix", kind: "self", input: "self", rev: null },
    { path: "modules/nixos/suites/kubernetes/default.nix", kind: "self", input: "self", rev: null },
    { path: "modules/nixos/router/default.nix", kind: "self", input: "self", rev: null },
    { path: "modules/crystal-forge/client.nix", kind: "input", input: "crystal-forge", rev: "TASK-433-policy-poam-workflows" },
    { path: "modules/stig/sshd/default.nix", kind: "input", input: "crystal-forge", rev: "TASK-433-policy-poam-workflows" },
    { path: "modules/stig/banner/default.nix", kind: "input", input: "crystal-forge", rev: "TASK-433-policy-poam-workflows" },
    { path: "modules/stig/audit/default.nix", kind: "input", input: "crystal-forge", rev: "TASK-433-policy-poam-workflows" },
    { path: "modules/stig/kernel/default.nix", kind: "input", input: "crystal-forge", rev: "TASK-433-policy-poam-workflows" },
    { path: "nixos-modules/home-manager.nix", kind: "input", input: "home-manager", rev: "release-26.05" },
    { path: "modules/impermanence.nix", kind: "input", input: "impermanence", rev: "a11c4a7" },
    { path: "module.nix", kind: "input", input: "disko", rev: "v1.12.0" },
    { path: "modules/stylix.nix", kind: "input", input: "stylix", rev: "release-25.11" },
    { path: "nixos-modules/vault-agent.nix", kind: "input", input: "vault-service", rev: "0f34b1c" },
    { path: "nixos/host.nix", kind: "input", input: "microvm", rev: "e2fa5d6" },
  ];
  const MOD = {}; MODULES.forEach(m => { MOD[m.path] = m; });

  /* ── The shared base blob ────────────────────────────────────────────────
     Built ONCE for the whole page, standing in for the per-(flake rev) blob
     the server stores once and every host on that rev references. Roughly the
     size of a real host's option set, so the UI is exercised against a
     realistic row count rather than a toy list. */
  const SERVICES = ["openssh","nginx","postgresql","prometheus","grafana","chrony","journald","resolved","timesyncd","fail2ban","node-exporter","logrotate","cron","dbus","udev","auditd","firewalld","haproxy","redis","vector"];
  const LEAVES = ["enable","package","user","group","port","openFirewall","dataDir","logLevel","extraConfig","settings.MaxConnections","settings.Timeout","settings.LogFormat","after","wants","restartTriggers","environmentFile","stateDirectory","runtimeDirectory"];
  const PROGRAMS = ["bash","zsh","git","vim","tmux","gnupg","ssh","less","nano","htop"];
  const SYSCTL = ["kernel.dmesg_restrict","kernel.kptr_restrict","kernel.yama.ptrace_scope","net.ipv4.conf.all.rp_filter","net.ipv4.tcp_syncookies","net.ipv6.conf.all.accept_ra","fs.protected_hardlinks","vm.swappiness","net.core.somaxconn"];

  const BASE_OPTIONS = (function buildBase() {
    const r = rngFor("cf-base-optionset");
    const out = [];
    const push = (path, value, type) => {
      // A minority of options are defined by more than one module; the first
      // entry wins, matching the module system's priority merge.
      const n = r() < 0.18 ? 2 : 1;
      const defs = [];
      for (let i = 0; i < n; i++) {
        let m = pick(r, MODULES).path;
        if (defs.includes(m)) m = MODULES[(MODULES.findIndex(x => x.path === m) + 3) % MODULES.length].path;
        defs.push(m);
      }
      out.push({ path, value, type, defs });
    };

    SERVICES.forEach(svc => LEAVES.forEach(leaf => {
      const t = leaf === "enable" || leaf === "openFirewall" ? "bool" : leaf === "port" ? "port" : leaf.includes("settings.") ? "str" : "str";
      const v = t === "bool" ? (r() > 0.5 ? "true" : "false") : t === "port" ? String(1000 + Math.floor(r() * 60000)) : `"${leaf.split(".").pop()}-${Math.floor(r() * 900)}"`;
      push(`services.${svc}.${leaf}`, v, t);
    }));
    PROGRAMS.forEach(p => ["enable","package","shellAliases","extraConfig","enableCompletion"].forEach(leaf =>
      push(`programs.${p}.${leaf}`, leaf === "enable" || leaf === "enableCompletion" ? (r() > 0.4 ? "true" : "false") : `"${p}-${Math.floor(r()*400)}"`, leaf.startsWith("enable") ? "bool" : "str")));
    SYSCTL.forEach(k => push(`boot.kernel.sysctl."${k}"`, String(Math.floor(r() * 3)), "int"));
    for (let i = 0; i < 40; i++) push(`systemd.services.cf-worker-${i}.serviceConfig.Restart`, `"on-failure"`, "str");
    for (let i = 0; i < 120; i++) push(`environment.etc."cf/${i}.conf".text`, `"generated"`, "lines");
    ["hardware","fonts","i18n","virtualisation","documentation","xdg","zramSwap","powerManagement"].forEach(ns => {
      for (let i = 0; i < 60; i++) push(`${ns}.${pick(r, ["enable","settings","extraOptions","packages","config","defaultFonts","memoryPercent"])}${i ? "." + i : ""}`, r() > 0.5 ? "true" : `"v${i}"`, r() > 0.5 ? "bool" : "str");
    });
    return out;
  })();

  /* ── Per-host delta ──────────────────────────────────────────────────────
     The only thing stored per system: options whose value or provenance
     differs from the base blob. Small by construction. */
  function hostOverlay(sys, rev, r) {
    const env = sys.environment;
    const prod = env === "production";
    const short = (sys.nixosVersion || "25.05").slice(0, 5);
    // Older revisions legitimately carried different values: hardening landed
    // over time, stateVersion moved, control counts grew.
    const era = r();                       // 0 = oldest-looking, 1 = newest
    const hardened = era > 0.35;           // sshd hardening merged
    const auditRules = era > 0.55;         // stig audit rules merged
    const olderState = era < 0.4;
    const rows = [
      ["networking.hostName", `"${sys.hostname}"`, "str", ["modules/nixos/system/networking/default.nix"]],
      ["networking.domain", `"${env}.cf.internal"`, "null or str", ["modules/nixos/system/networking/default.nix"]],
      ["networking.firewall.enable", "true", "bool", ["modules/stig/kernel/default.nix", "nixos/modules/profiles/qemu-guest.nix"]],
      ["networking.firewall.allowedTCPPorts", "[ 22 9100 ]", "list of port", ["modules/nixos/services/prometheus/default.nix", "modules/nixos/system/networking/default.nix"]],
      ["services.openssh.enable", "true", "bool", ["modules/nixos/system/default.nix"]],
      ["services.openssh.settings.PermitRootLogin", `"${hardened ? (prod ? "no" : "prohibit-password") : "yes"}"`, "str", hardened ? ["modules/stig/sshd/default.nix", "nixos/modules/services/networking/ssh/sshd.nix"] : ["nixos/modules/services/networking/ssh/sshd.nix"]],
      ["services.openssh.settings.PasswordAuthentication", hardened ? "false" : "true", "bool", hardened ? ["modules/stig/sshd/default.nix", "nixos/modules/services/networking/ssh/sshd.nix"] : ["nixos/modules/services/networking/ssh/sshd.nix"]],
      ["services.openssh.settings.X11Forwarding", "false", "bool", ["modules/stig/sshd/default.nix"]],
      ["services.prometheus.exporters.node.enable", "true", "bool", ["modules/nixos/services/prometheus/default.nix"]],
      ["services.prometheus.exporters.node.port", "9100", "port", ["modules/nixos/services/prometheus/default.nix", "nixos/modules/services/monitoring/prometheus/exporters.nix"]],
      ["services.grafana.enable", prod ? "true" : "false", "bool", ["modules/nixos/services/grafana/default.nix"]],
      ["security.audit.enable", "true", "bool", ["modules/stig/audit/default.nix", "nixos/modules/security/audit.nix"]],
      ["security.auditd.enable", "true", "bool", ["modules/stig/audit/default.nix"]],
      ["security.sudo.execWheelOnly", "true", "bool", ["modules/stig/kernel/default.nix"]],
      ["boot.kernel.sysctl.\"kernel.dmesg_restrict\"", "1", "int", ["modules/stig/kernel/default.nix"]],
      ["boot.loader.systemd-boot.enable", "true", "bool", ["modules/nixos/system/default.nix"]],
      ["users.mutableUsers", "false", "bool", ["modules/nixos/user/default.nix", "nixos/modules/config/users-groups.nix"]],
      ["users.users.svc-forge.isSystemUser", "true", "bool", ["modules/crystal-forge/client.nix"]],
      ["home-manager.useGlobalPkgs", "true", "bool", ["nixos-modules/home-manager.nix"]],
      ["environment.persistence.\"/persist\".directories", "[ /var/log /var/lib/nixos ... ]", "list of str", ["modules/impermanence.nix"]],
      ["stylix.enable", "true", "bool", ["modules/stylix.nix"]],
      ["crystal-forge.client.enable", "true", "bool", ["modules/crystal-forge/client.nix"]],
      ["crystal-forge.client.serverHost", `"crystal-forge.internal"`, "str", ["modules/crystal-forge/client.nix"]],
      ["crystal-forge.client.environment", `"${env}"`, "str", ["modules/nixos/system/default.nix", "modules/crystal-forge/client.nix"]],
      ["crystal-forge.client.deploymentPolicy", `"${sys.deploymentPolicy}"`, "enum", ["modules/nixos/system/default.nix"]],
      ["crystal-forge.stig.banner.enable", "true", "bool", ["modules/stig/banner/default.nix"]],
      ["crystal-forge.stig.sshdHardening.enable", "true", "bool", ["modules/stig/sshd/default.nix"]],
      ["crystal-forge.stig.auditRules.enable", auditRules && prod ? "true" : "false", "bool", ["modules/stig/audit/default.nix"]],
      ["crystal-forge.stig.controlCount", String((prod ? 28 : 22) - (auditRules ? 0 : 6)), "int", ["modules/stig/audit/default.nix"]],
      ["system.stateVersion", `"${olderState ? "24.11" : short}"`, "str", ["modules/nixos/system/default.nix"]],
      ["time.timeZone", `"UTC"`, "str", ["modules/nixos/system/default.nix"]],
      ["nix.settings.experimental-features", `[ "nix-command" "flakes" ]`, "list of str", ["modules/nixos/system/default.nix"]],
      ["nix.gc.automatic", "true", "bool", ["modules/nixos/system/default.nix"]],
      // Package-typed: no text form exists. Elements are attached below.
      ["environment.systemPackages", null, "list of package", ["modules/nixos/system/default.nix", "modules/nixos/services/prometheus/default.nix", "modules/crystal-forge/client.nix"]],
      // Function-valued: nothing comparable to show, and we say so.
      ["nixpkgs.overlays", null, "list of function to attrs", ["modules/nixos/system/default.nix"]],
      // Submodule: diffed by descending into its own options.
      ["systemd.services.cf-agent.serviceConfig", null, "attrs of submodule", ["modules/crystal-forge/client.nix"]],
      // Extractor could not evaluate this one; the error is reported verbatim.
      ["services.nginx.virtualHosts", null, "attrs of submodule", ["modules/nixos/services/grafana/default.nix"]],
      ["services.journald.extraConfig", `"SystemMaxUse=${olderState ? 1 : 2}G"`, "lines", ["modules/nixos/services/prometheus/default.nix"]],
    ];
    // Modules that did not exist yet at older revisions simply have no rows.
    const gated = auditRules ? rows : rows.filter(([p]) => !p.startsWith("security.auditd"));
    return gated.map(([path, value, type, defs]) => {
      const kind = valueKind(type);
      const row = { path, value, type, defs };
      if (kind === "package") {
        row.elements = packageElements(r, 6 + Math.floor(r() * 5));
        row.value = `${row.elements.length} packages`;
      } else if (kind === "opaque") {
        row.value = "«function»";
      } else if (kind === "submodule") {
        row.value = `{ ${1 + Math.floor(r() * 4)} attrs }`;
      } else if (row.value === null || row.value === undefined) {
        row.value = "«no text form»";
      }
      if (path === "services.nginx.virtualHosts") {
        row.evalError = "error: attribute 'sslCertificate' missing (while evaluating services.nginx.virtualHosts.\"default\")";
        row.value = null;
      }
      return row;
    });
  }

  /* Per-revision view of the shared base blob. The blob itself is stored once
     per (flake rev, module set) — this applies the revision's own values to a
     slice of it rather than copying the whole array, which is exactly how a
     base+delta store answers a query for an older revision. */
  function baseAtRev(r) {
    const perturbed = new Map();
    const n = 30 + Math.floor(r() * 40);
    for (let i = 0; i < n; i++) {
      const idx = Math.floor(r() * BASE_OPTIONS.length);
      const o = BASE_OPTIONS[idx];
      if (!o || perturbed.has(o.path)) continue;
      let value = o.value;
      if (o.type === "bool") value = o.value === "true" ? "false" : "true";
      else if (o.type === "port" || o.type === "int") value = String(Math.max(1, Number(o.value.replace(/\D/g, "") || 1) + (r() < 0.5 ? -1 : 1) * (1 + Math.floor(r() * 40))));
      else value = o.value.replace(/-(\d+)"$/, (_, d) => `-${Math.max(0, Number(d) + Math.floor(r() * 60) - 30)}"`);
      // A minority also changed which module won the merge.
      const defs = r() < 0.15 && o.defs.length > 1 ? [o.defs[1], o.defs[0]] : o.defs;
      perturbed.set(o.path, { ...o, value, defs });
    }
    return perturbed;
  }

  /* Each definition carries its ORIGIN INPUT, not just a file path. In a flake
     with dozens of inputs the same relative path can exist in several trees,
     and "which input, at which rev" is the question an operator is actually
     asking when a value surprises them. */

  /* ── Value serialization policy ──────────────────────────────────────────
     How an option's value is stored and diffed is decided by its declared
     TYPE, never by inspecting the value and guessing. The module system hands
     us the type for free (options.<path>.type.name), which is what makes this
     reliable rather than heuristic — and unknown/exotic types fall into the
     opaque bucket by default, so the failure mode is "changed, not comparable"
     instead of a fabricated before/after.

       scalar   bool, int, str, port, enum, path, lines
                → serialize to a string, text-diff.  Most options land here.
       list     list of str/port/int, attrs of str
                → serialize elements, diff element-wise.
       package  package, list of package
                → NEVER serializable as text: the real value is a derivation.
                  Reduce each element to { name, version, store } — data nix
                  already computed for the closure — and diff by name. This is
                  strictly more useful than a text diff:
                      − openssl-3.3.2   + openssl-3.4.0
       submodule
                → neither scalar nor package (users.users, systemd.services,
                  services.nginx.virtualHosts). Diff by descending into the
                  submodule's own options with the same table, depth-capped.
       opaque   function to *, unspecified, raw, anything unrecognised
                → no comparable value. Report that it changed; invent nothing.
       unevaluated
                → the extractor's tryEval failed. Report the error, not a value.

     Cost is nil: type comes from the eval we already run, package name/version
     from the closure list we already extract. This is serialization policy, not
     new computation. */
  function valueKind(type) {
    // Order matters: "list of function to attrs" is OPAQUE, not a list. Test
    // for functions unanchored and before the list branch.
    if (/function/.test(type)) return "opaque";
    if (/package/.test(type)) return "package";
    if (/^(unspecified|raw)/.test(type)) return "opaque";
    if (/submodule/.test(type)) return "submodule";
    if (/^(list of|attrs of)/.test(type)) return "list";
    return "scalar";
  }

  /* Package-typed values carry structured elements, not a rendered string. */
  const PKG_POOL = [
    ["git", "2.47.1"], ["vim", "9.1.0862"], ["curl", "8.11.0"], ["jq", "1.7.1"],
    ["openssl", "3.4.0"], ["coreutils", "9.5"], ["systemd", "256.10"], ["openssh", "9.9p1"],
    ["python3", "3.12.7"], ["nix", "2.24.11"], ["rsync", "3.3.0"], ["htop", "3.3.0"],
  ];
  function packageElements(r, n) {
    return PKG_POOL.slice(0, n).map(([name, version]) => ({
      name, version,
      store: `/nix/store/${Array.from({length:8},()=>"0123456789abcdfghijklmnpqrsvwxyz"[Math.floor(r()*32)]).join("")}…-${name}-${version}`,
    }));
  }

  const decorate = (o) => ({
    path: o.path, value: o.value, type: o.type,
    kind: valueKind(o.type),
    elements: o.elements || null,   // package/list options: structured, not text
    evalError: o.evalError || null, // tryEval failure, surfaced not hidden
    source: o.defs[0],
    sourceInput: (MOD[o.defs[0]] || {}).input || "self",
    overridden: o.defs.length > 1,
    defs: o.defs.map((f, i) => {
      const m = MOD[f] || { input: "self", rev: null };
      return { file: f, input: m.input, rev: m.rev, winning: i === 0,
        note: i === 0 ? (o.defs.length > 1 ? "highest priority" : "only definition") : "overridden" };
    }),
  });

  /* Per-host index. Holds the overlay and a merged path list — NOT a copy of
     the base rows. The base array is shared by reference across every system,
     which is the client-side echo of content-addressed base blobs. */
  const _hosts = {};
  /* Keyed by (host, rev). Options are a property of the REVISION, so viewing an
     older generation means reading that generation's cached option blob — not
     re-evaluating anything. Historical blobs are already stored for every
     deployed generation, which is what makes looking backwards free. */
  function hostIndex(sys, rev) {
    const at = rev || sys.commit;
    const key = `${sys.id}|${at}`;
    if (_hosts[key]) return _hosts[key];
    const r = rngFor(sys.hostname + "|" + at + "|cfg");
    const overlay = hostOverlay(sys, at, r);
    const overlayPaths = new Set(overlay.map(o => o.path));
    const revBase = baseAtRev(r);
    // Merged view = overlay rows first, then this revision's base rows.
    const merged = overlay.concat(
      BASE_OPTIONS.filter(o => !overlayPaths.has(o.path)).map(o => revBase.get(o.path) || o)
    ).map(decorate);
    merged.sort((a, b) => a.path.localeCompare(b.path));

    const modules = MODULES.map(m => ({
      ...m,
      label: m.path.replace(/\/default\.nix$/, "").replace(/\.nix$/, ""),
      sets: 0, wins: 0,
    }));
    const byPath = {}; modules.forEach(m => { byPath[m.path] = m; });
    merged.forEach(o => {
      o.defs.forEach(d => { if (byPath[d.file]) byPath[d.file].sets++; });
      if (byPath[o.source]) byPath[o.source].wins++;
    });

    // Diff vs the previously deployed generation. Server-computed in reality:
    // the two option blobs are compared in the backend and only the changed
    // rows are sent, never both full sets.
    const prevGen = Math.max(1, (sys.generation || 2) - 1);
    const changed = [];
    /* Always include the package-typed row and the unevaluated row, so the
       type-driven diff paths are visible on every host rather than on whichever
       one the rng happened to pick. */
    const pinned = ["environment.systemPackages", "services.nginx.virtualHosts"];
    const pool = overlay.filter(o => o.path !== "networking.hostName")
      .sort((a, b) => (pinned.indexOf(b.path)) - (pinned.indexOf(a.path)));
    const used = new Set();
    const n = 4 + Math.floor(r() * 3);
    for (let i = 0; i < n; i++) {
      let idx = i < pinned.length ? i : Math.floor(r() * pool.length), guard = 0;
      while (used.has(idx) && guard++ < 40) idx = Math.floor(r() * pool.length);
      used.add(idx);
      const o = pool[idx]; if (!o) continue;
      const kind = valueKind(o.type);
      /* The diff shape follows the value policy above — the mechanism is the
         same cheap hash-map comparison of two stored blobs either way; only the
         RENDERING of the value differs by type. */
      if (o.evalError) {
        changed.push({ path: o.path, kind: "changed", valueKind: "unevaluated", comparable: false,
          reason: "not evaluated at this revision", evalError: o.evalError, source: o.defs[0] });
        continue;
      }
      if (kind === "opaque" || kind === "submodule") {
        changed.push({ path: o.path, kind: "changed", valueKind: kind, comparable: false,
          reason: kind === "opaque" ? "function-valued: no comparable form" : "submodule: compared per sub-option",
          source: o.defs[0] });
        continue;
      }
      if (kind === "package") {
        // Element-wise by name+version — the useful diff, and free because the
        // closure package list is already extracted.
        const els = o.elements || [];
        const removed = els.slice(0, 1).map(e => ({ ...e, version: e.version.replace(/(\d+)$/, (m2) => String(Math.max(0, Number(m2) - 1))) }));
        const added = els.slice(0, 1);
        changed.push({ path: o.path, kind: "changed", valueKind: "package", comparable: true,
          pkgAdded: added, pkgRemoved: removed,
          pkgUnchanged: Math.max(0, els.length - added.length), source: o.defs[0] });
        continue;
      }
      const change = i === 0 ? "changed" : pick(r, ["changed", "changed", "added"]);
      let from = o.value;
      if (change === "added") from = null;
      else if (o.type === "bool") from = o.value === "true" ? "false" : "true";
      else if (o.type === "int" || o.type === "port") from = String(Number(String(o.value).replace(/\D/g, "") || 1) - 1);
      else from = String(o.value).replace(/"$/, '-prev"');
      changed.push({ path: o.path, kind: change, valueKind: "scalar", comparable: true, from, to: o.value, source: o.defs[0] });
    }
    const changedMap = {}; changed.forEach(c => { changedMap[c.path] = c; });

    const closureMib = 1100 + Math.floor(r() * 900);
    const idx = {
      merged, changed, changedMap, prevGen, rev: at,
      modules: modules.filter(m => m.sets > 0),
      counts: { all: merged.length, overridden: merged.filter(o => o.overridden).length, changed: changed.length },
      facts: {
        drv: `/nix/store/${Array.from({ length: 32 }, () => "0123456789abcdfghijklmnpqrsvwxyz"[Math.floor(r() * 32)]).join("")}-nixos-system-${sys.hostname}-${sys.nixosVersion || "25.05"}`,
        packages: 620 + Math.floor(r() * 340),
        closure: `${(closureMib / 1024).toFixed(2)} GiB`,
        evalSeconds: (2.1 + r() * 6).toFixed(1),
        evaluatedAt: `${1 + Math.floor(r() * 9)}h ago`,
        cached: r() > 0.35,
        optionCount: merged.length,
        deltaRows: overlay.length,
      },
    };
    _hosts[key] = idx;
    return idx;
  }

  const wait = (ms) => new Promise(res => setTimeout(res, ms));

  /* ── The client-facing API ───────────────────────────────────────────────
     Every method is async and returns only what a page needs. `query` is the
     important one: search + filter + offset/limit resolve server-side, so the
     browser holds one page of rows regardless of fleet or option-set size. */
  window.ConfigAPI = {
    async summary(sys, rev) {
      const idx = hostIndex(sys, rev);
      await wait(60 + Math.random() * 90);
      return { facts: idx.facts, modules: idx.modules, prevGen: idx.prevGen, counts: idx.counts, rev: idx.rev };
    },
    async query(sys, { q = "", filter = "all", offset = 0, limit = 60, rev = null } = {}) {
      const idx = hostIndex(sys, rev);
      await wait(90 + Math.random() * 120); // stand-in for the round trip
      const needle = q.trim().toLowerCase();
      let rows = idx.merged;
      if (filter === "overridden") rows = rows.filter(o => o.overridden);
      else if (filter === "changed") rows = rows.filter(o => idx.changedMap[o.path]);
      // Rows whose value has no text form (package, function, unevaluated) must
      // still be findable — match on type and eval error too, and never assume
      // value is a string.
      if (needle) rows = rows.filter(o =>
        String(o.path || "").toLowerCase().includes(needle) ||
        String(o.value ?? "").toLowerCase().includes(needle) ||
        String(o.type || "").toLowerCase().includes(needle) ||
        String(o.evalError || "").toLowerCase().includes(needle) ||
        String(o.source || "").toLowerCase().includes(needle) ||
        String(o.sourceInput || "").toLowerCase().includes(needle));
      const total = rows.length;
      return {
        total,
        offset,
        limit,
        rows: rows.slice(offset, offset + limit).map(o => ({ ...o, change: idx.changedMap[o.path] || null })),
        counts: idx.counts,
      };
    },
  };
})();
