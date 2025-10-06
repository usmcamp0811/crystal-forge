---
layout: top-title-two-cols
color: dark
columns: is-8
class: text-left
---

:: title ::

# How It Works

:: left ::

<div class="text-xs leading-snug fade-in-left">

### <span style="font-size: 0.95rem;">Agent (on every NixOS system)</span>

<ul class="list-disc ml-4 space-y-1">
  <li>Watches <b><code>/run/current-system</code></b> (inotify) and fingerprints hardware info</li>
  <li>Sends a <b>heartbeat every 15 min</b> (and on change)</li>
  <li>If server replies with a new target, the agent <b>auto-switches</b> to that config — pulls closure from caches (S3 / Nix binary cache / Attic) and activates</li>
</ul>

### <span style="font-size: 0.95rem;">Server (Crystal Forge core)</span>

<ul class="list-disc ml-4 space-y-1">
  <li><b>Coordinates</b> agents and desired configs</li>
  <li><b>Dry-run evaluates</b> configs and verifies target derivation paths</li>
  <li>Records <b>state, history, audits</b></li>
</ul>

### <span style="font-size: 0.95rem;">Builder (scalable workers)</span>

<ul class="list-disc ml-4 space-y-1">
  <li>Evaluates <b>flakes</b> and <b>builds derivations</b></li>
  <li>Runs <b>vulnix</b> scans on closures</li>
  <li><b>Pushes</b> results to caches (S3 / Nix / Attic)</li>
  <li>Talks to DB directly (doesn’t need the server to orchestrate builds)</li>
</ul>

</div>

:: right ::

<div class="text-[0.7rem] leading-snug fade-in-right">
  <img src="/assets/cf-architecture.png" alt="Agent/Server/Builder flow" class="max-w-[350px] opacity-95 mb-2" />

  <div class="bg-[#141821] border border-[#2f2a4f] rounded p-2 font-mono text-[0.6rem] text-[#cbb6ff] leading-tight">
  # Agent → Server heartbeat<br/>
  POST /api/heartbeat<br/>
  { "host":"chesty", "current":"/nix/store/abcd…-nixos-system-chesty-25.05", "fingerprint":"sha256:…"}<br/><br/>
  # Server → Agent response<br/>
  { "target":"/nix/store/ef01…-nixos-system-chesty-25.05", "reason":"policy:update", "deploy":true }
  </div>

  <div class="text-[0.65rem] text-[#999] italic mt-2 leading-tight">
    Policy changes flow from <b>Server</b> → <b>Agent</b>; build artifacts flow from <b>Builder</b> → caches → <b>Agent</b>.
  </div>
</div>

<style>
.fade-in-left { opacity:0; transform:translateX(-10px); animation:fadeLeft 1.0s ease forwards; }
.fade-in-right{ opacity:0; transform:translateX( 10px); animation:fadeRight 1.0s ease .2s forwards; }
@keyframes fadeLeft { to { opacity:1; transform:none; } }
@keyframes fadeRight{ to { opacity:1; transform:none; } }
</style>
