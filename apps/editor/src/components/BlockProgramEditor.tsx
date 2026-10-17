import { useEffect, useRef, useState } from 'react'
import * as Blockly from 'blockly'
import type { BlockProgramDef, NodeSummary } from '../ipc/document'
import { compileWorkspace, defineGameBlocks, GAME_THEME, GAME_TOOLBOX, loadProgramIntoWorkspace, setNodeOptions } from '../blocks/gameBlocks'

/**
 * The "Blocks" tab of `ProgramEditorPanel` — a Scratch-style visual editor
 * for a Programmable State's `ProgramDef::Blocks` program, backed by
 * Blockly (`apps/editor/src/blocks/gameBlocks.ts` owns the actual block
 * definitions and the compiler/decompiler between Blockly's workspace and
 * `BlockProgramDef`).
 *
 * Unlike `NodeGraphEditor` (a plain, fully-controlled SVG view that
 * re-renders straight from its `graph` prop on every change), Blockly is an
 * imperative, stateful widget — it owns its own DOM/SVG tree, undo stack,
 * scroll position, and in-progress drag state. Re-deriving its contents
 * from a `program` prop on every parent re-render (the way `NodeGraphEditor`
 * does) would be actively destructive here — same "don't fight the
 * library's own state" reasoning `CodeEditor.tsx`'s Monaco wrapper already
 * follows for the same class of problem. So this component:
 * - injects the workspace and loads `program` into it exactly once, on
 *   mount (`useEffect` with an empty dependency array);
 * - pushes every local edit up via `onChange` (debounced — a single block
 *   drag fires many granular Blockly events, and firing a full document
 *   update on each one would thrash `setDoc` for no benefit);
 * - never re-loads `program` back into an already-mounted workspace.
 *
 * The consequence: if the *caller* swaps in a genuinely different program
 * (the user selects a different Programmable State while this tab is open)
 * without remounting this component, the workspace would keep showing the
 * *previous* state's blocks. `ProgramEditorPanel` avoids this by mounting
 * this component with `key={stateName}` — a state switch is a fresh
 * mount, not a prop update, the same forced-remount pattern React's own
 * docs recommend for resetting a component's internal/imperative state on
 * identity change.
 */
