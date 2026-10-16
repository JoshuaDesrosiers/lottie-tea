/**
 * Shared state for the **Test Area** (`components/TestAreaPanel.tsx`): the
 * scene's latest Toasty Lite export and the few settings that reach outside
 * the panel — the editor's own title bar hosts the "Nav bar" test slot, and
 * `App` keeps the panel open in Playground when asked.
 */
import type { LiteFile } from '../lite/runtime'

export interface TestAreaState {
  /** The current scene as Toasty Lite (null until exported). */
  file: LiteFile | null
  /** Show the animation in the editor's title bar, at this size (px). */
  nav: { active: boolean; size: number }
  /** Playground becomes the Test Area: the mock fills the stage. */
  showInPlayground: boolean
  /** The panel's current settings (the Playground stage renders with them). */
  settings: unknown
}

let state: TestAreaState = { file: null, nav: { active: false, size: 24 }, showInPlayground: false, settings: null }
const listeners = new Set<() => void>()

export const testAreaStore = {
  get: () => state,
  subscribe(fn: () => void) {
    listeners.add(fn)
    return () => {
      listeners.delete(fn)
    }
  },
  set(patch: Partial<TestAreaState>) {
    state = { ...state, ...patch }
    listeners.forEach((fn) => fn())
  },
}
