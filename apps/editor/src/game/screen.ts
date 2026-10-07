/**
 * Responsive screens on the TS side — the twin of
 * `an1mbyt3_core::layout::responsive_view` (keep the two in step), used by
 * the exported player to size its canvas and by the editor to draw the
 * main camera's frame for the "preview on" device.
 */
import type { ScreenSettings } from '../ipc/document'

/** Device presets for Design mode's "preview on" picker (CSS px). */
export const SCREEN_PRESETS: { label: string; width: number; height: number }[] = [
  { label: 'Phone (portrait)', width: 390, height: 844 },
  { label: 'Phone (landscape)', width: 844, height: 390 },
  { label: 'Tablet (portrait)', width: 820, height: 1180 },
  { label: 'Tablet (landscape)', width: 1180, height: 820 },
  { label: 'Laptop', width: 1440, height: 900 },
  { label: 'Desktop', width: 1920, height: 1080 },
]

/**
 * The world-unit view a responsive scene shows on a `screen` (CSS px):
 * Unity's *Scale With Screen Size* — the log-space blend of the width and
 * height ratios against the design size (`reference`), so the matched
 * axis keeps the design's scale and the other shows more or less world.
 */
export function responsiveView(reference: { width: number; height: number }, screen: { width: number; height: number }, settings: ScreenSettings): { width: number; height: number } {
  const rw = Math.max(1, reference.width), rh = Math.max(1, reference.height)
  const sw = Math.max(1, screen.width), sh = Math.max(1, screen.height)
  const m = Math.max(0, Math.min(1, settings.matchWidthOrHeight))
  const logW = Math.log(sw / rw), logH = Math.log(sh / rh)
  const scale = Math.max(1e-4, Math.exp(logW + (logH - logW) * m))
  return { width: sw / scale, height: sh / scale }
}
