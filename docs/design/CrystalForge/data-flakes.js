// Flakes mock data — registry + commits per flake + file diffs

const FLAKE_REGISTRY = [
  {
    id: "fl-infra",
    name: "infrastructure",
    url: "git+ssh://git@gitlab.cf.internal/ops/nixos-infra",
    branch: "main",
    description: "Production & staging NixOS host configurations",
    environment: "production",
    systemCount: 12,
    lastSyncAt: "4m ago",
    status: "synced",
    latestCommit: "a3f8c12",
    latestMessage: "stig: enforce audit rules for sudo",
    latestAuthor: "mreyes",
    latestAt: "2h ago",
    totalCommits: 847,
  },
  {
    id: "fl-web",
    name: "web-services",
    url: "git+ssh://git@gitlab.cf.internal/ops/nixos-web",
    branch: "main",
    description: "Web-tier and API service configurations",
    environment: "production",
    systemCount: 8,
    lastSyncAt: "6m ago",
    status: "synced",
    latestCommit: "c7e1902",
    latestMessage: "nginx: bump to 1.27.4, add HSTS headers",
    latestAuthor: "jpark",
    latestAt: "5h ago",
    totalCommits: 312,
  },
  {
    id: "fl-edge",
    name: "edge-gateway",
    url: "git+ssh://git@gitlab.cf.internal/ops/nixos-edge",
    branch: "main",
    description: "Edge node and WireGuard gateway configurations",
    environment: "edge",
    systemCount: 7,
    lastSyncAt: "1m ago",
    status: "synced",
    latestCommit: "4d2a801",
    latestMessage: "wireguard: add peer for sgp-01",
    latestAuthor: "dchen",
    latestAt: "1d ago",
    totalCommits: 204,
  },
  {
    id: "fl-build",
    name: "build-farm",
    url: "git+ssh://git@gitlab.cf.internal/ops/nixos-build",
    branch: "main",
    description: "Nix builder and Hydra worker configurations",
    environment: "production",
    systemCount: 5,
    lastSyncAt: "22m ago",
    status: "syncing",
    latestCommit: "9f0c344",
    latestMessage: "hydra: increase max-jobs to 64",
    latestAuthor: "ops-bot",
    latestAt: "8h ago",
    totalCommits: 119,
  },
  {
    id: "fl-lab",
    name: "lab-nodes",
    url: "git+ssh://git@gitlab.cf.internal/ops/nixos-lab",
    branch: "dev",
    description: "Lab and development host configurations",
    environment: "lab",
    systemCount: 5,
    lastSyncAt: "3h ago",
    status: "error",
    latestCommit: "1b7e5f0",
    latestMessage: "fix: reset lab-rig-01 after wipe",
    latestAuthor: "mreyes",
    latestAt: "3d ago",
    totalCommits: 88,
    errorMsg: "SSH key rejected by remote: Permission denied (publickey)",
  },
];

// Build/eval status for commits — vary by commit index
const COMMIT_PIPELINE_STATUS = [
  { eval: "complete", build: "cache-pushed",  deploy: "up-to-date" },
  { eval: "complete", build: "building",       deploy: "pending"    },
  { eval: "complete", build: "complete",       deploy: "up-to-date" },
  { eval: "failed",   build: null,             deploy: null         },
  { eval: "complete", build: "cache-pushed",   deploy: "up-to-date" },
  { eval: "complete", build: "complete",       deploy: "behind"     },
  { eval: "pending",  build: null,             deploy: null         },
  { eval: "complete", build: "failed",         deploy: null         },
];

