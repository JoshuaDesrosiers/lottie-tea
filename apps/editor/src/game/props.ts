/**
 * Scene **properties** at runtime (Rive's view-model values; authored as
 * `an1mbyt3_core::PropertyDef`, see `crates/an1mbyt3-core/src/events.rs`).
 *
 * One live value per property of the scene being played, starting from its
 * authored default. Everything reads and writes the same store:
 * - scripts: `ctx.props.score`, `ctx.props.score += 1`;
 * - On Click actions: Set Property;
 * - Property Binding components (applied every frame by `ScriptHost`);
 * - HTML UI: `data-bind="score"` shows a value, `data-prop="volume"` on an
 *   input / checkbox / select writes one (`htmlOverlay.ts`).
 *
 * Values are coerced to the property's type on every write, so a number
 * stays a number however it was set ("3" from an input → 3). Values reset
 * on every Play and every scene switch (defaults again).
 */
import type { OpaqueId, PropertySummary } from '../ipc/document'

/** The authored properties (call whenever the document summary changes).
 * Stored on the *active* store (`sceneProps`), so each embedded player
 * keeps its own game's definitions (see `runtimeContext.ts`). */
export function setPropertyDefs(list: readonly PropertySummary[] | undefined): void {
  sceneProps.defs = [...(list ?? [])]
}

/** A value as `kind` (`Number` / `Bool` / `Text` / `Color`). */
export function coerce(kind: PropertySummary['kind'], value: unknown): unknown {
  switch (kind) {
    case 'Number': {
      const n = typeof value === 'number' ? value : Number(value)
      return Number.isFinite(n) ? n : 0
    }
    case 'Bool':
      return value === true || value === 1 || value === 'true' || value === '1' || value === 'on'
    case 'Text':
      return value == null ? '' : String(value)
    case 'Color': {
      if (Array.isArray(value) && value.length >= 3) return [Number(value[0]), Number(value[1]), Number(value[2]), value.length > 3 ? Number(value[3]) : 1]
      if (typeof value === 'string' && /^#?[0-9a-f]{6}$/i.test(value)) {
        const h = value.replace('#', '')
        return [parseInt(h.slice(0, 2), 16) / 255, parseInt(h.slice(2, 4), 16) / 255, parseInt(h.slice(4, 6), 16) / 255, 1]
      }
      return [1, 1, 1, 1]
    }
  }
}

function parseDefault(p: PropertySummary): unknown {
  try {
    return coerce(p.kind, JSON.parse(p.default))
  } catch {
    return coerce(p.kind, p.default)
  }
}

export class SceneProps {
  /** The authored properties of every scene (`setPropertyDefs`). */
  defs: PropertySummary[] = []
  private sceneKey = ''
  private values = new Map<string, unknown>()
  private kinds = new Map<string, PropertySummary['kind']>()
  /** Bumped on every change (bindings / HTML only re-apply on change). */
  version = 0

  /** Switches to `scene`'s properties (fresh defaults) if it isn't current. */
  ensureScene(scene: OpaqueId): void {
    const key = `${scene.idx}.${scene.version}`
    if (key === this.sceneKey) {
      // New properties added while playing: pick them up.
      for (const p of this.defs) {
        if (p.artboard.idx === scene.idx && p.artboard.version === scene.version && !this.kinds.has(p.name)) {
          this.define(p)
          this.version++
        }
      }
      return
    }
    this.sceneKey = key
    this.values.clear()
    this.kinds.clear()
    for (const p of this.defs) if (p.artboard.idx === scene.idx && p.artboard.version === scene.version) this.define(p)
    this.version++
  }

  private define(p: PropertySummary) {
    this.kinds.set(p.name, p.kind)
    this.values.set(p.name, parseDefault(p))
  }

  /** A new play session: back to defaults on next use. */
  reset(): void {
    this.sceneKey = ''
    this.version++
  }

  has(name: string): boolean {
    return this.kinds.has(name)
  }

  get(name: string): unknown {
    return this.values.get(name)
  }

  kind(name: string): PropertySummary['kind'] | undefined {
    return this.kinds.get(name)
  }

  /** Sets a property (coerced to its type). Unknown names are reported
   * once by the caller; returns whether it exists. */
  set(name: string, value: unknown): boolean {
    const kind = this.kinds.get(name)
    if (!kind) return false
    const next = coerce(kind, value)
    const prev = this.values.get(name)
    if (JSON.stringify(prev) !== JSON.stringify(next)) {
      this.values.set(name, next)
      this.version++
    }
    return true
  }

  /** `"+1"` / `"-2.5"` add to a number; anything else is a plain value
   * (JSON if it parses). What On Click's Set Property uses. */
  apply(name: string, text: string): boolean {
    const kind = this.kinds.get(name)
    if (!kind) return false
    if (kind === 'Number' && /^[+-]\s*\d/.test(text.trim())) return this.set(name, Number(this.get(name) ?? 0) + Number(text.replace(/\s/g, '')))
    if (kind === 'Bool' && text.trim() === '!') return this.set(name, !this.get(name))
    let value: unknown = text
    try {
      value = JSON.parse(text)
    } catch {
      // plain text
    }
    return this.set(name, value)
  }

  names(): string[] {
    return [...this.kinds.keys()]
  }

  /** What scripts see as `ctx.props`: read / write by name. */
  proxy(onUnknown: (name: string) => void): Record<string, any> {
    return new Proxy({} as Record<string, any>, {
      get: (_, key) => (typeof key === 'string' ? this.get(key) : undefined),
      set: (_, key, value) => {
        if (typeof key === 'string' && !this.set(key, value)) onUnknown(key)
        return true
      },
      has: (_, key) => typeof key === 'string' && this.has(key),
      ownKeys: () => this.names(),
      getOwnPropertyDescriptor: (_, key) => (typeof key === 'string' && this.has(key) ? { enumerable: true, configurable: true, value: this.get(key) } : undefined),
    })
  }
}

/**
 * The *active* runtime property store. The editor has one; every embedded
 * player (`player/engine.ts`) has its own, swapped in around that
 * player's frame / API calls by `runtimeContext.ts`. An ES module live
 * binding: importers always see the current one.
 */
export let sceneProps = new SceneProps()

/** Makes `next` the active store; returns the one it replaced. Only
 * `runtimeContext.ts` should call this. */
export function swapSceneProps(next: SceneProps): SceneProps {
  const previous = sceneProps
  sceneProps = next
  return previous
}

/** Text for a value: `format` with `{}` replaced (numbers trimmed). */
export function formatProp(value: unknown, format = ''): string {
  const text = typeof value === 'number' ? String(Math.round(value * 1000) / 1000) : Array.isArray(value) ? value.map((v) => Math.round(Number(v) * 100) / 100).join(', ') : String(value ?? '')
  return format && format.includes('{}') ? format.split('{}').join(text) : text
}
