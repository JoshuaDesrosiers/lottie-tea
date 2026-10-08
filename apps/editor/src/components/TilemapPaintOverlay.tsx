import type { RefObject } from 'react'
import { screenToWorld, type Camera, type CanvasSize } from '../camera'
import type { NodeSummary, OpaqueId } from '../ipc/document'

/** The "paint tiles onto a Tilemap node" click-catcher — mirrors
 * `PathEditOverlay`'s own shape (a transparent absolutely-positioned overlay
 * sibling to the canvas, doing its own screen -> world -> local conversion).
 * Click paints `selectedTile` (the Tile Palette panel's own current
 * selection — `TilePalettePanel.tsx`, now a real dockable panel, not part of
 * this overlay) into the clicked cell; right-click erases. */
export function TilemapPaintOverlay({
  containerRef,
  selectedNode,
  selectedTile,
  camera,
  canvasSize,
  onSetTile,
}: {
  containerRef: RefObject<HTMLDivElement | null>
  selectedNode: NodeSummary | null
  selectedTile: number
  camera: Camera
  canvasSize: CanvasSize
  onSetTile: (node: OpaqueId, x: number, y: number, value: number | null) => void
}) {
  const tilemap = selectedNode?.tilemap ?? null
  if (!tilemap || !selectedNode) return null
  const { translation, rotation, scale } = selectedNode.worldTransform
  const anchor = selectedNode.transform.anchor
  const cosR = Math.cos(rotation)
  const sinR = Math.sin(rotation)

  const worldToLocal = (wx: number, wy: number) => {
    const dx = wx - translation.x
    const dy = wy - translation.y
    const unrotatedX = dx * cosR + dy * sinR
    const unrotatedY = -dx * sinR + dy * cosR
    return { x: unrotatedX / scale.x + anchor.x, y: unrotatedY / scale.y + anchor.y }
  }
  // `box`: this overlay's own element (it fills the canvas). The shared
  // `containerRef` belongs to `TransformHandles`, which isn't mounted while
  // painting — it was null here, so cursor positions weren't offset by the
  // canvas's position and painting landed off target whenever the canvas
  // doesn't start at the window's top-left (docked panels).
  const clientToCell = (clientX: number, clientY: number, box: Element | null) => {
    const rect = (box ?? containerRef.current)?.getBoundingClientRect()
    const world = screenToWorld(clientX - (rect?.left ?? 0), clientY - (rect?.top ?? 0), camera, canvasSize)
    const local = worldToLocal(world.x, world.y)
    const totalWidth = tilemap.mapWidth * tilemap.cellSize
    const totalHeight = tilemap.mapHeight * tilemap.cellSize
    const x = Math.floor((local.x + totalWidth / 2) / tilemap.cellSize)
    const y = Math.floor((totalHeight / 2 - local.y) / tilemap.cellSize)
    if (x < 0 || y < 0 || x >= tilemap.mapWidth || y >= tilemap.mapHeight) return null
    return { x, y }
  }

  const handlePointerDown = (e: React.PointerEvent) => {
    const cell = clientToCell(e.clientX, e.clientY, e.currentTarget)
    if (!cell) return
    e.preventDefault()
    onSetTile(selectedNode.id, cell.x, cell.y, e.button === 2 ? null : selectedTile)
  }

  return <div onPointerDown={handlePointerDown} onContextMenu={(e) => e.preventDefault()} className="absolute inset-0 cursor-crosshair" />
}
