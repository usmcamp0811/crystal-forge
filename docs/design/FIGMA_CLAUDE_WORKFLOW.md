# Claude + Figma Workflow for Crystal Forge Redesign

## Quick Start Guide

### What You Have

✅ **FIGMA_DESIGN_EXTRACTION.md** - Complete UI audit with:
- 80+ components inventory
- 18 page layouts & wireframes
- Full design system tokens
- Current UX pain points
- Redesign recommendations

✅ **FIGMA_COLOR_PALETTE.json** - All colors in structured JSON:
- Dark & light theme palettes
- Status colors (health, deployment, CVE, pipeline)
- Tailwind color references
- Ready to import or reference

✅ **Current Implementation** - Working web-UI at:
- Code: `/home/mcamp/code/crystal-forge/dev/packages/web-ui/`
- Design tokens: `src/theme.rs`
- CSS variables: `assets/app.css`

---

## Step 1: Set Up Figma Project

### Create Structure

```
Crystal Forge Redesign/
├── 🎨 Design System
│   ├── Colors (create color styles from JSON)
│   ├── Typography (text styles)
│   ├── Components (library)
│   └── Layout Grids
├── 📱 Pages - Mobile
│   ├── Dashboard
│   ├── Systems
│   └── ... (other pages)
├── 💻 Pages - Desktop
│   ├── Dashboard
│   ├── Systems
│   └── ... (other pages)
└── 🔄 Prototypes
    └── User Flows
```

### Import Colors to Figma

**Option 1: Manual (Recommended for Control)**
1. Open Figma → Your project
2. Select "Assets" panel → Color styles
3. Create color groups:
   - `Brand/Purple/Default` → `#82699b`
   - `Brand/Purple/Hover` → `#8616e0`
   - `Brand/Berry/Default` → `#6f1649`
   - ... (continue from FIGMA_COLOR_PALETTE.json)

4. Use Figma Variables for dark/light mode switching:
   - Create variable collection: "Theme"
   - Add modes: "Dark" (default), "Light"
   - For each color, create variable with both mode values

**Option 2: Plugin**
- Use a Figma plugin like "JSON to Figma Variables" or "Tokens Studio"
- Import FIGMA_COLOR_PALETTE.json
- Map to Figma variables

---

## Step 2: Ask Claude to Build Design System

### Example Prompts for Claude in Figma

**Start with Atomic Components:**

```
"Using the Crystal Forge design system from this context:

Brand purple: #82699b (default), #8616e0 (hover)
Border radius: 8px for buttons
Padding: 8px vertical, 16px horizontal
Typography: 16px base, 600 weight for buttons

Create a button component with these variants:
- Primary (purple background, white text)
- Danger (berry red background, white text)  
- Success (emerald green background, white text)
- Ghost (transparent with hover)

Each variant should have Normal, Hover, and Disabled states.
Use auto-layout and ensure it's production-ready."
```

**Then Build Molecules:**

```
"Create a System Card component based on this description:

The card should display:
- Hostname (top left, large bold text)
- Environment badge (top right, pill-shaped)
- Health status (colored dot + text, e.g., "Healthy" in emerald)
- Deployment status (colored badge, e.g., "Up to Date")
- IP address (secondary text, muted)
- Last deployed timestamp (small, muted)
- Action buttons at bottom (Edit, Deploy, Remove)

Use the Crystal Forge design tokens:
- Card background: #111827
- Card border: #1f2937
- Border radius: 8px
- Padding: 24px

Make it responsive - collapsible to mobile size.
Include variants for different health states (Healthy, Warning, Critical, Offline)."
```

**Build Full Pages:**

```
"Design the Crystal Forge Dashboard page using this layout:

[Reference the Dashboard wireframe from FIGMA_DESIGN_EXTRACTION.md]

Focus on:
1. Improving information hierarchy - make critical metrics stand out
2. Reducing cognitive load - use progressive disclosure
3. Adding visual breathing room - current design feels cramped
4. Making status colors more meaningful with subtle backgrounds

Use the existing color palette but feel free to adjust spacing, typography scale, or component sizes to improve UX.

Show me 2-3 variations with different approaches to information density."
```

---

## Step 3: Iterate on UX Improvements

### Priority Pain Points to Fix

Ask Claude to help you solve these specific issues:

#### Problem 1: Dashboard Overload
```
"The current Crystal Forge dashboard shows:
- 4 stat cards
- 2 donut charts  
- Build queue
- Recent deployments table
- CVE summary

All at once. This is overwhelming for new users.

Redesign the dashboard with:
1. A clear visual hierarchy (what should users look at first?)
2. Progressive disclosure (hide less critical info initially)
3. Contextual CTAs (guide users to next actions)
4. Better use of color to indicate urgency

Show me a redesigned dashboard that feels calm but informative."
```

