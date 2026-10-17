/**
 * Shared state for the **Test Area** (`components/TestAreaPanel.tsx`): the
 * scene's latest Toasty Lite export plus the panel's settings. Settings live
 * here (not in the panel) because three places read and write them: the
 * panel itself, the Playground stage (`Use as Playground` — clicking the
 * mock app's own nav there switches pages too) and the editor's title bar,
 * which hosts the optional "in the title bar" test slot.
 */
import type { LiteFile } from '../lite/runtime'

/** Which device frame the mock app is shown in. */
export type TestDevice = 'phone' | 'desktop'
/** The mock app's pages — each a different "feel" to test against. */
export type TestPage = 'home' | 'article' | 'code'

export interface TestSettings {
  device: TestDevice
  page: TestPage
  /** The element your animation replaces, remembered per page. */
  slot: Record<TestPage, string>
  /** Number of actions under the feed's first post. */
  count: number
  /** Icon-sized slots render at this size (px). */
  size: number
  /** Also show the animation in the editor's own title bar. */
  inTitleBar: boolean
  /** …at this size (px). */
  navSize: number
  /** Text next to an icon slot (nav rows, tabs, actions). */
  label: string
  dark: boolean
  /** Real photos and text instead of crossed-out wireframe boxes. */
  hifi: boolean
  /** Hovering the whole slot (e.g. a nav row) drives the `hover` input. */
  rowHover: boolean
  /** Playground becomes the Test Area: the mock app fills the stage. */
  showInPlayground: boolean
  /** Full-size slots: crop to fill (`cover`) or fit inside (`contain`). */
  cover: boolean
}

export const TEST_DEFAULTS: TestSettings = {
  device: 'phone',
  page: 'home',
  slot: { home: 'action:0', article: 'clap', code: 'run' },
  count: 4,
  size: 26,
  inTitleBar: false,
  navSize: 24,
  label: 'Likes',
  dark: false,
  hifi: true,
  rowHover: true,
  showInPlayground: false,
  cover: false,
}

const KEY = 'an1.testArea.v3'

function load(): TestSettings {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<TestSettings>
    return { ...TEST_DEFAULTS, ...saved, slot: { ...TEST_DEFAULTS.slot, ...(saved.slot ?? {}) } }
  } catch {
    return TEST_DEFAULTS
  }
}

export interface TestAreaState {
  /** The current scene as Toasty Lite (null until exported). */
  file: LiteFile | null
  /** Whether the Test Area panel is mounted — the title bar slot only shows while it is. */
  panelOpen: boolean
  settings: TestSettings
}

let state: TestAreaState = { file: null, panelOpen: false, settings: load() }
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
  /** Patch the settings and remember them for next time. */
  setSettings(patch: Partial<TestSettings>) {
    const settings = { ...state.settings, ...patch }
    try {
      localStorage.setItem(KEY, JSON.stringify(settings))
    } catch {
      // Not remembered; still applies.
    }
    testAreaStore.set({ settings })
  },
}
