# Crystal Forge UI/UX Design Extraction for Figma

**Purpose**: This document extracts the current Crystal Forge web-UI implementation to enable redesign in Figma with Claude's assistance.

**Date**: April 19, 2026

---

## Executive Summary

Crystal Forge currently has a **functional but improvable** web-UI built with Dioxus (Rust → WASM). The implementation includes:
- 80+ components across 14 domain areas
- 18 routes/pages
- Comprehensive dark/light theme system
- Tailwind CSS + custom design tokens
- Responsive mobile-first design

**Key UX Challenges** to address in Figma redesign:
1. Information density vs clarity balance
2. Navigation hierarchy and discoverability
3. Workflow optimization for common tasks
4. Visual hierarchy and scanning patterns
5. Mobile experience refinement
6. Accessibility and keyboard navigation

---

## Design System Tokens

### Brand Colors

```
Primary Brand Purple: #8B5CF6 (violet-500)
  Hover: #7C3AED (violet-600)
  
Danger/Berry Red: #E11D48 (rose-600)
  Hover: #BE123C (rose-700)
  
Success Green: #10B981 (emerald-500)
  Hover: #059669 (emerald-600)
```

### Theme Colors (Dark Mode - Default)

**Surfaces**:
- Page Background: `#0f0f0f` (zinc-950)
- Sidebar: `#1a1a1a` (zinc-900)
- Card Background: `#1f1f1f` (zinc-900/90)
- Card Border: `#27272a` (zinc-800)
- Divider: `#27272a` (zinc-800)
- Subtle Background: `#18181b` (zinc-900)
- Modal Backdrop: `rgba(0, 0, 0, 0.75)` with backdrop-blur

**Text**:
- Primary: `#fafafa` (zinc-50) - headings, important values
- Secondary: `#a1a1aa` (zinc-400) - labels, descriptions
- Muted: `#71717a` (zinc-500) - timestamps, version numbers
- Disabled: `#52525b` (zinc-600)

**Interactive**:
- Input Background: `#18181b` (zinc-900)
- Input Border: `#27272a` (zinc-800)
- Input Border Focus: `#3f3f46` (zinc-700)
- Hover Background: `#27272a80` (zinc-800/50)
- Focus Ring: `#8b5cf680` (violet-500/50)

### Theme Colors (Light Mode)

**Surfaces**:
- Page Background: `#fafafa` (zinc-50)
- Sidebar: `#f4f4f5` (zinc-100)
- Card Background: `#ffffff` (white)
- Card Border: `#e4e4e7` (zinc-200)
- Divider: `#e4e4e7` (zinc-200)
- Subtle Background: `#f4f4f5` (zinc-100)

**Text**:
- Primary: `#18181b` (zinc-900)
- Secondary: `#52525b` (zinc-600)
- Muted: `#71717a` (zinc-500)
- Disabled: `#a1a1aa` (zinc-400)

**Interactive**:
- Input Background: `#ffffff` (white)
- Input Border: `#d4d4d8` (zinc-300)
- Input Border Focus: `#a1a1aa` (zinc-400)
- Hover Background: `#f4f4f580` (zinc-100/50)

### Status Colors

**Health Status**:
- Healthy: `#34d399` (emerald-400) - text, bg: `#34d39920`, border: `#34d39940`
- Warning: `#fbbf24` (amber-400) - text, bg: `#fbbf2420`, border: `#fbbf2440`
- Critical: `#f87171` (red-400) - text, bg: `#f8717120`, border: `#f8717140`
- Offline: `#6b7280` (gray-500) - text, bg: `#6b728020`, border: `#6b728040`

**Deployment Status**:
- Up to Date: `#34d399` (emerald-400), bg: `#34d39920`
- Behind: `#fbbf24` (amber-400), bg: `#fbbf2420`
- Ahead: `#60a5fa` (blue-400), bg: `#60a5fa20`
- Never Deployed: `#6b7280` (gray-500), bg: `#6b728020`

**CVE Severity**:
- Critical: `#ef4444` (red-500), bg: `#ef444420`
- High: `#fb923c` (orange-400), bg: `#fb923c20`
- Medium: `#facc15` (yellow-400), bg: `#facc1520`
- Low: `#60a5fa` (blue-400), bg: `#60a5fa20`

