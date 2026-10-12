/**
 * The editor's icon set: plain inline SVGs, not an icon-font/library import.
 * AN1MBYT3_BUILD_PLAN.md §15 calls out "one consistent, custom-drawn icon
 * set" as a polish requirement — mixing icon-font styles (rounded vs sharp,
 * filled vs outline) is one of the fastest ways an editor UI reads as
 * unpolished. Every icon here shares the same stroke width (1.6), size
 * (16px viewbox at a shared scale), and `currentColor` fill/stroke so they
 * always match whatever text color surrounds them. This is a starting set,
 * not the final icon design — grown as panels need new icons, following the
 * same stroke-weight/size convention rather than pulling in a library once
 * that set grows large.
 */

const STROKE = 1.6

export function ChevronDownIcon() {
  return (
    <svg width="10" height="10" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

/** The toolbar's "Load Demo Scene" action — a small four-point sparkle,
 * the common "something generated/magic" glyph. */
export function SparkleIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path
        d="M8 1.5c.3 2.4 1 3.7 2.2 4.8 1.2 1.1 2.6 1.7 5.3 1.7-2.7 0-4.1.6-5.3 1.7C9 10.8 8.3 12.1 8 14.5c-.3-2.4-1-3.7-2.2-4.8C4.6 8.6 3.2 8 .5 8c2.7 0 4.1-.6 5.3-1.7C7 5.2 7.7 3.9 8 1.5z"
        fill="currentColor"
      />
    </svg>
  )
}

export function PlusIcon() {
  return (
    <svg width="10" height="10" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path d="M8 3v10M3 8h10" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
    </svg>
  )
}

/** Toolbar "Add Bone" action — two joints and a connecting shaft, the same
 * visual shorthand every 2D rigging tool (Spine, DragonBones, Rive) uses for
 * a bone. */
export function BoneIcon() {
  return (
    <svg width="10" height="10" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <circle cx="3.5" cy="3.5" r="2" stroke="currentColor" strokeWidth={STROKE} />
      <circle cx="12.5" cy="12.5" r="1.3" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M5 5l6 6" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
    </svg>
  )
}

/** Rail toggle for the Components panel — a plain wrench, clearer at rail
 * size than reusing one of the node-kind badge icons (which read as too
 * small/ambiguous once shrunk to a rail button). */
export function WrenchIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path
        d="M10.5 2.5a3 3 0 0 0-3.9 3.9L2.8 10.2a1.5 1.5 0 0 0 2.1 2.1l3.8-3.8a3 3 0 0 0 3.9-3.9l-2 2-1.6-1.6z"
        stroke="currentColor"
        strokeWidth={STROKE}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

/** Stacked squares — the rail's "Effects" (post-processing panel) toggle,
 * evoking a layered effect stack the same way `WrenchIcon` evokes tools. */
export function EffectsIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="3" y="3" width="7" height="7" rx="1" stroke="currentColor" strokeWidth={STROKE} />
      <rect x="6" y="6" width="7" height="7" rx="1" stroke="currentColor" strokeWidth={STROKE} />
    </svg>
  )
}

/** Represents the artboard/canvas dock — the rail's "home" entry. */
export function CanvasIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="2" y="3" width="12" height="8" rx="1.5" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M5.5 13.5h5" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
    </svg>
  )
}

export function MenuIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path
        d="M3 4.5h10M3 8h10M3 11.5h10"
        stroke="currentColor"
        strokeWidth={STROKE}
        strokeLinecap="round"
      />
    </svg>
  )
}

export function UndoIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path
        d="M4 4.5v3.5h3.5M4.4 8A5 5 0 1 1 5 12"
        stroke="currentColor"
        strokeWidth={STROKE}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

export function RedoIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path
        d="M12 4.5v3.5H8.5M11.6 8A5 5 0 1 0 11 12"
        stroke="currentColor"
        strokeWidth={STROKE}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

export function MinimizeIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path d="M3.5 8h9" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
    </svg>
  )
}

export function MaximizeIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="3.5" y="3.5" width="9" height="9" rx="1" stroke="currentColor" strokeWidth={STROKE} />
    </svg>
  )
}

export function CloseIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
    </svg>
  )
}

