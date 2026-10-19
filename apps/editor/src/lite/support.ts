/**
 * **What Toasty Lite exports** — one registry the editor checks so authors
 * can tell, while building, what will survive Export → Toasty Lite (the
 * Canvas2D runtime, `./runtime.ts`), plus the **Lite mode** switch:
 *
 * - **Lite mode on:** menus only offer what Lite draws (Add Component,
 *   Image Effects, scene Effects, the Add menu), so a scene built this way
 *   exports with nothing left out.
 * - **Lite mode off (normal):** everything is offered; things Lite leaves
 *   out carry an asterisk ("*") with a "not in Toasty Lite" tooltip.
 *
 * Keep in step with `crates/an1mbyt3-format/src/lite.rs` — it decides what
 * is really exported; this list only labels the UI.
 */
import { useSyncExternalStore } from 'react'

/** Components (`NodeBehavior` names) Lite exports. Reference2D is a design
 * aid the export simply drops, so it's fine to use either way. */
export const LITE_COMPONENTS: ReadonlySet<string> = new Set(['StrokeStyle2D', 'UiButton', 'UiEvents', 'UiListeners', 'PropertyBindings', 'Camera', 'DropShadow2D', 'ImageEffects', 'Reference2D'])

/** Image Effects Lite draws (some with limits — see `LITE_EFFECT_NOTE`). */
export const LITE_IMAGE_EFFECTS: ReadonlySet<string> = new Set(['DropShadow', 'Stroke', 'Blur', 'Glow', 'Grain', 'GradientMap', 'Texture'])

/** Per-effect limits worth a note in Lite. */
export const LITE_EFFECT_NOTE: Readonly<Record<string, string>> = {
  Stroke: 'Lite draws Outside strokes only',
  Blur: 'Lite draws the Plain blur only',
}

/** Scene (post) effects Lite draws: the global texture. */
export const LITE_POST_EFFECTS: ReadonlySet<string> = new Set(['Texture'])

/** Node kinds Lite draws. */
export const LITE_NODE_KINDS: ReadonlySet<string> = new Set(['Shape', 'Group', 'Text', 'Image', 'Camera'])

/** Tooltip suffix for things Lite leaves out. */
export const NOT_IN_LITE = 'Not in the Toasty Lite export (full engine only)'

// ---- the Lite mode switch (persisted per browser) ----------------------

const KEY = 'an1.liteMode'
let liteMode = (() => {
  try {
    return localStorage.getItem(KEY) === '1'
  } catch {
    return false
  }
})()
const subs = new Set<() => void>()

export function setLiteMode(on: boolean) {
  liteMode = on
  try {
    localStorage.setItem(KEY, on ? '1' : '0')
  } catch {
    // Not remembered; still applies this session.
  }
  for (const f of subs) f()
}

export function getLiteMode(): boolean {
  return liteMode
}

/** Whether Lite mode is on (re-renders on change). */
export function useLiteMode(): boolean {
  return useSyncExternalStore(
    (f) => {
      subs.add(f)
      return () => subs.delete(f)
    },
    () => liteMode,
  )
}

/**
 * How a menu should treat one item given Lite support: `hide` it (Lite
 * mode, unsupported), `mark` it with an asterisk (normal mode,
 * unsupported) or show it as-is.
 */
export function liteTreatment(supported: boolean, lite: boolean): 'show' | 'mark' | 'hide' {
  if (supported) return 'show'
  return lite ? 'hide' : 'mark'
}