**Pipeline Stages**:
- Dry Run: `#9ca3af` (gray-400)
- Ready for Build: `#60a5fa` (blue-400)
- Building: `#818cf8` (indigo-400)
- Build Complete: `#a78bfa` (violet-400)
- Ready for Deploy: `#34d399` (emerald-400)

### Typography Scale

**Headings**:
- Page Title: `24px / 2rem`, font-weight: 700
- Section Title: `18px / 1.125rem`, font-weight: 600
- Stat Value: `30px / 1.875rem`, font-weight: 700

**Body Text**:
- Base: `16px / 1rem`, font-weight: 400
- Label: `14px / 0.875rem`, secondary color
- Caption: `12px / 0.75rem`, muted color
- Monospace (code/hashes): `14px / 0.875rem`, font-family: monospace

**Table Headers**: `12px / 0.75rem`, font-weight: 500, uppercase, letter-spacing: 0.05em

### Spacing System

**Page Layout**:
- Page padding: `32px / 2rem` (all sides)
- Card padding: `24px / 1.5rem`
- Card gap: `16px / 1rem`
- Section gap: `24px / 1.5rem`

**Component Spacing**:
- Table cell: horizontal `24px`, vertical `12px`
- Button padding: horizontal `16px`, vertical `8px`
- Input padding: horizontal `12px`, vertical `8px`

**Gaps**:
- Tight: `8px / 0.5rem`
- Normal: `16px / 1rem`
- Relaxed: `24px / 1.5rem`

### Border Radius

- Small (badges, pills): `4px / 0.25rem`
- Medium (buttons, inputs, cards): `8px / 0.5rem`
- Large (modals, panels): `12px / 0.75rem`
- Full (dots, avatars): `9999px`

### Shadows

```css
Card: 0 1px 3px rgba(0, 0, 0, 0.1)
Modal: 0 10px 25px rgba(0, 0, 0, 0.3)
Dropdown: 0 4px 6px rgba(0, 0, 0, 0.1)
```

---

## Page Layouts & Navigation

### Application Shell Structure

```
┌──────────────────────────────────────────────────────────┐
│ TopBar (fixed)                                           │
│  [Logo] [Theme Toggle] [User Menu]                      │
├─────────┬────────────────────────────────────────────────┤
│         │                                                │
│ Sidebar │  Main Content Area                            │
│ (fixed) │  (scrollable)                                 │
│         │                                                │
│  Nav    │  [Page Title]                                 │
│  Items  │  [Breadcrumbs/Tabs if applicable]            │
│         │                                                │
│         │  [Content: cards, tables, forms, etc.]        │
│         │                                                │
│         │                                                │
│         │                                                │
│         │                                                │
└─────────┴────────────────────────────────────────────────┘
```

**Desktop**: 
- Sidebar width: `256px` (expanded) / `64px` (collapsed)
- TopBar height: `64px`
- Sidebar collapsible via edge toggle button
- Main content: `max-width: 1536px`, centered

**Mobile** (< 768px):
- Sidebar becomes overlay drawer
- Hamburger menu in TopBar
- Full-width content

### Navigation Hierarchy

**Primary Navigation** (Sidebar):
1. 🏠 Dashboard
2. 🖥️ Systems
3. 🌍 Environments
4. 📦 Flakes
5. 🔨 Builds
6. 📊 Evaluations
7. 🏗️ Builders
8. 💾 Caches
9. 🛡️ CVEs (admin only)
10. 📋 Deployment Policies
11. ⚙️ Admin (admin only)
12. 🎨 Style Guide (dev mode)

**Secondary Navigation**:
- Within pages: Tabs (e.g., System Detail: Info / Logs)
- Filters and view toggles (e.g., Systems: card view / table view)

---

## Page Inventory & Wireframes

### 1. Dashboard (`/`)

**Purpose**: Fleet-wide overview at a glance

**Layout**:
```
┌─────────────────────────────────────────────┐
│ Dashboard                                   │
├─────────┬─────────┬─────────┬───────────────┤
│ Stat    │ Stat    │ Stat    │ Stat          │
│ Card    │ Card    │ Card    │ Card          │
│ (Total) │ (Healthy)│(Behind) │(Critical CVEs)│
├─────────┴─────────┴─────────┴───────────────┤
│                                             │
│ Fleet Health Breakdown (donut chart)       │
│                                             │
├──────────────────────┬──────────────────────┤
│ Deployment Status    │ Build Queue          │
│ Breakdown            │ Panel                │
│ (donut chart)        │ (live updates)       │
├──────────────────────┴──────────────────────┤
│ Recent Deployments List                     │
│ [table with 5 most recent]                  │
├─────────────────────────────────────────────┤
│ CVE Summary Panel                           │
│ [severity breakdown]                        │
└─────────────────────────────────────────────┘
```

