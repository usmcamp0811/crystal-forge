---
type: Historical Reference
title: "Figma Extraction: User Workflows, Design Challenges and Redesign Plan"
description: "Records the April 2026 analysis of deploy, add-system, and CVE investigation workflows, the design challenges found, the phased Figma redesign recommendations, and the questions prepared for Claude in Figma."
tags:
  - crystal-forge
  - figma
  - redesign
  - workflows
  - ux
implementation_status: historical
status: deprecated
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-03T22:54:52-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/design/FIGMA_DESIGN_EXTRACTION.md at commit 3b23d36f"
    title: "Crystal Forge UI/UX Design Extraction for Figma"
---
# Figma Extraction: User Workflows, Design Challenges and Redesign Plan

> **Status:** historical. This concept holds the second half of the April 19, 2026 design extraction. It records the pain points of the workflows at that date and a proposed Figma redesign plan. Treat the pain points as dated observations. The audit that these sections build on is in [Crystal Forge UI/UX Design Extraction for Figma](figma-design-extraction-snapshot.md).

## User Workflows

### Workflow 1: Deploy a System Update

1. Navigate to **Flakes** page
2. See new commit in timeline
3. Click "View Evaluation" → redirects to `/evaluations/:commit_id`
4. Review evaluation results (all systems passed)
5. Navigate to **Systems** page
6. Filter by environment (e.g., "production")
7. Select system → click "Deploy"
8. Confirm in modal → deploy initiated
9. Navigate to **Builds** page to monitor build progress
10. Build completes → system auto-deploys (if policy allows)
11. Return to **Systems** page → see deployment status "up to date"

**Pain Points**:
- Too many page transitions
- No quick deploy from flakes view
- Build monitoring requires separate page
- No deployment confirmation/success feedback

### Workflow 2: Add a New System

1. Navigate to **Systems** page
2. Click "Add System" button
3. Fill out form in modal:
   - Hostname
   - Environment (dropdown)
   - SSH public key
   - IP address
   - Architecture
4. Submit → system created
5. System appears in list with "never deployed" status
6. User must separately deploy to get system operational

**Pain Points**:
- Form is long and not grouped logically
- No inline validation
- No guidance on what happens after creation
- SSH key requires manual copy/paste (no generator in this flow)

### Workflow 3: Investigate a Critical CVE

1. See CVE count on **Dashboard** → click to `/cves`
2. Filter by severity: "Critical"
3. See list of critical CVEs
4. Click CVE row → (no detail view, just external link)
5. Manually cross-reference affected systems
6. Navigate to each system individually to check status

**Pain Points**:
- No unified remediation view
- Can't see which specific systems are affected inline
- No bulk actions to update multiple systems
- No timeline of when CVE was introduced

---

## Design Challenges & Opportunities

### 1. Information Density vs Clarity

**Challenge**: Crystal Forge manages complex infrastructure with lots of metadata (IPs, hashes, versions, statuses). Showing everything leads to cognitive overload; hiding too much loses context.

**Opportunity**: Use progressive disclosure, collapsible sections, and clear visual hierarchy to surface critical info first.

### 2. Real-time Updates

**Challenge**: Builds, deployments, and health status change frequently. UI must reflect this without being distracting.

**Opportunity**: Subtle animations, toast notifications for significant events, live badges that update in place.

### 3. Workflow Efficiency

**Challenge**: Common tasks (deploy, rollback, check CVEs) require too many clicks and page transitions.

**Opportunity**: Contextual actions, quick actions menu, keyboard shortcuts, bulk operations.

### 4. Mobile Experience

**Challenge**: Infrastructure management is rarely mobile-first, but users may need to check status or deploy urgently from a phone.

**Opportunity**: Focus mobile on monitoring and simple actions, not complex forms.

### 5. Discoverability

**Challenge**: New users don't know where to start or what features exist.

**Opportunity**: Onboarding coach, contextual help, empty states with CTAs, tooltips.

### 6. Visual Consistency

**Challenge**: Many components, many pages, many developers → visual drift.

**Opportunity**: Rigorous design system in Figma with variants, clear usage guidelines.

---

## Figma Redesign Recommendations

### Phase 1: Design System Setup

1. **Create Color Styles**:
   - All brand colors, theme colors, status colors as Figma color styles
   - Separate light/dark mode variables

2. **Create Text Styles**:
   - All typography scales as text styles
   - Include color + size + weight

3. **Create Component Library**:
   - Buttons (all variants)
   - Input fields
   - Cards (all types)
   - Badges
   - Modals
   - Tables
   - Navigation components
   - Use Auto Layout and Variants

