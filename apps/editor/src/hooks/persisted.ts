/**
 * Small `localStorage` helpers for UI layout that should survive restarts
 * (dock positions, which panels are open, the bottom dock). Every access is
 * guarded: storage can be unavailable or hold stale/garbage data, in which
 * case the caller's defaults win.
 */
import { useEffect, useState } from 'react'

const PREFIX = 'an1mbyt3.layout.'

export function loadPersisted<T>(key: string, fallback: T, valid: (value: unknown) => value is T): T {
  try {
    const raw = window.localStorage.getItem(PREFIX + key)
    if (raw === null) return fallback
    const parsed: unknown = JSON.parse(raw)
    return valid(parsed) ? parsed : fallback
  } catch {
    return fallback
  }
}

export function savePersisted(key: string, value: unknown): void {
  try {
    window.localStorage.setItem(PREFIX + key, JSON.stringify(value))
  } catch {
    // Storage full or blocked: layout just won't persist this time.
  }
}

/** `useState` that loads from / writes back to storage under `key`. */
export function usePersistedState<T>(key: string, fallback: T, valid: (value: unknown) => value is T) {
  const [value, setValue] = useState<T>(() => loadPersisted(key, fallback, valid))
  useEffect(() => {
    savePersisted(key, value)
  }, [key, value])
  return [value, setValue] as const
}

export const isBoolean = (v: unknown): v is boolean => typeof v === 'boolean'
export const isNumber = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v)
