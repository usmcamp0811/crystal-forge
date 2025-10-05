---
layout: top-title-two-cols
color: dark
columns: is-6
class: text-left
---
:: title ::
# Get Involved
:: left ::
<div class="text-base leading-relaxed fade-in-left">
  <p>
    Crystal Forge is <b>open source</b> — and still early in its journey.
    We're building the foundation for <b>secure, verifiable Nix-based infrastructure</b>
    and we want your input, testing, and ideas.
  </p>
  <ul class="list-disc ml-6 mt-4">
    <li>📍 <b>Status:</b> Core agent/server/builder loop working</li>
    <li>🛣️ <b>Roadmap:</b> Web UI, reporting system, and policy modules (STIG/RMF)</li>
    <li>🤝 <b>Contribute:</b> Code, docs, or ideas — all help welcome</li>
  </ul>
  <div class="mt-6 text-sm text-[#a8a8cc] leading-relaxed">
    Join the conversation, submit issues, or start contributing to the
    Crystal Forge ecosystem. Together, we can make secure, declarative infrastructure the norm.
  </div>
</div>
:: right ::
<div class="flex flex-col justify-center items-center h-full fade-in-right">
  <img src="/assets/cf-wants-you.png" alt="Crystal Forge Vision"
       class="max-w-[220px] opacity-95 mb-8 drop-shadow-lg" />
  
  <div class="w-full max-w-[320px] space-y-3">
    <div class="flex items-center gap-3 px-4 py-2 rounded-lg bg-[#1a1a2e] bg-opacity-40 hover:bg-opacity-60 transition-all">
      <img src="https://cdn.simpleicons.org/gitlab/cbb6ff" alt="GitLab" class="w-5 h-5 flex-shrink-0" loading="lazy" />
      <span class="text-xs leading-tight">
        <span class="text-[#a8a8cc] block mb-0.5">Project</span>
        <a href="https://gitlab.com/crystal-forge/crystal-forge" 
           class="text-[#cbb6ff] font-semibold hover:text-[#d4c6ff] transition-colors" 
           target="_blank" rel="noopener noreferrer">
          gitlab.com/crystal-forge/crystal-forge
        </a>
      </span>
    </div>
    <div class="flex items-center gap-3 px-4 py-2 rounded-lg bg-[#1a1a2e] bg-opacity-40 hover:bg-opacity-60 transition-all">
      <img src="https://cdn.simpleicons.org/bluesky/cbb6ff" alt="Bluesky" class="w-5 h-5 flex-shrink-0" loading="lazy" />
      <span class="text-xs leading-tight">
        <span class="text-[#a8a8cc] block mb-0.5">Follow on Bluesky</span>
        <a href="https://bsky.app/profile/matt-camp.com" 
           class="text-[#cbb6ff] font-semibold hover:text-[#d4c6ff] transition-colors" 
           target="_blank" rel="noopener noreferrer">
          @matt-camp.com
        </a>
      </span>
    </div>
    <div class="flex items-center gap-3 px-4 py-2 rounded-lg bg-[#1a1a2e] bg-opacity-40 hover:bg-opacity-60 transition-all">
      <img src="https://cdn.simpleicons.org/reddit/cbb6ff" alt="Reddit" class="w-5 h-5 flex-shrink-0" loading="lazy" />
      <span class="text-xs leading-tight">
        <span class="text-[#a8a8cc] block mb-0.5">Join on Reddit</span>
        <a href="https://www.reddit.com/user/USMCamp0811" 
           class="text-[#cbb6ff] font-semibold hover:text-[#d4c6ff] transition-colors" 
           target="_blank" rel="noopener noreferrer">
          u/USMCamp0811
        </a>
      </span>
    </div>
  </div>
</div>

<style>
.drop-shadow-lg {
  filter: drop-shadow(0 0 12px rgba(160,130,255,.3));
}
.fade-in-left {
  opacity: 0;
  transform: translateX(-10px);
  animation: fadeLeft 1.0s ease forwards;
}
.fade-in-right {
  opacity: 0;
  transform: translateX(10px);
  animation: fadeRight 1.0s ease 0.2s forwards;
}
@keyframes fadeLeft { to { opacity: 1; transform: none; } }
@keyframes fadeRight { to { opacity: 1; transform: none; } }
a {
  cursor: pointer;
  pointer-events: auto;
}
</style>
