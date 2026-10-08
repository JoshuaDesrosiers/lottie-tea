import { useEffect, useRef, useState } from 'react'
import { renderCel, renderFrame, thumbnail, type PaintCel, type PaintSession } from '../paint/session'
import { Slider } from './Slider'

/**
 * Paint mode's timeline (Aseprite / Flash style): one row per layer (top
 * layer first), one cell per frame. A cell is that layer's artwork on that
 * frame; clicking one opens it in scratch-paint (`PaintAssetEditor`, which
 * also shows the other layers and the neighbouring frames around it). Plus
 * frame and layer actions, frame rate, onion skin, and a looping preview of
 * the whole animation (every visible layer composited).
 */
export function PaintFrameStrip({
  session,
  live,
  liveVersion,
  onSelectCel,
  onAddFrame,
  onDuplicateFrame,
  onDeleteFrame,
  onMoveFrame,
  onSetFps,
  onToggleOnion,
  onAddLayer,
  onDeleteLayer,
  onMoveLayer,
  onRenameLayer,
  onSetLayerVisible,
  onSetLayerOpacity,
}: {
  session: PaintSession
  /** The current cel as scratch-paint last exported it (not yet in `session`). */
  live: PaintCel | null
  /** Bumps whenever `live` changes, so its thumbnails redraw. */
  liveVersion: number
  onSelectCel: (layer: number, frame: number) => void
  onAddFrame: () => void
  onDuplicateFrame: () => void
  onDeleteFrame: () => void
  onMoveFrame: (delta: -1 | 1) => void
  onSetFps: (fps: number) => void
  onToggleOnion: () => void
  onAddLayer: () => void
  onDeleteLayer: () => void
  /** +1 = up (in front), -1 = down. */
  onMoveLayer: (delta: -1 | 1) => void
  onRenameLayer: (layer: number, name: string) => void
  onSetLayerVisible: (layer: number, visible: boolean) => void
  onSetLayerOpacity: (layer: number, opacity: number) => void
}) {
  const [celThumbs, setCelThumbs] = useState<(string | null)[][]>([])
  const [playing, setPlaying] = useState(false)
  const [previewFrame, setPreviewFrame] = useState(0)
  const [renaming, setRenaming] = useState<number | null>(null)
  const frames = useRef<HTMLCanvasElement[]>([])
  const preview = useRef<HTMLCanvasElement>(null)
  const [previewVersion, setPreviewVersion] = useState(0)

  const celOf = (layer: number, frame: number): PaintCel | null =>
    layer === session.currentLayer && frame === session.currentFrame ? live ?? session.layers[layer]?.cels[frame] ?? null : session.layers[layer]?.cels[frame] ?? null

  // Cel thumbnails, and every frame composited for the preview.
  useEffect(() => {
    let cancelled = false
    ;(async () => {
      const thumbs: (string | null)[][] = []
      for (let l = 0; l < session.layers.length; l++) {
        const row: (string | null)[] = []
        for (let f = 0; f < session.frameCount; f++) {
          const cel = celOf(l, f)
          row.push(cel ? thumbnail(await renderCel(cel), 56) : null)
        }
        thumbs.push(row)
      }
      const rendered: HTMLCanvasElement[] = []
      for (let f = 0; f < session.frameCount; f++) {
        rendered.push(await renderFrame(session, f, f === session.currentFrame ? { layer: session.currentLayer, cel: celOf(session.currentLayer, f) } : undefined))
      }
      if (cancelled) return
      frames.current = rendered
      setCelThumbs(thumbs)
      setPreviewVersion((v) => v + 1)
    })().catch((err) => console.warn('timeline thumbnails failed', err))
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session, live, liveVersion])

  // The looping preview.
  useEffect(() => {
    if (!playing) return
    const timer = window.setInterval(() => setPreviewFrame((f) => (f + 1) % Math.max(1, session.frameCount)), 1000 / Math.max(1, session.fps))
    return () => window.clearInterval(timer)
  }, [playing, session.fps, session.frameCount])
  useEffect(() => {
    const canvas = preview.current
    if (!canvas) return
    const source = frames.current[playing ? previewFrame % Math.max(1, frames.current.length) : session.currentFrame]
    const context = canvas.getContext('2d')!
    context.clearRect(0, 0, canvas.width, canvas.height)
    if (source) context.drawImage(source, 0, 0, canvas.width, canvas.height)
  }, [previewFrame, playing, previewVersion, session.currentFrame])

  const button = 'h-6 rounded-md px-2 text-[11px] text-ink-dim hover:bg-overlay/10 hover:text-ink disabled:opacity-40 disabled:hover:bg-transparent'
  const checker = 'bg-[repeating-conic-gradient(#d8d8de_0_25%,#fff_0_50%)] [background-size:8px_8px]'
  // Top layer first, like every layers panel.
  const rows = session.layers.map((_, i) => session.layers.length - 1 - i)
  return (
    <div className="pointer-events-auto absolute bottom-3 left-1/2 z-20 flex max-h-[45%] max-w-[calc(100%-2rem)] -translate-x-1/2 items-stretch gap-3 rounded-xl border border-panel-border bg-panel p-2 shadow-2xl">
      {/* Preview */}
      <div className="flex shrink-0 flex-col items-center gap-1">
        <canvas ref={preview} width={128} height={96} className={`rounded-md ${checker}`} />
        <div className="flex items-center gap-1">
          <button onClick={() => setPlaying((p) => !p)} className={button} title={playing ? 'Stop the preview' : 'Play the animation in this preview'}>
            {playing ? '■ Stop' : '▶ Play'}
          </button>
          <label className="flex items-center gap-1 text-[11px] text-ink-dim" title="Frames per second">
            <input
              type="number"
              min={1}
              max={60}
              value={session.fps}
              onChange={(e) => onSetFps(Math.min(60, Math.max(1, Number(e.target.value) || 12)))}
              className="h-6 w-11 rounded bg-overlay/10 px-1 text-ink outline-none"
            />
            fps
          </label>
        </div>
      </div>

      <div className="flex min-w-0 flex-col gap-1">
        {/* Actions */}
        <div className="flex flex-wrap items-center gap-1">
          <span className="mr-1 text-[11px] font-medium uppercase tracking-wide text-ink-dim">Frame</span>
          <button onClick={onAddFrame} className={button} title="New blank frame after this one">
            + New
          </button>
          <button onClick={onDuplicateFrame} className={button} title="Copy this frame (every layer) — then change the copy a little: the usual way to animate">
            Duplicate
          </button>
          <button onClick={() => onMoveFrame(-1)} disabled={session.currentFrame === 0} className={button} title="Move this frame earlier">
            ←
          </button>
          <button onClick={() => onMoveFrame(1)} disabled={session.currentFrame >= session.frameCount - 1} className={button} title="Move this frame later">
            →
          </button>
          <button onClick={onDeleteFrame} disabled={session.frameCount <= 1} className={button} title="Delete this frame">
            Delete
          </button>
          <span className="mx-1 h-4 w-px bg-overlay/15" />
          <span className="mr-1 text-[11px] font-medium uppercase tracking-wide text-ink-dim">Layer</span>
          <button onClick={onAddLayer} className={button} title="New layer above this one">
            + New
          </button>
          <button onClick={() => onMoveLayer(1)} disabled={session.currentLayer >= session.layers.length - 1} className={button} title="Move this layer up (in front)">
            ↑
          </button>
          <button onClick={() => onMoveLayer(-1)} disabled={session.currentLayer === 0} className={button} title="Move this layer down (behind)">
            ↓
          </button>
          <button onClick={onDeleteLayer} disabled={session.layers.length <= 1} className={button} title="Delete this layer (every frame of it)">
            Delete
          </button>
          <span className="mx-1 h-4 w-px bg-overlay/15" />
          <label className="flex items-center gap-1 text-[11px] text-ink-dim" title="Show the previous frame (red) and next frame (blue) faintly under this one">
            <input type="checkbox" checked={session.onion} onChange={onToggleOnion} />
            Onion skin
          </label>
        </div>

        {/* Grid: layer rows × frame cells */}
        <div className="min-h-0 overflow-auto pb-1">
          <table className="border-separate border-spacing-1">
            <thead>
              <tr>
                <th />
                {Array.from({ length: session.frameCount }, (_, f) => (
                  <th key={f} className={`text-center text-[10px] font-medium tabular-nums ${f === session.currentFrame ? 'text-accent' : 'text-ink-dim'}`}>
                    <button onClick={() => onSelectCel(session.currentLayer, f)} className="w-full rounded hover:bg-overlay/10" title={`Frame ${f + 1}`}>
                      {f + 1}
                    </button>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((l) => {
                const layer = session.layers[l]
                const current = l === session.currentLayer
                return (
                  <tr key={l}>
                    <td className={`rounded-md px-1 ${current ? 'bg-accent/15' : ''}`}>
                      <div className="flex w-36 items-center gap-1">
                        <button
                          onClick={() => onSetLayerVisible(l, !layer.visible)}
                          className={`flex h-5 w-5 shrink-0 items-center justify-center rounded text-[11px] hover:bg-overlay/10 ${layer.visible ? 'text-ink' : 'text-ink-dim/50'}`}
                          title={layer.visible ? 'Hide this layer' : 'Show this layer'}
                        >
                          {layer.visible ? '●' : '○'}
                        </button>
                        {renaming === l ? (
                          <input
                            autoFocus
                            defaultValue={layer.name}
                            onBlur={(e) => {
                              setRenaming(null)
                              const name = e.target.value.trim()
                              if (name && name !== layer.name) onRenameLayer(l, name)
                            }}
                            onKeyDown={(e) => {
                              if (e.key === 'Enter') (e.target as HTMLInputElement).blur()
                              if (e.key === 'Escape') setRenaming(null)
                            }}
                            className="min-w-0 flex-1 rounded bg-overlay/10 px-1 text-[11px] text-ink outline-none"
                          />
                        ) : (
                          <button
                            onClick={() => onSelectCel(l, session.currentFrame)}
                            onDoubleClick={() => setRenaming(l)}
                            className={`min-w-0 flex-1 truncate text-left text-[11px] ${current ? 'font-medium text-ink' : 'text-ink-dim'}`}
                            title="Click to draw on this layer · double-click to rename"
                          >
                            {layer.name}
                          </button>
                        )}
                        <div className="w-12 shrink-0" title={`Opacity ${Math.round(layer.opacity * 100)}%`}>
                          <Slider min={0} max={1} step={0.01} value={layer.opacity} onChange={(opacity) => onSetLayerOpacity(l, opacity)} />
                        </div>
                      </div>
                    </td>
                    {Array.from({ length: session.frameCount }, (_, f) => {
                      const selected = current && f === session.currentFrame
                      const thumb = celThumbs[l]?.[f]
                      return (
                        <td key={f}>
                          <button
                            onClick={() => onSelectCel(l, f)}
                            className={`block overflow-hidden rounded border-2 ${selected ? 'border-accent' : f === session.currentFrame ? 'border-accent/30' : 'border-transparent hover:border-overlay/30'}`}
                            title={`${layer.name} · frame ${f + 1}`}
                          >
                            <div className={`h-[33px] w-11 ${checker} ${layer.visible ? '' : 'opacity-40'}`}>{thumb && <img src={thumb} alt="" className="h-full w-full" draggable={false} />}</div>
                          </button>
                        </td>
                      )
                    })}
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
        <span className="text-[10px] text-ink-dim">, / . switch frames · click a cell to draw on that layer & frame · double-click a layer name to rename</span>
      </div>
    </div>
  )
}