#### Problem 2: Systems List Filters
```
"The Systems page has filter dropdowns but they feel disconnected from results.

Current flow:
1. User opens filter dropdown
2. Selects values (e.g., "Critical" health)
3. Dropdown closes
4. Results update below
5. User can't see what filters are active unless they reopen dropdown

Redesign this to:
- Show active filters as removable pills/chips
- Provide quick filter presets ("Show critical systems", "Show behind on deploys")
- Include a clear visual indicator of filtered vs. total count
- Make clearing filters obvious

Design a better filter UI."
```

#### Problem 3: System Detail Cards
```
"The System Detail page shows 5 info cards (System, Hardware, Network, Security, Agent).

Problems:
- All cards are same size regardless of content importance
- Hardware metrics (80% disk usage) lack context (is this bad?)
- No visual hierarchy - everything has equal weight
- Can't see trends (is disk usage growing?)

Redesign the System Detail view to:
- Use card sizes to indicate importance
- Add subtle progress bars or gauges for metrics (with warning thresholds)
- Show mini sparklines for trending data
- Use color more strategically (not just for status badges)

Create a more scannable, informative system detail layout."
```

#### Problem 4: Build Monitoring
```
"The Builds page has a two-pane layout (queue + details).

Issues:
- Queue list grows very long (no pagination/filtering)
- Build logs are raw text dumps (hard to parse)
- No visual progress indicator during build
- Can't see relationship between builds and systems

Redesign the build monitoring experience:
- Better visual representation of queue state
- Smart log filtering (errors, warnings, key events)
- Clear progress visualization
- Link builds back to systems/commits visually

Make build monitoring less tedious."
```

#### Problem 5: Mobile Experience
```
"The current design is mobile-responsive but not mobile-optimized.

On mobile:
- Sidebar becomes drawer (good)
- Cards stack vertically (okay)
- Tables overflow horizontally (bad)
- Forms are long and tedious (bad)
- No quick actions (bad)

Redesign key mobile screens:
1. Dashboard - focus on critical info only
2. Systems List - make card view better, skip table view
3. System Detail - prioritize actionable info
4. Add quick action menu for common tasks (deploy, rollback)

Design for monitoring and simple actions, not complex forms."
```

---

## Step 4: Create Interactive Prototypes

### Prototype Key Workflows

**Workflow 1: Deploy a System Update**

```
"Create an interactive prototype for this workflow:

User goal: Deploy the latest commit to a production system

Current flow (too many steps):
1. Flakes page → see new commit
2. Click "View Evaluation" → new page
3. Review eval results → navigate away
4. Systems page → find system → click  
5. Click "Deploy" → modal → confirm
6. Builds page → monitor build
7. Back to Systems → verify deployment

Redesigned flow should:
- Start from Dashboard (show "New commit available" alert)
- Allow one-click deploy with inline preview
- Show build progress in-context (toast or drawer)
- Confirm success without navigation

Prototype both flows so I can compare."
```

**Workflow 2: Investigate CVE**

```
"Create an interactive prototype for:

User sees "3 critical CVEs" on dashboard.
Goal: Understand impact and take action.

Redesigned flow:
1. Click CVE count → opens CVE drawer (not new page)
2. See list of CVEs with affected system counts
3. Click CVE → expands to show:
   - Severity + description
   - List of affected systems (inline)
   - Remediation suggestions
   - Bulk action: "Update all affected systems"
4. Click system → quick preview pops up
5. Take action without losing context

Prototype this as a smooth, in-context investigation flow."
```

---

## Step 5: Prepare for Developer Handoff

### Annotate Components

Ask Claude to help you create developer-ready specs:

```
"Take the Button component we designed and create a developer handoff spec.

Include:
- Exact spacing values (padding, gaps)
- Typography (size, weight, line height)
- Color tokens (reference CSS variable names like --cf-primary-btn)
- State variations (normal, hover, focus, disabled)
- Accessibility notes (focus ring, keyboard interaction)
- Animation timing (if any)

Format it as a structured spec a Rust/Dioxus developer can implement."
```

### Export Assets

For icons, logos, or custom graphics:
- Export as SVG (Select → Export → SVG)
- Use SVGO to optimize if needed
- Store in `packages/web-ui/assets/icons/`

---

## Step 6: Incremental Rollout Plan

### Phase 1: Design System Foundation
1. ✅ Review extracted design tokens
2. 🎨 Refine in Figma (adjust if needed)
3. 💻 Update `theme.rs` with new tokens
4. 🧪 Rebuild component library in code

### Phase 2: High-Impact Pages
1. 🏠 Dashboard (most visited)
2. 🖥️ Systems List & Detail (core functionality)
3. 🔐 Login/Register (first impression)

### Phase 3: Complex Pages
4. 🔨 Builds (needs most UX love)
5. 🛡️ CVEs (critical but less frequent)
6. 📋 Policies (complex, needs simplification)

