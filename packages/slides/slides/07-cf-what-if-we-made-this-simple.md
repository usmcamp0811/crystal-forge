---
layout: top-title-two-cols
color: dark
columns: is-9
class: text-left
---

:: title ::

# What If We Made This Simple?

:: left ::

<div class="text-base leading-relaxed fade-in-left">

We’ve seen the problems with traditional tools — drift, duplication, and uncertainty.  
Nix gives us the <b>deterministic foundation</b> to fix that.

Now imagine combining that with <b>continuous verification</b>...

<ul class="list-disc ml-6 mt-4">
  <li>Track the <b>configuration source</b> that defines your entire fleet.</li>
  <li>Log each system’s <b>evaluated output</b> into a database.</li>
  <li>Compare <b>actual vs. expected</b> — in a single lookup.</li>
</ul>

<div class="mt-6 text-sm text-[#a8a8cc] leading-relaxed">
If you can compute the system’s <b>derivation path</b>, you already know what it <b>should be</b>.  
Crystal Forge turns that insight into a continuous, auditable feedback loop —  
monitoring compliance without redeploying a thing.
</div>

<!-- <div class="mt-6 bg-[#141821] text-[#cbb6ff] text-sm font-mono px-3 py-2 rounded border border-[#2f2a4f] fade-in-left"> -->
<!-- $ crystal-forge query compliance reckless<br> -->
<!-- System matches expected derivation ✅ -->
<!-- </div> -->

</div>

:: right ::

<div class="flex flex-col justify-center items-center h-full fade-in-right">
  <img src="/assets/cf-simplified-truth.png" alt="Crystal Forge Simplified Architecture" class="max-w-[420px] opacity-95" />
  <div class="text-xs text-[#999] italic mt-3">
    One file. One query. Real-time fleet compliance.
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
