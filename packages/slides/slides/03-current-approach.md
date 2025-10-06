---
layout: top-title-two-cols
color: dark
columns: is-9
class: text-left
---

:: title ::

# The Current Approach

:: left ::

<div class="text-base leading-relaxed fade-in-left">
  <p>Everyone is working hard — just not together.</p>
  <ul class="list-disc ml-6 mt-4">
    <li><b>Sysadmins:</b> keeping systems running and applying patches.</li>
    <li><b>Security Teams:</b> running scans and hunting CVEs after the fact.</li>
    <li><b>Compliance Officers:</b> gathering screenshots and reports to prove it all happened.</li>
  </ul>
  <div class="mt-6 text-sm text-[#a8a8cc]">
    Each team is <b>working in isolation</b>, relying on manual checks and spreadsheets.  
    Configuration drift, delayed patching, and human error all create cracks where risk hides.
  </div>
  <div class="mt-4 p-3 rounded-lg bg-[#1a1a2e] bg-opacity-60 border-l-2 border-[#6f5eff] text-sm text-[#d8d8ff]">
    <b>The "Don't Touch It" Problem:</b> Once a system gets its ATO, teams are afraid to touch it.
    Production servers accumulate patch after patch, slowly drifting from dev environments.
    <b>The delta grows unknown</b> — and so does the risk.
  </div>
</div>

:: right ::

<div class="flex flex-col justify-center items-center h-full gap-4 fade-in-right">
  <img src="/assets/sysadmin.png" alt="Sysadmin Icon" class="w-[110px] opacity-80" />
  <div class="h-[2px] w-[80px] bg-gradient-to-r from-[#6f5eff] to-transparent opacity-40"></div>
  <img src="/assets/security.png" alt="Security Icon" class="w-[110px] opacity-80" />
  <div class="h-[2px] w-[80px] bg-gradient-to-r from-[#6f5eff] to-transparent opacity-40"></div>
  <img src="/assets/compliance.png" alt="Compliance Icon" class="w-[110px] opacity-80" />
  <div class="mt-4 text-xs text-[#999] italic">
    Each working in silos — disconnected from one another.
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
</style>
