/**
 * Graph <-> Code conversion for a Programmable State's `ProgramDef` — what
 * makes `ProgramEditorPanel`'s two tabs two *views* of one program instead
 * of a destructive either/or toggle (switching tabs used to reset whichever
 * one you weren't looking at back to empty). `generateCode` (Graph -> Code)
 * always succeeds — it's a plain deterministic compiler over a graph that
 * can already only express what this DSL's vocabulary covers.
 * `parseCode` (Code -> Graph) is necessarily narrower: it understands
 * exactly the vocabulary `generateCode` emits (a uniform "every operation
 * is a named function call" convention — see `scriptRuntime.ts`'s own doc
 * comment for why bare operators/`Math.*` aren't used for the custom
 * nodes), so a script that stays within that shape round-trips, and one
 * that doesn't fails with a clear reason rather than silently corrupting
 * or discarding the graph.
 *
 * This deliberately mirrors "how the parallax demo was first built" (the
 * user's own framing): that demo is hand-assembled `GraphNodeDef`/
 * `GraphEdgeDef` literals in Rust; this is the same translation, done live,
 * in both directions, for whatever graph an author builds in the Nodes tab.
 *
 * Known gaps, always reported rather than silently dropped: `FillColor*`
 * `GetProperty`/`SetProperty` (no matching `ComponentRef` property exists
 * yet) and `GetNumberInput`/`GetBoolInput` (Code mode has no access to a
 * state machine's live `Inputs` yet) have no Code equivalent — `generateCode`
 * emits a clearly-commented `0` placeholder and reports `fullyRepresentable:
 * false` rather than pretending the translation is complete.
 */

import type { GraphEdgeDef, GraphNodeDef, GraphNodeKind, GraphProperty, NodeGraphDef, NodeSummary, OpaqueId } from './ipc/document'

// ---------------------------------------------------------------------------
// Graph -> Code
// ---------------------------------------------------------------------------

const PROPERTY_TO_JS: Partial<Record<GraphProperty, string>> = {
  TranslationX: 'x',
  TranslationY: 'y',
  Rotation: 'rotation',
  ScaleX: 'scaleX',
  ScaleY: 'scaleY',
  AnchorX: 'anchorX',
  AnchorY: 'anchorY',
}

const JS_TO_PROPERTY: Record<string, GraphProperty> = Object.fromEntries(Object.entries(PROPERTY_TO_JS).map(([k, v]) => [v as string, k as GraphProperty]))

/** Topological order (Kahn's algorithm), same shape as `NodeGraphEditor`'s
 * own layout math and `an1mbyt3_runtime::graph::evaluate_graph` — any node
 * left over after the main pass (a cycle) is appended afterward in id
 * order, so generation never gets stuck, matching the Rust evaluator's own
 * "a cycle still gets a well-defined result" stance. */
function topologicalOrder(graph: NodeGraphDef): number[] {
  const ids = graph.nodes.map((n) => n.id)
  const inDegree = new Map<number, number>(ids.map((id) => [id, 0]))
  const dependents = new Map<number, number[]>()
  for (const edge of graph.edges) {
    if (!inDegree.has(edge.fromNode) || !inDegree.has(edge.toNode)) continue
    inDegree.set(edge.toNode, (inDegree.get(edge.toNode) ?? 0) + 1)
    if (!dependents.has(edge.fromNode)) dependents.set(edge.fromNode, [])
    dependents.get(edge.fromNode)!.push(edge.toNode)
  }
  const queue = ids.filter((id) => inDegree.get(id) === 0).sort((a, b) => a - b)
  const order: number[] = []
  const remaining = new Map(inDegree)
  while (queue.length > 0) {
    const id = queue.shift()!
    order.push(id)
    for (const dep of dependents.get(id) ?? []) {
      remaining.set(dep, (remaining.get(dep) ?? 0) - 1)
      if (remaining.get(dep) === 0) queue.push(dep)
    }
  }
  if (order.length < ids.length) {
    order.push(...ids.filter((id) => !order.includes(id)).sort((a, b) => a - b))
  }
  return order
}

