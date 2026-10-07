import Editor, { loader, type Monaco } from '@monaco-editor/react'
import * as monacoEditor from 'monaco-editor'
import { useRef } from 'react'
import type { NodeSummary } from '../ipc/document'

/**
 * The Code tab's editor for a Programmable State's `ProgramDef.Code` — a
 * real VS Code-style editor (Monaco is literally the component VS Code
 * itself is built on), not a plain `<textarea>`, per the user's own ask.
 * Loaded from the bundled `monaco-editor` npm package (`loader.config`
 * below) rather than `@monaco-editor/react`'s default jsdelivr-CDN loader
 * — this is a desktop Tauri app, not a page that can assume live internet
 * access, so the editor has to work fully offline.
 *
 * No language-service web worker is wired up (`vite-plugin-monaco-editor`
 * territory — real, but out of scope here): Monaco still tokenizes/
 * highlights JavaScript with its built-in Monarch grammar with no worker at
 * all, and the *custom* completions this registers (`ctx`'s fields,
 * `getCompByID`/`getCompByClass`, and this artboard's own node ids/classes)
 * don't need one either — only the stock TS IntelliSense/diagnostics would.
 * The one visible cost: no red-squiggly type-checking on typos. Acceptable
 * for a small per-state script; worth revisiting if scripts grow larger.
 */

loader.config({ monaco: monacoEditor })

const CTX_SUGGESTIONS: monacoEditor.languages.CompletionItem[] = [
  { label: 'ctx.time', kind: 17 /* Property */, insertText: 'ctx.time', detail: 'number — seconds since this state became active', range: undefined as never },
  { label: 'ctx.deltaSeconds', kind: 17, insertText: 'ctx.deltaSeconds', detail: 'number — seconds since the last frame', range: undefined as never },
  { label: 'ctx.mouseX', kind: 17, insertText: 'ctx.mouseX', detail: 'number — live pointer X, artboard space', range: undefined as never },
  { label: 'ctx.mouseY', kind: 17, insertText: 'ctx.mouseY', detail: 'number — live pointer Y, artboard space', range: undefined as never },
  {
    label: 'getCompByID',
    kind: 1 /* Function */,
    insertText: "getCompByID('$1')",
    insertTextRules: 4 /* InsertAsSnippet */,
    detail: "(id: string) => ComponentRef | null — look up a node by its Inspector 'Id' field",
    range: undefined as never,
  },
  {
    label: 'getCompByClass',
    kind: 1,
    insertText: "getCompByClass('$1')",
    insertTextRules: 4,
    detail: '(className: string) => ComponentRef[] — every node tagged with this class',
    range: undefined as never,
  },
  // These exist specifically so the Nodes tab can translate into (and back
  // out of) Code without losing anything — `programCodegen.ts` emits calls
  // to exactly these names. Preferring them over bare `+`/`-`/`*`/`/` (for
  // the arithmetic ones) is what keeps a hand-written script convertible
  // back to Nodes later, not just executable.
  { label: 'add(a, b)', kind: 1, insertText: 'add($1, $2)', insertTextRules: 4, detail: 'number — matches an Add node', range: undefined as never },
  { label: 'subtract(a, b)', kind: 1, insertText: 'subtract($1, $2)', insertTextRules: 4, detail: 'number — matches a Subtract node', range: undefined as never },
  { label: 'multiply(a, b)', kind: 1, insertText: 'multiply($1, $2)', insertTextRules: 4, detail: 'number — matches a Multiply node', range: undefined as never },
  { label: 'divide(a, b)', kind: 1, insertText: 'divide($1, $2)', insertTextRules: 4, detail: 'number — matches a Divide node', range: undefined as never },
  { label: 'greaterThan(a, b)', kind: 1, insertText: 'greaterThan($1, $2)', insertTextRules: 4, detail: '1 or 0 — matches a Greater Than node', range: undefined as never },
  { label: 'lessThan(a, b)', kind: 1, insertText: 'lessThan($1, $2)', insertTextRules: 4, detail: '1 or 0 — matches a Less Than node', range: undefined as never },
  { label: 'equals(a, b)', kind: 1, insertText: 'equals($1, $2)', insertTextRules: 4, detail: '1 or 0 — matches an Equals node', range: undefined as never },
  { label: 'and(a, b)', kind: 1, insertText: 'and($1, $2)', insertTextRules: 4, detail: '1 or 0 — matches an And node', range: undefined as never },
  { label: 'or(a, b)', kind: 1, insertText: 'or($1, $2)', insertTextRules: 4, detail: '1 or 0 — matches an Or node', range: undefined as never },
  { label: 'not(a)', kind: 1, insertText: 'not($1)', insertTextRules: 4, detail: '1 or 0 — matches a Not node', range: undefined as never },
  { label: 'select(cond, a, b)', kind: 1, insertText: 'select($1, $2, $3)', insertTextRules: 4, detail: 'number — matches a Select node ("logical filter")', range: undefined as never },
  { label: 'lerp(a, b, t)', kind: 1, insertText: 'lerp($1, $2, $3)', insertTextRules: 4, detail: 'number — matches a Lerp node', range: undefined as never },
  { label: 'clamp(v, min, max)', kind: 1, insertText: 'clamp($1, $2, $3)', insertTextRules: 4, detail: 'number — matches a Clamp node', range: undefined as never },
  { label: 'random(seed)', kind: 1, insertText: 'random($1)', insertTextRules: 4, detail: 'number 0..1 — matches a Random node, same hash as the Graph evaluator', range: undefined as never },
]

