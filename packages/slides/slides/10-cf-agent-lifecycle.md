---
layout: top-title-two-cols
color: dark
columns: is-8
class: text-left
---

:: title ::

# Agent Lifecycle

:: left ::

<div class="text-xs leading-snug fade-in-left">

- Periodically sends **heartbeats** (every 15 min or on change)
- **Cryptographically signs** each report using a private key
- Receives **deployment instructions** securely from the server
- Performs safe, **atomic system switch** (rollback on failure)
- Supports **self-updates** from verified sources

<div class="mt-4 text-[#a8a8cc] text-[0.8rem] leading-snug">
In future versions, agents could extend beyond NixOS —  
monitoring <b>Kubernetes clusters</b> or <b>Numtide’s system-manager</b> systems.  
All via the same lightweight, trust-based heartbeat model.
</div>

<div class="mt-4 text-[#a8a8cc] text-[0.8rem] italic leading-snug">
Unlike Ansible or Chef, Crystal Forge agents never rely on human-triggered playbooks —  
they act only when cryptographically verified instructions are received,  
ensuring <b>hands-off, auditable, and failure-safe deployments</b>.
</div>

</div>

:: right ::

<div class="text-[0.7rem] fade-in-right">
  <img src="/assets/cf-agent-lifecycle.png" alt="Agent Lifecycle Diagram" class="max-w-[400px] opacity-95 mb-3" />
  <div class="text-[#999] italic text-[0.65rem]">
    Signed heartbeats → Verified instructions → Atomic deploy → Rollback if needed.
  </div>
</div>

<style>
.fade-in-left { opacity:0; transform:translateX(-10px); animation:fadeLeft 1.0s ease forwards; }
.fade-in-right{ opacity:0; transform:translateX( 10px); animation:fadeRight 1.0s ease .2s forwards; }
@keyframes fadeLeft { to { opacity:1; transform:none; } }
@keyframes fadeRight{ to { opacity:1; transform:none; } }
</style>
