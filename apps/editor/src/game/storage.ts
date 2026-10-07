/**
 * `ctx.storage` — persistent save data for games (Unity's `PlayerPrefs`,
 * the web's `localStorage`). Values are anything JSON can hold; they survive
 * closing the editor, reloading an exported game, and leaving Playground.
 *
 * Everything lives under one key prefix so a game's saves never collide
 * with the editor's own settings. The editor and every exported game opened
 * from the same place share the browser's storage for that origin; give
 * keys a game-specific name (`'royale'`, `'mygame.progress'`).
 *
 * Storage can be unavailable (private windows, blocked site data) — reads
 * then return `null` and writes do nothing, so a game simply starts fresh
 * instead of crashing.
 */

const PREFIX = 'an1mbyt3.save.'

export interface ScriptStorage {
  /** The saved value for `key`, or `null` if none (or unreadable). */
  get(key: string): unknown
  /** Saves `value` (anything JSON-serializable) under `key`. */
  set(key: string, value: unknown): void
  /** Deletes `key`. */
  remove(key: string): void
  /** Every saved key. */
  keys(): string[]
}

export const gameStorage: ScriptStorage = {
  get(key) {
    try {
      const raw = localStorage.getItem(PREFIX + key)
      return raw === null ? null : (JSON.parse(raw) as unknown)
    } catch {
      return null
    }
  },
  set(key, value) {
    try {
      localStorage.setItem(PREFIX + key, JSON.stringify(value))
    } catch (err) {
      console.warn(`ctx.storage.set('${key}') failed`, err)
    }
  },
  remove(key) {
    try {
      localStorage.removeItem(PREFIX + key)
    } catch {
      // unavailable — nothing to remove
    }
  },
  keys() {
    try {
      const out: string[] = []
      for (let i = 0; i < localStorage.length; i++) {
        const k = localStorage.key(i)
        if (k?.startsWith(PREFIX)) out.push(k.slice(PREFIX.length))
      }
      return out
    } catch {
      return []
    }
  },
}
