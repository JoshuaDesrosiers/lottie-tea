import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { showConfirm } from './Dialogs'
import type { ArtboardSummary, DocumentSummary, NodeSummary, OpaqueId, ProgramDef, TextFileSummary } from '../ipc/document'
import { addTextFile, deleteTextFile, renameTextFile, setStateProgram, setTextFileText, type WasmEditor } from '../ipc/wasmEditor'
import { fileRef, fileRefPath, patchProjectFile } from '../game/projectFiles'
import type { ConsoleOrigin } from '../game/console'
import { CodeEditor, languageForExtension, type CodeEditorInstance } from './CodeEditor'
import { ConsolePanel } from './ConsolePanel'

/**
 * The **Code** mode: an IDE for the project's code.
 *
 * - **Explorer** (left): the project's *Files* — HTML, CSS, JS… (`TextFile`
 *   assets, real files under `Assets/Code/` in a project folder) as a folder
 *   tree with New / Rename / Delete — and every *Scene script*: each
 *   Programmable State's Code, grouped by scene › state machine › layer.
 * - **Editor** (centre): Monaco tabs, one per open file / script, each with
 *   its own undo history; HTML / CSS / JS highlighting, the scripting
 *   completions for JS. Edits apply as you type (HTML UI using a file
 *   re-renders live; scripts take effect on the next frame in Playground).
 * - **Console** (bottom): the game console; clicking an error's location
 *   jumps to that line.
 *
 * Files are wired into the game by reference: an HTML UI component's HTML /
 * CSS, or a state's Code, can be `@file:<path>` (the Inspector / script
 * editor offer the files to pick from).
 */

/** Where to open: a file (by path) or a state's script, optionally at a line. */
export type CodeLocation =
  | { kind: 'file'; path: string; line?: number }
  | { kind: 'state'; scene: string; layer: string | null; state: string; line?: number; column?: number }

type Tab =
  | { kind: 'file'; key: string; file: OpaqueId }
  | { kind: 'state'; key: string; artboard: OpaqueId; stateMachine: OpaqueId; layer: string | null; state: string }

const idKey = (id: OpaqueId) => `${id.idx}.${id.version}`
const sameId = (a: OpaqueId, b: OpaqueId) => a.idx === b.idx && a.version === b.version

const TEMPLATES: Record<string, string> = {
  html: '<div class="panel">\n  <h1>Hello!</h1>\n  <p>Edit me in the Code tab.</p>\n  <button data-trigger="start">Start</button>\n</div>\n',
  css: '.panel {\n  box-sizing: border-box;\n  width: 100%;\n  height: 100%;\n  padding: 20px;\n  border-radius: 16px;\n  font-family: system-ui, sans-serif;\n  color: #f4f1ff;\n  background: #2a2340;\n}\n',
  js: "// A script file. Run it from a state: set that state's Code to\n//   @file:<this file's path>\n// (or pick it in the state's code editor).\n\nconst ref = (id) => getCompByID(id)\n",
}

interface StateEntry {
  artboard: ArtboardSummary
  stateMachine: { id: OpaqueId; name: string }
  layer: string | null
  state: string
  program: ProgramDef | null
}

/** Every state with a program, across every scene. */
function sceneScripts(doc: DocumentSummary | null): StateEntry[] {
  const out: StateEntry[] = []
  for (const artboard of doc?.artboards ?? []) {
    for (const sm of artboard.stateMachines ?? []) {
      for (const s of sm.states) if (s.program) out.push({ artboard, stateMachine: sm, layer: null, state: s.name, program: s.program })
      for (const layer of sm.layers ?? []) for (const s of layer.states) if (s.program) out.push({ artboard, stateMachine: sm, layer: layer.name, state: s.name, program: s.program })
    }
  }
  return out
}

function stateTabKey(artboard: OpaqueId, sm: OpaqueId, layer: string | null, state: string) {
  return `state:${idKey(artboard)}|${idKey(sm)}|${layer ?? ''}|${state}`
}