export function BlockProgramEditor({ program, nodes, onChange }: { program: BlockProgramDef; nodes: NodeSummary[]; onChange: (program: BlockProgramDef) => void }) {
  const containerRef = useRef<HTMLDivElement>(null)
  const workspaceRef = useRef<Blockly.WorkspaceSvg | null>(null)
  // Read inside the change listener via a ref (not a dependency) so the
  // listener registered once at mount always calls the *latest* `onChange`
  // without needing to re-inject Blockly whenever the parent passes a new
  // function identity.
  const onChangeRef = useRef(onChange)
  onChangeRef.current = onChange

  // Not a block — a per-program setting for how often its scripts actually
  // tick, independent of however fast the display redraws (see
  // `BlockProgramDef.ticksPerSecond`'s own doc comment). Read once on mount
  // (this component's own "load once, never re-sync from props" convention
  // — see the doc comment above), tracked in a ref so the debounced change
  // listener and the flush-on-unmount below always send the *current* value
  // without needing to re-inject Blockly when it changes.
  const [ticksPerSecond, setTicksPerSecond] = useState<number | null>(program.ticksPerSecond ?? null)
  const ticksPerSecondRef = useRef(ticksPerSecond)
  ticksPerSecondRef.current = ticksPerSecond

  useEffect(() => {
    setNodeOptions(nodes.map((n) => ({ id: n.id, name: n.name })))
  }, [nodes])

  useEffect(() => {
    if (!containerRef.current) return
    defineGameBlocks()
    // Blockly's default scrollbar is a thick (~15px), fully-opaque bar —
    // this app's own scrollbars everywhere else are a thin 10px translucent
    // strip (`index.css`'s `::-webkit-scrollbar-thumb`). `scrollbarThickness`
    // is a Blockly-wide static, not a per-workspace/theme option, so it has
    // to be set before `inject` (harmless to set repeatedly on remount).
    Blockly.Scrollbar.scrollbarThickness = 8
    const workspace = Blockly.inject(containerRef.current, {
      toolbox: GAME_TOOLBOX,
      theme: GAME_THEME,
      // Scratch's own renderer — rounded "pill" reporter/value blocks
      // instead of the classic jagged puzzle-piece notch shape `geras`
      // (Blockly's default) draws.
      renderer: 'zelos',
      trashcan: true,
      zoom: { controls: true, wheel: true },
      // No click/drag/delete sound effects — this editor doesn't play UI
      // sounds anywhere else, and Blockly's defaults would be the one
      // exception.
      sounds: false,
    })
    workspaceRef.current = workspace
    // Dropdowns (node / property pickers, key lists…) are bounded by the
    // workspace's own box by default, so in a small Blocks panel they got
    // squashed or pushed off-screen and you had to move the canvas to
    // reach them. Bound them by the whole window instead: Blockly then
    // places them above or below the field, wherever there's room.
    Blockly.DropDownDiv.setBoundsElement(document.body)
    loadProgramIntoWorkspace(workspace, program)

    // The flyout (the popup list of draggable blocks a toolbox category
    // opens) mirrors the *main* workspace's zoom by default — dragging a
    // block out after zooming the canvas in/out otherwise makes the
    // palette's own blocks visibly grow/shrink along with it, when they
    // should read as a constant-size reference palette regardless of
    // canvas zoom (matching Scratch's own palette, and this app's other
    // panels, none of which scale with canvas zoom either). No public
    // option for this — the standard override is replacing the flyout's
    // own scale getter with a constant.
    const flyout = workspace.getFlyout() as Blockly.Flyout | null
    if (flyout) flyout.getFlyoutScale = () => 1

    let debounce: ReturnType<typeof setTimeout> | null = null
    const handleChange = (event: Blockly.Events.Abstract) => {
      if (event.isUiEvent) return
      if (debounce) clearTimeout(debounce)
      debounce = setTimeout(() => onChangeRef.current({ ...compileWorkspace(workspace), ticksPerSecond: ticksPerSecondRef.current }), 250)
    }
    workspace.addChangeListener(handleChange)

    return () => {
      // Flush (not just cancel) any pending debounced edit before tearing
      // down — this is the fix for blocks "disappearing" after switching
      // tabs repeatedly: switching to Nodes/Code unmounts this component
      // (`ProgramEditorPanel`'s tab render is a plain conditional, not a
      // hide/show), and canceling a debounce without flushing silently
      // dropped whatever edit hadn't hit the 250ms trailing edge yet —
      // switch away quickly enough after dragging a block in and that edit
      // (sometimes the only one so far) never reached `program` at all, so
      // reopening Blocks loaded the stale, edit-less program instead.
      if (debounce) clearTimeout(debounce)
      onChangeRef.current({ ...compileWorkspace(workspace), ticksPerSecond: ticksPerSecondRef.current })
      workspace.removeChangeListener(handleChange)
      workspace.dispose()
      workspaceRef.current = null
    }
    // Mount-only — see this component's own doc comment for why `program`
    // is deliberately not a dependency here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // The ticks/sec field isn't a Blockly event, so it needs its own path to
  // `onChange` (the debounced Blockly listener above never fires for it).
  const handleTicksPerSecondChange = (raw: string) => {
    const value = raw.trim() === '' ? null : Math.max(0, Number(raw))
    const next = value !== null && Number.isFinite(value) ? value : null
    setTicksPerSecond(next)
    if (workspaceRef.current) onChangeRef.current({ ...compileWorkspace(workspaceRef.current), ticksPerSecond: next })
  }

  return (
    <div className="relative h-full w-full">
      {/* Blockly's built-in zoom controls (+/-/reset) and trashcan render at
          a fixed size baked into its own core CSS — noticeably larger than
          every other icon button in this app (`components/icons.tsx`'s own
          16-20px convention). There's no public "size" option for them, so
          this shrinks them the standard way Blockly integrators do: a CSS
          transform on their (stable, documented) class names, scoped to
          just this component via the wrapping div rather than a global
          stylesheet change. `transform-box: fill-box` makes the `center`
          origin resolve against each control's own SVG bounding box (the
          default SVG transform-origin behaves like `0 0` otherwise, which
          would shrink them *and* fling them toward the corner). */}
      <style>{`
        .blocklyZoom, .blocklyTrash {
          transform: scale(0.65);
          transform-box: fill-box;
          transform-origin: center;
        }
        /* "Tinted glass" block look — a slightly translucent fill (so the
           dark workspace behind shows through a little) plus a soft, low
           blur shadow instead of Blockly's default flat, fully-opaque
           block fill. Excludes the shadow-block variant (\`an1_number\`'s
           default-value pills, see gameBlocks.ts's own doc comment on
           shadow blocks) — those already read as visually "recessed"
           by Blockly's own shadow styling, and stacking this glass
           treatment on top of that made them look washed out. */
        .blocklyDraggable:not(.blocklyShadow) > .blocklyPath {
          fill-opacity: 0.88;
          filter: drop-shadow(0 1px 3px rgba(0, 0, 0, 0.35));
        }
      `}</style>
      {/* Program-level setting, not a block — how often this program's own
          scripts tick, independent of the render frame rate. Blank = every
          render frame (uncapped), matching this field's day-one absence. */}
      <div className="absolute right-2 top-2 z-10 flex items-center gap-1.5 rounded-md border border-overlay/10 bg-black/40 px-2 py-1 text-xs text-overlay/70 backdrop-blur-sm">
        <label htmlFor="an1-ticks-per-second">Ticks/sec</label>
        <input
          id="an1-ticks-per-second"
          type="number"
          min={0}
          step={1}
          placeholder="uncapped"
          value={ticksPerSecond ?? ''}
          onChange={(e) => handleTicksPerSecondChange(e.target.value)}
          className="w-16 rounded border border-overlay/10 bg-overlay/5 px-1 py-0.5 text-white outline-none focus:border-overlay/30"
        />
      </div>
      <div ref={containerRef} className="h-full w-full" />
    </div>
  )
}
