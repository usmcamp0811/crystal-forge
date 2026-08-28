/* Flake explorer — the flake as a set of OUTPUTS, at a specific revision.

   Outputs are a property of a COMMIT, not of a flake. Nothing guarantees two
   commits declare the same hosts, export the same modules, or lock the same
   inputs — an author can delete half the fleet or rewrite the module tree in one
   commit. So every query here is keyed by (flake, sha), and the pane reports
   what CHANGED from the previous commit, because that delta is the thing an
   operator actually needs to notice.

   Cost: one cached read per revision — `nix flake show` for outputs, flake.lock
   for inputs, and the option DECLARATIONS that the host eval already produces
   (options.<path>.declarations). No extra per-host evaluation, so a flake page
   costs the same whether the flake defines 3 hosts or 300.

   Modelled on a genuinely complex flake (gitlab:usmcamp0811/dotfiles: ~90
   inputs, several divergent nixpkgs channels, hosts enumerated from a
   directory, third-party modules injected into nixosModules). Nothing here
   assumes Snowfall or any particular flake framework. */
(function () {
  function hash(s) { let h = 2166136261; for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); } return h >>> 0; }
  function rngFor(seed) { let x = hash(seed) || 1; return () => { x ^= x << 13; x >>>= 0; x ^= x >> 17; x ^= x << 5; x >>>= 0; return x / 4294967296; }; }

  const UNMANAGED_POOL = ["vm-test-01", "vm-test-02", "bootstrap-iso", "installer", "sandbox-rig"];

  /* Each module carries the options it DECLARES (not the values a host merges).
     Declarations are a property of the module at a revision, come free from the
     eval already run for builds, and are identical for every host — so they
     cache once per rev and cost nothing per host. */
  const MODULE_POOL = [
    { path: "modules/nixos/system/default.nix", name: "system", desc: "base system: nix settings, gc, timezone, stateVersion", opts: [
      ["enable", "bool", "false"], ["stateVersion", "str", "\"26.05\""], ["gc.automatic", "bool", "true"], ["gc.olderThan", "str", "\"30d\""], ["timeZone", "str", "\"UTC\""], ["experimentalFeatures", "list of str", "[ \"nix-command\" \"flakes\" ]"],
    ]},
    { path: "modules/nixos/system/networking/default.nix", name: "system/networking", desc: "hostname, domain, firewall defaults", opts: [
      ["enable", "bool", "false"], ["hostName", "str", "null"], ["domain", "null or str", "null"], ["firewall.strict", "bool", "true"], ["firewall.extraTCPPorts", "list of port", "[ ]"], ["useDHCP", "bool", "false"],
    ]},
    { path: "modules/nixos/system/zfs/default.nix", name: "system/zfs", desc: "zfs pools, snapshots, scrub timers", opts: [
      ["enable", "bool", "false"], ["pools", "list of str", "[ ]"], ["autoSnapshot.enable", "bool", "true"], ["autoSnapshot.frequent", "int", "4"], ["scrubInterval", "str", "\"weekly\""], ["trim.enable", "bool", "true"],
    ]},
    { path: "modules/nixos/user/default.nix", name: "user", desc: "declarative users, immutable /etc/passwd", opts: [
      ["enable", "bool", "false"], ["users", "attrs of submodule", "{ }"], ["mutable", "bool", "false"], ["defaultShell", "package", "pkgs.bash"], ["sudoNoPassword", "bool", "false"],
    ]},
    { path: "modules/nixos/security/gpg/default.nix", name: "security/gpg", desc: "gpg agent, pinentry, key trust", opts: [
      ["enable", "bool", "false"], ["pinentry", "enum", "\"curses\""], ["agent.enableSSHSupport", "bool", "true"], ["trustedKeys", "list of str", "[ ]"],
    ]},
    { path: "modules/nixos/services/prometheus/default.nix", name: "services/prometheus", desc: "node exporter, scrape config", opts: [
      ["enable", "bool", "false"], ["exporters.node.enable", "bool", "true"], ["exporters.node.port", "port", "9100"], ["scrapeInterval", "str", "\"30s\""], ["retention", "str", "\"15d\""], ["remoteWrite", "null or str", "null"],
    ]},
    { path: "modules/nixos/services/grafana/default.nix", name: "services/grafana", desc: "grafana server, provisioned dashboards", opts: [
      ["enable", "bool", "false"], ["port", "port", "3000"], ["domain", "str", "\"localhost\""], ["dashboards", "list of path", "[ ]"], ["auth.ldap.enable", "bool", "false"], ["anonymousAccess", "bool", "false"],
    ]},
    { path: "modules/nixos/services/k3s/default.nix", name: "services/k3s", desc: "k3s server/agent role, cluster token", opts: [
      ["enable", "bool", "false"], ["role", "enum", "\"agent\""], ["serverAddr", "str", "\"\""], ["tokenFile", "null or path", "null"], ["extraFlags", "list of str", "[ ]"], ["disableTraefik", "bool", "true"],
    ]},
    { path: "modules/nixos/services/ldap-server/default.nix", name: "services/ldap-server", desc: "openldap, schema, TLS", opts: [
      ["enable", "bool", "false"], ["baseDn", "str", "\"\""], ["tls.enable", "bool", "true"], ["schemas", "list of path", "[ ]"], ["replication.enable", "bool", "false"],
    ]},
    { path: "modules/nixos/services/adguard/default.nix", name: "services/adguard", desc: "dns filtering, upstream resolvers", opts: [
      ["enable", "bool", "false"], ["port", "port", "3000"], ["upstreams", "list of str", "[ \"1.1.1.1\" ]"], ["blocklists", "list of str", "[ ]"],
    ]},
    { path: "modules/nixos/suites/kubernetes/default.nix", name: "suites/kubernetes", desc: "aggregate: k3s + cni + registry + metrics", opts: [
      ["enable", "bool", "false"], ["role", "enum", "\"agent\""], ["registry.enable", "bool", "true"], ["metrics.enable", "bool", "true"], ["cni", "enum", "\"flannel\""],
    ]},
    { path: "modules/nixos/router/default.nix", name: "router", desc: "zone-based routing, nat, dhcp, dns", opts: [
      ["enable", "bool", "false"], ["wanInterface", "str", "\"\""], ["zones", "attrs of submodule", "{ }"], ["nat.enable", "bool", "true"], ["dhcp.enable", "bool", "true"], ["dns.enable", "bool", "true"], ["ipv6.enable", "bool", "false"],
    ]},
  ];

  const INPUT_POOL = [
    { name: "nixpkgs", url: "github:NixOS/nixpkgs/release-26.05", rev: "8f2a1c9", updated: "3d ago", staleDays: 3, flakeFamily: "nixpkgs" },
    { name: "unstable", url: "github:NixOS/nixpkgs/nixos-unstable", rev: "d41e07b", updated: "1d ago", staleDays: 1, flakeFamily: "nixpkgs" },
    { name: "old-nixpkgs", url: "github:NixOS/nixpkgs/release-25.05", rev: "3c9f402", updated: "8mo ago", staleDays: 243, flakeFamily: "nixpkgs" },
    { name: "home-manager", url: "github:nix-community/home-manager/release-26.05", rev: "b7c0e51", updated: "5d ago", staleDays: 5 },
    { name: "crystal-forge", url: "gitlab:crystal-forge/crystal-forge/TASK-433-policy-poam-workflows", rev: "e91a774", updated: "4h ago", staleDays: 0, managed: true },
    { name: "impermanence", url: "github:nix-community/impermanence", rev: "a11c4a7", updated: "2mo ago", staleDays: 61 },
    { name: "disko", url: "github:nix-community/disko/v1.12.0", rev: "9d1f2b8", updated: "6w ago", staleDays: 42 },
    { name: "stylix", url: "github:danth/stylix/release-25.11", rev: "77bc019", updated: "3w ago", staleDays: 21 },
    { name: "sops-nix", url: "github:Mic92/sops-nix", rev: "c0eb1f5", updated: "9d ago", staleDays: 9 },
    { name: "nixos-hardware", url: "github:NixOS/nixos-hardware", rev: "5f81d33", updated: "12d ago", staleDays: 12 },
    { name: "microvm", url: "github:astro/microvm.nix", rev: "e2fa5d6", updated: "5mo ago", staleDays: 152 },
    { name: "vault-service", url: "gitlab:campground/vault-service/main", rev: "0f34b1c", updated: "2d ago", staleDays: 2, managed: true },
    { name: "flake-utils", url: "github:numtide/flake-utils", rev: "11707dc", updated: "7mo ago", staleDays: 213 },
    { name: "pre-commit-hooks", url: "github:cachix/pre-commit-hooks.nix", rev: "4e743fc", updated: "1mo ago", staleDays: 30 },
  ];

  /* Outputs AT ONE REVISION. Seeded by sha, so each commit legitimately
     declares its own host set, module set, and lock — including the case where
     a commit guts the flake down to almost nothing. */
  function outputsAt(flake, sha) {
    const r = rngFor(`${flake.id}|${sha}|outputs`);
    const managed = (typeof SYSTEMS !== "undefined" ? SYSTEMS : []).filter(s => s.flake === flake.name);

    // Some commits are structural: a rewrite that drops most hosts and modules.
    const gutted = r() < 0.12;

    const declaredManaged = managed.filter((s, i) => gutted ? i < Math.max(1, Math.floor(managed.length * 0.25)) : r() > 0.06);
    const nUnmanaged = gutted ? 0 : Math.floor(r() * 3);
    const declaredOnly = UNMANAGED_POOL.slice(0, nUnmanaged).map(h => ({
      hostname: h, declared: true, managedSystem: null, arch: "x86_64-linux",
      note: h.startsWith("vm-") ? "VM host, never deployed" : "no agent check-in",
    }));
    const declaredSet = new Set(declaredManaged.map(s => s.hostname));
    const systems = managed.map(s => ({
      hostname: s.hostname,
      declared: declaredSet.has(s.hostname),
      managedSystem: s,
      arch: "x86_64-linux",
      environment: s.environment,
      note: null,
    })).concat(declaredOnly);

    const nMods = gutted ? 2 + Math.floor(r() * 2) : 5 + Math.floor(r() * (MODULE_POOL.length - 5));
    const modules = MODULE_POOL.slice(0, nMods).map(m => ({
      path: m.path, name: m.name, desc: m.desc,
      consumers: Math.max(0, Math.round(declaredManaged.length * (0.15 + r() * 0.85))),
      options: m.opts.map(([p, t, d]) => ({ path: `${flake.namespace || "cf"}.${m.name.replace(/\//g, ".")}.${p}`, type: t, default: d })),
    })).sort((a, b) => b.consumers - a.consumers);

    const nInputs = gutted ? 2 : 8 + Math.floor(r() * (INPUT_POOL.length - 8));
    const inputs = INPUT_POOL.slice(0, nInputs).map(inp => ({
      ...inp,
      // Locked revs move between commits; that's usually the whole content of a
      // `nix flake update` commit.
      rev: r() < 0.25 ? inp.rev.slice(0, 4) + Math.floor(r() * 900 + 100) : inp.rev,
      transitive: inp.flakeFamily === "nixpkgs" ? 0 : 1 + Math.floor(r() * 12),
      follows: inp.flakeFamily === "nixpkgs" ? null : (r() < 0.6 ? "nixpkgs" : null),
    }));
    const channels = [...new Set(inputs.filter(i => i.flakeFamily === "nixpkgs").map(i => i.name))];

    return {
      sha, gutted, systems, modules, inputs, channels,
      counts: {
        declared: systems.filter(s => s.declared).length,
        managed: managed.length,
        declaredOnly: declaredOnly.length,
        orphaned: systems.filter(s => !s.declared).length,
        modules: modules.length,
        inputs: inputs.length,
        transitiveTotal: inputs.reduce((a, i) => a + i.transitive, inputs.length),
        channels: channels.length,
        staleInputs: inputs.filter(i => i.staleDays > 90).length,
      },
    };
  }

  const _cache = {};

  /* prevSha lets the pane report the delta. Server-side this is two cached
     output blobs compared in the backend — never two full sets shipped to the
     browser. */
  window.getFlakeOutputs = function (flake, sha, prevSha) {
    const key = `${flake.id}|${sha}|${prevSha || ""}`;
    if (_cache[key]) return _cache[key];
    const out = outputsAt(flake, sha || flake.latestCommit);
    if (prevSha) {
      const prev = outputsAt(flake, prevSha);
      const pHosts = new Set(prev.systems.filter(s => s.declared).map(s => s.hostname));
      const nHosts = new Set(out.systems.filter(s => s.declared).map(s => s.hostname));
      const pMods = new Set(prev.modules.map(m => m.path));
      const nMods = new Set(out.modules.map(m => m.path));
      const pIn = {}; prev.inputs.forEach(i => { pIn[i.name] = i.rev; });
      out.delta = {
        prevSha,
        hostsAdded: [...nHosts].filter(h => !pHosts.has(h)),
        hostsRemoved: [...pHosts].filter(h => !nHosts.has(h)),
        modulesAdded: [...nMods].filter(m => !pMods.has(m)),
        modulesRemoved: [...pMods].filter(m => !nMods.has(m)),
        inputsBumped: out.inputs.filter(i => pIn[i.name] && pIn[i.name] !== i.rev).map(i => ({ name: i.name, from: pIn[i.name], to: i.rev })),
        inputsAdded: out.inputs.filter(i => !pIn[i.name]).map(i => i.name),
        inputsRemoved: prev.inputs.filter(i => !out.inputs.some(o => o.name === i.name)).map(i => i.name),
      };
      out.delta.any = out.delta.hostsAdded.length + out.delta.hostsRemoved.length +
        out.delta.modulesAdded.length + out.delta.modulesRemoved.length +
        out.delta.inputsBumped.length + out.delta.inputsAdded.length + out.delta.inputsRemoved.length;
    }
    _cache[key] = out;
    return out;
  };
})();
