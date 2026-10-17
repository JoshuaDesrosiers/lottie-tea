import { Select } from './Select'
import { useState } from 'react'
import type { NodeSummary, ProgramDef } from '../ipc/document'
import { generateCode, parseCode } from '../programCodegen'
import { BlockProgramEditor } from './BlockProgramEditor'
import { CodeEditor } from './CodeEditor'
import { fileRef, fileRefPath, listProjectFiles, requestOpenFile, resolveFileRef } from '../game/projectFiles'
import { NodeGraphEditor } from './NodeGraphEditor'

/**
 * The drilled-in view of one Programmable State's `program` — what
 * `StateMachineDock` swaps in for `StateMachineGraph` once a Programmable
 * State's box is double-clicked there (see that file's `onDoubleClick`
 * wiring and the small "▸" badge it draws on any state whose `program`
 * isn't `null`). Takes the whole dock's width/height, the same "a real node
 * canvas needs more room than a side column" reasoning that moved
 * transitions/blend editing out of the Inspector and down into this
 * bottom dock in the first place.
 *
 * Three tabs, three *views* — `ProgramDef` can only actually run as one of
 * `Graph`/`Code`/`Blocks` at a time (see that Rust type's own doc comment),
 * so only one tab's edits are ever "live" at once:
 * - Nodes <-> Code translate into each other instead of discarding, via
 *   `programCodegen.ts`: Nodes -> Code always succeeds (`generateCode`) — a
 *   graph can only ever use vocabulary that already has a Code equivalent,
 *   or a clearly commented placeholder for the handful of things that
 *   don't yet (fill-color properties, state-machine number/bool inputs).
 *   Code -> Nodes (`parseCode`) only succeeds for code that stays within
 *   that same generated vocabulary — hand-written code using arbitrary JS
 *   fails with a specific reason shown inline, leaving the Code tab (and
 *   the program underneath) untouched rather than silently losing it.
 * - Blocks is deliberately *not* part of that translation (Phase 1's build
 *   plan scopes Blocks<->Graph/Code conversion out entirely) — switching
 *   into or out of the Blocks tab never converts anything; it just shows
 *   whichever program `program.kind` already is, same "viewing a
 *   non-matching tab shows a blank default without touching the real
 *   program until you actually edit there" behavior Nodes/Code already
 *   have for each other's kind.
 */
