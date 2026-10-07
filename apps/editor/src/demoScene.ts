/**
 * A small, colorful built-in scene — a rocket with nested groups, a real
 * masked window, an animated flame, and a scattering of stars — imported
 * through the exact same `import_svg` pipeline a user's own dropped-in file
 * goes through (`TopToolbar`'s "Load Demo Scene" action, `App.tsx`'s
 * `handleLoadDemo`). Deliberately not a synthetic document built by hand:
 * running it through the real SVG importer is what actually proves
 * group-hierarchy preservation, clip-path-to-mask conversion, and the Layers
 * panel's resulting tree/drag/mask-badge UI all work end to end, rather than
 * a hand-built document that only exercises the editor UI and never touches
 * the importer at all.
 *
 * Shape: `Rocket` (group) contains `Body` (group: a curved-hull path plus a
 * window circle clipped by `windowClip` — imports as a synthetic masked
 * group, `GroupNode::mask` set for real), `Fins` (group: two swept curved
 * paths), a curved nose-cone path, and `Flame` (a teardrop path at the
 * base — `handleLoadDemo` finds it by name and keyframes its scale/fill
 * alongside the rocket's own hover, so more than one piece is visibly
 * animated). Every curved piece uses a `<path>` with cubic Béziers instead
 * of a plain `<polygon>` — considerably less "blocky programmer art" than
 * this scene's first pass. The nose cone's base (y≈118) sits well clear of
 * the window (cy=160, top edge y=134) so nothing overlaps it in z-order.
 * Four standalone stars sit outside any group — good, obviously-independent
 * targets for trying Ctrl/Cmd/Shift-click multi-select and a group drag-move.
 */
export const DEMO_SVG = `<svg viewBox="0 0 400 400" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <clipPath id="windowClip"><circle cx="200" cy="160" r="27"/></clipPath>
  </defs>
  <g id="Rocket">
    <path id="Nose" d="M200,42 C182,72 167,98 161,118 L239,118 C233,98 218,72 200,42 Z" fill="#ffb454"/>
    <g id="Body">
      <path d="M200,118 C240,118 252,160 252,210 C252,270 230,308 200,320 C170,308 148,270 148,210 C148,160 160,118 200,118 Z" fill="#e8ecf5"/>
      <circle cx="200" cy="160" r="27" fill="#6d8cff" clip-path="url(#windowClip)"/>
    </g>
    <g id="Fins">
      <path d="M158,258 C128,264 98,286 80,322 C106,316 132,304 162,288 C161,277 159,267 158,258 Z" fill="#ff6d6d"/>
      <path d="M242,258 C272,264 302,286 320,322 C294,316 268,304 238,288 C239,277 241,267 242,258 Z" fill="#ff6d6d"/>
    </g>
    <path id="Flame" d="M200,318 C183,340 176,366 200,392 C224,366 217,340 200,318 Z" fill="#ffcc66"/>
  </g>
  <circle cx="65" cy="55" r="6" fill="#ffe08a"/>
  <circle cx="335" cy="85" r="4" fill="#ffe08a"/>
  <circle cx="55" cy="310" r="5" fill="#ffe08a"/>
  <circle cx="345" cy="335" r="4" fill="#ffe08a"/>
</svg>`
