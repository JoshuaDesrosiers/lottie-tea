import { Component, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from 'react'
import { Provider } from 'react-redux'
import { IntlProvider } from 'react-intl'
import { createStore, combineReducers } from 'redux'
// @ts-expect-error — vendored JS, not typed (forked directly from
// scratchfoundation/scratch-paint per the user's own explicit choice —
// see this component's own doc comment for the integration seam and the
// AGPL-3.0 licensing note that comes with it).
import ScratchPaintEditor, { ScratchPaintReducer, setOnionSkin, setOverSkin, setPickSource, startEyeDropper } from '../vendor/scratch-paint/index.js'
// @ts-expect-error — plain JS (no declaration file), same reasoning as the
// scratch-paint import above.
import { PaintToolPalette } from './PaintToolPalette.jsx'
// @ts-expect-error — no declaration file for this fork.
import paper from '@scratch/paper'
import { BLANK_PNG, celAt, gridCanvas, onionSkin, referenceCanvas, renderFrame, renderLayers, stack, withLive, type PaintCel, type PaintSession } from '../paint/session'
import { PaintFrameStrip } from './PaintFrameStrip'
import { PaintBrushBar } from './PaintBrushBar'
import { DEFAULT_VIEW, PaintCanvasBar, type PaintView } from './PaintCanvasBar'

/** Every one of scratch-paint's ~17 tool-mode containers (`SelectMode`/
 * `BrushMode`/etc., mounted invisibly by `paint-editor.jsx` — see that
 * file's own doc comment) calls `this.tool.remove()` in its own
 * `componentWillUnmount`, on a `paper.Tool` tied to `paper`'s global
 * `PaperScope` singleton (`paper.setup`/`paper.remove` both mutate global
 * state, not a per-instance one). React StrictMode's dev-only double-invoke
 * (mount, unmount, mount-again) exercises this exact teardown twice in a
 * row; an unmount-time throw from any one of them would escape every error
 * boundary above it (a boundary can't catch its own subtree's unmount error
 * while it's *also* unmounting in the same commit). Patching this once,
 * centrally, at the `paper.Tool` prototype level is far more tractable than
 * editing all ~17 files identically — same reasoning as
 * `paper-canvas.jsx`'s own hardened `paper.remove()` call, just for the
 * *other* half of paper.js's teardown surface this integration exercises.
 */
if (paper?.Tool?.prototype && !paper.Tool.prototype.__an1mbyt3PatchedRemove) {
  const originalRemove = paper.Tool.prototype.remove
  paper.Tool.prototype.remove = function patchedRemove(...args: unknown[]) {
    try {
      return originalRemove.apply(this, args)
    } catch (err) {
      console.error('paper.Tool#remove() threw during unmount — ignored so teardown can continue', err)
    }
  }
  paper.Tool.prototype.__an1mbyt3PatchedRemove = true
}

/**
 * Paint Asset Mode — mounts the actual `scratchfoundation/scratch-paint`
 * editor (forked directly into `src/vendor/scratch-paint`, not reimplemented
 * — see that directory; copied from its `src/`, with only the mechanical
 * changes needed to build under Vite instead of its own webpack config:
 * every `.css` renamed to `.module.css` for Vite's CSS Modules convention,
 * and `vite.config.ts` defines an empty `process.env` since `process` isn't
 * a real global in a Vite browser bundle) as a full-screen view, separate
 * from the main artboard canvas entirely — a `TopToolbar` mode
 * (`MODES`, `TopToolbar.tsx`) alongside Design/Animate/State Machine/
 * Playground: picking it swaps out the canvas region for this component
 * (`App.tsx`'s own mode-gated render), the same "one region, one mode's
 * content at a time" pattern those other modes already use for the bottom
 * dock. This is its own isolated drawing surface with its own undo stack/
 * zoom/tool state, not an overlay on the live document the way
 * `PathEditOverlay`/the old on-canvas Pen tool attempt was. Gives every
 * scratch-paint tool for free: Pen (real
 * click-drag-to-curve, unlike this app's own from-scratch attempt),
 * Brush, Eraser, shapes, Fill, Text, Select/reshape, vector *and* bitmap
 * modes — all of scratch-paint's own UI and interaction, not a port.
 *
 * **Integration seam**: scratch-paint's own public contract is exactly
 * `<PaintEditor image={svgString} onUpdateImage={fn} .../>` (see
 * `containers/paint-editor.jsx`'s own doc comment) — `onUpdateImage` fires
 * on every edit, but *not* with just the image as its one argument the way
 * that doc comment's own wording suggests: the real call
 * (`hocs/update-image-hoc.jsx`) is `(isVector, image, centerX, centerY)` —
 * a real bug this integration hit once already, reading the first argument
 * as the image. This wrapper keeps the most recent SVG string in a ref
 * (`svgRef`) and only actually commits anything to *our* document when the
 * author clicks "Save as Resource" — at that
 * point the SVG goes through the *already-existing* `import_svg` (same
 * path the Shapes menu's "Import SVG" button uses) and, if it produced
 * exactly one `Shape` node, `add_component` (`Command::AddComponent`) turns
 * it into a reusable library resource — "create variations" is then the
 * *already-existing* Components panel's own variant mechanism
 * (`addComponentVariant`), not anything new. A drawing that imports as more
 * than one node (multiple colors/shapes/text) is left as plain document
 * nodes for now rather than wrapped as a component — `add_component`
 * (`editor.rs`) only accepts a single `Shape` node today, a real, honest
 * scope limit of this pass, not a silent drop.
 *
 * **Redux**: scratch-paint's whole internal tool/mode/undo state is Redux
 * (`ScratchPaintReducer`, mounted under the `scratchPaint` key —
 * `containers/paint-editor.jsx`'s own `mapStateToProps` reads
 * `state.scratchPaint.*`) — a wholly separate store from this app's own
 * plain-React-hooks state, scoped entirely to this component's own
 * lifetime (a fresh store per mount, not shared with the rest of the app).
 *
 * **License note**: scratch-paint is AGPL-3.0-only. Forking its source
 * directly into this app's own tree (rather than depending on it as an
 * unmodified npm package) very likely brings that copyleft obligation
 * along for the combined work — this is a real licensing decision with
 * consequences for how an1mbyt3 itself can be distributed, not a purely
 * technical one, and was made explicitly at the user's own direction
 * rather than assumed here.
 *
 * **Why the action bar is a portal, merged into `TopToolbar`**: this
 * component's own canvas region (its outermost `absolute inset-0` div)
 * deliberately spans the *entire* viewport, including the strip behind
 * `TitleBar`/`TopToolbar` — same full-bleed-under-the-chrome convention the
 * live artboard's own `CanvasArea` already uses (floating panels paint on
 * top via plain DOM order). That's harmless for a rendered scene with
 * nothing interactive underneath, but scratch-paint's action bar
 * (`FixedToolsContainer`/`ModeToolsContainer`, rendered inside the vendored
 * `paint-editor.jsx`) is real interactive UI — an earlier pass gave it its
 * *own* separate floating row and hit exactly this: visually *and*
 * pointer-event trapped behind `TopToolbar`'s mode tabs (some buttons were
 * unclickable). The fix ended up being "merge it into `TopToolbar`'s own
 * row entirely" (the user's own ask, once the separate-row version still
 * read as two disconnected toolbars) — when not in Paint mode,
 * `TopToolbar` shows its normal Shapes/Bone/Save/Undo-Redo cluster instead,
 * never both. Its actual Group/Flip/Copy/etc. handlers only work wired to
 * `onUpdateImage`, which only reaches this deep via prop-threading down
 * through `UpdateImageHOC`/`SelectionHOC`/`UndoHOC`/`KeyboardShortcutsHOC`
 * — not reachable from a true sibling like `TopToolbar`. A
 * `ReactDOM.createPortal` (in `paint-editor.jsx` itself) keeps it logically
 * nested exactly where those props/context flow correctly, while rendering
 * its actual DOM output into `actionBarTarget` — a `<div>` `TopToolbar`
 * itself renders and positions inside its own row; `App.tsx` owns the
 * state (`useState<HTMLDivElement | null>`) since both this component and
 * `TopToolbar` need it and are true siblings.
 */
/** The latest artwork scratch-paint reported (`onUpdateImage`) for the cel
 * being edited — a vector SVG or a bitmap `ImageData`, plus its rotation
 * center. "Save as Resource" (`App.tsx`) merges it into the session. */
export type PaintImage = PaintCel

/** What scratch-paint should load. Changing `id` is what makes scratch-paint
 * clear and load `image` (its own `imageId` contract — see
 * `containers/paint-editor.jsx`); `center` is the image's rotation center,
 * which is what keeps every frame registered on the same spot. */
type PaintLoad = { id: string; image: string | null; format: 'svg' | 'png'; center: [number, number] | null }

function imageDataUrl(data: ImageData): string {
  const canvas = document.createElement('canvas')
  canvas.width = data.width
  canvas.height = data.height
  canvas.getContext('2d')!.putImageData(data, 0, 0)
  return canvas.toDataURL('image/png')
}

/** The scratch-paint load for `cel` — a blank one keeps the editor in the
 * mode it was in (`blankFormat`). */
function loadFor(cel: PaintCel | null, id: string, blankFormat: 'svg' | 'png'): PaintLoad {
  if (!cel) return { id, image: blankFormat === 'png' ? BLANK_PNG : null, format: blankFormat, center: null }
  if (cel.kind === 'svg') return { id, image: cel.svg, format: 'svg', center: cel.center }
  return { id, image: cel.kind === 'image' ? cel.dataUrl : imageDataUrl(cel.bitmap), format: 'png', center: cel.center }
}

/** Encodes a bitmap-mode drawing (`ImageData`) as PNG bytes, for saving it
 * as a bitmap Drawing resource. */
export async function imageDataToPng(data: ImageData): Promise<Uint8Array> {
  const canvas = document.createElement('canvas')
  canvas.width = data.width
  canvas.height = data.height
  const context = canvas.getContext('2d')
  if (!context) throw new Error('no 2D canvas available')
  context.putImageData(data, 0, 0)
  const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'))
  if (!blob) throw new Error('PNG encoding failed')
  return new Uint8Array(await blob.arrayBuffer())
}

class PaintErrorBoundary extends Component<{ children: ReactNode; onClose: () => void }, { error: Error | null }> {
  state = { error: null as Error | null }
  static getDerivedStateFromError(error: Error) {
    return { error }
  }
  render() {
    if (this.state.error) {
      return (
        <div className="flex h-full flex-col items-center justify-center gap-3 text-ink-dim">
          <p className="text-sm">Paint mode failed to load.</p>
          <p className="max-w-md text-center text-xs text-ink-dim/70">{this.state.error.message}</p>
          <button onClick={this.props.onClose} className="rounded-md bg-overlay/10 px-3 py-1.5 text-sm text-ink hover:bg-overlay/15">
            Close
          </button>
        </div>
      )
    }
    return this.props.children
  }
}

export function PaintAssetEditor({
  visible,
  actionBarTarget,
  imageRef,
  session,
  sessionKey,
  onSessionChange,
  toolsTarget,
  name,
  onNameChange,
  onClose,
}: {
  /** `App.tsx` keeps this component permanently mounted (see its own
   * render-site comment) and toggles this instead of conditionally
   * mounting/unmounting it — both so the live paper.js canvas/Redux store
   * never gets torn down and rebuilt (the same class of bug that already
   * broke the main artboard canvas once this session) and so a painting
   * actually survives switching to another mode and back, per the user's
   * own request. `display: none` when `false` — paper.js has no
   * continuous render loop of its own, so this costs nothing while hidden. */
  visible: boolean
  /** The portal target `TopToolbar` renders (merged into its own row,
   * alongside the mode tabs) — see this component's own doc comment on why
   * a portal. `App.tsx` owns this state (it has to be visible to both this
   * component and `TopToolbar`, true siblings). */
  actionBarTarget: HTMLDivElement | null
  /** Where the latest artwork is kept — `App.tsx` owns this too (its own
   * "Save as Resource" handler, triggered from `TopToolbar`, reads it), this
   * component only ever writes to it. `null` until something is drawn. */
  imageRef: RefObject<PaintImage | null>
  /** The frames × layers being drawn (`paint/session.ts`); `App.tsx` owns it
   * (Save reads it). The cel being edited lives in scratch-paint — its
   * latest export is `imageRef`, folded into the session (`withLive`)
   * whenever another cel is opened. */
  session: PaintSession
  /** Changes when a different drawing is opened / a new one started:
   * reloads the current cel. */
  sessionKey: string
  onSessionChange: (session: PaintSession) => void
  /** The Design icon rail's slot for Paint's tools (see `IconRail`). */
  toolsTarget: HTMLDivElement | null
  name: string
  onNameChange: (name: string) => void
  onClose: () => void
}) {
  const store = useMemo(() => createStore(combineReducers({ scratchPaint: ScratchPaintReducer })), [])

  // ---- Frames: which cel scratch-paint has loaded ----
  const loads = useRef(0)
  // The mode of the last cel touched, so a blank frame opens in it too.
  const lastFormat = useRef<'svg' | 'png'>('svg')
  const [load, setLoad] = useState<PaintLoad>(() => loadFor(celAt(session, session.currentLayer, session.currentFrame), `${sessionKey}-0`, 'svg'))
  const [liveVersion, setLiveVersion] = useState(0)
  // Grid / reference image: view aids, not part of the drawing.
  const [view, setView] = useState<PaintView>(DEFAULT_VIEW)
  useEffect(() => {
    // A different drawing (or New): open its current cel.
    const cel = celAt(session, session.currentLayer, session.currentFrame)
    imageRef.current = cel
    setLoad(loadFor(cel, `${sessionKey}-${++loads.current}`, lastFormat.current))
    // Only when the drawing itself changes — frame switches load below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionKey])

  /** Store the cel being edited, apply `change`, open the resulting cel. */
  const switchTo = (change: (committed: PaintSession) => PaintSession, reload = true) => {
    const next = change(withLive(session, imageRef.current))
    onSessionChange(next)
    if (!reload) return
    const cel = celAt(next, next.currentLayer, next.currentFrame)
    imageRef.current = cel
    setLoad(loadFor(cel, `${sessionKey}-${++loads.current}`, lastFormat.current))
  }
  const eachLayer = (s: PaintSession, edit: (cels: (PaintCel | null)[]) => (PaintCel | null)[]): PaintSession['layers'] => s.layers.map((layer) => ({ ...layer, cels: edit([...layer.cels]) }))
  const frameActions = {
    select: (frame: number) => frame !== session.currentFrame && switchTo((s) => ({ ...s, currentFrame: frame })),
    selectCel: (layer: number, frame: number) =>
      (layer !== session.currentLayer || frame !== session.currentFrame) && switchTo((s) => ({ ...s, currentLayer: layer, currentFrame: frame })),
    add: () =>
      switchTo((s) => ({ ...s, layers: eachLayer(s, (cels) => (cels.splice(s.currentFrame + 1, 0, null), cels)), frameCount: s.frameCount + 1, currentFrame: s.currentFrame + 1 })),
    duplicate: () =>
      switchTo((s) => ({ ...s, layers: eachLayer(s, (cels) => (cels.splice(s.currentFrame + 1, 0, cels[s.currentFrame] ?? null), cels)), frameCount: s.frameCount + 1, currentFrame: s.currentFrame + 1 })),
    remove: () =>
      session.frameCount > 1 &&
      switchTo((s) => ({ ...s, layers: eachLayer(s, (cels) => (cels.splice(s.currentFrame, 1), cels)), frameCount: s.frameCount - 1, currentFrame: Math.min(s.currentFrame, s.frameCount - 2) })),
    // The cel being edited moves along with its frame: no reload needed.
    move: (delta: -1 | 1) =>
      switchTo((s) => {
        const to = s.currentFrame + delta
        if (to < 0 || to >= s.frameCount) return s
        return { ...s, layers: eachLayer(s, (cels) => ([cels[s.currentFrame], cels[to]] = [cels[to], cels[s.currentFrame]], cels)), currentFrame: to }
      }, false),
  }

  /** Rename / show-hide / opacity: no reload (the cel being edited stays). */
  const setLayer = (index: number, patch: Partial<PaintSession['layers'][number]>) => {
    const committed = withLive(session, imageRef.current)
    onSessionChange({ ...committed, layers: committed.layers.map((layer, i) => (i === index ? { ...layer, ...patch } : layer)) })
  }
  const layerActions = {
    add: () =>
      switchTo((s) => {
        const layers = [...s.layers]
        layers.splice(s.currentLayer + 1, 0, { name: `Layer ${s.layers.length + 1}`, visible: true, opacity: 1, cels: Array.from({ length: s.frameCount }, () => null) })
        return { ...s, layers, currentLayer: s.currentLayer + 1 }
      }),
    remove: () =>
      session.layers.length > 1 &&
      switchTo((s) => {
        const layers = s.layers.filter((_, i) => i !== s.currentLayer)
        return { ...s, layers, currentLayer: Math.min(s.currentLayer, layers.length - 1) }
      }),
    // The cel being edited moves with its layer: no reload.
    move: (delta: -1 | 1) =>
      switchTo((s) => {
        const to = s.currentLayer + delta
        if (to < 0 || to >= s.layers.length) return s
        const layers = [...s.layers]
        ;[layers[s.currentLayer], layers[to]] = [layers[to], layers[s.currentLayer]]
        return { ...s, layers, currentLayer: to }
      }, false),
  }

  // `,` / `.` step through frames (Flash / Aseprite muscle memory).
  useEffect(() => {
    if (!visible) return
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) return
      if (e.ctrlKey || e.metaKey || e.altKey) return
      if (e.key === 'i' || e.key === 'I') startEyeDropper(store, e.shiftKey)
      else if (e.key === ',' && session.currentFrame > 0) frameActions.select(session.currentFrame - 1)
      else if (e.key === '.' && session.currentFrame < session.frameCount - 1) frameActions.select(session.currentFrame + 1)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })

  // What's shown around the cel being drawn: under it, the onion skin
  // (neighbouring frames, tinted) and this frame's lower layers; over it,
  // this frame's higher layers. All reference only — never exported.
  useEffect(() => {
    if (!visible) {
      setOnionSkin(null)
      setOverSkin(null)
      setPickSource(null)
      return
    }
    let cancelled = false
    const f = session.currentFrame
    const l = session.currentLayer
    ;(async () => {
      let onion: HTMLCanvasElement | null = null
      if (session.onion && session.frameCount > 1) {
        const previous = f > 0 ? await renderFrame(session, f - 1) : null
        const next = f < session.frameCount - 1 ? await renderFrame(session, f + 1) : null
        onion = onionSkin(previous, next)
      }
      const below = await renderLayers(session, f, 0, l - 1)
      const above = await renderLayers(session, f, l + 1, session.layers.length - 1)
      if (cancelled) return
      const reference = view.reference && view.referenceVisible ? referenceCanvas(view.reference.image, view.referenceOpacity) : null
      setOnionSkin(stack(reference, stack(onion, below)))
      setOverSkin(stack(above, view.grid ? gridCanvas(view.gridSize) : null))
      // The eyedropper also picks from the reference and the other layers
      // (not the tinted onion skin).
      setPickSource(stack(view.reference && view.referenceVisible ? referenceCanvas(view.reference.image, 1) : null, stack(below, above)))
    })().catch((err) => console.warn('onion skin failed', err))
    return () => {
      cancelled = true
    }
  }, [visible, session, load.id, view])

  // paper.js's own `PaperCanvas.componentDidMount` calls `paper.setup(canvas)`
  // exactly once, measuring the canvas's real pixel size at that instant —
  // and never re-measures afterward except on a window `resize` event (see
  // `paper-canvas.jsx`'s `onViewResize`, wired to `paper.view.on('resize',
  // ...)`, not a `ResizeObserver`). This component being permanently mounted
  // (see the `visible` prop's own doc comment) means that *first* mount
  // would otherwise happen while the wrapping div is still `display: none`
  // (0x0) — before the author has ever opened Paint mode — and the view
  // would stay stuck at zero size forever, since nothing about later
  // toggling `visible` true ever fires a window resize event. Deferring the
  // actual scratch-paint tree's mount until the first real `visible: true`
  // (then never unmounting it again) keeps both promises: `paper.setup()`
  // only ever runs against a canvas with its true on-screen size, and the
  // painting still survives switching away and back, per the user's own
  // request.
  const [hasBeenVisible, setHasBeenVisible] = useState(visible)
  useLayoutEffect(() => {
    if (visible) setHasBeenVisible(true)
  }, [visible])

  // scratch-paint's exports for the cel being edited (its rotation center
  // too — kept per cel so frames stay registered, `paint/session.ts`).
  // `UpdateImageHOC` calls this as `(isVector, image, centerX, centerY)`;
  // bitmap mode passes an `ImageData`. Stable, and it only *schedules* the
  // timeline refresh: re-rendering anything in the same task as the stroke
  // ending is what made the next stroke lag.
  const liveTimer = useRef(0)
  const handleUpdateImage = useCallback((isVector: boolean, image: string | ImageData, centerX?: number, centerY?: number) => {
    const center: [number, number] | null = typeof centerX === 'number' && typeof centerY === 'number' ? [centerX, centerY] : null
    imageRef.current = isVector ? { kind: 'svg', svg: image as string, center } : { kind: 'bitmap', bitmap: image as ImageData, center }
    lastFormat.current = isVector ? 'svg' : 'png'
    window.clearTimeout(liveTimer.current)
    liveTimer.current = window.setTimeout(() => setLiveVersion((v) => v + 1), 180)
  }, [imageRef])
  // Memoized: state changes here (timeline thumbnails, onion skin, view
  // aids) must not re-render scratch-paint's whole component tree — in
  // React's dev build that alone was ~90 ms per stroke.
  const scratchEditor = useMemo(
    () => (
      <ScratchPaintEditor
                imageFormat={load.format}
                imageId={load.id}
                image={load.image ?? undefined}
                rotationCenterX={load.center?.[0]}
                rotationCenterY={load.center?.[1]}
                rtl={false}
                zoomLevelId="an1mbyt3-paint"
                // `UpdateImageHOC`'s real call signature (`hocs/update-image-hoc.jsx`,
                // both `handleUpdateVector`/`handleUpdateBitmap`) is
                // `(isVector, image, centerX, centerY)` — a real bug in this
                // integration, not scratch-paint's own doc comment's fault
                // ("called with the new image" undersells it): treating the
                // *first* argument as the image (as an earlier version of
                // this callback did) meant `svgRef.current` was actually
                // being set to the plain boolean `isVector`, never a real SVG
                // string — "Save as Resource" silently had nothing real to
                // import, every time, and only a `console.error` (easy to
                // miss without devtools open) marked it as rejected.
                // Bitmap mode passes an `ImageData` (`handleUpdateBitmap`) —
                // kept too, so bitmap drawings can be saved (as PNGs).
                // The center args are the image's rotation center — kept per cel
                // so frames stay registered (`paint/session.ts`).
                onUpdateImage={handleUpdateImage}
                onUpdateName={onNameChange}
                name={name}
                actionBarTarget={actionBarTarget}
      />
    ),
    [load, name, actionBarTarget, onNameChange, handleUpdateImage],
  )

  return (
    // `select-none`: drags on the canvas otherwise select (and tint) page text.
    <div className={visible ? 'absolute inset-0 select-none bg-canvas' : 'hidden'}>
      {hasBeenVisible && (
        <PaintErrorBoundary onClose={onClose}>
          <Provider store={store}>
            <IntlProvider locale="en" defaultLocale="en" messages={{}}>
              {scratchEditor}
              <PaintToolPalette target={toolsTarget} />
              <PaintCanvasBar store={store} view={view} onChange={setView} below={<PaintBrushBar store={store} />} />
              <PaintFrameStrip
                session={session}
                live={imageRef.current}
                liveVersion={liveVersion}
                onSelectCel={frameActions.selectCel}
                onAddFrame={frameActions.add}
                onDuplicateFrame={frameActions.duplicate}
                onDeleteFrame={() => frameActions.remove()}
                onMoveFrame={frameActions.move}
                onSetFps={(fps) => onSessionChange({ ...session, fps })}
                onToggleOnion={() => onSessionChange({ ...session, onion: !session.onion })}
                onAddLayer={layerActions.add}
                onDeleteLayer={() => layerActions.remove()}
                onMoveLayer={layerActions.move}
                onRenameLayer={(layer, name) => setLayer(layer, { name })}
                onSetLayerVisible={(layer, visible) => setLayer(layer, { visible })}
                onSetLayerOpacity={(layer, opacity) => setLayer(layer, { opacity })}
              />
            </IntlProvider>
          </Provider>
        </PaintErrorBoundary>
      )}
    </div>
  )
}
