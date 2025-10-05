---
layout: top-title-two-cols
color: dark
columns: is-8
class: text-left
---

:: title ::

# Reporting

:: left ::

<div class="text-xs leading-snug fade-in-left">

- Outputs in **industry-standard formats** (e.g. OSCAL, JSON, CSV, PDF)
- Integrates with existing **cybersecurity tooling** (Splunk, Tenable, Nessus, OpenRMF)
- **Automated report generation** — driven by system state and policy
- **Human-readable + machine-parseable** outputs for dual audiences

<div class="mt-4 text-[#a8a8cc] text-[0.8rem] leading-snug">
Crystal Forge already aggregates the full state of every system it manages —  
configuration, vulnerabilities, build history, and compliance posture.  
That data is everything required to generate **ATO-ready documentation**.
</div>

<div class="mt-3 text-[#a8a8cc] text-[0.8rem] leading-snug">
By exporting directly to <b>OSCAL</b> (Open Security Controls Assessment Language),  
Crystal Forge will fit seamlessly into existing <b>RMF workflows</b> —  
no manual crosswalks or separate audit teams needed.
</div>

<div class="mt-3 text-[#a8a8cc] italic text-[0.8rem] leading-snug">
Future versions will introduce a web-based <b>Reporting Dashboard</b>,  
providing one-click evidence generation and live compliance analytics.  
What once took weeks of manual effort will be reproducible from code in seconds.
</div>

</div>

:: right ::

<div class="text-[0.7rem] fade-in-right">
  <img src="/assets/cf-reporting.png" alt="Crystal Forge Reporting Concept" class="max-w-[400px] opacity-95 mb-3" />
  <div class="text-[#999] italic text-[0.65rem]">
    One source of truth → OSCAL-compliant reports → Accelerated ATO.
  </div>
</div>

<style>
.fade-in-left { opacity:0; transform:translateX(-10px); animation:fadeLeft 1.0s ease forwards; }
.fade-in-right{ opacity:0; transform:translateX( 10px); animation:fadeRight 1.0s ease .2s forwards; }
@keyframes fadeLeft { to { opacity:1; transform:none; } }
@keyframes fadeRight{ to { opacity:1; transform:none; } }
</style>
