---
layout: top-title-two-cols
color: dark
columns: is-8
class: text-left
---

:: title ::

# The Vision

:: left ::

<div class="text-base leading-relaxed fade-in-left">

- **Deterministic infrastructure meets compliance**  
  Nix gives us cryptographic assurance of configuration identity.  
  Crystal Forge extends that to <b>prove compliance</b> across fleets.

- **Make the right thing easy**  
  Secure, reproducible systems shouldn’t require heroics —  
  CF automates the boring, provable parts.

- **Reduce cognitive load on teams**  
  One configuration language. One source of truth.  
  One way to know everything is as-declared.

- **Open-source foundation for regulated NixOS**  
  Built so homelabbers, enterprises, and defense orgs alike  
  can use the same verifiable stack — no proprietary lock-in.

<div class="mt-6 text-sm text-[#a8a8cc] leading-relaxed">
Crystal Forge’s goal is to make <b>Nix as ubiquitous in secure environments</b>  
as Red Hat once was — bringing deterministic, auditable infrastructure  
to every level of deployment.
</div>

</div>

:: right ::

<div class="flex flex-col justify-center items-center h-full fade-in-right">
  <img src="/assets/cf-vision.png" alt="Crystal Forge Vision Diagram" class="max-w-[230px] opacity-95 mb-3 rounded-lg shadow-lg" />
  <div class="text-xs text-[#999] italic">
    Open foundations. Reproducible systems. Auditable security.
  </div>
</div>

<style>
.fade-in-left { opacity:0; transform:translateX(-10px); animation:fadeLeft 1.0s ease forwards; }
.fade-in-right{ opacity:0; transform:translateX( 10px); animation:fadeRight 1.0s ease .3s forwards; }
@keyframes fadeLeft { to { opacity:1; transform:none; } }
@keyframes fadeRight{ to { opacity:1; transform:none; } }
</style>