export function generateCode(graph: NodeGraphDef, nodes: NodeSummary[]): { code: string; fullyRepresentable: boolean } {
  const nodeById = new Map(graph.nodes.map((n) => [n.id, n]))
  const elementIdOf = (id: OpaqueId) => nodes.find((n) => n.id.idx === id.idx && n.id.version === id.version)?.elementId

  // `${nodeId}:${socket}` -> the JS expression that reads that output —
  // either an inline source expression (`ctx.time`, a `getCompByID` call, a
  // literal) or a `const nN` variable name for anything with its own line.
  const exprAt = new Map<string, string>()
  const lines: string[] = []
  let fullyRepresentable = true

  function resolveInput(node: GraphNodeDef, socket: number): string {
    const wire = [...graph.edges].reverse().find((e) => e.toNode === node.id && e.toSocket === socket)
    const override = node.overrides.find((o) => o.socketIndex === socket)
    if (wire && !override?.useOverride) {
      return exprAt.get(`${wire.fromNode}:${wire.fromSocket}`) ?? '0'
    }
    return String(override?.value ?? 0)
  }

  for (const id of topologicalOrder(graph)) {
    const node = nodeById.get(id)
    if (!node) continue
    const kind = node.kind

    if (kind.kind === 'Constant') {
      exprAt.set(`${id}:0`, String(kind.value))
      continue
    }
    if (kind.kind === 'Time') {
      exprAt.set(`${id}:0`, 'ctx.time')
      continue
    }
    if (kind.kind === 'MousePosition') {
      exprAt.set(`${id}:0`, 'ctx.mouseX')
      exprAt.set(`${id}:1`, 'ctx.mouseY')
      continue
    }
    if (kind.kind === 'Random') {
      exprAt.set(`${id}:0`, `random(${kind.value.seed})`)
      continue
    }
    if (kind.kind === 'GetProperty') {
      const prop = PROPERTY_TO_JS[kind.value.property]
      const elId = elementIdOf(kind.value.node)
      if (prop && elId) {
        exprAt.set(`${id}:0`, `getCompByID('${elId}').${prop}`)
      } else {
        exprAt.set(`${id}:0`, '0 /* unsupported: GetProperty */')
        fullyRepresentable = false
      }
      continue
    }
    if (kind.kind === 'SetProperty') {
      const prop = PROPERTY_TO_JS[kind.value.property]
      const elId = elementIdOf(kind.value.node)
      // `global: true` (world-space translation) has no Code-mode
      // equivalent yet — no `setGlobalX`/`setGlobalY` script helper exists
      // to express the parent-chain conversion `evaluate_graph` does for a
      // Graph-authored node. Reported the same "known gap" way as an
      // unmapped property, rather than silently emitting a plain (wrong,
      // local-space) assignment.
      if (prop && elId && !kind.value.global) {
        lines.push(`getCompByID('${elId}').${prop} = ${resolveInput(node, 0)};`)
      } else if (kind.value.global) {
        lines.push(`// unsupported: SetProperty ${kind.value.property} (Global) has no Code equivalent yet`)
        fullyRepresentable = false
      } else {
        lines.push(`// unsupported: SetProperty ${kind.value.property} has no Code equivalent yet`)
        fullyRepresentable = false
      }
      continue
    }
    if (kind.kind === 'GetNumberInput' || kind.kind === 'GetBoolInput') {
      exprAt.set(`${id}:0`, `0 /* unsupported: ${kind.kind} */`)
      fullyRepresentable = false
      continue
    }

    const varName = `n${id}`
    const a = () => resolveInput(node, 0)
    const b = () => resolveInput(node, 1)
    const c = () => resolveInput(node, 2)
    const expr: string = (() => {
      switch (kind.kind) {
        case 'Add':
          return `add(${a()}, ${b()})`
        case 'Subtract':
          return `subtract(${a()}, ${b()})`
        case 'Multiply':
          return `multiply(${a()}, ${b()})`
        case 'Divide':
          return `divide(${a()}, ${b()})`
        case 'Min':
          return `Math.min(${a()}, ${b()})`
        case 'Max':
          return `Math.max(${a()}, ${b()})`
        case 'Sin':
          return `Math.sin(${a()})`
        case 'Cos':
          return `Math.cos(${a()})`
        case 'GreaterThan':
          return `greaterThan(${a()}, ${b()})`
        case 'LessThan':
          return `lessThan(${a()}, ${b()})`
        case 'Equals':
          return `equals(${a()}, ${b()})`
        case 'And':
          return `and(${a()}, ${b()})`
        case 'Or':
          return `or(${a()}, ${b()})`
        case 'Not':
          return `not(${a()})`
        case 'Select':
          return `select(${a()}, ${b()}, ${c()})`
        case 'Lerp':
          return `lerp(${a()}, ${b()}, ${c()})`
        case 'Clamp':
          return `clamp(${a()}, ${b()}, ${c()})`
        default:
          return '0'
      }
    })()
    lines.push(`const ${varName} = ${expr};`)
    exprAt.set(`${id}:0`, varName)
  }

  const header = fullyRepresentable
    ? ''
    : '// Some nodes here (fill-color read/write, or state-machine number/bool inputs) have no\n// Code equivalent yet, shown as a commented 0 — editing this program as Code will lose them.\n'
  return { code: header + lines.join('\n'), fullyRepresentable }
}

