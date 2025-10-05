---
layout: top-title-two-cols
color: dark
columns: is-9
class: text-left
---

:: title ::

# Nix Changes the Game

:: left ::

<div class="text-base leading-relaxed fade-in-left">

Nix replaces imperatives with <b>functional determinism</b>.

<ul class="list-disc ml-6 mt-4">
  <li><b>Deterministic builds:</b> Same inputs always produce the same outputs.</li>
  <li><b>Functional configuration:</b> Systems become pure functions of their inputs.</li>
  <li><b>Hash-based identity:</b> <code>one path = one exact configuration</code></li>
</ul>

<div class="mt-6 text-sm text-[#a8a8cc] leading-relaxed">
With Nix, the system state is <b>provable</b> and <b>inspectable</b>.<br>
If two configurations differ, their <code>/nix/store</code> paths differ.<br>
If they match, they are identical — byte for byte, bit for bit.
</div>

<div class="mt-6 bg-[#141821] text-[#cbb6ff] text-sm font-mono px-3 py-2 rounded border border-[#2f2a4f] fade-in-left">
$ readlink /run/current-system<br>
/nix/store/ab23k9hsy3...-nixos-system-reckless-25.05
</div>

</div>

:: right ::

<div class="flex flex-col justify-center items-center h-full fade-in-right">
  <img src="/assets/nix-functional-path.png" alt="Deterministic Path Diagram" class="max-w-[400px] opacity-90" />
  <div class="text-xs text-[#999] italic mt-3">
    One input → one output. No drift. No surprises.
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
