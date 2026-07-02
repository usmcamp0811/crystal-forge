// Spinner.jsx — unified loading indicators for Crystal Forge
// Exports: Spinner (indeterminate ring), Pulse (live dot), ProgressBar (linear)

/* ──────────────────────────────────────────────────────────────────
   Spinner — rotating arc ring, for in-progress / loading states
   Props:
     size   — number (px) or "sm" (14) | "md" (20) | "lg" (28)
     color  — CSS color string; defaults to brand purple
     style  — extra inline styles on the <svg>
   ────────────────────────────────────────────────────────────────── */
function Spinner({ size = 20, color, style, className = "", ...rest }) {
  const SIZES = { sm: 14, md: 20, lg: 28 };
  const sz  = typeof size === "number" ? size : (SIZES[size] ?? 20);
  const sw  = sz <= 14 ? 2 : sz <= 20 ? 2.25 : 2.75;         // stroke width
  const r   = (sz - sw) / 2;
  const c   = 2 * Math.PI * r;
  const arc = c * 0.27;                                        // visible arc ~27% of ring
  const col = color ?? "var(--cf-brand-purple)";

  return (
    <svg
      width={sz} height={sz}
      viewBox={`0 0 ${sz} ${sz}`}
      fill="none"
      className={`cf-spinner${className ? " " + className : ""}`}
      aria-label="Loading"
      role="status"
      style={{ flexShrink: 0, ...style }}
      {...rest}
    >
      {/* dim track */}
      <circle
        cx={sz / 2} cy={sz / 2} r={r}
        stroke={col}
        strokeOpacity={0.14}
        strokeWidth={sw}
      />
      {/* bright arc */}
      <circle
        cx={sz / 2} cy={sz / 2} r={r}
        stroke={col}
        strokeWidth={sw}
        strokeLinecap="round"
        strokeDasharray={`${arc} ${c - arc}`}
      />
    </svg>
  );
}


/* ──────────────────────────────────────────────────────────────────
   Pulse — small animated dot for "live stream" indicators
   Props:
     size   — diameter in px (default 7)
     color  — CSS color (default blue, matches ed-pulse)
     style  — extra inline styles
   ────────────────────────────────────────────────────────────────── */
function Pulse({ size = 7, color = "#60a5fa", style, className = "" }) {
  return (
    <span
      className={`cf-pulse${className ? " " + className : ""}`}
      aria-hidden="true"
      style={{ width: size, height: size, background: color, ...style }}
    />
  );
}


/* ──────────────────────────────────────────────────────────────────
   ProgressBar — horizontal progress bar, determinate or indeterminate
   Props:
     value        — 0..1 for determinate; omit or null for indeterminate sweep
     color        — fill color (default brand purple)
     height       — bar height in px (default 4)
     segments     — [{color, value}] for multi-segment bars (overrides value/color)
     style        — extra styles on the track element
   ────────────────────────────────────────────────────────────────── */
function ProgressBar({ value, color = "var(--cf-brand-purple)", height = 4, segments, style, className = "" }) {
  const indeterminate = value === undefined || value === null;

  return (
    <div
      className={`cf-bar${className ? " " + className : ""}`}
      role="progressbar"
      aria-valuenow={indeterminate ? undefined : Math.round(Math.min(1, Math.max(0, value)) * 100)}
      aria-valuemin={0}
      aria-valuemax={100}
      style={{ height, ...style }}
    >
      {segments ? (
        segments.map((s, i) => (
          <div
            key={i}
            className="cf-bar-fill"
            style={{
              width: `${Math.min(1, Math.max(0, s.value)) * 100}%`,
              background: s.color,
              transition: "none",
            }}
          />
        ))
      ) : indeterminate ? (
        <div className="cf-bar-sweep" style={{ background: color }} />
      ) : (
        <div
          className="cf-bar-fill"
          style={{
            width: `${Math.min(1, Math.max(0, value)) * 100}%`,
            background: color,
          }}
        />
      )}
    </div>
  );
}


Object.assign(window, { Spinner, Pulse, ProgressBar });