// ---------------------------------------------------------------------------
// Code -> Graph
// ---------------------------------------------------------------------------

type Token = { kind: 'number' | 'string' | 'ident' | 'punc'; value: string }

function tokenize(source: string): Token[] {
  // Strip `//` and `/* */` comments first — neither form nests, and neither
  // can appear inside one of our own string literals (ids are plain
  // words), so this simple pass is safe for the restricted grammar below.
  const stripped = source.replace(/\/\*[\s\S]*?\*\//g, ' ').replace(/\/\/[^\n]*/g, ' ')
  const tokens: Token[] = []
  const re = /-?\d+(?:\.\d+)?|'[^']*'|"[^"]*"|[A-Za-z_][A-Za-z0-9_]*|[().,=;]/g
  let match: RegExpExecArray | null
  while ((match = re.exec(stripped))) {
    const text = match[0]
    if (/^-?\d/.test(text)) tokens.push({ kind: 'number', value: text })
    else if (text[0] === "'" || text[0] === '"') tokens.push({ kind: 'string', value: text.slice(1, -1) })
    else if (/^[A-Za-z_]/.test(text)) tokens.push({ kind: 'ident', value: text })
    else tokens.push({ kind: 'punc', value: text })
  }
  return tokens
}

/** What one parsed sub-expression resolves to — either a freshly-referenced
 * (or newly created) graph node's output socket, or a bare literal that
 * becomes the *consuming* node's own `SocketOverride` instead of a wire. */
type Resolved = { kind: 'ref'; node: number; socket: number } | { kind: 'literal'; value: number }

/** Every `GraphNodeKind` member that's just a bare `{ kind: '...' }` tag
 * with no payload — the only ones a plain function-name lookup table can
 * construct directly. */
type BareGraphKind = Extract<GraphNodeKind, { kind: string } & { value?: never }>['kind']

const CALL_NODE_KINDS: Record<string, { kind: BareGraphKind; arity: number }> = {
  add: { kind: 'Add', arity: 2 },
  subtract: { kind: 'Subtract', arity: 2 },
  multiply: { kind: 'Multiply', arity: 2 },
  divide: { kind: 'Divide', arity: 2 },
  greaterThan: { kind: 'GreaterThan', arity: 2 },
  lessThan: { kind: 'LessThan', arity: 2 },
  equals: { kind: 'Equals', arity: 2 },
  and: { kind: 'And', arity: 2 },
  or: { kind: 'Or', arity: 2 },
  not: { kind: 'Not', arity: 1 },
  select: { kind: 'Select', arity: 3 },
  lerp: { kind: 'Lerp', arity: 3 },
  clamp: { kind: 'Clamp', arity: 3 },
}
const MATH_NODE_KINDS: Record<string, { kind: BareGraphKind; arity: number }> = {
  min: { kind: 'Min', arity: 2 },
  max: { kind: 'Max', arity: 2 },
  sin: { kind: 'Sin', arity: 1 },
  cos: { kind: 'Cos', arity: 1 },
}

class ParseError extends Error {}

class Parser {
  private pos = 0
  private nextId = 0
  private nodes: GraphNodeDef[] = []
  private edges: GraphEdgeDef[] = []
  private vars = new Map<string, { node: number; socket: number }>()
  private sourceNodes = new Map<string, number>()

  // (Plain fields, not constructor parameter properties — those aren't
  // erasable syntax, which this project's tsconfig requires.)
  private tokens: Token[]
  private sceneNodes: NodeSummary[]

  constructor(tokens: Token[], sceneNodes: NodeSummary[]) {
    this.tokens = tokens
    this.sceneNodes = sceneNodes
  }

  private peek(offset = 0): Token | undefined {
    return this.tokens[this.pos + offset]
  }

  private take(): Token {
    const t = this.tokens[this.pos++]
    if (!t) throw new ParseError('unexpected end of input')
    return t
  }

  private expectPunc(value: string): void {
    const t = this.take()
    if (t.kind !== 'punc' || t.value !== value) throw new ParseError(`expected '${value}', got '${t.value}'`)
  }

  private allocId(): number {
    return this.nextId++
  }