export function SaveIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path
        d="M3 3h8l2 2v8H3V3z"
        stroke="currentColor"
        strokeWidth={STROKE}
        strokeLinejoin="round"
      />
      <path d="M5.5 3v3.5h5V3" stroke="currentColor" strokeWidth={STROKE} strokeLinejoin="round" />
      <path d="M5.5 9.5h5v3.5h-5V9.5z" stroke="currentColor" strokeWidth={STROKE} strokeLinejoin="round" />
    </svg>
  )
}

export function SunIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <circle cx="8" cy="8" r="3" stroke="currentColor" strokeWidth={STROKE} />
      <path
        d="M8 1.5v2M8 12.5v2M14.5 8h-2M3.5 8h-2M12.5 3.5l-1.4 1.4M4.9 11.1l-1.4 1.4M12.5 12.5l-1.4-1.4M4.9 4.9L3.5 3.5"
        stroke="currentColor"
        strokeWidth={STROKE}
        strokeLinecap="round"
      />
    </svg>
  )
}

export function MoonIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path d="M13.5 9.5a5.5 5.5 0 0 1-7-7 5.5 5.5 0 1 0 7 7z" stroke="currentColor" strokeWidth={STROKE} strokeLinejoin="round" />
    </svg>
  )
}

export function GifExportIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="2" y="3" width="12" height="10" rx="1" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M2 6.5h12" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M5.5 3v3.5M10.5 3v3.5" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M5 11.5v-2.2M5 9.3c0-.6.5-1 1-1s1 .4 1 1-.5 1-1 1M9.2 11.5v-3.2M11.5 8.3H9.9M11.5 9.8h-1.3" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

/** A game controller in a window frame — "Export Game" (standalone web build). */
export function GameExportIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="1.5" y="4" width="13" height="8" rx="3" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M4.5 8h2.5M5.75 6.75v2.5" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
      <circle cx="10.5" cy="7.25" r="0.6" fill="currentColor" />
      <circle cx="11.9" cy="8.75" r="0.6" fill="currentColor" />
    </svg>
  )
}

/** Export Game → Desktop app: a monitor with a gamepad dot. */
export function DesktopExportIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="1.5" y="2.5" width="13" height="8.5" rx="1.5" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M6 14h4M8 11v3" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
      <path d="M5.5 6.75h2M6.5 5.75v2" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
      <circle cx="10.25" cy="6.75" r="0.6" fill="currentColor" />
    </svg>
  )
}

export function OpenIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path
        d="M2 5.5V4a1 1 0 0 1 1-1h3l1.3 1.5H13a1 1 0 0 1 1 1V5.5M2 5.5h12l-1.2 6.6a1 1 0 0 1-1 .9H4.2a1 1 0 0 1-1-.9L2 5.5z"
        stroke="currentColor"
        strokeWidth={STROKE}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

export function ImageIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="2" y="2.5" width="12" height="11" rx="1.2" stroke="currentColor" strokeWidth={STROKE} />
      <circle cx="5.5" cy="6" r="1.1" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M2.8 11.5 6 8.3l2.2 2.2 2.5-2.8 2.5 3.1" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

export function TrashIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path
        d="M3.5 4.5h9M6.5 4.5V3a1 1 0 0 1 1-1h1a1 1 0 0 1 1 1v1.5M4.5 4.5l.6 8.4a1 1 0 0 0 1 .9h3.8a1 1 0 0 0 1-.9l.6-8.4"
        stroke="currentColor"
        strokeWidth={STROKE}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

/** Rename/edit-in-place trigger — a plain pencil, the standard shorthand for
 * "make this text editable" every design tool uses. */
export function EditIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path
        d="M11 2.5l2.5 2.5L5 13.5l-3 .5.5-3z"
        stroke="currentColor"
        strokeWidth={STROKE}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

export function PlayIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M4 3l9 5-9 5V3z" />
    </svg>
  )
}

export function PauseIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <rect x="4" y="3" width="3" height="10" />
      <rect x="9" y="3" width="3" height="10" />
    </svg>
  )
}

export function JumpToStartIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path d="M4 3v10" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
      <path d="M12 3L5 8l7 5V3z" fill="currentColor" />
    </svg>
  )
}

export function JumpToEndIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path d="M12 3v10" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
      <path d="M4 3l7 5-7 5V3z" fill="currentColor" />
    </svg>
  )
}

/** Play once, stop at the end. */
export function LoopOnceIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path d="M2 8h10M9 5l3 3-3 3" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

