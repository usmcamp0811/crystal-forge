---
layout: top-title-two-cols
color: dark
columns: is-8
class: text-left
---

:: title ::

# Scope & Boundaries

:: left ::

<div class="text-xs leading-snug fade-in-left">

- **Not active monitoring** — use your existing EDR/SIEM/telemetry stack
- **Solves configuration compliance** — asserts what’s actually deployed
- **Not runtime security** — no user/process/network monitoring
- **Complements, not replaces** — fits alongside your current security tools

<div class="mt-4 text-[#a8a8cc] text-[0.8rem] leading-snug">
Crystal Forge is about <b>provable desired state</b>. It tells you whether a system
is running the <b>exact</b> configuration you claim — nothing more, nothing less.
You (or shared modules) decide <b>what “compliant” means</b>; CF verifies it’s deployed.
</div>

<div class="mt-3 text-[#a8a8cc] text-[0.8rem] leading-snug">
Future add-ons like <b>STIG modules</b> can define stricter baselines and waivers,
but that’s <i>policy</i> layered on top. CF’s core value is turning infrastructure into
<b>verifiable evidence</b> that scales from homelabs to enterprises.
</div>

</div>

:: right ::

<div class="text-[0.7rem] fade-in-right">
  <div class="ns-scope-card">
    <div class="ns-scope-title">Crystal Forge focuses on:</div>
    <ul class="ns-scope-list">
      <li>Configuration identity &amp; integrity</li>
      <li>Fleet-wide state attestation</li>
      <li>Auditability &amp; evidence generation</li>
    </ul>
    <div class="ns-scope-sep"></div>
    <div class="ns-scope-title opacity-80">Not in scope:</div>
    <ul class="ns-scope-list opacity-80">
      <li>Runtime threat detection</li>
      <li>User/session monitoring</li>
      <li>Network/behavior analytics</li>
    </ul>
  </div>

  <div class="text-[#999] italic text-[0.65rem] mt-3">
    Pair CF with your EDR, SIEM, and vuln scanners: <b>build once, prove always</b>.
  </div>
</div>

<style>
.ns-scope-card {
  background: #141821;
  border: 1px solid #2f2a4f;
  border-radius: 10px;
  padding: 14px;
  box-shadow: 0 0 20px rgba(111,94,255,0.15);
}
.ns-scope-title {
  font-weight: 600;
  color: #cbb6ff;
  margin-bottom: 6px;
}
.ns-scope-list {
  margin: 0 0 10px 1rem;
  padding: 0;
  list-style: disc;
}
.ns-scope-sep {
  height: 1px;
  background: linear-gradient(90deg, rgba(111,94,255,0.6), transparent);
  margin: 6px 0 10px 0;
}
.fade-in-left { opacity:0; transform:translateX(-10px); animation:fadeLeft 1.0s ease forwards; }
.fade-in-right{ opacity:0; transform:translateX( 10px); animation:fadeRight 1.0s ease .2s forwards; }
@keyframes fadeLeft { to { opacity:1; transform:none; } }
@keyframes fadeRight{ to { opacity:1; transform:none; } }
</style>