  private addNode(kind: GraphNodeKind): number {
    const id = this.allocId()
    this.nodes.push({ id, kind, position: { x: (this.nodes.length % 6) * 220, y: Math.floor(this.nodes.length / 6) * 100 }, overrides: [] })
    return id
  }

  private getOrCreateSource(key: string, make: () => GraphNodeKind): number {
    const existing = this.sourceNodes.get(key)
    if (existing !== undefined) return existing
    const id = this.addNode(make())
    this.sourceNodes.set(key, id)
    return id
  }

  private wireArg(consumerId: number, socket: number, resolved: Resolved): void {
    if (resolved.kind === 'literal') {
      const node = this.nodes.find((n) => n.id === consumerId)!
      node.overrides.push({ socketIndex: socket, value: resolved.value, useOverride: false })
    } else {
      this.edges.push({ fromNode: resolved.node, fromSocket: resolved.socket, toNode: consumerId, toSocket: socket })
    }
  }

  private resolveNodeByElementId(elementId: string): OpaqueId {
    const node = this.sceneNodes.find((n) => n.elementId === elementId)
    if (!node) throw new ParseError(`no node on this artboard has the id '${elementId}'`)
    return node.id
  }

  /** Parses one comma-separated argument list, already positioned just
   * after the opening `(`. */
  private parseArgs(): Resolved[] {
    const args: Resolved[] = []
    if (this.peek()?.kind === 'punc' && this.peek()?.value === ')') {
      this.take()
      return args
    }
    while (true) {
      args.push(this.parseExpr())
      const t = this.take()
      if (t.kind === 'punc' && t.value === ')') break
      if (!(t.kind === 'punc' && t.value === ',')) throw new ParseError(`expected ',' or ')', got '${t.value}'`)
    }
    return args
  }

  private parseExpr(): Resolved {
    const t = this.take()

    if (t.kind === 'number') return { kind: 'literal', value: Number.parseFloat(t.value) }

    if (t.kind === 'ident') {
      // `ctx.time` / `ctx.mouseX` / `ctx.mouseY`
      if (t.value === 'ctx') {
        this.expectPunc('.')
        const member = this.take()
        if (member.kind !== 'ident') throw new ParseError(`expected a property name after 'ctx.'`)
        if (member.value === 'time') return { kind: 'ref', node: this.getOrCreateSource('time', () => ({ kind: 'Time' })), socket: 0 }
        if (member.value === 'mouseX') return { kind: 'ref', node: this.getOrCreateSource('mouse', () => ({ kind: 'MousePosition' })), socket: 0 }
        if (member.value === 'mouseY') return { kind: 'ref', node: this.getOrCreateSource('mouse', () => ({ kind: 'MousePosition' })), socket: 1 }
        throw new ParseError(`'ctx.${member.value}' isn't something a Nodes graph can read from`)
      }

      // `Math.min(...)` / `Math.max(...)` / `Math.sin(...)` / `Math.cos(...)`
      if (t.value === 'Math') {
        this.expectPunc('.')
        const member = this.take()
        const entry = member.kind === 'ident' ? MATH_NODE_KINDS[member.value] : undefined
        if (!entry) throw new ParseError(`'Math.${member.kind === 'ident' ? member.value : '?'}' isn't one of the supported Math functions (min/max/sin/cos)`)
        this.expectPunc('(')
        const args = this.parseArgs()
        if (args.length !== entry.arity) throw new ParseError(`Math.${member.value} expects ${entry.arity} argument(s), got ${args.length}`)
        const id = this.addNode({ kind: entry.kind })
        args.forEach((arg, i) => this.wireArg(id, i, arg))
        return { kind: 'ref', node: id, socket: 0 }
      }

      // `getCompByID('id').prop`
      if (t.value === 'getCompByID') {
        this.expectPunc('(')
        const idTok = this.take()
        if (idTok.kind !== 'string') throw new ParseError(`getCompByID(...) expects a string literal id`)
        this.expectPunc(')')
        this.expectPunc('.')
        const prop = this.take()
        if (prop.kind !== 'ident') throw new ParseError(`expected a property name after 'getCompByID(...).'`)
        const property = JS_TO_PROPERTY[prop.value]
        if (!property) throw new ParseError(`'.${prop.value}' isn't a property a Nodes graph can read (x/y/rotation/scaleX/scaleY/anchorX/anchorY)`)
        const target = this.resolveNodeByElementId(idTok.value)
        const key = `get:${idTok.value}:${prop.value}`
        const id = this.getOrCreateSource(key, () => ({ kind: 'GetProperty', value: { node: target, property } }))
        return { kind: 'ref', node: id, socket: 0 }
      }

      // `random(seed)`
      if (t.value === 'random') {
        this.expectPunc('(')
        const seedTok = this.take()
        this.expectPunc(')')
        if (seedTok.kind !== 'number') throw new ParseError(`random(...) needs a plain numeric seed, not a wired value`)
        const seed = Math.max(0, Math.trunc(Number.parseFloat(seedTok.value)))
        const id = this.getOrCreateSource(`random:${seed}`, () => ({ kind: 'Random', value: { seed } }))
        return { kind: 'ref', node: id, socket: 0 }
      }

      // Any other bare call: add/subtract/.../lerp/clamp/select
      if (this.peek()?.kind === 'punc' && this.peek()?.value === '(') {
        const entry = CALL_NODE_KINDS[t.value]
        if (!entry) throw new ParseError(`'${t.value}(...)' isn't a recognized function — this DSL only understands the exact vocabulary generated code uses`)
        this.take() // '('
        const args = this.parseArgs()
        if (args.length !== entry.arity) throw new ParseError(`${t.value}(...) expects ${entry.arity} argument(s), got ${args.length}`)
        const id = this.addNode({ kind: entry.kind })
        args.forEach((arg, i) => this.wireArg(id, i, arg))
        return { kind: 'ref', node: id, socket: 0 }
      }

      // A bare identifier with no call — must be a previously-`const`-bound name.
      const bound = this.vars.get(t.value)
      if (!bound) throw new ParseError(`'${t.value}' is used before it's defined (or isn't a 'const nN' this parser recognizes)`)
      return { kind: 'ref', node: bound.node, socket: bound.socket }
    }

    throw new ParseError(`unexpected token '${t.value}'`)
  }

