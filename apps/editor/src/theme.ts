/** Shared dark/light theme state — lifted out of `TitleBar.tsx` so both it
 * (the title bar's own quick-toggle) and `IconRail`'s Settings button (the
 * more conventional place people actually look for this) can drive the same
 * state instead of each owning a separate, divergent copy. `index.css`'s own
 * `:root`/`:root[data-theme='light']` blocks are what actually respond to
 * the `data-theme` attribute this sets on `<html>`; this module is purely
 * the "what the current theme is, and remembering it across launches" half.
 */
export type Theme = 'dark' | 'light'

const THEME_STORAGE_KEY = 'an1mbyt3-theme'

/** Falls back to `'dark'` (this app's original, still-default look) for a
 * first launch with nothing saved yet. */
export function loadInitialTheme(): Theme {
  if (typeof window === 'undefined') return 'dark'
  return window.localStorage.getItem(THEME_STORAGE_KEY) === 'light' ? 'light' : 'dark'
}

export function applyTheme(theme: Theme) {
  document.documentElement.setAttribute('data-theme', theme)
  window.localStorage.setItem(THEME_STORAGE_KEY, theme)
}

/** The panel *style* axis, independent of dark/light: `'glass'` (this app's
 * original look — translucent, blurred, rounded panes floating over a
 * full-window canvas) or `'solid'` (hard, opaque, square panes flush to each
 * other that *enclose* the canvas instead, IDE-style). `index.css`'s
 * `:root[data-style='solid']` block is the surface half; `layoutMetrics.ts`
 * (gaps) and `App.tsx`'s viewport insets are the layout half. */
export type UiStyle = 'glass' | 'solid'

const STYLE_STORAGE_KEY = 'an1mbyt3-ui-style'

export function loadInitialStyle(): UiStyle {
  if (typeof window === 'undefined') return 'glass'
  return window.localStorage.getItem(STYLE_STORAGE_KEY) === 'solid' ? 'solid' : 'glass'
}

export function applyStyle(style: UiStyle) {
  document.documentElement.setAttribute('data-style', style)
  window.localStorage.setItem(STYLE_STORAGE_KEY, style)
}

/** The WGPU canvas's own clear color (the work-surface area outside the
 * artboard bounds) for each theme — `an1mbyt3-renderer`/`an1mbyt3-wasm` have
 * no concept of "app theme" themselves, only whatever color `render()` is
 * told to clear to each frame (`App.tsx`'s render loop passes this every
 * frame), so this is the one place that color is actually chosen. Dark
 * matches this engine's own original, long-standing hardcoded value
 * (`wgpu::Color { r: 0.11, g: 0.11, b: 0.13 }`); light is a clean neutral
 * mid-gray "work surface" — deliberately not pure white, so a white artboard
 * still reads as a distinct sheet sitting on top of it, the same Figma/
 * Adobe Fresco convention this app's dark canvas already followed (a
 * *darker* canvas than the panel chrome around it) just mirrored for light. */
export const CLEAR_COLOR: Record<Theme, readonly [number, number, number]> = {
  dark: [0.11, 0.11, 0.13],
  light: [0.85, 0.85, 0.87],
}

/** The artboard's background grid color for each theme — `an1mbyt3-wasm`'s
 * own `ARTBOARD_GRID_COLOR` constant used to hold this (plus a fixed 0.85
 * alpha it still owns), a fixed dark gray tuned to sit just *lighter* than
 * the dark-mode clear color so the grid actually reads against it (see that
 * constant's own doc comment history). A flat, un-inverted reuse of that
 * same dark gray in light mode read as much too strong/heavy against the
 * lighter clear color behind it (the literal bug report this fixes) — light
 * mode instead goes slightly *darker* than its own clear color, by roughly
 * the same contrast delta dark mode already uses in the other direction, so
 * the grid reads as a gentle guide line in both themes rather than a harsh
 * one in either. */
export const GRID_COLOR: Record<Theme, readonly [number, number, number]> = {
  dark: [0.2, 0.2, 0.23],
  light: [0.76, 0.76, 0.78],
}
