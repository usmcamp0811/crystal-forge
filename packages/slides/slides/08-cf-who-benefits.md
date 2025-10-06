---
layout: top-title
color: dark
class: text-left
---

:: title ::

# Who Benefits

:: default ::

<div class="grid grid-cols-2 gap-10 mt-10 fade-in">
  <div class="flex items-start space-x-4">
    <img src="/assets/cto.png" alt="CTO Icon" class="w-[60px] opacity-90 mt-1" />
    <div>
      <div class="text-lg font-semibold text-[#cbb6ff]">CTOs</div>
      <div class="text-sm text-[#d1d1e0] leading-relaxed">
        Instantly answer <b>“Are we patched?”</b> with cryptographic certainty — no reports, no waiting.
      </div>
    </div>
  </div>

  <div class="flex items-start space-x-4">
    <img src="/assets/sysadmin.png" alt="Sysadmin Icon" class="w-[60px] opacity-90 mt-1" />
    <div>
      <div class="text-lg font-semibold text-[#cbb6ff]">Sysadmins</div>
      <div class="text-sm text-[#d1d1e0] leading-relaxed">
        Manage the entire fleet through a <b>single configuration set</b> — no conflicting playbooks, no drift.
      </div>
    </div>
  </div>

  <div class="flex items-start space-x-4">
    <img src="/assets/security.png" alt="Security Icon" class="w-[60px] opacity-90 mt-1" />
    <div>
      <div class="text-lg font-semibold text-[#cbb6ff]">Security Teams</div>
      <div class="text-sm text-[#d1d1e0] leading-relaxed">
        Gain <b>complete visibility</b> into what’s actually running — detect vulnerable or outdated builds instantly.
      </div>
    </div>
  </div>

  <div class="flex items-start space-x-4">
    <img src="/assets/compliance.png" alt="Compliance Icon" class="w-[60px] opacity-90 mt-1" />
    <div>
      <div class="text-lg font-semibold text-[#cbb6ff]">Compliance Officers</div>
      <div class="text-sm text-[#d1d1e0] leading-relaxed">
        Generate <b>audit-ready evidence</b> in real time — traceable, verifiable, and tamper-resistant.
      </div>
    </div>
  </div>
</div>

<div class="absolute right-0 bottom-0 opacity-10">
    <img src="/assets/cf-background-img.png" alt="" class="bg-fullpage" />
</div>

<style>
.fade-in {
  opacity: 0;
  transform: translateY(10px);
  animation: fadeIn 1.2s ease forwards;
}
.bg-fullpage {
  position: fixed;      /* stays behind content */
  inset: 0;             /* full viewport */
  width: 100vw;
  height: 100vh;
  object-fit: cover;    /* fills while preserving aspect */
  object-position: center;
  z-index: -1;          /* behind slide content */
  opacity: 0.12;        /* tweak to taste */
  pointer-events: none; /* don’t block clicks */
}
@keyframes fadeIn {
  to { opacity: 1; transform: none; }
}
</style>