export function ProgramEditorPanel({
  stateName,
  program,
  nodes,
  onChange,
  onBack,
}: {
  stateName: string
  program: ProgramDef | null
  nodes: NodeSummary[]
  onChange: (program: ProgramDef | null) => void
  onBack: () => void
}) {
  const [tab, setTab] = useState<'Nodes' | 'Code' | 'Blocks'>(program?.kind === 'Code' ? 'Code' : program?.kind === 'Blocks' ? 'Blocks' : 'Nodes')
  const [conversionError, setConversionError] = useState<string | null>(null)
  const [incomplete, setIncomplete] = useState(false)

  function selectTab(next: 'Nodes' | 'Code' | 'Blocks') {
    setConversionError(null)
    if (next === 'Code' && program?.kind === 'Graph') {
      const { code, fullyRepresentable } = generateCode(program.value, nodes)
      setIncomplete(!fullyRepresentable)
      onChange({ kind: 'Code', value: code })
    } else if (next === 'Nodes' && program?.kind === 'Code') {
      const result = parseCode(program.value, nodes)
      if ('error' in result) {
        setConversionError(result.error)
        return // stay on the Code tab — the program itself is untouched.
      }
      setIncomplete(false)
      onChange({ kind: 'Graph', value: result.graph })
    }
    setTab(next)
  }

  return (
    <div className="flex h-full flex-col gap-1.5">
      <div className="flex items-center gap-2">
        <button onClick={onBack} className="shrink-0 rounded-md bg-overlay/8 px-2 py-1 text-[11px] text-ink-dim hover:text-ink">
          ← State Machine
        </button>
        <span className="truncate text-xs font-medium text-ink">{stateName}</span>
        <div className="ml-auto flex shrink-0 gap-1">
          <button
            onClick={() => selectTab('Nodes')}
            className={`rounded-md px-2 py-1 text-[11px] ${tab === 'Nodes' ? 'bg-accent/20 text-accent' : 'bg-overlay/8 text-ink-dim hover:text-ink'}`}
          >
            Nodes
          </button>
          <button
            onClick={() => selectTab('Code')}
            className={`rounded-md px-2 py-1 text-[11px] ${tab === 'Code' ? 'bg-accent/20 text-accent' : 'bg-overlay/8 text-ink-dim hover:text-ink'}`}
          >
            Code
          </button>
          <button
            onClick={() => selectTab('Blocks')}
            className={`rounded-md px-2 py-1 text-[11px] ${tab === 'Blocks' ? 'bg-accent/20 text-accent' : 'bg-overlay/8 text-ink-dim hover:text-ink'}`}
          >
            Blocks
          </button>
        </div>
      </div>

      {conversionError && (
        <p className="rounded-md bg-red-500/10 px-2 py-1 text-[11px] text-red-300">
          Can't convert to Nodes: {conversionError}
        </p>
      )}

      {program === null ? (
        <p className="text-[11px] text-ink-dim">This state is no longer a Programmable State.</p>
      ) : tab === 'Nodes' ? (
        <div className="min-h-0 flex-1">
          <NodeGraphEditor
            graph={program.kind === 'Graph' ? program.value : { nodes: [], edges: [] }}
            nodes={nodes}
            onChange={(graph) => onChange({ kind: 'Graph', value: graph })}
          />
        </div>
      ) : tab === 'Blocks' ? (
        <div className="flex min-h-0 flex-1 flex-col gap-1.5">
          <p className="text-[10px] leading-snug text-ink-dim">
            Scratch-style blocks — real Rust, runs everywhere (Playground, the State Machine tab, and any exported/embedded player), same as Nodes.
            Switching here from Nodes/Code doesn't convert anything yet — starts blank until you drop in a "when state triggered" block.
          </p>
          <div className="min-h-0 flex-1 overflow-hidden rounded-md border border-overlay/10">
            <BlockProgramEditor
              key={stateName}
              program={program.kind === 'Blocks' ? program.value : { scripts: [] }}
              nodes={nodes}
              onChange={(value) => onChange({ kind: 'Blocks', value })}
            />
          </div>
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-1.5">
          <p className="text-[10px] leading-snug text-ink-dim">
            Runs only inside this desktop editor (Playground, or the State Machine tab with "Run Programs" on) — a Graph program runs everywhere,
            including any exported/embedded player. Try <code>let square = getCompByID('shape(0)'); square.x = ctx.mouseX</code>.
          </p>
          {incomplete && (
            <p className="rounded-md bg-amber-500/10 px-2 py-1 text-[10px] text-amber-300">
              This graph used something Code can't represent yet (fill-color read/write, or a state-machine input) — shown as a commented 0 below.
            </p>
          )}
          <div className="min-h-0 flex-1 overflow-hidden rounded-md border border-overlay/10">
            <ProgramSource program={program} nodes={nodes} onChange={onChange} />
          </div>
        </div>
      )}
    </div>
  )
}

/** The Code tab's body: the state's code inline, or a project script file
 * it runs (`@file:Scripts/x.js`) — picked from the "Runs" list, edited in
 * the Code tab. */
function ProgramSource({ program, nodes, onChange }: { program: ProgramDef; nodes: NodeSummary[]; onChange: (program: ProgramDef) => void }) {
  const value = program.kind === 'Code' ? program.value : ''
  const path = fileRefPath(value)
  const scripts = listProjectFiles().filter((p) => /\.(m?js)$/i.test(p))
  return (
    <div className="flex h-full flex-col gap-1">
      <div className="flex items-center gap-1.5 text-[11px]">
        <span className="text-ink-dim">Runs</span>
        <Select
          value={path ?? ''}
          onChange={(e) => {
            const next = e.target.value
            if (next) onChange({ kind: 'Code', value: fileRef(next) })
            // Back to inline: start from the file's code so nothing is lost.
            else onChange({ kind: 'Code', value: (path && resolveFileRef(value)) || '' })
          }}
          className="rounded bg-overlay/10 px-1.5 py-0.5 text-ink outline-none"
          title="Inline code, or a script file from the project (Code tab)"
        >
          <option value="">Inline code</option>
          {scripts.map((p) => (
            <option key={p} value={p}>
              {p}
            </option>
          ))}
        </Select>
        {path && (
          <button onClick={() => requestOpenFile(path)} className="rounded bg-accent/20 px-2 py-0.5 text-ink hover:bg-accent/30">
            Edit in Code tab
          </button>
        )}
      </div>
      <div className="min-h-0 flex-1">
        {path ? (
          <CodeEditor value={resolveFileRef(value) ?? `// ${path} is missing`} onChange={() => {}} nodes={nodes} readOnly path={`ref:${path}`} />
        ) : (
          <CodeEditor value={value} onChange={(v) => onChange({ kind: 'Code', value: v })} nodes={nodes} />
        )}
      </div>
    </div>
  )
}
