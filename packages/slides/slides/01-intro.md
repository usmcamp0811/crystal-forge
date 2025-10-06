---
layout: top-title-two-cols
color: dark
columns: is-9
---

:: title ::

# What is Crystal Forge?

:: left ::

<div class="bluf-box">
  <b>BLUF:</b> Crystal Forge tracks what's actually running on your NixOS fleet and compares it to what should be running—giving you cryptographic proof of compliance.
</div>
<div class="text-base leading-relaxed fade-in-left">
  <p>
    Every CTO asks: <b>"Are all our systems patched and compliant?"</b>
    With traditional tools, the answer is <b>"probably"</b> or <b>"let me check"</b>.
    With Crystal Forge, the answer is a <b>database query</b>.
  </p>
  <p class="mt-6">
    Because NixOS systems have a single deterministic path that identifies their exact configuration, Crystal Forge can:
  </p>
  <ul class="list-disc ml-4 mt-2">
    <li>Track every configuration in your fleet</li>
    <li>Compare actual vs. expected state</li>
    <li>Detect drift immediately</li>
    <li>Generate audit-ready compliance reports</li>
  </ul>
</div>
:: right ::
<div class="flex justify-center items-center h-full fade-in-right">
  <img src="/assets/cf.png" class="max-w-[250px]" />
</div>

<style>
.bluf-box {
  @apply text-sm p-4 mb-6 rounded-lg border-l-4;
  border-image: linear-gradient(180deg, #a58cff, #6f5eff) 1;
  background: rgba(255, 255, 255, 0.05);
  backdrop-filter: blur(6px);
  color: #d8d8ff;
  box-shadow: 0 0 12px rgba(120, 90, 255, 0.25);
}
h1::after {
  content: "";
  display: block;
  width: 220px;
  height: 3px;
  margin-top: 0.4rem;
  background: linear-gradient(90deg, rgba(150,120,255,0.8), rgba(255,255,255,0));
  box-shadow: 0 0 8px rgba(150,120,255,0.5);
  animation: shimmer 4s linear infinite;
}
@keyframes shimmer {
  0% { opacity: 0.7; transform: scaleX(0.8); }
  50% { opacity: 1; transform: scaleX(1); }
  100% { opacity: 0.7; transform: scaleX(0.8); }
}
.fade-in-left {
  opacity: 0;
  transform: translateX(-10px);
  animation: fadeLeft 1.2s ease forwards;
}
.fade-in-right {
  opacity: 0;
  transform: translateX(10px);
  animation: fadeRight 1.2s ease 0.4s forwards;
}
@keyframes fadeLeft { to { opacity: 1; transform: none; } }
@keyframes fadeRight { to { opacity: 1; transform: none; } }
</style>