**Widgets**:
- 4 stat cards (grid-cols-1 sm:grid-cols-2 xl:grid-cols-4)
- Fleet Health donut chart
- Deployment Status donut chart
- Build Queue panel (real-time)
- Recent Deployments table
- CVE Summary panel

**Current UX Issues**:
- Information overload for first-time users
- No clear workflow guidance
- Static layout (not customizable)

---

### 2. Systems List (`/systems`)

**Purpose**: Browse and manage all NixOS systems

**Layout**:
```
┌─────────────────────────────────────────────┐
│ Systems                                     │
├─────────────────────────────────────────────┤
│ [Search] [Filter: Health ▼] [Filter: Env ▼]│
│ [View Toggle: Cards / Table]                │
├─────────────────────────────────────────────┤
│                                             │
│ ┌─────────┐ ┌─────────┐ ┌─────────┐       │
│ │ System  │ │ System  │ │ System  │       │
│ │ Card    │ │ Card    │ │ Card    │       │
│ │         │ │         │ │         │       │
│ └─────────┘ └─────────┘ └─────────┘       │
│                                             │
│ (Card View - grid of system cards)          │
│                                             │
│ OR                                          │
│                                             │
│ ┌───────────────────────────────────────┐  │
│ │ Table View                            │  │
│ │ [Sortable columns]                    │  │
│ └───────────────────────────────────────┘  │
└─────────────────────────────────────────────┘
```

**Components**:
- Search input
- Multi-select filter dropdowns (health, environment, deployment status)
- View toggle (card/table)
- System cards (grid layout) or table
- Each card shows: hostname, environment, health, deployment status, IP, last deployed
- Actions: Edit, Deploy, Remove

**Current UX Issues**:
- Filters feel disconnected from results
- Card view wastes space on large screens
- Table view lacks quick actions
- No bulk operations

---

### 3. System Detail (`/systems/:id`)

**Purpose**: Deep dive into a single system

**Layout**:
```
┌─────────────────────────────────────────────┐
│ [← Back] system-hostname                    │
├─────────────────────────────────────────────┤
│ Tabs: [Info] [Logs]                         │
├─────────────────────────────────────────────┤
│ Info Tab:                                   │
│                                             │
│ ┌─────────────────┐ ┌──────────────────┐   │
│ │ System Info     │ │ Hardware Info    │   │
│ │ Card            │ │ Card             │   │
│ └─────────────────┘ └──────────────────┘   │
│                                             │
│ ┌─────────────────┐ ┌──────────────────┐   │
│ │ Network Info    │ │ Security Info    │   │
│ │ Card            │ │ Card             │   │
│ └─────────────────┘ └──────────────────┘   │
│                                             │
│ ┌──────────────────────────────────────┐   │
│ │ Agent Status Card                    │   │
│ └──────────────────────────────────────┘   │
└─────────────────────────────────────────────┘
```

**Info Cards**:
- **System Info**: Environment, health, deployment status, IP, store path, current generation
- **Hardware**: Architecture, CPU cores, memory, disk usage
- **Network**: Hostname, IPv4, IPv6, MAC addresses
- **Security**: SSH public key, firewall status
- **Agent**: Connection status, last heartbeat, version

**Logs Tab**:
- Real-time agent logs
- Filterable by level (info, warn, error)

**Current UX Issues**:
- Cards all same size regardless of content
- No visual hierarchy (all equal weight)
- Hardware metrics lack context (is 80% disk usage bad?)
- No historical trends

---

### 4. Environments (`/environments`)

**Purpose**: Manage deployment environments (dev, staging, prod)