/** Loop from the start again. */
export function LoopIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path
        d="M3.5 8a4.5 4.5 0 0 1 7.6-3.2M12.5 8a4.5 4.5 0 0 1-7.6 3.2"
        stroke="currentColor"
        strokeWidth={STROKE}
        strokeLinecap="round"
      />
      <path d="M10.6 3.2v2.4H8.2M5.4 12.8v-2.4h2.4" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

/** Ping-pong (bounce back and forth). */
export function PingPongIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path
        d="M5 5L2 8l3 3M11 5l3 3-3 3M2 8h12"
        stroke="currentColor"
        strokeWidth={STROKE}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

export function ZoomInIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M13 13l-2.5-2.5M7 4.8v4.4M4.8 7h4.4" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
    </svg>
  )
}

export function ZoomOutIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M13 13l-2.5-2.5M4.8 7h4.4" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
    </svg>
  )
}

/** `NodeKind::Shape` — a plain filled square. */
/** Tile Palette panel's rail toggle — a plain 2x2 tile grid. */
export function TileGridIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="2" y="2" width="5" height="5" rx="0.75" stroke="currentColor" strokeWidth={STROKE} />
      <rect x="9" y="2" width="5" height="5" rx="0.75" stroke="currentColor" strokeWidth={STROKE} />
      <rect x="2" y="9" width="5" height="5" rx="0.75" stroke="currentColor" strokeWidth={STROKE} />
      <rect x="9" y="9" width="5" height="5" rx="0.75" stroke="currentColor" strokeWidth={STROKE} />
    </svg>
  )
}

/** The IconRail's "Timeline" toggle — a clock face, matching the Timeline
 * panel's own "scrub along a clock" mental model. */
export function ClockIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <circle cx="8" cy="8" r="6" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M8 4.5V8l2.5 2" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
    </svg>
  )
}

/** The IconRail's "State Machine" toggle — 3 connected nodes, matching the
 * state graph's own node-and-transition shape. */
export function GraphIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <circle cx="3.5" cy="4" r="1.75" stroke="currentColor" strokeWidth={STROKE} />
      <circle cx="12.5" cy="4" r="1.75" stroke="currentColor" strokeWidth={STROKE} />
      <circle cx="8" cy="12" r="1.75" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M5 5.2 6.7 10.4M11 5.2 9.3 10.4" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
    </svg>
  )
}

/** The rail's Glass/Solid panel-style toggle — a window split into a
 * sidebar and a main area, i.e. "panes that enclose the canvas." */
export function PaneLayoutIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="2" y="2.5" width="12" height="11" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M6 2.5v11M6 10h8" stroke="currentColor" strokeWidth={STROKE} />
    </svg>
  )
}

export function CheckIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path d="M3 8.5 6.5 12 13 4.5" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

/** The Shapes menu's "Add Camera" entry — a simple camera-body-plus-lens
 * glyph, matching `TileGridIcon`'s own plain-geometric-shape style. */
export function CameraKindIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="2" y="4.5" width="12" height="8" rx="1.25" stroke="currentColor" strokeWidth={STROKE} />
      <circle cx="8" cy="8.5" r="2.25" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M5.5 4.5 6.5 3h3l1 1.5" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

/** A bulb with rays — Light 2D nodes. */
export function LightKindIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <circle cx="8" cy="8" r="3" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M8 1.5v1.75M8 12.75v1.75M1.5 8h1.75M12.75 8h1.75M3.4 3.4l1.25 1.25M11.35 11.35l1.25 1.25M3.4 12.6l1.25-1.25M11.35 4.65l1.25-1.25" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
    </svg>
  )
}

/** A comet streak — Trail Renderer nodes. */
export function TrailKindIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <circle cx="12" cy="4" r="2" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M10.4 5.6 3 13M8.6 4.4 2.5 9M11.6 7.4 7 13.5" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
    </svg>
  )
}

export function ShapeKindIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="3" y="3" width="10" height="10" rx="1.5" stroke="currentColor" strokeWidth={STROKE} />
    </svg>
  )
}

/** Shapes panel preset — a plain ellipse. */
export function EllipsePresetIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <ellipse cx="8" cy="8" rx="5.5" ry="5.5" stroke="currentColor" strokeWidth={STROKE} />
    </svg>
  )
}

