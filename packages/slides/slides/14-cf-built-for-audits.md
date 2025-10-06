---
layout: top-title-two-cols
color: dark
columns: is-8
class: text-left
---

:: title ::

# Built for Audits

:: left ::

<div class="text-xs leading-snug fade-in-left">

- **RMF compliance made easier** — configurations can map directly to RMF controls
- **STIG exception management** — reasons for waivers live beside the code
- **Framework-agnostic** — easily align to RMF, STIG, CIS, NIST 800-53, or custom policies
- **Evidence generation, not evidence gathering** — reports are derived from configs, not checklists

<div class="mt-4 text-[#a8a8cc] text-[0.8rem] leading-snug">
Locked-down environments are the ones that benefit most from Crystal Forge.  
Deploying to a STIG-hardened machine becomes painless when compliance is just another <b>Nix module</b>.  
Toggle a module to shift between baselines — <b>Low → Moderate → High</b> — without rebuilding your workflow.
</div>

<div class="mt-3 text-[#a8a8cc] text-[0.8rem] leading-snug">
Crystal Forge already ensures its own presence before a deployment begins.  
The same mechanism can enforce <b>STIG modules</b>, <b>RMF policies</b>, or <b>organizational controls</b>  
as prerequisites, embedding compliance directly into the pipeline.
</div>

<div class="mt-3 text-[#a8a8cc] italic text-[0.8rem] leading-snug">
In future versions, deployment policies will directly reference frameworks —  
so you can prove, not just claim, that a system meets its security baseline.
</div>

</div>

:: right ::

<div class="text-[0.7rem] fade-in-right">
  <img src="/assets/cf-built-for-audits.png" alt="Built for Audits Concept" class="max-w-[400px] opacity-95 mb-3" />
  <div class="text-[#999] italic text-[0.65rem]">
    Compliance as code — frameworks become functions, evidence becomes automatic.
  </div>
</div>

<style>
.fade-in-left { opacity:0; transform:translateX(-10px); animation:fadeLeft 1.0s ease forwards; }
.fade-in-right{ opacity:0; transform:translateX( 10px); animation:fadeRight 1.0s ease .2s forwards; }
@keyframes fadeLeft { to { opacity:1; transform:none; } }
@keyframes fadeRight{ to { opacity:1; transform:none; } }
</style>