const FLAKE_COMMITS = (() => {
  const base = {
  "fl-infra": [
    { sha:"a3f8c12", msg:"stig: enforce audit rules for sudo",        author:"mreyes",  at:"2h ago",  files:3, add:28, del:4  },
    { sha:"f1d9022", msg:"cve: patch openssl to 3.3.2",               author:"ops-bot", at:"1d ago",  files:2, add:12, del:8  },
    { sha:"8c4b311", msg:"atlas-02: add prometheus node exporter",    author:"dchen",   at:"2d ago",  files:1, add:14, del:0  },
    { sha:"77aef00", msg:"bump nixpkgs to 24.11.20260401",            author:"ops-bot", at:"3d ago",  files:1, add:2,  del:2  },
    { sha:"3c12889", msg:"orion-db: add pgbackup systemd timer",      author:"jpark",   at:"5d ago",  files:2, add:31, del:0  },
    { sha:"a22fc08", msg:"harden sshd: disable password auth",        author:"mreyes",  at:"1w ago",  files:1, add:6,  del:3  },
    { sha:"bc10201", msg:"feat: enable sops-nix for secrets",         author:"dchen",   at:"1w ago",  files:4, add:44, del:12 },
    { sha:"0e9f177", msg:"wireguard: rotate preshared keys",          author:"ops-bot", at:"2w ago",  files:2, add:8,  del:8  },
  ],
  "fl-web": [
    { sha:"c7e1902", msg:"nginx: bump to 1.27.4, add HSTS headers",  author:"jpark",   at:"5h ago",  files:2, add:18, del:6  },
    { sha:"2fa8031", msg:"gaia-web: scale up worker pool to 8",       author:"mreyes",  at:"1d ago",  files:1, add:3,  del:1  },
    { sha:"d90c411", msg:"fix: restart nginx on cert renewal",        author:"ops-bot", at:"2d ago",  files:1, add:8,  del:2  },
    { sha:"5e3b200", msg:"tls: enforce TLS 1.3, drop 1.0/1.1",       author:"jpark",   at:"4d ago",  files:1, add:4,  del:4  },
    { sha:"1a7c900", msg:"add gzip compression module",              author:"dchen",   at:"1w ago",  files:1, add:12, del:0  },
  ],
  "fl-edge": [
    { sha:"4d2a801", msg:"wireguard: add peer for sgp-01",            author:"dchen",   at:"1d ago",  files:2, add:14, del:0  },
    { sha:"9a01fc2", msg:"edge-nyc: fix MTU mismatch",                author:"mreyes",  at:"3d ago",  files:1, add:3,  del:3  },
    { sha:"7c88ef1", msg:"refactor: split per-region config",         author:"jpark",   at:"1w ago",  files:5, add:82, del:40 },
    { sha:"221b300", msg:"initial edge fleet setup",                  author:"mreyes",  at:"2w ago",  files:8, add:210,del:0  },
  ],
  "fl-build": [
    { sha:"9f0c344", msg:"hydra: increase max-jobs to 64",            author:"ops-bot", at:"8h ago",  files:1, add:2,  del:2  },
    { sha:"c3a1702", msg:"builder: add graviton arm64 node",          author:"dchen",   at:"2d ago",  files:2, add:22, del:4  },
    { sha:"551b0a1", msg:"hydra-03: bump to 32 cores",                author:"mreyes",  at:"5d ago",  files:1, add:1,  del:1  },
  ],
  "fl-lab": [
    { sha:"1b7e5f0", msg:"fix: reset lab-rig-01 after wipe",         author:"mreyes",  at:"3d ago",  files:2, add:18, del:12 },
    { sha:"0a2cf11", msg:"lab: add dev-node-04 config",               author:"dchen",   at:"1w ago",  files:2, add:28, del:0  },
  ],
  };

  // Pad fl-infra to ~80 commits to demonstrate scrolling at scale
  const MSGS = [
    "deps: bump nixpkgs", "fix: cgroups v2 quirk", "feat: add tailscale module",
    "stig: tighten kernel hardening", "ci: add sops-nix gating",
    "atlas-03: bump RAM to 64GB", "orion-cache: add zstd level=19",
    "loki: increase chunk size", "grafana: pin to 11.6.2",
    "hosts: rename phoenix → phoenix-01", "ssh: add ed25519 host key",
    "iptables: drop ICMP redirects", "kernel: enable BPF LSM",
    "audit: add execve rule", "wireguard: rotate keys",
    "users: add ops-bot ssh key", "remove unused debug module",
    "kernel: bump LTS to 6.6.62", "fix: race in services.acme",
    "metrics: ship to prometheus", "stig: V-230309 — disable IPv6 redirect",
    "vault: add audit log to /var/log", "fix: gnome-keyring on headless",
    "k3s: bump to v1.31.4+k3s1", "ci: cache nix store across jobs",
  ];
  const AUTHORS = ["mreyes","jpark","dchen","ops-bot","kthomas","arao","linus.h"];
  const TIMES = (i) => i < 1 ? "2h ago" : i < 4 ? `${i}d ago` : i < 14 ? `${Math.floor(i)}d ago` : i < 35 ? `${Math.floor(i/7)}w ago` : `${Math.floor(i/30)}mo ago`;
  const ALPHA = "abcdef0123456789";
  const sha = (n) => Array.from({length:7}, (_,i)=>ALPHA[(n*i*53+i*17+n*7) % 16]).join("");

  const extra = [];
  for (let i = base["fl-infra"].length; i < 78; i++) {
    extra.push({
      sha: sha(i+9),
      msg: MSGS[i % MSGS.length],
      author: AUTHORS[i % AUTHORS.length],
      at: TIMES(i),
      files: 1 + (i % 4),
      add: 4 + (i*7 % 30),
      del: i % 9,
    });
  }
  base["fl-infra"] = [...base["fl-infra"], ...extra];
  return base;
})();