/** Shapes panel preset — a plain point-up triangle. */
export function TrianglePresetIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path d="M8 2.5l5.5 11h-11z" stroke="currentColor" strokeWidth={STROKE} strokeLinejoin="round" />
    </svg>
  )
}

/** Shapes panel preset — a five-pointed star. */
export function StarPresetIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path
        d="M8 1.5l1.8 3.9 4.2.5-3.1 3 .8 4.3L8 11.2l-3.7 2 .8-4.3-3.1-3 4.2-.5z"
        stroke="currentColor"
        strokeWidth={STROKE}
        strokeLinejoin="round"
      />
    </svg>
  )
}

/** `NodeKind::Group` — nested/stacked squares. */
export function GroupKindIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="2.5" y="2.5" width="8" height="8" rx="1" stroke="currentColor" strokeWidth={STROKE} />
      <rect x="5.5" y="5.5" width="8" height="8" rx="1" stroke="currentColor" strokeWidth={STROKE} />
    </svg>
  )
}

/** `NodeKind::Bone` — a simple two-joint bone glyph. */
export function BoneKindIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <circle cx="4" cy="4" r="2" stroke="currentColor" strokeWidth={STROKE} />
      <circle cx="12" cy="12" r="2" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M5.4 5.4l5.2 5.2" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
    </svg>
  )
}

/** `NodeKind::Instance` — a component instance (dashed copy of a shape). */
export function InstanceKindIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="2.5" y="2.5" width="8" height="8" rx="1" stroke="currentColor" strokeWidth={STROKE} strokeDasharray="2 1.5" />
      <rect x="5.5" y="5.5" width="8" height="8" rx="1" fill="currentColor" fillOpacity="0.25" stroke="currentColor" strokeWidth={STROKE} />
    </svg>
  )
}

/** A group's `GroupNode::mask` relationship — an ellipse cutting a corner off
 * a rect, the standard "this clips that" glyph. Used both as a badge on a
 * masked `Group`'s own row and on the specific child row acting as its mask
 * (`LayersPanel`'s drag-to-mask indicator). */
export function MaskIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="2.5" y="2.5" width="9" height="9" rx="1" stroke="currentColor" strokeWidth={STROKE} />
      <circle cx="11" cy="11" r="3.5" fill="currentColor" fillOpacity="0.35" stroke="currentColor" strokeWidth={STROKE} />
    </svg>
  )
}

/** `NodeKind::Text` — a plain "T" glyph. */
export function TextKindIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path d="M3.5 4h9M8 4v8" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
    </svg>
  )
}

export function SettingsIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <circle cx="8" cy="8" r="2.4" stroke="currentColor" strokeWidth={STROKE} />
      <path
        d="M8 1.6v1.6M8 12.8v1.6M14.4 8h-1.6M3.2 8H1.6M12.3 3.7l-1.1 1.1M4.8 11.1l-1.1 1.1M12.3 12.3l-1.1-1.1M4.8 4.9L3.7 3.7"
        stroke="currentColor"
        strokeWidth={STROKE}
        strokeLinecap="round"
      />
    </svg>
  )
}

/** Colour picker: pick a colour from the screen. */
export function EyedropperIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path d="M10.2 2.8a1.9 1.9 0 0 1 2.7 2.7l-1.4 1.4-2.7-2.7 1.4-1.4z" stroke="currentColor" strokeWidth={STROKE} strokeLinejoin="round" />
      <path d="M8.8 4.2l3 3M9.9 6.1l-6 6-2 .6.6-2 6-6" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

/** Copy (two stacked sheets). */
export function CopyIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="5.5" y="5.5" width="8" height="8" rx="1.5" stroke="currentColor" strokeWidth={STROKE} />
      <path d="M10.5 3.5v-.5A1.5 1.5 0 0 0 9 1.5H4A1.5 1.5 0 0 0 2.5 3v5A1.5 1.5 0 0 0 4 9.5h.5" stroke="currentColor" strokeWidth={STROKE} strokeLinecap="round" />
    </svg>
  )
}

/** Paste (a clipboard). */
export function PasteIcon() {
  return (
    <svg width="13" height="13" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="3" y="3" width="10" height="11.5" rx="1.5" stroke="currentColor" strokeWidth={STROKE} />
      <rect x="5.5" y="1.5" width="5" height="3" rx="1" stroke="currentColor" strokeWidth={STROKE} />
    </svg>
  )
}
