---
layout: top-title
color: dark
class: text-left
---

:: title ::

# The Compliance Burden

:: default ::

<div class="text-base mt-6 leading-relaxed fade-in-left">
Every CTO — and every home-lab tinkerer — faces the same question:
</div>

<div class="text-xl mt-2 mb-6 italic text-[#cbb6ff] fade-in-left">
<b>"Are all our systems actually running what we think they are?"</b>
</div>

<ul class="list-disc ml-6 text-base fade-in-left">
  <li>Are all systems patched and up-to-date?</li>
  <li>Can we prove that configuration drift isn’t hiding somewhere?</li>
  <li>When something breaks, do we even know what version was running?</li>
</ul>

<div class="mt-6 text-sm text-[#a8a8cc] fade-in-left">
Whether it's a <b>home lab</b> with five machines or a <b>global organization</b> with thousands,
the challenge is the same — maintaining confidence that your fleet matches your intent.
</div>

<div class="absolute right-0 bottom-0 opacity-10">
  <img src="/assets/cf.png" class="max-w-[400px]" />
</div>

<style>
.fade-in-left {
  opacity: 0;
  transform: translateX(-10px);
  animation: fadeLeft 1.2s ease forwards;
}
@keyframes fadeLeft {
  to { opacity: 1; transform: none; }
}
</style>