const CHANGED_FILE_POOL = [
  { name:"hosts/atlas-01/default.nix",            ext:"nix",  add:12, del:4  },
  { name:"modules/stig/audit_rules/default.nix",  ext:"nix",  add:18, del:2  },
  { name:"flake.nix",                              ext:"nix",  add:2,  del:2  },
  { name:"modules/networking/wireguard.nix",       ext:"nix",  add:14, del:0  },
  { name:"modules/services/nginx.nix",             ext:"nix",  add:8,  del:6  },
  { name:"modules/security/sshd.nix",              ext:"nix",  add:6,  del:3  },
  { name:".sops.yaml",                             ext:"yaml", add:4,  del:0  },
  { name:"pkgs/custom/default.nix",               ext:"nix",  add:22, del:8  },
];

function flakeCommitFiles(sha, n) {
  const seed = sha.split("").reduce((a,c)=>a+c.charCodeAt(0),0);
  return CHANGED_FILE_POOL
    .map((f,i) => ({...f, _sort:((seed*(i+1)*9301)%100)}))
    .sort((a,b)=>a._sort-b._sort)
    .slice(0, Math.min(n, CHANGED_FILE_POOL.length));
}

// Environments a flake currently spans — DERIVED from the systems that use it,
// not an assigned property. A flake (a git repo of nixosConfigurations) can have
// hosts in production, staging, dev, etc. simultaneously.
function flakeEnvironments(flake) {
  const name = typeof flake === "string" ? flake : (flake && flake.name);
  const sys = (typeof SYSTEMS !== "undefined" ? SYSTEMS : []).filter(s => s.flake === name);
  let envs = [...new Set(sys.map(s => s.environment))];
  if (typeof ENVIRONMENTS !== "undefined") {
    const order = ENVIRONMENTS.map(e => e.name);
    envs.sort((a, b) => order.indexOf(a) - order.indexOf(b));
  }
  return envs;
}

function flakeFileDiff(file) {
  if (file.ext === "yaml") return `--- a/${file.name}
+++ b/${file.name}
@@ -2,4 +2,7 @@
 creation_rules:
   - path_regex: secrets/.*\\.yaml$
     age: >-
-      age1abc123
+      age1abc123,
+      age1xyz789`;
  // Longer realistic diff with multiple hunks for stress-testing scroll
  return `--- a/${file.name}
+++ b/${file.name}
@@ -14,8 +14,14 @@
 {
   services.openssh = {
     enable = true;
-    settings.PasswordAuthentication = true;
+    settings.PasswordAuthentication = false;
+    settings.KbdInteractiveAuthentication = false;
+    settings.PermitRootLogin = "no";
+    settings.MaxAuthTries = 3;
+    settings.ClientAliveInterval = 300;
+    settings.ClientAliveCountMax = 0;
   };
 }
@@ -42,12 +48,28 @@
   networking = {
     hostName = "atlas-01";
     domain = "cf.internal";
-    firewall.allowedTCPPorts = [ 22 80 443 ];
+    firewall = {
+      allowedTCPPorts = [ 22 80 443 9100 9090 ];
+      allowedUDPPorts = [ 51820 ];
+      logRefusedConnections = true;
+      logRefusedPackets = false;
+      extraCommands = ''
+        iptables -A INPUT -p tcp --dport 22 -m connlimit --connlimit-above 4 -j REJECT
+        iptables -A INPUT -m state --state INVALID -j DROP
+      '';
+    };
+    nameservers = [ "10.0.0.1" "10.0.0.2" ];
+    defaultGateway = "10.0.0.1";
   };

   security.sudo.extraRules = [
     {
       users = [ "ops" ];
-      commands = [ "ALL" ];
+      commands = [
+        { command = "/run/current-system/sw/bin/systemctl"; options = [ "NOPASSWD" ]; }
+        { command = "/run/current-system/sw/bin/journalctl"; options = [ "NOPASSWD" ]; }
+      ];
     }
   ];
@@ -98,6 +120,18 @@
   systemd.services.crystal-forge-agent = {
     description = "Crystal Forge agent daemon";
     wantedBy = [ "multi-user.target" ];
+    after = [ "network-online.target" ];
+    wants = [ "network-online.target" ];
+    serviceConfig = {
+      Type = "simple";
+      ExecStart = "\${pkgs.crystal-forge-agent}/bin/cf-agent --config /etc/cf/agent.toml";
+      Restart = "always";
+      RestartSec = "10s";
+      User = "cf-agent";
+      Group = "cf-agent";
+      DynamicUser = false;
+      NoNewPrivileges = true;
+      ProtectSystem = "strict";
+    };
   };
 }`;
}