  private parseStatement(): void {
    const t = this.take()

    if (t.kind === 'ident' && t.value === 'const') {
      const name = this.take()
      if (name.kind !== 'ident') throw new ParseError(`expected a variable name after 'const'`)
      this.expectPunc('=')
      const value = this.parseExpr()
      this.expectPunc(';')
      if (value.kind === 'ref') {
        this.vars.set(name.value, { node: value.node, socket: value.socket })
      } else {
        // A bare literal bound to a name — materialize a real Constant node
        // for it so it stays addressable, same as `generateCode` always
        // giving every `Constant` its own line.
        const id = this.addNode({ kind: 'Constant', value: value.value })
        this.vars.set(name.value, { node: id, socket: 0 })
      }
      return
    }

    if (t.kind === 'ident' && t.value === 'getCompByID') {
      this.expectPunc('(')
      const idTok = this.take()
      if (idTok.kind !== 'string') throw new ParseError(`getCompByID(...) expects a string literal id`)
      this.expectPunc(')')
      this.expectPunc('.')
      const prop = this.take()
      if (prop.kind !== 'ident') throw new ParseError(`expected a property name after 'getCompByID(...).'`)
      const property = JS_TO_PROPERTY[prop.value]
      if (!property) throw new ParseError(`'.${prop.value}' isn't a settable property (x/y/rotation/scaleX/scaleY/anchorX/anchorY)`)
      this.expectPunc('=')
      const value = this.parseExpr()
      this.expectPunc(';')
      const target = this.resolveNodeByElementId(idTok.value)
      const id = this.addNode({ kind: 'SetProperty', value: { node: target, property, global: false } })
      this.wireArg(id, 0, value)
      return
    }

    throw new ParseError(`expected 'const ...' or 'getCompByID(...).prop = ...', got '${t.value}'`)
  }

  parse(): NodeGraphDef {
    while (this.pos < this.tokens.length) this.parseStatement()
    return { nodes: this.nodes, edges: this.edges }
  }
}

/** Attempts to parse `source` as a Nodes graph — succeeds only for code
 * that stays within `generateCode`'s own vocabulary (the exact function
 * names it emits, `ctx.time`/`mouseX`/`mouseY`, `getCompByID(...).prop`,
 * `Math.min`/`max`/`sin`/`cos`, `const name = ...;` bindings). Returns the
 * reason it failed rather than throwing, so a caller can show it and leave
 * the existing program untouched. */
export function parseCode(source: string, nodes: NodeSummary[]): { graph: NodeGraphDef } | { error: string } {
  try {
    const graph = new Parser(tokenize(source), nodes).parse()
    return { graph }
  } catch (err) {
    return { error: err instanceof ParseError ? err.message : `couldn't parse this code as a Nodes graph (${String(err)})` }
  }
}
