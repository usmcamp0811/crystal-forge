---
layout: top-title-two-cols
color: dark
columns: is-8
class: text-left
---

:: title ::

# Build Coordination

:: left ::

<div class="text-xs leading-snug fade-in-left">

### <span style="font-size: 0.95rem;">Builder Workers</span>

- One or many distributed workers
- **Evaluate derivations** and their transitive dependencies
- Build heavy dependencies (e.g. Firefox, Chrome) first — **then** the final NixOS system
- Run **vulnerability scans** on every closure (currently <b>Vulnix</b>, others possible)
- Push built artifacts to shared caches (S3 / Attic / Nix)

<div class="mt-3 text-[#a8a8cc] text-[0.8rem] leading-snug">
Every builder operates directly from the <b>Crystal Forge database</b>,  
allowing horizontal scaling — multiple builders can work in parallel  
without central orchestration bottlenecks.
</div>

<div class="mt-3 text-[#a8a8cc] text-[0.8rem] italic leading-snug">
Think of it like a <b>Docker registry</b> for Nix derivations:  
build once, cache forever, and pull everywhere.
</div>

<div class="mt-3 text-[#a8a8cc] text-[0.8rem] leading-snug">
Future versions could deploy Builders inside <b>Kubernetes</b>  
for dynamic scaling of distributed builds — one cluster, many nodes, unified caches.
</div>

</div>

:: right ::

<div class="text-[0.7rem] fade-in-right">
  <img src="/assets/cf-build-coordination.png" alt="Crystal Forge Builder Architecture" class="max-w-[400px] opacity-95 mb-3" />
  <div class="text-[#999] italic text-[0.65rem]">
    Distributed builders share one DB and one cache — parallel builds, unified results.
  </div>
</div>

<style>
.fade-in-left { opacity:0; transform:translateX(-10px); animation:fadeLeft 1.0s ease forwards; }
.fade-in-right{ opacity:0; transform:translateX( 10px); animation:fadeRight 1.0s ease .2s forwards; }
@keyframes fadeLeft { to { opacity:1; transform:none; } }
@keyframes fadeRight{ to { opacity:1; transform:none; } }
</style>