/** Every `ComponentRef` property, offered right after typing `ref.` — a
 * plain word-list completion, not real member-type inference (there's no
 * language service running to know `ref` came from `getCompByID`), but
 * good enough since a `ComponentRef` only ever has this one fixed shape. */
const COMPONENT_REF_PROPERTIES = ['id', 'name', 'classes', 'x', 'y', 'rotation', 'scaleX', 'scaleY', 'anchorX', 'anchorY']

/** Monaco's completion-provider callback argument types (the editor's own
 * `monaco-editor` typings, reached through `@monaco-editor/react`'s `Monaco`). */
type CompletionProvider = Parameters<Monaco['languages']['registerCompletionItemProvider']>[1]
type CompletionModel = Parameters<CompletionProvider['provideCompletionItems']>[0]
type CompletionPosition = Parameters<CompletionProvider['provideCompletionItems']>[1]

export function CodeEditor({ value, onChange, nodes }: { value: string; onChange: (value: string) => void; nodes: NodeSummary[] }) {
  const nodesRef = useRef(nodes)
  nodesRef.current = nodes

  function handleMount(_editor: unknown, monaco: Monaco) {
    monaco.languages.registerCompletionItemProvider('javascript', {
      triggerCharacters: ['.', "'", '"'],
      provideCompletionItems(model: CompletionModel, position: CompletionPosition) {
        const word = model.getWordUntilPosition(position)
        const range = { startLineNumber: position.lineNumber, endLineNumber: position.lineNumber, startColumn: word.startColumn, endColumn: word.endColumn }
        const linePrefix = model.getLineContent(position.lineNumber).slice(0, position.column - 1)

        // Inside a string literal right after getCompByID(/getCompByClass( —
        // suggest this artboard's own live ids/classes instead of the
        // generic ctx/getCompBy* suggestions, so an author sees exactly
        // what's actually on the canvas rather than guessing an id.
        if (/getCompByID\(\s*['"][^'"]*$/.test(linePrefix)) {
          return {
            suggestions: nodesRef.current
              .filter((n) => n.elementId)
              .map((n) => ({ label: n.elementId, kind: 12 /* Value */, insertText: n.elementId, detail: n.name, range })),
          }
        }
        if (/getCompByClass\(\s*['"][^'"]*$/.test(linePrefix)) {
          const classes = Array.from(new Set(nodesRef.current.flatMap((n) => n.classes)))
          return { suggestions: classes.map((c) => ({ label: c, kind: 12, insertText: c, range })) }
        }
        if (/\bref\.$|\.\w*$/.test(linePrefix) && /(comp|ref|node|square|shape)\w*\.\w*$/i.test(linePrefix)) {
          return {
            suggestions: COMPONENT_REF_PROPERTIES.map((p) => ({ label: p, kind: 9 /* Field */, insertText: p, range })),
          }
        }

        return { suggestions: CTX_SUGGESTIONS.map((s) => ({ ...s, range })) }
      },
    })
  }

  return (
    <Editor
      height="100%"
      language="javascript"
      theme="vs-dark"
      value={value}
      onChange={(next) => onChange(next ?? '')}
      onMount={handleMount}
      options={{
        fontSize: 12,
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        automaticLayout: true,
        tabSize: 2,
        wordWrap: 'on',
      }}
    />
  )
}
