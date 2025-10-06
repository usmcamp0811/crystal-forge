---
layout: top-title-two-cols
color: dark
columns: is-8
class: text-left
---

:: title ::

# Immutable by Design

:: left ::

<div class="text-xs leading-snug fade-in-left">

- Systems are **immutable** — what you deploy is exactly what runs
- Configuration changes are **recorded as events**, not ad-hoc mutations
- Every state transition leaves a **cryptographic audit trail**
- Tampering isn’t just discouraged — it’s <b>impossible without detection</b>
- Future **STIG modules** and **policy exception tracking** integrate directly with versioned configs
  - Each waiver or deviation becomes a first-class, reviewable object
  - Simplifying compliance documentation and accreditation

<div class="mt-4 text-[#a8a8cc] text-[0.8rem] italic leading-snug">
With Crystal Forge, compliance isn’t a separate process —  
it’s built into the fabric of how systems evolve.
</div>

</div>

:: right ::

<div class="text-[0.7rem] fade-in-right">
  <img src="/assets/cf-immutable-design.png" alt="Immutable System Design Concept" class="max-w-[400px] opacity-95 mb-3" />
  <div class="text-[#999] italic text-[0.65rem]">
    Immutable deployments. Traceable changes. Built-in accountability.
  </div>
</div>

<style>
.fade-in-left { opacity:0; transform:translateX(-10px); animation:fadeLeft 1.0s ease forwards; }
.fade-in-right{ opacity:0; transform:translateX( 10px); animation:fadeRight 1.0s ease .2s forwards; }
@keyframes fadeLeft { to { opacity:1; transform:none; } }
@keyframes fadeRight{ to { opacity:1; transform:none; } }
</style>
