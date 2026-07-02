// Server management / admin — users, roles, OIDC mappings, audit log, server info

const ADMIN_USERS = [
  { id:"u1", name:"Mira Reyes",   email:"mreyes@acme.io",     role:"admin",    source:"oidc",  groups:["cf-admins"],           envs:["all"],                       status:"active",   lastLogin:"2m ago",   mfa:true },
  { id:"u2", name:"Jordan Park",  email:"jpark@acme.io",      role:"operator", source:"oidc",  groups:["cf-operators","sre"],  envs:["production","staging"],      status:"active",   lastLogin:"1h ago",   mfa:true },
  { id:"u3", name:"Dana Chen",    email:"dchen@acme.io",      role:"operator", source:"oidc",  groups:["cf-operators"],        envs:["edge","lab"],                status:"active",   lastLogin:"3h ago",   mfa:true },
  { id:"u4", name:"Kit Thomas",   email:"kthomas@acme.io",    role:"viewer",   source:"oidc",  groups:["cf-viewers"],          envs:["staging"],                   status:"active",   lastLogin:"yesterday",mfa:false },
  { id:"u5", name:"ops-bot",      email:"ops-bot@acme.io",    role:"operator", source:"local", groups:[],                      envs:["all"],                       status:"active",   lastLogin:"just now", mfa:false, serviceAccount:true },
  { id:"u6", name:"Avery Rao",    email:"arao@acme.io",       role:"viewer",   source:"oidc",  groups:["cf-viewers"],          envs:["dev"],                       status:"disabled", lastLogin:"3w ago",   mfa:false },
  { id:"u7", name:"audit-export", email:"audit@acme.io",      role:"viewer",   source:"local", groups:[],                      envs:["all"],                       status:"active",   lastLogin:"6h ago",   mfa:false, serviceAccount:true },
];

const OIDC_MAPPINGS = [
  { id:"m1", group:"cf-admins",    role:"admin",    envs:["all"],                  users:1, priority:1 },
  { id:"m2", group:"cf-operators", role:"operator", envs:["production","staging"], users:2, priority:2 },
  { id:"m3", group:"sre",          role:"operator", envs:["all"],                  users:1, priority:3 },
  { id:"m4", group:"cf-viewers",   role:"viewer",   envs:[],                       users:2, priority:4 },
];

const ROLE_DEFS = [
  { role:"admin",    desc:"Full control — manage users, servers, all environments.", color:"#f87171",
    perms:["Manage users & OIDC", "Edit server config", "All operator powers", "View audit log"] },
  { role:"operator", desc:"Deploy, build, evaluate, and manage assigned environments.", color:"#60a5fa",
    perms:["Deploy & rollback", "Trigger eval/build", "Cancel jobs", "Accept CVEs", "Edit flakes/systems"] },
  { role:"viewer",   desc:"Read-only access to dashboards and reports.", color:"#9ca3af",
    perms:["View all dashboards", "Export reports", "Read audit log (own actions)"] },
];

const AUDIT_LOG = [
  { id:"a1",  at:"2m ago",    actor:"mreyes",   action:"cve.accept",        target:"CVE-2025-31822 (dev, lab)",          ip:"10.2.4.18",   kind:"security" },
  { id:"a2",  at:"8m ago",    actor:"jpark",    action:"system.deploy",     target:"atlas-01 → a3f8c12",                 ip:"10.2.4.31",   kind:"deploy" },
  { id:"a3",  at:"14m ago",   actor:"ops-bot",  action:"build.complete",    target:"linux-6.6.72 on hydra-01",           ip:"10.0.1.9",    kind:"build" },
  { id:"a4",  at:"22m ago",   actor:"dchen",    action:"flake.sync",        target:"edge-gateway",                       ip:"10.2.4.44",   kind:"config" },
  { id:"a5",  at:"38m ago",   actor:"mreyes",   action:"user.role_change",  target:"kthomas: operator → viewer",         ip:"10.2.4.18",   kind:"security" },
  { id:"a6",  at:"1h ago",    actor:"jpark",    action:"eval.cancel",       target:"web-services@c7e1902",               ip:"10.2.4.31",   kind:"build" },
  { id:"a7",  at:"1h ago",    actor:"mreyes",   action:"oidc.mapping_edit", target:"cf-operators → operator",            ip:"10.2.4.18",   kind:"security" },
  { id:"a8",  at:"2h ago",    actor:"dchen",    action:"builder.rotate_key",target:"graviton-01",                        ip:"10.2.4.44",   kind:"security" },
  { id:"a9",  at:"3h ago",    actor:"kthomas",  action:"auth.login",        target:"OIDC (keycloak)",                    ip:"10.5.2.7",    kind:"auth" },
  { id:"a10", at:"3h ago",    actor:"arao",     action:"auth.login_denied", target:"account disabled",                   ip:"10.5.2.99",   kind:"auth" },
  { id:"a11", at:"5h ago",    actor:"mreyes",   action:"cache.create",      target:"crystal-forge-edge-cache",           ip:"10.2.4.18",   kind:"config" },
  { id:"a12", at:"6h ago",    actor:"ops-bot",  action:"system.deploy",     target:"gaia-web-02 → c7e1902",              ip:"10.0.1.9",    kind:"deploy" },
  { id:"a13", at:"8h ago",    actor:"jpark",    action:"policy.edit",       target:"stig-ssh-hardening",                 ip:"10.2.4.31",   kind:"security" },
  { id:"a14", at:"yesterday", actor:"mreyes",   action:"user.create",       target:"arao (viewer)",                      ip:"10.2.4.18",   kind:"security" },
  { id:"a15", at:"yesterday", actor:"dchen",    action:"system.rollback",   target:"edge-fra-01 → gen #142",             ip:"10.2.4.44",   kind:"deploy" },
];

