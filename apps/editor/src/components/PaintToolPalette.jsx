import { useDispatch, useSelector } from 'react-redux'
import { changeMode } from '../vendor/scratch-paint/reducers/modes'
import { changeFormat } from '../vendor/scratch-paint/reducers/format'
import { VectorModes, BitmapModes } from '../vendor/scratch-paint/lib/modes'
import Formats, { isBitmap } from '../vendor/scratch-paint/lib/format'

import selectIcon from '../vendor/scratch-paint/components/select-mode/select.svg'
import reshapeIcon from '../vendor/scratch-paint/components/reshape-mode/reshape.svg'
import brushIcon from '../vendor/scratch-paint/components/brush-mode/brush.svg'
import eraserIcon from '../vendor/scratch-paint/components/eraser-mode/eraser.svg'
import fillIcon from '../vendor/scratch-paint/components/fill-mode/fill.svg'
import textIcon from '../vendor/scratch-paint/components/text-mode/text.svg'
import lineIcon from '../vendor/scratch-paint/components/line-mode/line.svg'
import ovalIcon from '../vendor/scratch-paint/components/oval-mode/oval.svg'
import rectIcon from '../vendor/scratch-paint/components/rect-mode/rectangle.svg'

const VECTOR_TOOLS = [
  { mode: VectorModes.SELECT, icon: selectIcon, label: 'Select' },
  { mode: VectorModes.RESHAPE, icon: reshapeIcon, label: 'Reshape' },
  { mode: VectorModes.BRUSH, icon: brushIcon, label: 'Brush' },
  { mode: VectorModes.ERASER, icon: eraserIcon, label: 'Eraser' },
  { mode: VectorModes.FILL, icon: fillIcon, label: 'Fill' },
  { mode: VectorModes.TEXT, icon: textIcon, label: 'Text' },
  { mode: VectorModes.LINE, icon: lineIcon, label: 'Line' },
  { mode: VectorModes.OVAL, icon: ovalIcon, label: 'Oval' },
  { mode: VectorModes.RECT, icon: rectIcon, label: 'Rectangle' },
]

const BITMAP_TOOLS = [
  { mode: BitmapModes.BIT_SELECT, icon: selectIcon, label: 'Select' },
  { mode: BitmapModes.BIT_BRUSH, icon: brushIcon, label: 'Brush' },
  { mode: BitmapModes.BIT_ERASER, icon: eraserIcon, label: 'Eraser' },
  { mode: BitmapModes.BIT_FILL, icon: fillIcon, label: 'Fill' },
  { mode: BitmapModes.BIT_TEXT, icon: textIcon, label: 'Text' },
  { mode: BitmapModes.BIT_LINE, icon: lineIcon, label: 'Line' },
  { mode: BitmapModes.BIT_OVAL, icon: ovalIcon, label: 'Oval' },
  { mode: BitmapModes.BIT_RECT, icon: rectIcon, label: 'Rectangle' },
]

/**
 * an1mbyt3's own tool-switcher for Paint Asset Mode — real buttons, our own
 * Tailwind styling (matching `IconRail`'s `RailButton` look), dispatching
 * `changeMode(...)` directly against the same scratch-paint Redux store
 * `PaintAssetEditor.tsx` creates. This *replaces* scratch-paint's own
 * `modeSelector` icon strip (removed from `paint-editor.jsx`'s layout
 * entirely) — but the actual tool *containers* (`SelectMode`/`BrushMode`/
 * etc.) are still mounted there (invisibly): each one's own
 * `componentDidMount`/`componentWillReceiveProps` is what activates the
 * real paper.js `Tool` for whichever mode `state.scratchPaint.mode` names,
 * reacting to the Redux state regardless of what changed it — this palette
 * only ever needs to dispatch the same action they're already listening
 * for, never touch paper.js directly itself.
 */
export function PaintToolPalette() {
  const dispatch = useDispatch()
  const mode = useSelector((state) => state.scratchPaint.mode)
  const format = useSelector((state) => state.scratchPaint.format)
  const tools = isBitmap(format) ? BITMAP_TOOLS : VECTOR_TOOLS

  return (
    <div className="absolute left-2 top-1/2 z-10 flex -translate-y-1/2 flex-col gap-1 rounded-lg border border-panel-border bg-panel p-1.5 backdrop-blur-panel">
      {tools.map((tool) => (
        <button
          key={tool.mode}
          title={tool.label}
          onClick={() => dispatch(changeMode(tool.mode))}
          className={`flex h-9 w-9 items-center justify-center rounded-md transition-colors ${
            mode === tool.mode ? 'bg-accent/15' : 'hover:bg-overlay/5'
          }`}
        >
          {/* These icon files are plain SVGs with a hardcoded dark fill
              (`#575E75`, scratch-paint's own original on-light-background
              convention) baked into the file itself — not a `currentColor`
              stroke a CSS `color` could recolor, and not controlled by the
              `$variable` reskin in `css/colors.module.css` either, since
              that only touches the vendored *stylesheets*. `brightness(0)
              invert(N)` recolors any icon color to solid black (`N: 0`) or
              white (`N: 1`), the standard CSS trick for recoloring a
              fixed-color image icon that can't otherwise be retinted —
              `--icon-invert` (`index.css`) is `1` in dark mode (this app's
              original, dark-mode-only assumption) and `0` in light, so this
              stays visible against either panel instead of staying
              hardcoded white and washing out on a light one. */}
          <img src={tool.icon} draggable={false} className="h-5 w-5" style={{ filter: 'brightness(0) invert(var(--icon-invert))', opacity: mode === tool.mode ? 1 : 0.6 }} />
        </button>
      ))}
      <div className="my-1 h-px w-full bg-panel-border" />
      {/* Vector<->Bitmap format toggle — a plain `changeFormat` dispatch is
          enough: `containers/paint-editor.jsx`'s own `componentDidUpdate`
          (unrelated to this component, already wired) reacts to
          `state.scratchPaint.format` changing regardless of what dispatched
          it, and runs the actual raster<->vector conversion
          (`convertToVector`/`convertToBitmap`) itself. */}
      <button
        title={isBitmap(format) ? 'Convert to Vector' : 'Convert to Bitmap'}
        onClick={() => dispatch(changeFormat(isBitmap(format) ? Formats.VECTOR : Formats.BITMAP))}
        className="flex h-9 w-9 items-center justify-center rounded-md text-[10px] font-semibold text-ink-dim transition-colors hover:bg-overlay/5 hover:text-ink"
      >
        {isBitmap(format) ? 'VEC' : 'BMP'}
      </button>
    </div>
  )
}