/** A folder tree of file paths. */
interface Folder {
  name: string
  path: string
  folders: Folder[]
  files: TextFileSummary[]
}
function buildTree(files: TextFileSummary[]): Folder {
  const root: Folder = { name: '', path: '', folders: [], files: [] }
  for (const f of files) {
    const parts = f.path.split('/')
    let at = root
    for (const part of parts.slice(0, -1)) {
      let next = at.folders.find((d) => d.name === part)
      if (!next) {
        next = { name: part, path: at.path ? `${at.path}/${part}` : part, folders: [], files: [] }
        at.folders.push(next)
      }
      at = next
    }
    at.files.push(f)
  }
  const sort = (d: Folder) => {
    d.folders.sort((a, b) => a.name.localeCompare(b.name))
    d.folders.forEach(sort)
  }
  sort(root)
  return root
}

const EXT_COLOR: Record<string, string> = { html: 'text-orange-400', css: 'text-sky-400', js: 'text-yellow-300', json: 'text-emerald-300' }
function FileIcon({ ext }: { ext: string }) {
  return <span className={`w-7 shrink-0 font-mono text-[9px] font-bold uppercase ${EXT_COLOR[ext] ?? 'text-ink-dim'}`}>{ext || 'txt'}</span>
}

export function CodeWorkspace({
  editor,
  doc,
  nodes,
  theme,
  request,
  onDocumentChange,
  onRun,
}: {
  editor: WasmEditor | null
  doc: DocumentSummary | null
  nodes: NodeSummary[]
  theme: 'dark' | 'light'
  /** Open this (a Console error's location, an Inspector "Edit file"). */
  request: (CodeLocation & { nonce: number }) | null
  onDocumentChange: (doc: DocumentSummary) => void
  onRun: () => void
}) {
  const files = doc?.textFiles ?? []
  const scripts = useMemo(() => sceneScripts(doc), [doc])
  const [tabs, setTabs] = useState<Tab[]>([])
  const [active, setActive] = useState<string | null>(null)
  const [creating, setCreating] = useState<string | null>(null)
  const [renaming, setRenaming] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [cursor, setCursor] = useState({ line: 1, column: 1 })
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set())
  const editorRef = useRef<CodeEditorInstance | null>(null)
  const pendingLine = useRef<{ key: string; line: number; column?: number } | null>(null)
  // Script edits are committed (one undo step) after a short pause.
  const scriptDrafts = useRef(new Map<string, { tab: Tab & { kind: 'state' }; text: string; timer: number }>())
  const refreshTimer = useRef<number | null>(null)

  const findFile = (id: OpaqueId) => files.find((f) => sameId(f.id, id))
  const findState = (tab: Tab & { kind: 'state' }) => scripts.find((s) => sameId(s.artboard.id, tab.artboard) && sameId(s.stateMachine.id, tab.stateMachine) && s.layer === tab.layer && s.state === tab.state)

  const open = (tab: Tab, at?: { line?: number; column?: number }) => {
    setTabs((ts) => (ts.some((t) => t.key === tab.key) ? ts : [...ts, tab]))
    setActive(tab.key)
    if (at?.line) pendingLine.current = { key: tab.key, line: at.line, column: at.column }
  }
  const openFile = (f: TextFileSummary, line?: number) => open({ kind: 'file', key: `file:${idKey(f.id)}`, file: f.id }, { line })
  const openState = (s: StateEntry, at?: { line?: number; column?: number }) =>
    open({ kind: 'state', key: stateTabKey(s.artboard.id, s.stateMachine.id, s.layer, s.state), artboard: s.artboard.id, stateMachine: s.stateMachine.id, layer: s.layer, state: s.state }, at)

  // Outside requests (Console links, Inspector "Edit file").
  useEffect(() => {
    if (!request) return
    if (request.kind === 'file') {
      const f = files.find((x) => x.path === request.path)
      if (f) openFile(f, request.line)
    } else {
      const s = scripts.find((x) => x.artboard.name === request.scene && x.layer === (request.layer ?? null) && x.state === request.state)
      if (s) {
        // A state whose code lives in a file opens the file.
        const path = s.program?.kind === 'Code' ? fileRefPath(s.program.value) : null
        const f = path ? files.find((x) => x.path === path) : null
        if (f) openFile(f, request.line)
        else openState(s, { line: request.line, column: request.column })
      }
    }
  }, [request?.nonce]) // eslint-disable-line react-hooks/exhaustive-deps

  // Drop tabs whose file / state is gone.
  useEffect(() => {
    setTabs((ts) => ts.filter((t) => (t.kind === 'file' ? !!findFile(t.file) : !!findState(t))))
  }, [doc]) // eslint-disable-line react-hooks/exhaustive-deps

  // Pending script edits are committed when leaving.
  useEffect(() => () => flushScripts(), []) // eslint-disable-line react-hooks/exhaustive-deps

  const activeTab = tabs.find((t) => t.key === active) ?? tabs[tabs.length - 1] ?? null

  function flushScripts() {
    for (const [key, d] of scriptDrafts.current) {
      window.clearTimeout(d.timer)
      commitScript(d.tab, d.text)
      scriptDrafts.current.delete(key)
    }
  }

  function commitScript(tab: Tab & { kind: 'state' }, text: string) {
    if (!editor) return
    try {
      onDocumentChange(setStateProgram(editor, tab.artboard, tab.stateMachine, tab.state, { kind: 'Code', value: text }, tab.layer))
    } catch (e) {
      setError(String(e))
    }
  }

  const refreshSoon = () => {
    if (refreshTimer.current) window.clearTimeout(refreshTimer.current)
    refreshTimer.current = window.setTimeout(() => {
      refreshTimer.current = null
      if (editor) onDocumentChange(editor.document_summary() as DocumentSummary)
    }, 700)
  }

  const onEdit = (tab: Tab, text: string) => {
    if (!editor) return
    if (tab.kind === 'file') {
      const f = findFile(tab.file)
      if (!f) return
      setTextFileText(editor, tab.file, text)
      patchProjectFile(f.path, text)
      refreshSoon()
    } else {
      const prev = scriptDrafts.current.get(tab.key)
      if (prev) window.clearTimeout(prev.timer)
      const timer = window.setTimeout(() => {
        scriptDrafts.current.delete(tab.key)
        commitScript(tab, text)
      }, 600)
      scriptDrafts.current.set(tab.key, { tab, text, timer })
    }
  }

  const createFile = (path: string) => {
    if (!editor || !path.trim()) return
    const ext = path.split('.').pop()?.toLowerCase() ?? ''
    const next = addTextFile(editor, path, TEMPLATES[ext] ?? '')
    onDocumentChange(next)
    const made = next.textFiles.find((f) => !files.some((g) => sameId(g.id, f.id)))
    if (made) openFile(made)
  }

  const rename = (f: TextFileSummary, path: string) => {
    if (!editor) return
    try {
      onDocumentChange(renameTextFile(editor, f.id, path))
      setError(null)
    } catch (e) {
      setError(String(e))
    }
  }

  /** Removes a state's program (the state goes back to just playing its
   * animation). Undo brings it back. */
  const removeScript = async (entry: StateEntry) => {
    if (!editor) return
    const where = `${entry.artboard.name} › ${entry.stateMachine.name}${entry.layer ? ` › ${entry.layer}` : ''} › ${entry.state}`
    if (!(await showConfirm(`Delete the code in ${where}? The state keeps playing its animation. (Undo brings it back.)`, { title: 'Delete script', okLabel: 'Delete', danger: true }))) return
    const key = stateTabKey(entry.artboard.id, entry.stateMachine.id, entry.layer, entry.state)
    const draft = scriptDrafts.current.get(key)
    if (draft) window.clearTimeout(draft.timer)
    scriptDrafts.current.delete(key)
    setTabs((ts) => ts.filter((t) => t.key !== key))
    onDocumentChange(setStateProgram(editor, entry.artboard.id, entry.stateMachine.id, entry.state, null, entry.layer))
  }

  const remove = async (f: TextFileSummary) => {
    if (!editor) return
    if (!(await showConfirm(`Delete ${f.path}? (Undo brings it back.)`, { title: 'Delete file', okLabel: 'Delete', danger: true }))) return
    onDocumentChange(deleteTextFile(editor, f.id))
  }

  // ---- the active tab's content ----
  let content = ''
  let language = 'javascript'
  let title = ''
  let readOnlyNote: string | null = null
  if (activeTab?.kind === 'file') {
    const f = findFile(activeTab.file)
    content = f?.text ?? ''
    language = languageForExtension(f?.extension ?? '')
    title = f?.path ?? ''
  } else if (activeTab?.kind === 'state') {
    const s = findState(activeTab)
    const draft = scriptDrafts.current.get(activeTab.key)
    if (s?.program?.kind === 'Code') {
      const path = fileRefPath(s.program.value)
      if (path) readOnlyNote = `This state runs ${path}. Edit that file instead.`
      content = draft?.text ?? s.program.value
    } else if (s?.program) {
      readOnlyNote = `This state is built with ${s.program.kind === 'Graph' ? 'Nodes' : 'Blocks'} — switch it to Code in the State Machine tab to edit it here.`
      content = ''
    }
    title = s ? `${s.artboard.name} › ${s.layer ? `${s.layer} › ` : ''}${s.state}` : ''
  }

  const onMount = (ed: CodeEditorInstance) => {
    editorRef.current = ed
    ed.onDidChangeCursorPosition((e) => setCursor({ line: e.position.lineNumber, column: e.position.column }))
  }
  // Jump to a requested line once that tab's text is showing.
  useEffect(() => {
    const p = pendingLine.current
    const ed = editorRef.current
    if (!p || !ed || p.key !== activeTab?.key) return
    pendingLine.current = null
    window.setTimeout(() => {
      ed.revealLineInCenter(p.line)
      ed.setSelection({ startLineNumber: p.line, startColumn: 1, endLineNumber: p.line, endColumn: ed.getModel()?.getLineMaxColumn(p.line) ?? 1 })
      ed.focus()
    }, 30)
  })

  const tree = buildTree(files)
  const toggle = (path: string) => setCollapsed((c) => {
    const n = new Set(c)
    if (n.has(path)) n.delete(path)
    else n.add(path)
    return n
  })

  const renderFolder = (folder: Folder, depth: number): ReactNode => (
    <>
      {folder.folders.map((d) => (
        <div key={d.path}>
          <button onClick={() => toggle(d.path)} className="flex w-full items-center gap-1 rounded px-1 py-0.5 text-left text-ink-dim hover:bg-overlay/10 hover:text-ink" style={{ paddingLeft: 4 + depth * 12 }}>
            <span className="w-3 text-[9px]">{collapsed.has(d.path) ? '▸' : '▾'}</span>
            <span className="truncate">{d.name}</span>
          </button>
          {!collapsed.has(d.path) && renderFolder(d, depth + 1)}
        </div>
      ))}
      {folder.files.map((f) => {
        const key = `file:${idKey(f.id)}`
        const name = f.path.split('/').pop() ?? f.path
        return (
          <div key={key} className={`group flex items-center gap-1 rounded pr-1 ${activeTab?.key === key ? 'bg-accent/20 text-ink' : 'text-ink-dim hover:bg-overlay/10 hover:text-ink'}`} style={{ paddingLeft: 4 + depth * 12 + 12 }}>
            {renaming === key ? (
              <input
                autoFocus
                defaultValue={f.path}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    rename(f, e.currentTarget.value)
                    setRenaming(null)
                  } else if (e.key === 'Escape') setRenaming(null)
                }}
                onBlur={() => setRenaming(null)}
                className="min-w-0 flex-1 rounded bg-overlay/15 px-1 py-0.5 text-ink outline-none"
              />
            ) : (
              <button onClick={() => openFile(f)} onDoubleClick={() => setRenaming(key)} className="flex min-w-0 flex-1 items-center py-0.5 text-left" title={`${f.path} — double-click to rename`}>
                <FileIcon ext={f.extension} />
                <span className="truncate">{name}</span>
              </button>
            )}
            <button onClick={() => navigator.clipboard?.writeText(fileRef(f.path))} title={`Copy reference (${fileRef(f.path)})`} className="hidden rounded px-1 text-[10px] group-hover:block hover:bg-overlay/15">
              @
            </button>
            <button onClick={() => setRenaming(key)} title="Rename / move" className="hidden rounded px-1 text-[10px] group-hover:block hover:bg-overlay/15">
              ✎
            </button>
            <button onClick={() => remove(f)} title="Delete" className="hidden rounded px-1 text-[10px] text-red-400 group-hover:block hover:bg-overlay/15">
              ✕
            </button>
          </div>
        )
      })}
    </>
  )

  return (
    <div className="flex h-full min-h-0 gap-2 text-[11px]">
      {/* Explorer */}
      <div className="flex w-60 shrink-0 flex-col overflow-hidden rounded-md border border-panel-border bg-panel">
        <div className="flex items-center justify-between border-b border-panel-border px-2 py-1.5">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-ink-dim">Files</span>
          <div className="flex gap-0.5">
            {(['html', 'css', 'js'] as const).map((ext) => (
              <button key={ext} onClick={() => setCreating(`${ext === 'js' ? 'Scripts' : 'UI'}/new.${ext}`)} title={`New .${ext} file`} className="rounded px-1.5 py-0.5 font-mono text-[9px] uppercase text-ink-dim hover:bg-overlay/10 hover:text-ink">
                +{ext}
              </button>
            ))}
          </div>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-1">
          {creating !== null && (
            <input
              autoFocus
              defaultValue={creating}
              onFocus={(e) => {
                const v = e.currentTarget.value
                const slash = v.lastIndexOf('/') + 1
                e.currentTarget.setSelectionRange(slash, v.lastIndexOf('.') > slash ? v.lastIndexOf('.') : v.length)
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  createFile(e.currentTarget.value)
                  setCreating(null)
                } else if (e.key === 'Escape') setCreating(null)
              }}
              onBlur={() => setCreating(null)}
              placeholder="Folder/name.html"
              className="mb-1 w-full rounded bg-overlay/15 px-2 py-1 text-ink outline-none"
            />
          )}
          {files.length === 0 && creating === null && <div className="px-2 py-1 text-ink-dim">No files yet — make one with +html / +css / +js.</div>}
          {renderFolder(tree, 0)}
          <div className="mt-3 border-t border-panel-border px-1 pt-2 text-[10px] font-semibold uppercase tracking-wider text-ink-dim">Scene scripts</div>
          {scripts.length === 0 && <div className="px-2 py-1 text-ink-dim">No Programmable States yet.</div>}
          {scripts.map((s) => {
            const key = stateTabKey(s.artboard.id, s.stateMachine.id, s.layer, s.state)
            const kind = s.program?.kind === 'Code' ? (fileRefPath(s.program.value) ? 'file' : 'js') : s.program?.kind === 'Graph' ? 'nodes' : 'blocks'
            return (
              <div key={key} className={`group flex w-full items-center gap-1 rounded px-1 py-0.5 ${activeTab?.key === key ? 'bg-accent/20 text-ink' : 'text-ink-dim hover:bg-overlay/10 hover:text-ink'}`}>
                <button onClick={() => openState(s)} className="flex min-w-0 flex-1 items-center gap-1 text-left" title={`${s.artboard.name} › ${s.stateMachine.name}${s.layer ? ` › ${s.layer}` : ''} › ${s.state}`}>
                  <FileIcon ext={kind === 'js' ? 'js' : kind} />
                  <span className="truncate">
                    <span className="opacity-60">{s.artboard.name} › </span>
                    {s.layer ? `${s.layer} › ` : ''}
                    {s.state}
                  </span>
                </button>
                <button onClick={() => removeScript(s)} title="Delete this state's code" className="hidden shrink-0 rounded px-1 text-[10px] text-red-400 group-hover:block hover:bg-overlay/15">
                  ✕
                </button>
              </div>
            )
          })}
        </div>
      </div>

      {/* Editor + Console */}
      <div className="flex min-w-0 flex-1 flex-col gap-2">
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-md border border-panel-border bg-panel">
          <div className="flex items-center border-b border-panel-border">
            <div className="flex min-w-0 flex-1 overflow-x-auto">
              {tabs.map((t) => {
                const label = t.kind === 'file' ? (findFile(t.file)?.path.split('/').pop() ?? '?') : t.state
                return (
                  <div key={t.key} className={`group flex shrink-0 items-center gap-1 border-r border-panel-border px-2 py-1.5 ${activeTab?.key === t.key ? 'bg-overlay/10 text-ink' : 'text-ink-dim hover:text-ink'}`}>
                    <button onClick={() => setActive(t.key)} className="max-w-[160px] truncate">
                      {t.kind === 'state' && <span className="mr-1 text-yellow-300">ƒ</span>}
                      {label}
                    </button>
                    <button
                      onClick={() => {
                        if (t.kind === 'state' && scriptDrafts.current.has(t.key)) flushScripts()
                        setTabs((ts) => ts.filter((x) => x.key !== t.key))
                      }}
                      className="rounded px-0.5 opacity-50 hover:bg-overlay/15 hover:opacity-100"
                      title="Close"
                    >
                      ✕
                    </button>
                  </div>
                )
              })}
            </div>
            <button onClick={() => { flushScripts(); onRun() }} className="m-1 shrink-0 rounded bg-accent px-3 py-1 text-[11px] font-semibold text-white hover:brightness-110" title="Play the game">
              ▶ Play
            </button>
          </div>
          {error && (
            <div className="flex items-center justify-between bg-red-500/15 px-2 py-1 text-red-300">
              {error}
              <button onClick={() => setError(null)}>✕</button>
            </div>
          )}
          {readOnlyNote && <div className="bg-amber-400/10 px-2 py-1 text-amber-200">{readOnlyNote}</div>}
          <div className="min-h-0 flex-1">
            {activeTab ? (
              <CodeEditor
                key={activeTab.key}
                path={activeTab.key}
                value={content}
                language={language}
                theme={theme}
                nodes={nodes}
                readOnly={!!readOnlyNote}
                onChange={(text) => onEdit(activeTab, text)}
                onEditorMount={onMount}
              />
            ) : (
              <div className="flex h-full flex-col items-center justify-center gap-2 text-ink-dim">
                <div className="text-[13px] text-ink">Open a file or a scene script from the Explorer.</div>
                <div>HTML UI can use a file: Inspector → HTML UI → Source → File.</div>
                <div>A state can run a file: set its Code to @file:Scripts/name.js.</div>
              </div>
            )}
          </div>
          <div className="flex items-center gap-3 border-t border-panel-border px-2 py-0.5 text-[10px] text-ink-dim">
            <span className="truncate">{title}</span>
            <span className="ml-auto">{language}</span>
            <span className="tabular-nums">
              Ln {cursor.line}, Col {cursor.column}
            </span>
          </div>
        </div>
        <div className="h-48 shrink-0 rounded-md border border-panel-border bg-panel p-2">
          <ConsolePanel
            onOpenCode={(o: ConsoleOrigin) => {
              const s = scripts.find((x) => x.artboard.name === o.scene && x.layer === (o.layer ?? null) && x.state === o.state)
              if (!s) return
              const path = s.program?.kind === 'Code' ? fileRefPath(s.program.value) : null
              const f = path ? files.find((x) => x.path === path) : null
              if (f) openFile(f, o.line)
              else openState(s, { line: o.line, column: o.column })
            }}
          />
        </div>
      </div>
    </div>
  )
}
