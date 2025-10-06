---
layout: top-title-two-cols
color: dark
columns: is-9
class: text-left
---

:: title ::

# A Single Source of Truth

:: left ::

<div class="text-base leading-relaxed fade-in-left">

In Nix, <b>derivation paths</b> are <b>cryptographic identities</b> — not guesses.

<ul class="list-disc ml-6 mt-4">
  <li><b>If you know the path, you know everything.</b></li>
  <li><b>Reusable modules:</b> The same config can safely power any system.</li>
  <li><b>No duplication, no drift:</b> Every component composes into one unified graph.</li>
</ul>

<div class="mt-6 text-sm text-[#a8a8cc] leading-relaxed">
Unlike tools like Ansible, where one playbook might silently override another,  
Nix <b>refuses ambiguity</b> — everything merges into a single, abstract, functional truth.<br><br>
If two modules define the same thing, Nix errors out.<br>
If it builds, you can prove it’s identical everywhere.
</div>

<div class="mt-6 bg-[#141821] text-[#cbb6ff] text-sm font-mono px-3 py-2 rounded border border-[#2f2a4f] fade-in-left">
/nix/store/4a0p3cbxvn7d...-system-config-desktop<br>
/nix/store/4a0p3cbxvn7d...-system-config-server
</div>

</div>

:: right ::

<div class="flex flex-col justify-center items-center h-full fade-in-right">
  <img src="/assets/nix-single-truth.png" alt="Single Source of Truth Diagram" class="max-w-[400px] opacity-95" />
  <div class="text-xs text-[#999] italic mt-3">
    One definition. Many systems. Always identical.
  </div>
</div>

<style>
.fade-in-left {
  opacity: 0;
  transform: translateX(-10px);
  animation: fadeLeft 1.2s ease forwards;
}
.fade-in-right {
  opacity: 0;
  transform: translateX(10px);
  animation: fadeRight 1.2s ease 0.3s forwards;
}
@keyframes fadeLeft { to { opacity: 1; transform: none; } }
@keyframes fadeRight { to { opacity: 1; transform: none; } }

code {
  background: rgba(255, 255, 255, 0.05);
  border-radius: 3px;
  padding: 0.15rem 0.3rem;
}
</style>
