---
type: UI Design
title: "Design System: Accessibility, Responsive Design and Motion"
description: "Defines the WCAG 2.1 AA accessibility requirements, ARIA and focus rules, breakpoint and mobile behavior, z-index layers, and approved animations for the web UI."
tags:
  - crystal-forge
  - web-ui
  - design-system
  - accessibility
  - responsive
implementation_status: partial
generated:
  by: opencode/claude-sonnet-5-5
  at: 2026-10-04T21:00:00-05:00
sources:
  - id: origin
    resource: "Crystal Forge repository file docs/ui-ux-design-system.md at commit 3b23d36f"
    title: "Crystal Forge UI/UX Design System"
  - id: code-1
    resource: "Crystal Forge repository file packages/web-ui/assets/app.css at commit 3b23d36f"
    title: "app.css"
  - id: code-2
    resource: "Crystal Forge repository file packages/web-ui/src/components/layout/sidebar.rs at commit 3b23d36f"
    title: "sidebar.rs"
  - id: code-3
    resource: "Crystal Forge repository file packages/web-ui/src/components/layout/topbar.rs at commit 3b23d36f"
    title: "topbar.rs"
  - id: code-4
    resource: "Crystal Forge repository file packages/web-ui/src/components/notifications/toast.rs at commit 3b23d36f"
    title: "toast.rs"
---

# Design System: Accessibility, Responsive Design and Motion

This concept holds the Accessibility Requirements, Responsive Design, and Animation & Motion sections of the [Crystal Forge UI/UX Design System](design-system-overview.md).

## Accessibility Requirements

Crystal Forge targets **WCAG 2.1 Level AA** compliance.

### Required ARIA Attributes

```rust
// Buttons with icon-only
button {
    aria_label: "Close modal",
    // icon SVG
}

// Status regions
div {
    role: "status",
    aria_live: "polite",
    // Dynamic content
}

// Form errors
input {
    aria_invalid: "{has_error}",
    aria_describedby: "error-{field_id}",
}
p { id: "error-{field_id}", "{error_message}" }

// Modal
div {
    role: "dialog",
    aria_modal: "true",
    aria_labelledby: "modal-title",
}
```

### Focus Indicators

All interactive elements MUST have visible focus:

```rust
// Use the focus ring class
button { class: "cf-focus-ring", /* ... */ }
input { class: "cf-input cf-focus-ring", /* ... */ }
a { class: "cf-focus-ring", /* ... */ }
```

The `cf-focus-ring` class provides:
```css
.cf-focus-ring:focus,
.cf-focus-ring:focus-visible {
  outline: none;
  box-shadow: 0 0 0 2px var(--cf-focus-ring);
}
```

### Screen Reader Considerations

- Use semantic HTML (`button`, `nav`, `main`, `table`)
- Provide `aria-label` for icon-only buttons
- Use `sr-only` class for visually hidden but accessible text
- Announce dynamic changes with `aria-live` regions

---

## Responsive Design

Crystal Forge is **desktop-first** with mobile support.

### Breakpoint System

| Breakpoint | Width | Target |
|------------|-------|--------|
| Default | <768px | Mobile |
| `md` | 768px+ | Tablet |
| `lg` | 1024px+ | Desktop |
| `xl` | 1280px+ | Large desktop |
| `2xl` | 1536px+ | Ultra-wide |

### Mobile Behavior

| Component | Desktop | Mobile |
|-----------|---------|--------|
| Sidebar | `16rem` expanded, `4rem` collapsed rail | Replaced below 768px by a drawer (`w-64`) that opens from the topbar menu button (`aria-label` "Open navigation menu") |
| Grid | 3-4 columns | 1 column |
| Tables | Horizontal scroll | Card view or horizontal scroll |
| Modals | Centered, max-width | Full width, bottom sheet |
| Split panes | Side-by-side | Stacked |

### Z-Index Layering System

The UI uses named bands, not a fixed 0 to 80 scale. Pick the band that matches the role of the element. Do not invent a new value when an existing band fits.

| Band | Z-index | Examples |
|------|---------|----------|
| Page content | 0 to 6 | Sticky table headers, timeline rails |
| Sidebar rail | 20 | `components/layout/sidebar.rs` |
| Popovers and pickers | 30 to 60 | `.poams-picker` (30), bulk-action bars (55), coach bubble (60) |
| Mobile drawer overlay and drawer | 40 and 50 | `components/layout/sidebar.rs` |
| Modal overlays and dialogs | 50 | `ConfirmDialog` and the key modals (Tailwind `z-50`) |
| Side drawers | 80 | `.compliance-evidence-drawer` |
| Page-specific overlays and modals | 180 to 201 | CVE triage backdrop (200) and modal (201) |
| Toast | 220 | `components/notifications/toast.rs` |
| Setup coach | 225 to 240 | Spotlight (225), dock and pill (230), tour (240) |
| Classification banner | 990 | `components/layout/app_shell.rs` |
| Development banner | 1000 | `components/layout/dev_banner.rs` |

A toast MUST stay above modals. A modal MUST stay above the sidebar and the mobile drawer.

---

## Animation & Motion

Animation is **subtle and functional**, never decorative.

### Approved Animations

| Animation | Usage | Duration |
|-----------|-------|----------|
| `animate-spin` | Loading spinners | Continuous |
| `transition-colors` | Button hovers | 150ms |
| `transition-opacity` | Fade in/out | 150ms |

### Transitions

```rust
// Button hover
button { class: "transition-colors duration-150 cf-primary-btn hover:..." }

// Modal fade
div { class: "transition-opacity duration-150 {opacity_class}" }
```

### Reduced Motion

Respect `prefers-reduced-motion`:

```css
@media (prefers-reduced-motion: reduce) {
  .animate-spin {
    animation: none;
  }
}
```

## Related concepts

- [Crystal Forge UI/UX Design System (overview)](design-system-overview.md)
- [Design System: Theming and Color System](design-system-theming-and-color.md)
- [Design System: Typography, Spacing and Layout](design-system-typography-and-layout.md)
- [Design System: Component and Interaction Patterns](design-system-components-and-interaction.md)
- [Design System: Accessibility, Responsive Design and Motion](design-system-accessibility-responsive-motion.md)
- [Design System: Naming Conventions, Anti-Patterns and Decision Framework](design-system-conventions-and-anti-patterns.md)
- [Web UI Coding Standards](web-ui-coding-standards.md)
- [Frontend Component Isolation Standards](component-isolation-standards.md)

## Migration verification notes

- Claim: Mobile: sidebar hidden with hamburger menu; <768px.
  Finding: Mobile drawer in sidebar.rs (`is_mobile_drawer_open`) toggled from topbar.rs.
  Evidence: components/layout/sidebar.rs, topbar.rs
  Case: none
- Claim: Reduced motion rule for `.animate-spin`.
  Finding: app.css has `prefers-reduced-motion` blocks for selected animations (attention flash, coach, others) but none targeting `.animate-spin` (defined in tailwind.css).
  Evidence: assets/app.css lines 2017, 3825, 6918, 6946; tailwind.css:361
  Case: implementation incomplete
- Claim: Toast uses aria-live; modal uses role/aria-modal; `.sr-only` exists.
  Finding: Toast uses `role` status/alert with `aria_live` polite/assertive; `.sr-only` defined at app.css:817. Other ARIA claims not checked per component.
  Evidence: toast.rs; assets/app.css
  Case: partially checked
