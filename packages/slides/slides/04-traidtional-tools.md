---
layout: top-title-two-cols
color: dark
columns: is-9
class: text-left
---

:: title ::

# Why Traditional Tools Fall Short

:: left ::

<div class="text-base leading-relaxed fade-in-left">

Legacy automation tools were built to <b>execute commands</b>, not to <b>guarantee outcomes</b>.

<ul class="list-disc ml-6 mt-4">
  <li><b>Ansible / Chef / Puppet:</b> Stateful, imperative, and order-dependent.</li>
  <li><b>Half-deployed playbooks:</b> Fail mid-run and leave systems in limbo.</li>
  <li><b>No guaranteed end state:</b> “Success” just means “no errors.”</li>
  <li><b>Configuration drift:</b> Any admin can “fix” something manually — and no one knows.</li>
</ul>

<div class="mt-6 text-sm text-[#a8a8cc] leading-relaxed">
Without strict <b>process discipline</b> and full <b>re-deployment from scratch</b> every time, these tools 
can’t ensure consistency. You end up with systems that look right — but aren’t provably correct.
</div>

</div>

:: right ::

<div class="flex flex-col justify-center items-center h-full fade-in-right">
  <img src="/assets/imperative-drift.png" alt="Imperative Drift Diagram" class="max-w-[360px] opacity-90" />
  <div class="text-xs text-[#999] italic mt-3">
    Imperative tools execute tasks — they don’t enforce the final state.
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
