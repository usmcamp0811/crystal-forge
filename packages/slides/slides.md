---
theme: ./themes/slidev-theme-neversink
lineNumbers: true
layout: cover
color: dark
class: text-right
neversink_string: "Crystal Forge: Compliance-Native Infrastructure for NixOS"
colorSchema: light
routerMode: hash
title: Crystal Forge
---

<!-- Load fonts -->
<link href="https://fonts.googleapis.com/css2?family=Cinzel+Decorative:wght@700&family=Titillium+Web:wght@300;600&display=swap" rel="stylesheet">

<style>
.slidev-layout.cover {
  background: radial-gradient(circle at center, #0b0d11 0%, #141821 100%);
  color: #f0f0f0;
}

/* Title Block */
h1 {
  font-family: 'Cinzel Decorative', serif;
  font-weight: 700;
  font-size: 3rem;
  display: inline-flex;
  align-items: center;
  gap: 1rem;
  white-space: nowrap;
  text-shadow: 0 0 12px rgba(255, 255, 255, 0.3);
  margin-bottom: 0.75rem;
  margin-left: auto;
  margin-right: auto;
  position: relative;
}

h1 img {
  width: 115px;
  height: 115px;
  filter: drop-shadow(0 0 12px rgba(160, 130, 255, 0.5));
  vertical-align: middle;
}

/* Underline with crystal-like glow */
h1::after {
  content: "";
  display: block;
  height: 3px;
  width: 80%;
  margin: 0.6rem auto 0 auto;
  border-radius: 2px;
  background: linear-gradient(
    90deg,
    rgba(150, 120, 255, 0.6),
    rgba(200, 180, 255, 0.9),
    rgba(150, 120, 255, 0.6)
  );
  box-shadow: 0 0 8px rgba(160, 130, 255, 0.4);
}

/* Subtitle and text animation */
.fade-in {
  opacity: 0;
  animation: fadeIn 1.5s ease forwards;
}
@keyframes fadeIn {
  to { opacity: 1; }
}

/* Subtitle spacing + body font */
body {
  font-family: 'Titillium Web', sans-serif;
}

.tagline {
  font-style: italic;
  opacity: 0.7;
  margin-top: 0.5rem;
}

.subtitle {
  margin-top: 1.5rem;
  font-size: 1rem;
}

.footer {
  margin-top: 1.25rem;
  opacity: 0.6;
  font-size: 0.8rem;
}
</style>

#

<div style="text-align:center;">
  <h1>
    <img src="/assets/cf.png" alt="Crystal Forge Logo" />
    Crystal Forge
  </h1>

  <div class="tagline fade-in">
    "Forging Trust Through Reproducibility"
  </div>

  <div class="footer fade-in">
    Created by Matt Camp • 2025
  </div>
</div>

<img
  referrerpolicy="no-referrer-when-downgrade"
  src="https://matomo.aicampground.com/matomo.php?idsite=5&amp;rec=1"
  style="border:0"
  alt=""
/>

---
src: ./slides/01-intro.md
---

---
# I. The Problem Space
---

---
src: ./slides/02-compliance-burden.md
---

---
src: ./slides/03-current-approach.md
---

---
src: ./slides/04-traditional-tools.md
---

---
### II. The Nix Foundation
---

---
src: ./slides/05-nix-changes-the-game.md
---

---
src: ./slides/06-nix-single-source.md
---

---
### III. Crystal Forge Solution
---

---
src: ./slides/07-cf-what-if-we-made-this-simple.md
---

---
src: ./slides/08-cf-who-benefits.md
---

---

### IV. Architecture & Components

---
src: ./slides/09-cf-how-it-works.md
---

---
src: ./slides/10-cf-agent-lifecycle.md
---

---
src: ./slides/11-cf-build-coordination.md
---

### V. Key Advantages

---
src: ./slides/12-cf-beyond-config-mgmt.md
---

---
src: ./slides/13-cf-immutable-by-desding.md
---

---

### VI. Compliance & Reporting

**Slide 13: "Built for Audits"**

- RMF compliance made easier
- STIG exception management
- Framework-agnostic approach
- Evidence generation, not evidence gathering

**Slide 14: "Reporting (Coming Soon)"**

- Industry-standard formats
- Integration with existing cyber tools
- Automated report generation
- Human-readable + machine-parseable

---

### VII. What Crystal Forge Is Not

**Slide 15: "Scope & Boundaries"**

- Not active monitoring (use existing tools)
- Solves configuration compliance
- Not runtime security (that's separate)
- Complements, doesn't replace security stack

---

### VIII. Closing

**Slide 16: "The Vision"**

- Deterministic infrastructure meets compliance
- Make the right thing easy
- Reduce cognitive load on teams
- Open source foundation for regulated NixOS

**Slide 17: "Get Involved"**

- Project status & roadmap
- How to contribute
- Where to find us