**Layout**:
```
┌─────────────────────────────────────────────┐
│ Environments                      [+ Add]   │
├─────────────────────────────────────────────┤
│                                             │
│ ┌───────────────────────────────────────┐  │
│ │ Environment Card: production          │  │
│ │ [Edit] [Remove]                       │  │
│ │                                       │  │
│ │ Systems: 12                           │  │
│ │ Policies: Auto-deploy on stable tag   │  │
│ └───────────────────────────────────────┘  │
│                                             │
│ ┌───────────────────────────────────────┐  │
│ │ Environment Card: staging             │  │
│ │ ...                                   │  │
│ └───────────────────────────────────────┘  │
└─────────────────────────────────────────────┘
```

**Components**:
- List of environment cards
- Each card: name, system count, associated policies
- Actions: Edit, Remove, Add

**Current UX Issues**:
- Doesn't show what makes each environment different
- No visual indicator of environment criticality
- Policies are listed but not explained

---

### 5. Flakes (`/flakes`)

**Purpose**: Visualize flake repository commit timeline

**Layout**:
```
┌─────────────────────────────────────────────┐
│ Flake Repository                            │
├─────────────────────────────────────────────┤
│                                             │
│ Commit Timeline (vertical)                  │
│                                             │
│ ● abc1234 - 2 hours ago                     │
│ │ feat: add new module                      │
│ │ [View Evaluation]                         │
│ │                                           │
│ ● def5678 - 1 day ago                       │
│ │ fix: update package                       │
│ │ [View Evaluation]                         │
│ │                                           │
│ ● ghi9012 - 3 days ago                      │
│   ...                                       │
└─────────────────────────────────────────────┘
```

**Components**:
- Vertical timeline of commits
- Each commit: hash (short), message, timestamp, author
- Link to evaluation view for each commit

**Current UX Issues**:
- No branch visualization
- No indication of which commits are deployed where
- Timeline can be very long with no pagination

---

### 6. Builds (`/builds`)

**Purpose**: Build control center - monitor and manage builds

**Layout**:
```
┌─────────────────────────────────────────────┐
│ Builds                                      │
├──────────────────────┬──────────────────────┤
│ Build Queue          │ Build Detail         │
│                      │                      │
│ [Queued]             │ [Selected build info]│
│ build-123            │                      │
│ build-124            │ System: server-01    │
│                      │ Status: Building     │
│ [Building]           │ Progress: 45%        │
│ build-122 ◄──────────│ Started: 2 min ago   │
│                      │                      │
│ [Complete]           │ Logs:                │
│ build-121            │ [build output...]    │
│ build-120            │                      │
│                      │                      │
│ [Failed]             │ [Actions]            │
│ build-119            │ [Retry] [Cancel]     │
└──────────────────────┴──────────────────────┘
```

**Components**:
- Left pane: Build queue grouped by status (queued, building, complete, failed)
- Right pane: Selected build details with live logs
- Worker status strip at top
- Metrics row showing throughput

**Current UX Issues**:
- Queue can grow very long
- No filtering or search
- Build logs are raw and hard to parse
- No retention policy shown

---

### 7. Evaluations (`/evaluations`)

**Purpose**: View evaluation history and results

**Layout**:
```
┌─────────────────────────────────────────────┐
│ Evaluations                                 │
├─────────────────────────────────────────────┤
│ [Table of evaluations]                      │
│                                             │
│ Commit    | Time       | Status  | Systems  │
│ abc1234   | 2h ago     | Success | 12/12    │
│ def5678   | 1d ago     | Success | 12/12    │
│ ghi9012   | 3d ago     | Failed  | 0/12     │
│ ...                                         │
└─────────────────────────────────────────────┘
```

**Click row** → Navigate to `/evaluations/:commit_id`

**Evaluation Detail Page**:
```
┌─────────────────────────────────────────────┐
│ Evaluation: abc1234                         │
├─────────────────────────────────────────────┤
│ Status: Success                             │
│ Evaluated: 2 hours ago                      │
│ Systems: 12/12 successful                   │
├─────────────────────────────────────────────┤
│ [Per-system evaluation results table]      │
│                                             │
│ System       | Status  | Store Path        │
│ server-01    | Success | /nix/store/...    │
│ server-02    | Success | /nix/store/...    │
│ ...                                         │
├─────────────────────────────────────────────┤
│ [View Logs] button                          │
└─────────────────────────────────────────────┘
```

**Current UX Issues**:
- Table is dense and hard to scan
- No diff view between evaluations
- Failures don't show why they failed inline

---

### 8. Builders (`/builders`)

**Purpose**: Manage remote builders (build machines)