4. **Create Layout Grids**:
   - Page layout grid
   - Card grid (responsive columns)
   - Mobile, tablet, desktop frames

### Phase 2: Page Redesigns (Priority Order)

1. **Dashboard** - First impression, most visited
2. **Systems List & Detail** - Core functionality
3. **Login/Register** - First user touchpoint
4. **Builds** - High complexity, needs UX love
5. **Environments** - Simpler, good for pattern refinement
6. **Flakes** - Timeline visualization opportunity
7. **Evaluations** - Data-heavy, needs clarity
8. **CVEs** - Security-critical, needs urgency signals
9. **Deployment Policies** - Complex rules, needs simplification
10. **Builders** - Fewer users, lower priority

### Phase 3: Interaction & Flow Design

1. **Prototype Key Workflows**:
   - Deploy a system update (end-to-end)
   - Add a new system (form flow)
   - Investigate a CVE (cross-page navigation)

2. **Motion Design**:
   - Page transitions
   - Loading states
   - Toast animations
   - Modal open/close

3. **Responsive Behavior**:
   - Mobile, tablet, desktop variants for each page
   - Breakpoint behavior

### Phase 4: Developer Handoff Prep

1. **Annotate Components**:
   - Spacing values
   - Color tokens (reference theme vars)
   - State variations (hover, focus, disabled)

2. **Create Specs**:
   - Redlines for complex layouts
   - Animation timing/easing
   - Responsive behavior notes

3. **Export Assets**:
   - Icons as SVG
   - Logo variants
   - Any custom graphics

---

## Next Steps for Figma Workflow

1. **Create Figma Project**: "Crystal Forge Redesign"

2. **Import Design Tokens**:
   - Use the color palette and typography scale from this document
   - Create Figma variables for dark/light theme

3. **Build Component Library**:
   - Start with atomic components (buttons, inputs, badges)
   - Build up to molecules (cards, modals)
   - Then organisms (navigation, page layouts)

4. **Redesign Priority Pages**:
   - Use Claude in Figma to iterate on designs
   - Focus on UX improvements identified in this document

5. **Get Feedback**:
   - Share Figma prototypes with users/stakeholders
   - Test workflows with real users if possible

6. **Prepare for Implementation**:
   - Export design specs
   - Update `theme.rs` tokens if design changes them
   - Plan incremental rollout (component by component, page by page)

---

## Appendix: File Paths Reference

**Design System**:
- `packages/web-ui/src/theme.rs` - Rust design tokens
- `packages/web-ui/assets/app.css` - CSS variables and custom styles
- `packages/web-ui/tailwind.css` - Generated Tailwind utilities

**Components**:
- `packages/web-ui/src/components/` - All UI components
- `packages/web-ui/src/components/layout/` - Layout components
- `packages/web-ui/src/components/modals/` - Modal dialogs
- `packages/web-ui/src/components/forms/` - Form components

**Pages**:
- `packages/web-ui/src/views/` - Page-level views
- `packages/web-ui/src/routes.rs` - Route definitions

**State**:
- `packages/web-ui/src/state/` - Global state management

**API**:
- `packages/web-ui/src/api/` - API client and models

---

## Questions for Claude in Figma

When working with Claude in Figma, consider asking:

1. **"How can I improve the information hierarchy on the Dashboard to reduce cognitive load?"**
2. **"Design a more efficient workflow for deploying a system update with fewer page transitions."**
3. **"Create a mobile-optimized version of the Systems list that doesn't sacrifice functionality."**
4. **"How can I visualize CVE impact across the fleet in a more actionable way?"**
5. **"Design an onboarding flow that helps new users understand Crystal Forge's capabilities."**
6. **"Improve the visual consistency between card-based and table-based views."**
7. **"Create a design system that makes dark mode the hero but doesn't neglect light mode."**
8. **"How can I use color, size, and spacing to better communicate urgency and priority?"**

---

**End of Design Extraction Document**

This document captures the current state of Crystal Forge's web-UI as of April 19, 2026. Use it as a baseline for redesigning in Figma with the goal of creating a more intuitive, efficient, and visually polished interface.

## Related concepts

- [Crystal Forge UI/UX Design Extraction for Figma](figma-design-extraction-snapshot.md)
- [Claude + Figma Workflow for Crystal Forge Redesign](figma-claude-redesign-workflow.md)
- [Figma color palette (JSON asset)](figma-color-palette.json)
- [Crystal Forge UI/UX Design System](design-system-overview.md)
