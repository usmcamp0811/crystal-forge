---
layout: top-title-two-cols
color: dark
columns: is-8
class: text-left
---

:: title ::

# Beyond Configuration Management

:: left ::

<div class="text-xs leading-snug fade-in-left">

- Not just “Ansible, but in Nix” — a **paradigm shift** to functional infrastructure
- **Purely declarative deployments:** build state → verify state → deploy atomically
- **No half-deployed systems** — it either builds completely or not at all
- **Drift detection built in** — hashes don’t lie
- Recreate any production system in a sandbox **bit-for-bit identical**
- Modular, composable configs make standards **easy to share and reuse**
- Infra teams define policies, app teams just **plug into the model**

<div class="mt-4 text-[#a8a8cc] text-[0.8rem] italic leading-snug">
Crystal Forge turns infrastructure into a <b>verifiable function</b> —  
given the same inputs, you get the same result — always.
</div>

</div>

:: right ::

<div class="text-[0.7rem] fade-in-right">
<div style="width: 300px; height: 380px; overflow: hidden; border-radius: 8px;">
  <img src="/assets/cf-functional-infra.png"
       alt="Functional Infrastructure Concept"
       style="width: 100%; height: auto; object-fit: cover; transform: translateY(-10px); opacity: 0.95;" />
</div>
  <div class="text-[#999] italic text-[0.65rem]">
    Functional, reproducible, drift-free — infrastructure that behaves like code.
  </div>
</div>

<style>
.fade-in-left { opacity:0; transform:translateX(-10px); animation:fadeLeft 1.0s ease forwards; }
.fade-in-right{ opacity:0; transform:translateX( 10px); animation:fadeRight 1.0s ease .2s forwards; }
@keyframes fadeLeft { to { opacity:1; transform:none; } }
@keyframes fadeRight{ to { opacity:1; transform:none; } }
</style>
