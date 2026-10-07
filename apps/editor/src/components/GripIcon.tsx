/** Six-dot grip glyph — an inline SVG rather than an icon-font glyph so it
 * matches exactly across platforms; a placeholder for the "one consistent,
 * custom-drawn icon set" AN1MBYT3_BUILD_PLAN.md §15 calls for long-term,
 * not the final icon design itself. Shared by every draggable panel's
 * handle (`DockablePanel`, `TopToolbar`) so they all grab the same way. */
export function GripIcon() {
  return (
    <svg width="10" height="16" viewBox="0 0 10 16" fill="currentColor" aria-hidden="true">
      {[0, 1].map((col) =>
        [0, 1, 2].map((row) => (
          <circle key={`${col}-${row}`} cx={col * 6 + 2} cy={row * 6 + 2} r="1.4" />
        )),
      )}
    </svg>
  )
}