**Layout**:
```
┌─────────────────────────────────────────────┐
│ Builders                          [+ Add]   │
├─────────────────────────────────────────────┤
│                                             │
│ ┌───────────────────────────────────────┐  │
│ │ Builder Card: builder-01              │  │
│ │ [Edit] [Remove]                       │  │
│ │                                       │  │
│ │ ● Online                              │  │
│ │ SSH: builder@builder-01.local:22      │  │
│ │ Systems: x86_64-linux                 │  │
│ │ Features: kvm, nixos-test             │  │
│ │ Speed: 100                            │  │
│ │ Max Jobs: 8                           │  │
│ │                                       │  │
│ │ Metrics:                              │  │
│ │ Total Builds: 456                     │  │
│ │ Success Rate: 98.5%                   │  │
│ └───────────────────────────────────────┘  │
└─────────────────────────────────────────────┘
```

**Components**:
- Grid of builder cards
- Each card: name, status, SSH connection, supported systems, features, metrics
- Actions: Add, Edit, Remove

**Current UX Issues**:
- No way to test builder connectivity
- Metrics don't show trends
- Features are just tags with no explanation

---

### 9. Caches (`/caches`)

**Purpose**: Manage binary caches

**Layout**:
```
┌─────────────────────────────────────────────┐
│ Caches                            [+ Add]   │
├─────────────────────────────────────────────┤
│ [List of cache configurations]             │
│                                             │
│ cache.nixos.org (public)                    │
│ cache.internal.example.com (private)        │
│ ...                                         │
└─────────────────────────────────────────────┘
```

**Note**: This page is less developed in current implementation.

---

### 10. CVEs (`/cves`)

**Purpose**: Security vulnerability tracking (admin only)

**Layout**:
```
┌─────────────────────────────────────────────┐
│ CVE Vulnerabilities                         │
├─────────────────────────────────────────────┤
│ [Filters: Severity, Package, Status]        │
├─────────────────────────────────────────────┤
│ [Table of CVEs]                             │
│                                             │
│ CVE ID      | Severity | Package | Systems │
│ CVE-2024-.. | Critical | openssl | 8/12    │
│ CVE-2024-.. | High     | glibc   | 12/12   │
│ ...                                         │
└─────────────────────────────────────────────┘
```

**Current UX Issues**:
- No remediation guidance
- Doesn't link to affected systems
- No timeline showing when CVE was introduced/fixed

---

### 11. Deployment Policies (`/deployment-policies`)

**Purpose**: Define automated deployment rules

**Layout**:
```
┌─────────────────────────────────────────────┐
│ Deployment Policies                [+ Add]  │
├─────────────────────────────────────────────┤
│                                             │
│ ┌───────────────────────────────────────┐  │
│ │ Policy Card                           │  │
│ │ [Edit] [Remove]                       │  │
│ │                                       │  │
│ │ Auto-deploy to production             │  │
│ │ Trigger: Tag matching stable-*        │  │
│ │ Target: Environment "production"      │  │
│ │ Require: All tests pass               │  │
│ └───────────────────────────────────────┘  │
└─────────────────────────────────────────────┘
```

**Components**:
- List of policy cards
- Each policy: name, trigger conditions, target environment, requirements
- Actions: Add, Edit, Remove

**Current UX Issues**:
- Policy syntax is complex but shown as plain text
- No visual indication of policy flow
- Can't see policy execution history

---

### 12. Admin (`/admin`)

**Purpose**: Server management and configuration (admin only)

**Layout**:
```
┌─────────────────────────────────────────────┐
│ Server Administration                       │
├─────────────────────────────────────────────┤
│ [System information, config health, etc.]   │
└─────────────────────────────────────────────┘
```

**Note**: Implementation details vary.

---

### 13. Login (`/login`)

**Purpose**: User authentication

**Layout**:
```
┌─────────────────────────────────────────────┐
│                                             │
│          ┌───────────────────┐             │
│          │  Crystal Forge    │             │
│          │                   │             │
│          │  [Username]       │             │
│          │  [Password]       │             │
│          │                   │             │
│          │  [Login Button]   │             │
│          │                   │             │
│          │  [Register link]  │             │
│          └───────────────────┘             │
│                                             │
└─────────────────────────────────────────────┘
```

**Current UX Issues**:
- Plain, generic login form
- No branding or personality
- No "forgot password" flow

