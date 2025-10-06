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

.slidecolor {
  background-color: rgb(179, 12, 12) !important;
}
.neversink-custom-red-scheme {
  --neversink-bg-color: rgb(179, 12, 12);
  --neversink-text-color: #ffffff;
  --neversink-border-color: rgba(255, 255, 255, 0.3);
}
:root {
  --slidev-theme-primary: rgb(179, 12, 12);
}

.slidev-layout {
  background-color: rgb(179, 12, 12) !important;
}
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
layout: section
color: dark
class: text-center
---

<link href="https://fonts.googleapis.com/css2?family=Cinzel+Decorative:wght@700&family=Titillium+Web:wght@300;600&display=swap" rel="stylesheet">

<style>
/* Background */
.slidev-layout.section {
  position: relative;
  background: radial-gradient(circle at 50% 20%, #101423 0%, #0b0e17 45%, #0a0d15 100%);
  color: #e9e9f6;
}
.slidev-layout.section::before {
  content: "";
  position: absolute;
  inset: 0;
  opacity: .12;              /* tweak texture strength */
  pointer-events: none;
}
/* Title block */
.section-title {
  font-family: 'Titillium Web', serif;
  font-weight: 700;
  font-size: clamp(2.4rem, 6vw, 4.2rem);
  line-height: 1.1;
  text-shadow: 0 0 14px rgba(170,140,255,.25);
  display: inline-block;
  margin-bottom: .5rem;
  position: relative;
}
.section-title::after {
  content: "";
  display: block;
  height: 4px;
  width: 60%;
  margin: .6rem auto 0;
  border-radius: 3px;
  background: linear-gradient(90deg, rgba(150,120,255,.9), rgba(200,185,255,.8), rgba(150,120,255,.9));
  box-shadow: 0 0 10px rgba(160,130,255,.45);
}
/* Subtitle */
.section-sub {
  font-family: 'Titillium Web', sans-serif;
  font-size: clamp(1rem, 2.4vw, 1.25rem);
  color: #c9c9e8;
  opacity: .9;
  margin-top: .6rem;
}
/* Watermark logo */
.section-watermark {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  pointer-events: none;
}
.section-watermark img {
  width: min(38vw, 520px);
  opacity: .06;
  filter: drop-shadow(0 0 12px rgba(160,130,255,.4));
}
</style>

<div class="section-watermark">
  <img src="/assets/cf.png" alt="" />
</div>

<div class="mt-24">
  <div class="section-title">The Problem Space</div>
  <div class="section-sub">Where traditional tooling breaks—and why determinism matters</div>
</div>

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
layout: section
color: dark
class: text-center
---

<link href="https://fonts.googleapis.com/css2?family=Cinzel+Decorative:wght@700&family=Titillium+Web:wght@300;600&display=swap" rel="stylesheet">

<style>
/* Background */
.slidev-layout.section {
  position: relative;
  background: radial-gradient(circle at 50% 20%, #101423 0%, #0b0e17 45%, #0a0d15 100%);
  color: #e9e9f6;
}
.slidev-layout.section::before {
  content: "";
  position: absolute;
  inset: 0;
  opacity: .12;
  pointer-events: none;
}

/* Title block */
.section-title {
  font-family: 'Titillium Web', serif;
  font-weight: 700;
  font-size: clamp(2.4rem, 6vw, 4.2rem);
  line-height: 1.1;
  text-shadow: 0 0 14px rgba(170,140,255,.25);
  display: inline-block;
  margin-bottom: .5rem;
  position: relative;
}
.section-title::after {
  content: "";
  display: block;
  height: 4px;
  width: 60%;
  margin: .6rem auto 0;
  border-radius: 3px;
  background: linear-gradient(90deg, rgba(150,120,255,.9), rgba(200,185,255,.8), rgba(150,120,255,.9));
  box-shadow: 0 0 10px rgba(160,130,255,.45);
}

/* Subtitle */
.section-sub {
  font-family: 'Titillium Web', sans-serif;
  font-size: clamp(1rem, 2.4vw, 1.25rem);
  color: #c9c9e8;
  opacity: .9;
  margin-top: .6rem;
}

/* Keyword chips */
.chips { display:flex; gap:.5rem; justify-content:center; flex-wrap:wrap; margin-top:.9rem; }
.chip {
  font-family: 'Titillium Web', sans-serif;
  font-size: .9rem;
  padding: .25rem .6rem;
  border: 1px solid rgba(180,160,255,.35);
  border-radius: 999px;
  background: rgba(255,255,255,.04);
  color: #dcdcf6;
}

/* Watermark (optional: swap to a Nix snowflake if you have one) */
.section-watermark {
  position: absolute; inset: 0; display: grid; place-items: center; pointer-events: none;
}
.section-watermark img {
  width: min(36vw, 500px);
  opacity: .06;
  filter: drop-shadow(0 0 12px rgba(160,130,255,.4));
}
</style>

<div class="section-watermark">
  <!-- If you have a Nix logo, use it here instead: /assets/nix-snowflake.svg -->
  <img src="/assets/cf.png" alt="" />
</div>

<div class="mt-24">
  <div class="section-title">A Foundation built on Nix</div>
  <div class="section-sub">Determinism • Purity • Reproducibility • Hash-Based Identity</div>
</div>

---
src: ./slides/05-nix-changes-the-game.md
---

---
src: ./slides/06-nix-single-source.md
---

---
layout: section
color: dark
class: text-center
---

<link href="https://fonts.googleapis.com/css2?family=Cinzel+Decorative:wght@700&family=Titillium+Web:wght@300;600&display=swap" rel="stylesheet">

<style>
/* Background */
.slidev-layout.section {
  position: relative;
  background: radial-gradient(circle at 50% 20%, #101423 0%, #0b0e17 45%, #0a0d15 100%);
  color: #e9e9f6;
}
.slidev-layout.section::before {
  content: "";
  position: absolute;
  inset: 0;
  opacity: .12;
  pointer-events: none;
}

/* Title block */
.section-title {
  font-family: 'Titillium Web', serif;
  font-weight: 700;
  font-size: clamp(2.4rem, 6vw, 4.2rem);
  line-height: 1.1;
  text-shadow: 0 0 14px rgba(170,140,255,.25);
  display: inline-block;
  margin-bottom: .5rem;
  position: relative;
}
.section-title::after {
  content: "";
  display: block;
  height: 4px;
  width: 60%;
  margin: .6rem auto 0;
  border-radius: 3px;
  background: linear-gradient(90deg, rgba(150,120,255,.9), rgba(200,185,255,.8), rgba(150,120,255,.9));
  box-shadow: 0 0 10px rgba(160,130,255,.45);
}

/* Subtitle */
.section-sub {
  font-family: 'Titillium Web', sans-serif;
  font-size: clamp(1rem, 2.4vw, 1.25rem);
  color: #c9c9e8;
  opacity: .9;
  margin-top: .6rem;
}

/* Keyword chips */
.chips {
  display:flex;
  gap:.5rem;
  justify-content:center;
  flex-wrap:wrap;
  margin-top:.9rem;
}
.chip {
  font-family: 'Titillium Web', sans-serif;
  font-size: .9rem;
  padding: .25rem .6rem;
  border: 1px solid rgba(180,160,255,.35);
  border-radius: 999px;
  background: rgba(255,255,255,.04);
  color: #dcdcf6;
}

/* Watermark */
.section-watermark {
  position: absolute; inset: 0; display: grid; place-items: center; pointer-events: none;
}
.section-watermark img {
  width: min(36vw, 500px);
  opacity: .06;
  filter: drop-shadow(0 0 12px rgba(160,130,255,.4));
}
</style>

<div class="section-watermark">
  <img src="/assets/cf.png" alt="" />
</div>

<div class="mt-24">
  <div class="section-title">The Crystal Forge Solution</div>
  <div class="section-sub">Functional Infrastructure • Verifiable Compliance • Immutable Systems</div>
</div>

---
src: ./slides/07-cf-what-if-we-made-this-simple.md
---

---
src: ./slides/08-cf-who-benefits.md
---

---
layout: section
color: dark
class: text-center
---

<link href="https://fonts.googleapis.com/css2?family=Cinzel+Decorative:wght@700&family=Titillium+Web:wght@300;600&display=swap" rel="stylesheet">

<style>
/* Background */
.slidev-layout.section {
  position: relative;
  background: radial-gradient(circle at 50% 20%, #0e111c 0%, #0a0c13 45%, #090b11 100%);
  color: #e8e8f5;
}
.slidev-layout.section::before {
  content: "";
  position: absolute;
  inset: 0;
  opacity: .1;
  pointer-events: none;
}

/* Title */
.section-title {
  font-family: 'Titillium Web', serif;
  font-weight: 700;
  font-size: clamp(2.6rem, 6vw, 4.4rem);
  text-shadow: 0 0 14px rgba(170,140,255,.25);
  display: inline-block;
  position: relative;
  margin-bottom: .5rem;
}
.section-title::after {
  content: "";
  display: block;
  height: 4px;
  width: 65%;
  margin: .6rem auto 0;
  border-radius: 3px;
  background: linear-gradient(90deg, rgba(150,120,255,.9), rgba(200,185,255,.8), rgba(150,120,255,.9));
  box-shadow: 0 0 10px rgba(160,130,255,.4);
}

/* Subtitle */
.section-sub {
  font-family: 'Titillium Web', sans-serif;
  font-size: clamp(1rem, 2.4vw, 1.25rem);
  color: #c7c7e4;
  opacity: .9;
  margin-top: .6rem;
}

/* Icon row */
.icon-row {
  display: flex;
  justify-content: center;
  gap: 2rem;
  margin-top: 1.5rem;
  flex-wrap: wrap;
}
.icon {
  display: flex;
  flex-direction: column;
  align-items: center;
  color: #dcdcf6;
  font-family: 'Titillium Web', sans-serif;
  font-size: .9rem;
  opacity: .85;
}
.icon img {
  width: 72px;
  height: 72px;
  margin-bottom: .5rem;
  filter: drop-shadow(0 0 8px rgba(160,130,255,.3));
  opacity: .8;
}

/* Watermark */
.section-watermark {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  pointer-events: none;
}
.section-watermark img {
  width: min(38vw, 500px);
  opacity: .05;
  filter: drop-shadow(0 0 12px rgba(160,130,255,.4));
}
</style>

<div class="section-watermark">
  <img src="/assets/cf.png" alt="Crystal Forge Watermark" />
</div>

<div class="mt-24">
  <div class="section-title">Architecture & Components</div>
  <div class="section-sub">Agents • Server • Builder • Caches • Database</div>
</div>

---
src: ./slides/09-cf-how-it-works.md
---

---
src: ./slides/10-cf-agent-lifecycle.md
---

---
src: ./slides/11-cf-build-coordination.md
---

---
layout: section
color: dark
class: text-center
---

<link href="https://fonts.googleapis.com/css2?family=Cinzel+Decorative:wght@700&family=Titillium+Web:wght@300;600&display=swap" rel="stylesheet">

<style>
/* Background */
.slidev-layout.section {
  position: relative;
  background: radial-gradient(circle at 50% 20%, #0e101a 0%, #11152a 40%, #0a0d17 100%);
  color: #f2f2fa;
}
.slidev-layout.section::before {
  content: "";
  position: absolute;
  inset: 0;
  opacity: .12;
  pointer-events: none;
}

/* Title block */
.section-title {
  font-family: 'Titillium Web', serif;
  font-weight: 700;
  font-size: clamp(2.4rem, 6vw, 4.2rem);
  line-height: 1.1;
  text-shadow: 0 0 18px rgba(255,215,140,.25);
  display: inline-block;
  margin-bottom: .5rem;
  position: relative;
}
.section-title::after {
  content: "";
  display: block;
  height: 4px;
  width: 65%;
  margin: .6rem auto 0;
  border-radius: 3px;
  background: linear-gradient(90deg, rgba(230,205,140,.9), rgba(190,170,255,.8), rgba(230,205,140,.9));
  box-shadow: 0 0 12px rgba(210,190,130,.45);
}

/* Subtitle */
.section-sub {
  font-family: 'Titillium Web', sans-serif;
  font-size: clamp(1rem, 2.4vw, 1.25rem);
  color: #dcdcf6;
  opacity: .9;
  margin-top: .6rem;
}

/* Watermark logo */
.section-watermark {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  pointer-events: none;
}
.section-watermark img {
  width: min(38vw, 520px);
  opacity: .06;
  filter: drop-shadow(0 0 14px rgba(230,205,140,.35));
}
</style>

<div class="section-watermark">
  <img src="/assets/cf.png" alt="" />
</div>

<div class="mt-24">
  <div class="section-title">Key Advantages</div>
  <div class="section-sub">Why functional infrastructure delivers measurable impact</div>
</div>

---
src: ./slides/12-cf-beyond-config-mgmt.md
---

---
src: ./slides/13-cf-immutable-by-desding.md
---

---
layout: section
color: dark
class: text-center
---

<link href="https://fonts.googleapis.com/css2?family=Cinzel+Decorative:wght@700&family=Titillium+Web:wght@300;600&display=swap" rel="stylesheet">

<style>
/* Background */
.slidev-layout.section {
  position: relative;
  background: radial-gradient(circle at 50% 25%, #101428 0%, #141832 40%, #0a0c19 100%);
  color: #e8e9f9;
}
.slidev-layout.section::before {
  content: "";
  position: absolute;
  inset: 0;
  opacity: .12;
  pointer-events: none;
}

/* Title block */
.section-title {
  font-family: 'Titillium Web', serif;
  font-weight: 700;
  font-size: clamp(2.4rem, 6vw, 4.2rem);
  line-height: 1.1;
  text-shadow: 0 0 18px rgba(160,140,255,.25);
  display: inline-block;
  margin-bottom: .5rem;
  position: relative;
}
.section-title::after {
  content: "";
  display: block;
  height: 4px;
  width: 65%;
  margin: .6rem auto 0;
  border-radius: 3px;
  background: linear-gradient(90deg, rgba(150,120,255,.9), rgba(100,180,255,.8), rgba(150,120,255,.9));
  box-shadow: 0 0 12px rgba(120,100,255,.45);
}

/* Subtitle */
.section-sub {
  font-family: 'Titillium Web', sans-serif;
  font-size: clamp(1rem, 2.4vw, 1.25rem);
  color: #d0d3f6;
  opacity: .9;
  margin-top: .6rem;
}

/* Watermark logo */
.section-watermark {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  pointer-events: none;
}
.section-watermark img {
  width: min(38vw, 520px);
  opacity: .06;
  filter: drop-shadow(0 0 14px rgba(160,140,255,.35));
}
</style>

<div class="section-watermark">
  <img src="/assets/cf.png" alt="" />
</div>

<div class="mt-24">
  <div class="section-title">Compliance & Reporting</div>
  <div class="section-sub">From immutable state to verifiable audit trails</div>
</div>

---
src: ./slides/14-cf-built-for-audits.md
---

---
src: ./slides/15-cf-reporting.md
---

---
layout: section
color: dark
class: text-center
---

<link href="https://fonts.googleapis.com/css2?family=Cinzel+Decorative:wght@700&family=Titillium+Web:wght@300;600&display=swap" rel="stylesheet">

<style>
/* Background */
.slidev-layout.section {
  position: relative;
  background: radial-gradient(circle at 50% 25%, #101428 0%, #141832 40%, #0a0c19 100%);
  color: #e8e9f9;
}
.slidev-layout.section::before {
  content: "";
  position: absolute;
  inset: 0;
  opacity: .12;
  pointer-events: none;
}

/* Title block */
.section-title {
  font-family: 'Titillium Web', serif;
  font-weight: 700;
  font-size: clamp(2.4rem, 6vw, 4.2rem);
  line-height: 1.1;
  text-shadow: 0 0 18px rgba(160,140,255,.25);
  display: inline-block;
  margin-bottom: .5rem;
  position: relative;
}
.section-title::after {
  content: "";
  display: block;
  height: 4px;
  width: 65%;
  margin: .6rem auto 0;
  border-radius: 3px;
  background: linear-gradient(90deg, rgba(150,120,255,.9), rgba(100,180,255,.8), rgba(150,120,255,.9));
  box-shadow: 0 0 12px rgba(120,100,255,.45);
}

/* Subtitle */
.section-sub {
  font-family: 'Titillium Web', sans-serif;
  font-size: clamp(1rem, 2.4vw, 1.25rem);
  color: #d0d3f6;
  opacity: .9;
  margin-top: .6rem;
}

/* Watermark logo */
.section-watermark {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  pointer-events: none;
}
.section-watermark img {
  width: min(38vw, 520px);
  opacity: .06;
  filter: drop-shadow(0 0 14px rgba(160,140,255,.35));
}
</style>

<div class="section-watermark">
  <img src="/assets/cf.png" alt="Crystal Forge Watermark" />
</div>

<div class="mt-24">
  <div class="section-title">What Crystal Forge Is Not</div>
  <div class="section-sub">Purpose-Built for Verifiable Configuration, Not Runtime Security</div>
</div>

---
src: ./slides/16-scope-boundaries.md
---

---
layout: section
color: dark
class: text-center
---

<link href="https://fonts.googleapis.com/css2?family=Cinzel+Decorative:wght@700&family=Titillium+Web:wght@300;600&display=swap" rel="stylesheet">

<style>
/* Background */
.slidev-layout.section {
  position: relative;
  background: radial-gradient(circle at 50% 30%, #0c0f1a 0%, #12162a 40%, #0a0c15 100%);
  color: #f0f0fa;
}
.slidev-layout.section::before {
  content: "";
  position: absolute;
  inset: 0;
  background: radial-gradient(circle at center, rgba(255,220,150,.08) 0%, transparent 70%);
  pointer-events: none;
}

/* Title */
.section-title {
  font-family: 'Titillium Web', serif;
  font-weight: 700;
  font-size: clamp(2.6rem, 6vw, 4.4rem);
  line-height: 1.1;
  text-shadow: 0 0 18px rgba(255,215,160,.25);
  display: inline-block;
  position: relative;
  margin-bottom: .5rem;
}
.section-title::after {
  content: "";
  display: block;
  height: 4px;
  width: 70%;
  margin: .6rem auto 0;
  border-radius: 3px;
  background: linear-gradient(90deg, rgba(255,215,140,.9), rgba(160,140,255,.8), rgba(255,215,140,.9));
  box-shadow: 0 0 14px rgba(255,215,160,.45);
}

/* Subtitle */
.section-sub {
  font-family: 'Titillium Web', sans-serif;
  font-size: clamp(1rem, 2.4vw, 1.25rem);
  color: #d9d9f5;
  opacity: .9;
  margin-top: .6rem;
}

/* Watermark */
.section-watermark {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  pointer-events: none;
}
.section-watermark img {
  width: min(40vw, 540px);
  opacity: .05;
  filter: drop-shadow(0 0 14px rgba(255,215,140,.4));
}
</style>

<div class="section-watermark">
  <img src="/assets/cf.png" alt="Crystal Forge Watermark" />
</div>

<div class="mt-24">
  <div class="section-title">Closing Thoughts</div>
  <div class="section-sub">Forging a new standard for verifiable infrastructure</div>
</div>

---
src: ./slides/17-vision.md
---

---
src: ./slides/18-get-involved.md
---