const SERVER_INFO = {
  version: "0.8.2",
  commit: "f3a9c01",
  uptime: "18d 4h",
  authMode: "OIDC (Keycloak)",
  oidcIssuer: "https://keycloak.acme.io/realms/crystal-forge",
  dbStatus: "healthy",
  dbSize: "2.4 GB",
  sessions: 6,
  tlsExpiry: "62d",
};

Object.assign(window, { ADMIN_USERS, OIDC_MAPPINGS, ROLE_DEFS, AUDIT_LOG, SERVER_INFO });

// Background / scheduled jobs — admin-configurable cron-like tasks
const BACKGROUND_JOBS = [
  { id:"j1", name:"Cache status poll",        desc:"Query binary caches to confirm tracked store paths still exist (detect GC eviction).", interval:"15m", enabled:true,  lastRun:"3m ago",  lastDuration:"4.2s",  nextRun:"in 12m", status:"healthy", impact:"low" },
  { id:"j2", name:"GC-eviction reconcile",    desc:"Flag configs whose derivations were garbage-collected so Scanning marks them needs-build.", interval:"1h",  enabled:true,  lastRun:"24m ago", lastDuration:"11s",   nextRun:"in 36m", status:"healthy", impact:"medium" },
  { id:"j3", name:"CVE DB refresh",           desc:"Pull latest NVD / advisory feeds into the local vulnerability database.", interval:"6h",  enabled:true,  lastRun:"1h ago",  lastDuration:"38s",   nextRun:"in 5h",  status:"healthy", impact:"low" },
  { id:"j4", name:"Agent heartbeat sweep",    desc:"Mark systems offline if no heartbeat past their interval; recompute fleet health.", interval:"1m",  enabled:true,  lastRun:"32s ago", lastDuration:"0.6s",  nextRun:"in 28s", status:"healthy", impact:"low" },
  { id:"j5", name:"Stale build-job reaper",   desc:"Re-queue or fail builds stuck past their timeout on dead builders.", interval:"5m",  enabled:true,  lastRun:"2m ago",  lastDuration:"1.1s",  nextRun:"in 3m",  status:"healthy", impact:"low" },
  { id:"j6", name:"Flake poll & sync",        desc:"Fetch tracked flake repos and enqueue evals for new commits.", interval:"5m",  enabled:true,  lastRun:"4m ago",  lastDuration:"6.8s",  nextRun:"in 1m",  status:"healthy", impact:"medium" },
  { id:"j7", name:"Session GC",               desc:"Expire idle sessions and purge revoked tokens.", interval:"30m", enabled:true,  lastRun:"18m ago", lastDuration:"0.3s",  nextRun:"in 12m", status:"healthy", impact:"low" },
  { id:"j8", name:"Audit log archival",       desc:"Roll audit events older than retention window to cold storage.", interval:"24h", enabled:false, lastRun:"never",   lastDuration:"—",     nextRun:"disabled", status:"disabled", impact:"medium" },
  { id:"j9", name:"Cache storage metrics",    desc:"Pull bucket size / object counts (CloudWatch, atticd) for the Caches view.", interval:"1h",  enabled:true,  lastRun:"41m ago", lastDuration:"9.4s",  nextRun:"in 19m", status:"degraded", impact:"medium", note:"edge-cache poll timed out last run" },
];
const JOB_INTERVALS = ["1m","5m","15m","30m","1h","6h","12h","24h","never"];

Object.assign(window, { BACKGROUND_JOBS, JOB_INTERVALS });

// Agent heartbeat configuration — global default + per-environment overrides
const HEARTBEAT_CONFIG = {
  globalIntervalSec: 60,
  staleMultiplier: 2,      // mark stale at N× interval missed
  offlineMultiplier: 5,    // mark offline at N× interval missed
  overrides: {
    production: 30,        // tighter heartbeat in prod
    edge: 120,             // edge links are slow/metered
    lab: 300,              // lab hosts ping rarely
  },
};
const HEARTBEAT_INTERVALS = [
  { v:15, l:"15s" }, { v:30, l:"30s" }, { v:60, l:"1m" },
  { v:90, l:"90s" }, { v:120, l:"2m" }, { v:300, l:"5m" }, { v:600, l:"10m" },
];

Object.assign(window, { HEARTBEAT_CONFIG, HEARTBEAT_INTERVALS });