---

### 14. Register (`/register`)

**Purpose**: User registration

**Layout**: Similar to login with additional fields (email, confirm password)

**Current UX Issues**:
- No indication of password requirements
- No email verification flow shown

---

### 15. Setup (`/setup`)

**Purpose**: Initial setup wizard for first-time installation

**Layout**: Multi-step wizard (implementation details vary)

---

## Component Library

### Cards

**Standard Card**:
- Background: `--cf-card-bg`
- Border: `1px solid --cf-card-border`
- Border radius: `8px`
- Padding: `24px`
- Shadow: subtle

**Variants**:
- Stat Card (centered stat display)
- System Card (multi-row info grid)
- Builder Card (builder info + metrics)
- Environment Card (env info + policies)
- Policy Card (policy rules)

### Badges

**Status Badge** (pill-shaped):
- Border radius: `9999px`
- Padding: `4px 12px`
- Font: `12px`, medium weight
- Color-coded by status (health, deployment, CVE severity)

**Variants**:
- Health badge (healthy, warning, critical, offline)
- Deployment badge (up to date, behind, ahead, never deployed)
- CVE severity badge (critical, high, medium, low)

### Buttons

**Primary Button**:
- Background: `--cf-primary-btn` (violet)
- Text: white
- Padding: `8px 16px`
- Border radius: `8px`
- Hover: darker shade

**Variants**:
- Danger button (red)
- Success button (green)
- Ghost button (transparent with hover bg)
- Icon button (square, icon only)

### Modals

**Structure**:
- Backdrop: `rgba(0,0,0,0.75)` with backdrop-blur
- Modal: card styling, centered
- Max width: `600px` (forms) to `1200px` (complex modals)
- Padding: `24px`
- Header: title + close button
- Footer: action buttons (right-aligned)

**Types**:
- Confirmation dialog (small, centered message)
- Form modal (add/edit entities)
- Detail modal (view logs, diffs)

### Tables

**Sortable Table**:
- Header: `--cf-table-header` styling, uppercase
- Rows: hover effect
- Cell padding: `12px 24px`
- Borders: subtle dividers

**Features**:
- Sortable columns (click header)
- Row selection (checkboxes)
- Inline actions (edit, delete icons)

### Forms

**Input Fields**:
- Background: `--cf-input-bg`
- Border: `--cf-input-border`
- Focus: `--cf-input-border-focus` + focus ring
- Padding: `8px 12px`
- Border radius: `8px`

**Types**:
- Text input
- Textarea
- Select dropdown
- Checkbox
- Radio buttons

**Validation**:
- Error state: red border + error message below
- Success state: green border + checkmark icon

### Notifications

**Toast**:
- Bottom-right corner
- Auto-dismiss after 5s
- Variants: success, error, warning, info
- Icon + message + close button

**Alert Banner**:
- Top of page content
- Full-width, colored background
- Dismissible
- Variants: info, warning, error

### Loading States

**Spinner**:
- Animated rotating circle
- Sizes: small, medium, large
- Colors: primary (violet) or muted (gray)

**Skeleton Loaders**:
- Placeholder blocks with shimmer animation
- Match shape of content (cards, tables, text)

### Charts

**Donut Chart**:
- SVG-based
- Interactive legend
- Color-coded segments
- Center: total count

**Usage**:
- Fleet health breakdown
- Deployment status breakdown
- CVE severity distribution

---

## Responsive Breakpoints

```
Mobile: < 640px (sm)
Tablet: 640px - 1024px (sm to lg)
Desktop: > 1024px (lg+)
Wide: > 1280px (xl+)
```

**Responsive Behavior**:
- Sidebar → Drawer on mobile
- Card grids: 1 column → 2 columns → 3-4 columns
- Tables: horizontal scroll on mobile OR collapse to cards
- Modals: full-screen on mobile, centered on desktop

---

## Accessibility Considerations

**Current Implementation**:
- Semantic HTML (nav, main, header, article)
- Focus states on interactive elements
- Keyboard navigation (Tab, Enter, Esc)
- ARIA labels on icon buttons

**Gaps to Address in Redesign**:
- Screen reader announcements for dynamic updates
- High contrast mode support
- Reduced motion preferences
- Better focus indicators (more visible)
- Keyboard shortcuts documentation

---

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
