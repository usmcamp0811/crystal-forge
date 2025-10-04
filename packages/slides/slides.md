---
theme: ./themes/slidev-theme-neversink
lineNumbers: true
layout: cover
color: dark
class: text-right
neversink_string: "Crystal Forge: Compliance-Native Infrastructure for NixOS"
colorSchema: light
routerMode: hash
title: Nix
---

## Crystal Forge: Compliance-Native Infrastructure for NixOS

### I. The Problem Space

**Slide 1: "The Compliance Burden"**

- What keeps CTOs up at night
- Are all systems patched? Are we sure?
- Who's running what configuration?

**Slide 2: "The Current Approach"**

- Sysadmins: keeping systems running
- Security teams: tracking deployed software
- Compliance officers: proving we did it right
- All working in silos, manually

**Slide 3: "Why Traditional Tools Fall Short"**

- Ansible/Chef/Puppet: stateful, imperative
- Half-deployed playbooks
- No guaranteed end state
- Systems can drift without detection

---

### II. The Nix Foundation

**Slide 4: "Nix Changes the Game"**

- Deterministic, functional system configuration
- One path = one exact configuration
- `readlink /run/current-system`

**Slide 5: "A Single Source of Truth"**

- Derivation paths are cryptographic identities
- If you know the path, you know everything
- No ambiguity, no guessing

---

### III. Crystal Forge Solution

**Slide 6: "What If We Made This Simple?"**

- Track the flake that defines your fleet
- Log every system output to a database
- Compare actual vs. expected: one lookup

**Slide 7: "Who Benefits"**

- CTOs: answer "are we patched?" instantly
- Sysadmins: easier fleet management
- Security: complete visibility
- Compliance: audit-ready evidence

---

### IV. Architecture & Components

**Slide 8: "How It Works"**

- Agent: monitors `/run/current-system` + fingerprints
- Server: coordinates, verifies, instructs
- Builder: evaluates flakes, builds derivations

**Slide 9: "Agent Lifecycle"**

- Periodic heartbeats to server
- Cryptographically signed reports
- Receives deployment instructions
- Self-updating capability

**Slide 10: "Build Coordination"**

- Builder workers (one or many)
- Evaluate derivations
- Push to cache (S3/Attic/Nix)
- Coordinate via shared database

---

### V. Key Advantages

**Slide 11: "Beyond Configuration Management"**

- Not just Ansible-in-Nix
- Purely functional deployments
- No half-deployed states
- Drift detection built in

**Slide 12: "Immutable by Design"**

- Systems can't change without notification
- Configuration changes are events
- Audit trail is automatic
- Exception tracking (STIG waivers, etc.)

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

---

**Flow Notes:**

This outline moves from **problem → foundation → solution → implementation → benefits → boundaries**. It avoids getting too technical too early, focuses on pain points your audience understands, and shows how Nix's properties make Crystal Forge possible (rather than just being another tool).

The "What Crystal Forge Is Not" slide is critical—it sets realistic expectations and positions CF as part of a larger security strategy, not a silver bullet.
