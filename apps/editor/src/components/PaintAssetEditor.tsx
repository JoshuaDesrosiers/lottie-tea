import { Component, useLayoutEffect, useMemo, useState, type ReactNode, type RefObject } from 'react'
import { Provider } from 'react-redux'
import { IntlProvider } from 'react-intl'
import { createStore, combineReducers } from 'redux'
// @ts-expect-error — vendored JS, not typed (forked directly from
// scratchfoundation/scratch-paint per the user's own explicit choice —
// see this component's own doc comment for the integration seam and the
// AGPL-3.0 licensing note that comes with it).
import ScratchPaintEditor, { ScratchPaintReducer } from '../vendor/scratch-paint/index.js'
// @ts-expect-error — plain JS (no declaration file), same reasoning as the
// scratch-paint import above.
import { PaintToolPalette } from './PaintToolPalette.jsx'
// @ts-expect-error — no declaration file for this fork.
import paper from '@scratch/paper'

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
  svgRef,
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
  /** Where the latest exported SVG is kept — `App.tsx` owns this too (its
   * own "Save as Resource" handler, triggered from `TopToolbar`, reads it),
   * this component only ever writes to it. */
  svgRef: RefObject<string>
  name: string
  onNameChange: (name: string) => void
  onClose: () => void
}) {
  const store = useMemo(() => createStore(combineReducers({ scratchPaint: ScratchPaintReducer })), [])

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

  return (
    <div className={visible ? 'absolute inset-0 bg-canvas' : 'hidden'}>
      {hasBeenVisible && (
        <PaintErrorBoundary onClose={onClose}>
          <Provider store={store}>
            <IntlProvider locale="en" defaultLocale="en" messages={{}}>
              <ScratchPaintEditor
                imageFormat="svg"
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
                onUpdateImage={(isVector: boolean, image: string) => {
                  if (isVector) svgRef.current = image
                }}
                onUpdateName={onNameChange}
                name={name}
                actionBarTarget={actionBarTarget}
              />
              <PaintToolPalette />
            </IntlProvider>
          </Provider>
        </PaintErrorBoundary>
      )}
    </div>
  )
}