### Phase 4: Polish
7. 📱 Mobile optimization
8. ⌨️ Keyboard navigation
9. ♿ Accessibility audit
10. 🎬 Animations & microinteractions

---

## Claude Prompting Best Practices

### ✅ DO:
- **Provide context**: Share design tokens, constraints, user pain points
- **Be specific**: "Create a card with X, Y, Z" not "make a nice card"
- **Request variations**: "Show me 3 different approaches"
- **Ask for rationale**: "Explain why you chose this layout"
- **Iterate**: "Good, but make the CTA more prominent"

### ❌ DON'T:
- **Be vague**: "Design something cool"
- **Skip constraints**: Always mention color palette, spacing system
- **Accept first draft**: Push for refinement
- **Forget accessibility**: Explicitly ask for a11y considerations

---

## Example Full Session with Claude

**You:**
```
I'm redesigning the Crystal Forge dashboard. Here's the current state:

[Paste relevant section from FIGMA_DESIGN_EXTRACTION.md]

Problems:
- Information overload (too much at once)
- No visual hierarchy (everything seems equally important)
- New users don't know where to start
- No workflow guidance

Design goals:
- Calm but informative
- Clear entry points for common tasks
- Prioritize critical alerts (CVEs, failed builds)
- Use color strategically (not everything is purple)

Color palette:
- Brand purple: #82699b
- Critical red: #f87171
- Warning amber: #fbbf24
- Healthy emerald: #34d399
- Page bg: #030712
- Card bg: #111827

Typography:
- Page title: 24px, bold
- Section title: 18px, semibold
- Body: 16px
- Caption: 12px, muted

Spacing:
- Page padding: 32px
- Card padding: 24px
- Card gap: 16px

Show me 3 different dashboard layouts:
1. Metric-focused (big numbers, clear status)
2. Workflow-focused (guide users to actions)
3. Alert-focused (surface problems first)

For each, explain the design rationale.
```

**Claude will respond with 3 Figma designs + explanations**

**You (iterate):**
```
I like option 2 (workflow-focused). 

Refine it:
- Make the "New commit available" alert more prominent (it's critical)
- Add a quick actions menu for common tasks
- Show the 3 most critical CVEs inline (not just a count)
- Add a "What's next?" section for new users

Keep the calm, spacious feel but make it more actionable.
```

**Claude will refine the design**

**You (finalize):**
```
Perfect. Now create a developer handoff spec for this dashboard.

Include:
- Component breakdown (what components are used)
- Layout structure (grid, flexbox, etc.)
- Responsive behavior (mobile, tablet, desktop)
- Data requirements (what API calls are needed)
- Interaction details (what happens when user clicks things)

Format it as markdown that I can give to the Rust/Dioxus developers.
```

---

## Resources

### Figma Plugins You Might Need

- **Tokens Studio** - For design token management
- **Stark** - Accessibility checking
- **Autoflow** - User flow diagrams
- **Content Reel** - Mock data generation
- **A11y - Color Contrast Checker** - WCAG compliance

### Crystal Forge Codebase References

- Design tokens: `packages/web-ui/src/theme.rs`
- Components: `packages/web-ui/src/components/`
- Views (pages): `packages/web-ui/src/views/`
- CSS variables: `packages/web-ui/assets/app.css`

### Design Inspiration

Since Crystal Forge is a NixOS fleet manager, look at:
- **Vercel Dashboard** - Clean, metric-focused
- **Railway** - Workflow-oriented deployment UI
- **Grafana** - Data visualization, dark mode
- **Linear** - Beautiful, fast, keyboard-first
- **GitHub Actions** - Build monitoring UI

---

## Final Tips

1. **Start small**: Don't try to redesign everything at once. Start with one component (like buttons) and get it perfect.

2. **Use variants heavily**: Figma components with variants make it easy to explore different states without duplicating.

3. **Prototype early**: Even rough prototypes help you feel the flow before committing to pixel-perfect designs.

4. **Test with real data**: Use realistic system names, IP addresses, timestamps - not "Lorem ipsum".

5. **Think in components**: Design with implementation in mind. Each Figma component should map to a Rust component.

6. **Accessibility first**: Check contrast ratios, keyboard navigation, screen reader labels from the start.

7. **Document decisions**: When Claude suggests something, ask why. Document the rationale for future reference.

8. **Share often**: Export PNGs or share Figma links to get feedback early and often.

---

## Next Steps

1. ✅ You have the extraction documents
2. 🎨 Create your Figma project
3. 💬 Start your first Claude session with the example prompts above
4. 🔄 Iterate until you love it
5. 📋 Create developer handoff specs
6. 💻 Implement incrementally in Crystal Forge

**Good luck with the redesign! You're setting up a much better workflow than iterating blindly in code.** 🚀
