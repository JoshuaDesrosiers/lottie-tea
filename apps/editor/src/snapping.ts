/**
 * Canva/Figma-style transform snapping: rotation snaps to common angles,
 * moving snaps to the artboard's own bounds and other shapes' edges/centers
 * (with a visual alignment guide while it's active). Pure functions only —
 * `App.tsx`'s move-drag and `TransformHandles`' rotate-drag each call these
 * and apply the result themselves, the same "component/caller owns the
 * drag, math lives in a shared module" split `camera.ts` already established.
 */

import type { ArtboardSummary, NodeSummary, OpaqueId, Vec2 } from './ipc/document'

/** Snap to the nearest multiple of `incrementDeg` if within `thresholdDeg`
 * of it, otherwise return `degrees` unchanged — Canva's own rotate-snap
 * behavior (free rotation everywhere else, a soft magnetic pull only near
 * 0/15/30/45/...). */
export function snapAngleDegrees(degrees: number, incrementDeg = 15, thresholdDeg = 4): number {
  const nearest = Math.round(degrees / incrementDeg) * incrementDeg
  return Math.abs(degrees - nearest) <= thresholdDeg ? nearest : degrees
}

export interface AABB {
  minX: number
  minY: number
  maxX: number
  maxY: number
}

/** A node's axis-aligned bounding box in world space, accounting for its
 * rotation/scale (so a rotated node's *visual* extent snaps correctly, not
 * its unrotated local box) — same corner-transform math `TransformHandles`
 * uses to draw its own outline. `translationOverride` lets a live drag ask
 * "what would this node's box be *at* this candidate position" without
 * needing to mutate the node first. */
export function nodeWorldAABB(node: NodeSummary, translationOverride?: Vec2): AABB {
  const translation = translationOverride ?? node.transform.translation
  const { rotation, scale } = node.transform
  const { localMin, localMax } = node
  const cos = Math.cos(rotation)
  const sin = Math.sin(rotation)

  const corners = [
    { x: localMin.x, y: localMin.y },
    { x: localMax.x, y: localMin.y },
    { x: localMax.x, y: localMax.y },
    { x: localMin.x, y: localMax.y },
  ].map((c) => {
    const sx = c.x * scale.x
    const sy = c.y * scale.y
    return { x: translation.x + (sx * cos - sy * sin), y: translation.y + (sx * sin + sy * cos) }
  })

  return {
    minX: Math.min(...corners.map((c) => c.x)),
    maxX: Math.max(...corners.map((c) => c.x)),
    minY: Math.min(...corners.map((c) => c.y)),
    maxY: Math.max(...corners.map((c) => c.y)),
  }
}

export interface SnapTargets {
  /** World-space x-positions a dragged shape's left/center/right can snap to. */
  x: number[]
  /** World-space y-positions a dragged shape's top/center/bottom can snap to. */
  y: number[]
}

/** Every x/y a moving node can snap to on this artboard — the artboard's own
 * edges/center, plus every *other* node's edges/center. Excludes
 * `excludeNodeId` (the node actually being dragged) so a shape never snaps
 * to itself. */
export function collectSnapTargets(artboard: ArtboardSummary, excludeNodeId: OpaqueId): SnapTargets {
  const x = [-artboard.width / 2, 0, artboard.width / 2]
  const y = [-artboard.height / 2, 0, artboard.height / 2]

  for (const node of artboard.nodes) {
    if (node.id.idx === excludeNodeId.idx && node.id.version === excludeNodeId.version) continue
    const box = nodeWorldAABB(node)
    x.push(box.minX, (box.minX + box.maxX) / 2, box.maxX)
    y.push(box.minY, (box.minY + box.maxY) / 2, box.maxY)
  }

  return { x, y }
}

export interface SnapResult {
  /** The candidate translation, adjusted by whatever snap took effect (0 on
   * an axis with no snap in range). */
  translation: Vec2
  /** The world x/y position a guide line should be drawn at, or `null` if
   * that axis didn't snap this frame. */
  guideX: number | null
  guideY: number | null
}

/** Snaps `node`'s bounding box (as it would be at `candidateTranslation`) to
 * the nearest in-range target on each axis independently — left/center/right
 * against `targets.x`, top/center/bottom against `targets.y`. `thresholdWorld`
 * is a screen-pixel distance already divided by the camera's zoom, so the
 * snap "pull" feels the same regardless of zoom level. */
export function snapTranslation(node: NodeSummary, candidateTranslation: Vec2, targets: SnapTargets, thresholdWorld: number): SnapResult {
  const box = nodeWorldAABB(node, candidateTranslation)

  function bestAxisSnap(candidates: number[], targetValues: number[]): { delta: number; guide: number | null } {
    let bestDelta = 0
    let bestGuide: number | null = null
    let bestAbs = thresholdWorld
    for (const c of candidates) {
      for (const t of targetValues) {
        const delta = t - c
        if (Math.abs(delta) <= bestAbs) {
          bestAbs = Math.abs(delta)
          bestDelta = delta
          bestGuide = t
        }
      }
    }
    return { delta: bestDelta, guide: bestGuide }
  }

  const xSnap = bestAxisSnap([box.minX, (box.minX + box.maxX) / 2, box.maxX], targets.x)
  const ySnap = bestAxisSnap([box.minY, (box.minY + box.maxY) / 2, box.maxY], targets.y)

  return {
    translation: { x: candidateTranslation.x + xSnap.delta, y: candidateTranslation.y + ySnap.delta },
    guideX: xSnap.guide,
    guideY: ySnap.guide,
  }
}
