/**
 * Scratch-style block definitions + the compiler/decompiler between a
 * Blockly workspace and `an1mbyt3_core::BlockProgramDef` (mirrored here as
 * `BlockProgramDef`/`Statement`/`Expr` — see `ipc/document.ts`). This is
 * the frontend half of Phase 1's "adopt Blockly" plan: a Programmable
 * State's `Blocks` program is real Rust (`an1mbyt3_runtime::block_interpreter`,
 * runs everywhere `Graph` mode does), not another editor-only scripting
 * surface like `Code` — this module's whole job is translating between
 * that Rust-shaped IR and the visual blocks an author actually drags
 * around, in both directions:
 *
 * - `compileWorkspace` (blocks -> IR): walks every top-level block. A
 *   `an1_when_state_triggered`/`an1_when_key_pressed` hat becomes one
 *   `BlockScript`; anything else sitting unconnected at the top level is
 *   simply not part of any script (an orphaned block does nothing, same as
 *   real Scratch).
 * - `loadProgramIntoWorkspace` (IR -> blocks): rebuilds a fresh block stack
 *   per script. This is *not* the same thing as `apps/editor/src/programCodegen.ts`'s
 *   `Graph`<->`Code` conversion (deliberately out of scope for this phase —
 *   see the build plan) — it never crosses between authoring *modes*, only
 *   between one mode's own persisted IR and its own visual editor, the same
 *   "load what was saved" round-trip any block/node editor needs regardless
 *   of how many other modes exist alongside it. Exact hand-arranged block
 *   *positions* aren't preserved (Blockly's own workspace layout isn't part
 *   of `BlockProgramDef` at all — only the logic is), so reopening a
 *   program re-lays it out top-to-bottom rather than restoring a
 *   pixel-exact canvas.
 *
 * Numeric value inputs (`an1_wait`'s seconds, `an1_repeat`'s count, the
 * binary-operator blocks' two operands, etc.) each carry a default *shadow*
 * block (`an1_number`, an editable inline number field) via Blockly's own
 * shadow-block mechanism — the same Scratch convention where a numeric slot
 * always has a usable default typed right into the socket itself, not a
 * separate draggable "number" block you have to go fetch from the palette
 * first. Dragging a real reporter (an operator, `an1_get_property`, ...)
 * into that slot covers the shadow; removing it reveals the shadow again.
 * `an1_number` itself is still a real block *type* (needed as the shadow's
 * target), it's just not offered in the Operators toolbox category anymore
 * — there's nothing to explicitly "add," it's already there. Boolean-shaped
 * slots (`an1_if_else`'s condition, `an1_and`/`an1_or`/`an1_not`'s operands)
 * deliberately get no shadow, matching Scratch's own empty hexagonal
 * boolean sockets.
 */
import * as Blockly from 'blockly'
import type { BlockProgramDef, BlockScript, Expr, GraphProperty, MathFn, NodeRef, OpaqueId, Statement } from '../ipc/document'

// A modern, muted "tinted glass" palette — desaturated jewel tones rather
// than Scratch's own bright, cartoonish primary hues (or this module's own
// first pass at a "techy" palette, which still read as too saturated/
// playful) — distinct per category for scanability, all sharing similar
// saturation/lightness so they read as one cohesive material, keyed off
// the same cool family the rest of this app's dark UI already lives in
// (`accent: #6d8cff`). Paired with `BlockProgramEditor.tsx`'s injected CSS
// (reduced fill-opacity + a soft drop-shadow) for the actual "glass" feel —
// color alone doesn't get there.
const COLOUR = {
  events: '#7c7ff0', // muted indigo
  control: '#d68a4c', // muted copper
  motion: '#4a90c4', // muted steel blue
  sensing: '#3ba99c', // muted teal
  operators: '#5cae74', // muted sage
  looks: '#b06a94', // muted mauve
} as const

function encodeNodeId(id: OpaqueId): string {
  return `${id.idx}:${id.version}`
}

function decodeNodeId(value: string): OpaqueId {
  const [idx, version] = value.split(':').map(Number)
  return { idx: idx || 0, version: version || 0 }
}

/** The synthetic option every `NODE`/`A`/`B` dropdown offers alongside real
 * node picks — selecting it authors `NodeRef::CurrentItem` (the node a
 * `ForEachNode` loop is currently on) instead of an explicit `Literal`.
 * Encoded as a value no real `encodeNodeId` output can ever collide with
 * (`encodeNodeId` only ever produces `\d+:\d+`). `NodeRef::RandomFromList`
 * has no dropdown option here — nothing in this phase's blocks authors one
 * yet (see `decodeNodeRef`'s own doc comment); it still round-trips fine if
 * some future block or hand-edited program produces one, just not
 * *creatable* from this picker.
 */
const CURRENT_ITEM_OPTION = '__current_item__'
/** Same synthetic-option idea as `CURRENT_ITEM_OPTION`, for `NodeRef::Myself`
 * — the clone a `CloneStarted` script instance is its own "when I start as
 * a clone" run for (Scratch's "myself"). Resolves to nothing outside that
 * context, same "degrades harmlessly" convention every other `NodeRef`
 * variant follows. */
const MYSELF_OPTION = '__myself__'

/** Decodes a `NODE`/`A`/`B` dropdown's field value back into a `NodeRef` —
 * the compiler-side counterpart to `nodeOptions`/`CURRENT_ITEM_OPTION`/
 * `MYSELF_OPTION`. */
function decodeNodeRef(value: string): NodeRef {
  if (value === CURRENT_ITEM_OPTION) return { kind: 'CurrentItem' }
  if (value === MYSELF_OPTION) return { kind: 'Myself' }
  return { kind: 'Literal', value: decodeNodeId(value) }
}

/** Encodes a `NodeRef` back into a dropdown field value — the decompiler-
 * side counterpart to `decodeNodeRef`. `RandomFromList` has no dropdown
 * representation (see `CURRENT_ITEM_OPTION`'s own doc comment) — falls back
 * to `CurrentItem`'s own encoding with a console warning rather than
 * crashing, on the same "never actually produced by these blocks, but
 * shouldn't corrupt the editor if it somehow shows up" principle
 * `blockFromExpr`'s `GetProperty`/`GetNumberInput`/`GetBoolInput` fallback
 * used before those got real blocks. */
function encodeNodeRef(node: NodeRef): string {
  switch (node.kind) {
    case 'Literal':
      return encodeNodeId(node.value)
    case 'CurrentItem':
      return CURRENT_ITEM_OPTION
    case 'Myself':
      return MYSELF_OPTION
    case 'RandomFromList':
      console.warn('gameBlocks: no block UI yet for NodeRef.RandomFromList, showing "current item" instead')
      return CURRENT_ITEM_OPTION
  }
}

/** Every other `Shape`/`Image` node the workspace's dynamic dropdowns
 * (`an1_go_to_xy`/`an1_set_property`/`an1_change_property`/`an1_touching`)
 * list, prefixed with the synthetic "current item" option every one of
 * those dropdowns also offers — Blockly's `FieldDropdown` supports a
 * *function* as its menu generator (re-invoked each time the dropdown
 * opens), so this module-level mutable list is how `BlockProgramEditor.tsx`
 * feeds live node data into blocks that were defined once, globally, at
 * module load — Blockly has no per-workspace block-definition concept to
 * hand this to more directly. Never empty beyond the current-item entry
 * (falls back to a placeholder real-node entry too) — `FieldDropdown`
 * throws on an empty options array. */
let nodeOptions: [string, string][] = [['★ current item', CURRENT_ITEM_OPTION], ['★ myself (clone)', MYSELF_OPTION], ['(no shapes yet)', '']]

export function setNodeOptions(nodes: { id: OpaqueId; name: string }[]): void {
  const real: [string, string][] = nodes.length > 0 ? nodes.map((n) => [n.name, encodeNodeId(n.id)]) : [['(no shapes yet)', '']]
  nodeOptions = [['★ current item', CURRENT_ITEM_OPTION], ['★ myself (clone)', MYSELF_OPTION], ...real]
}

const PROPERTY_OPTIONS: [string, string][] = [
  ['x', 'TranslationX'],
  ['y', 'TranslationY'],
  ['rotation', 'Rotation'],
  ['scale x', 'ScaleX'],
  ['scale y', 'ScaleY'],
  ['anchor x', 'AnchorX'],
  ['anchor y', 'AnchorY'],
  ['fill red', 'FillColorR'],
  ['fill green', 'FillColorG'],
  ['fill blue', 'FillColorB'],
  ['fill alpha', 'FillColorA'],
  ['visible', 'Visible'],
]

const BINARY_OPERATORS: { type: string; label: string; numeric: boolean }[] = [
  { type: 'an1_add', label: '+', numeric: true },
  { type: 'an1_subtract', label: '-', numeric: true },
  { type: 'an1_multiply', label: '×', numeric: true },
  { type: 'an1_divide', label: '÷', numeric: true },
  { type: 'an1_mod', label: 'mod', numeric: true },
  { type: 'an1_greater_than', label: '>', numeric: true },
  { type: 'an1_less_than', label: '<', numeric: true },
  { type: 'an1_equals', label: '=', numeric: true },
  // Boolean operators — deliberately no numeric shadow, matching Scratch's
  // own empty hexagonal boolean sockets (see this module's own doc comment).
  { type: 'an1_and', label: 'and', numeric: false },
  { type: 'an1_or', label: 'or', numeric: false },
]

/** Every key name a "when/while key [ ] pressed"/"key [ ] pressed?" block
 * can produce — a fixed dropdown instead of a free-text field specifically
 * so an author can never type a name that doesn't match what
 * `App.tsx`'s `normalizeKeyName` actually forwards from the browser (a
 * real bug this session: `KeyboardEvent.key` for the spacebar is `" "`,
 * not the word "space", so a freely-typed field could silently never
 * match). Every value here is exactly one `normalizeKeyName` can produce. */
const KEY_OPTIONS: [string, string][] = [
  ['space', 'space'],
  ['up arrow', 'up'],
  ['down arrow', 'down'],
  ['left arrow', 'left'],
  ['right arrow', 'right'],
  ['enter', 'enter'],
  ['escape', 'escape'],
  ['tab', 'tab'],
  ['shift', 'shift'],
  ...'abcdefghijklmnopqrstuvwxyz'.split('').map((c): [string, string] => [c, c]),
  ...'0123456789'.split('').map((d): [string, string] => [d, d]),
]

const MATH_FN_OPTIONS: [string, MathFn][] = [
  ['abs', 'Abs'],
  ['floor', 'Floor'],
  ['ceiling', 'Ceiling'],
  ['round', 'Round'],
  ['sqrt', 'Sqrt'],
  ['sin', 'Sin'],
  ['cos', 'Cos'],
  ['tan', 'Tan'],
  ['ln', 'Ln'],
  ['log10', 'Log10'],
]

/** A numeric `input_value` arg — see this module's own doc comment on
 * shadow blocks. The actual default-shadow attachment happens via
 * `numberShadowExtension` below, not here — a `shadow` key on an args0
 * entry looks like it should work (it's the same shape
 * `connection.setShadowState` takes) but Blockly's JSON block factory
 * (`inputFromJson`) silently never reads it; that key only means something
 * inside Blockly's own *serialized workspace* connection-state format. */
function numberShadow(name: string) {
  return { type: 'input_value', name }
}

const numberShadowExtensionCache = new Map<string, string>()

/** Registers (once per distinct name-set, cached) a Blockly extension that
 * attaches an `an1_number` shadow (default value 0) to each named
 * value-input the instant a block of this type is created — covers every
 * path a block instance comes from: a fresh flyout drag, `workspace.newBlock`
 * in this module's own decompiler, and reloading a saved program. Returns
 * the extension name to list in a block's JSON `extensions` array. */
function numberShadowExtension(names: string[]): string {
  const key = names.join(',')
  const cached = numberShadowExtensionCache.get(key)
  if (cached) return cached
  const extensionName = `an1_number_shadow_${key}`
  Blockly.Extensions.register(extensionName, function (this: Blockly.Block) {
    for (const name of names) {
      this.getInput(name)?.connection?.setShadowState({ type: 'an1_number', fields: { NUM: 0 } })
    }
  })
  numberShadowExtensionCache.set(key, extensionName)
  return extensionName
}

let defined = false

/** Registers every block type with Blockly — safe to call more than once
 * (guarded), since `BlockProgramEditor` calls it on every mount but block
 * definitions are process-global, not per-workspace. */
export function defineGameBlocks(): void {
  if (defined) return
  defined = true

  Blockly.common.defineBlocksWithJsonArray([
    { type: 'an1_when_state_triggered', message0: 'when state triggered', nextStatement: null, colour: COLOUR.events, tooltip: 'Runs once when this state becomes active.' },
    {
      type: 'an1_when_key_pressed',
      message0: 'when key %1 pressed',
      args0: [{ type: 'field_dropdown', name: 'KEY', options: KEY_OPTIONS }],
      message1: 'allow overlapping runs %1',
      args1: [{ type: 'field_checkbox', name: 'OVERLAP', checked: false }],
      nextStatement: null,
      colour: COLOUR.events,
      tooltip:
        'Runs once every time this key is pressed while this state is active. Off (default): a new press is ignored while the previous run is still going. On: every press starts another run alongside any still in progress — real Scratch\'s own behavior.',
    },
    {
      type: 'an1_while_key_pressed',
      message0: 'while key %1 pressed',
      args0: [{ type: 'field_dropdown', name: 'KEY', options: KEY_OPTIONS }],
      nextStatement: null,
      colour: COLOUR.events,
      tooltip:
        'Runs continuously for as long as this key is held — restarts on its own if the body finishes before the key is released (no Forever needed), and stops immediately, even mid-wait, the instant the key is released.',
    },
    {
      type: 'an1_when_clone_started',
      message0: 'when I start as a clone',
      nextStatement: null,
      colour: COLOUR.events,
      tooltip:
        'Runs once for a freshly-created clone, right after "create clone of" makes it. Inside this script, pick "★ myself (clone)" from any node dropdown to mean this specific clone — not whatever it was cloned from.',
    },
    {
      type: 'an1_wait',
      message0: 'wait %1 seconds',
      args0: [numberShadow('SECONDS')],
      extensions: [numberShadowExtension(['SECONDS'])],
      previousStatement: null,
      nextStatement: null,
      colour: COLOUR.control,
    },
    {
      type: 'an1_if',
      message0: 'if %1 then',
      args0: [{ type: 'input_value', name: 'CONDITION' }],
      message1: 'do %1',
      args1: [{ type: 'input_statement', name: 'THEN' }],
      previousStatement: null,
      nextStatement: null,
      colour: COLOUR.control,
    },
    {
      type: 'an1_if_else',
      message0: 'if %1 then',
      args0: [{ type: 'input_value', name: 'CONDITION' }],
      message1: 'do %1',
      args1: [{ type: 'input_statement', name: 'THEN' }],
      message2: 'else %1',
      args2: [{ type: 'input_statement', name: 'ELSE' }],
      previousStatement: null,
      nextStatement: null,
      colour: COLOUR.control,
    },
    {
      type: 'an1_repeat',
      message0: 'repeat %1',
      args0: [numberShadow('COUNT')],
      extensions: [numberShadowExtension(['COUNT'])],
      message1: 'do %1',
      args1: [{ type: 'input_statement', name: 'BODY' }],
      previousStatement: null,
      nextStatement: null,
      colour: COLOUR.control,
    },
    {
      type: 'an1_forever',
      message0: 'forever',
      message1: 'do %1',
      args1: [{ type: 'input_statement', name: 'BODY' }],
      previousStatement: null,
      // Deliberately no `nextStatement` — nothing can connect below a
      // `forever`, matching Scratch's own shape for this block exactly
      // (and `Statement::Forever`'s own semantics: it never falls through).
      colour: COLOUR.control,
    },
    {
      type: 'an1_for_each_node',
      message0: 'for each node with class %1',
      args0: [{ type: 'field_input', name: 'CLASS', text: 'enemy' }],
      message1: 'do %1',
      args1: [{ type: 'input_statement', name: 'BODY' }],
      previousStatement: null,
      nextStatement: null,
      colour: COLOUR.control,
      tooltip: 'Runs the body once per node tagged with this class (Inspector’s Classes field) — pick "★ current item" in a Motion/Sensing block’s node dropdown inside the body to act on whichever node this pass is on.',
    },
    {
      type: 'an1_key_pressed',
      message0: 'key %1 pressed?',
      args0: [{ type: 'field_dropdown', name: 'KEY', options: KEY_OPTIONS }],
      output: null,
      colour: COLOUR.sensing,
    },
    { type: 'an1_mouse_x', message0: 'mouse x', output: null, colour: COLOUR.sensing },
    { type: 'an1_mouse_y', message0: 'mouse y', output: null, colour: COLOUR.sensing },
    { type: 'an1_number', message0: '%1', args0: [{ type: 'field_number', name: 'NUM', value: 0 }], output: null, colour: COLOUR.operators },
    { type: 'an1_not', message0: 'not %1', args0: [{ type: 'input_value', name: 'A' }], output: null, colour: COLOUR.operators },
    ...BINARY_OPERATORS.map(({ type, label, numeric }) => ({
      type,
      // Both placeholders belong to `message0` itself, so both args live in
      // `args0` together (indexed in placeholder order: %1 -> args0[0], %2
      // -> args0[1]) — `args1` is only for a *second* message line
      // (`message1`), which this block doesn't have. Splitting them across
      // `args0`/`args1` (an earlier version of this) throws "args1 must
      // have a corresponding message" on every flyout render.
      message0: `%1 ${label} %2`,
      args0: numeric ? [numberShadow('A'), numberShadow('B')] : [{ type: 'input_value', name: 'A' }, { type: 'input_value', name: 'B' }],
      extensions: numeric ? [numberShadowExtension(['A', 'B'])] : [],
      inputsInline: true,
      output: null,
      colour: COLOUR.operators,
    })),
    {
      type: 'an1_get_number_input',
      message0: 'number input %1',
      args0: [{ type: 'field_input', name: 'NAME', text: 'speed' }],
      output: null,
      colour: COLOUR.sensing,
      tooltip: "Reads a live numeric input this node's own state machine declares.",
    },
    {
      type: 'an1_get_bool_input',
      message0: 'bool input %1',
      args0: [{ type: 'field_input', name: 'NAME', text: 'active' }],
      output: null,
      colour: COLOUR.sensing,
      tooltip: "Reads a live boolean input this node's own state machine declares.",
    },
    { type: 'an1_pi', message0: 'π', output: null, colour: COLOUR.operators, tooltip: 'The constant pi (~3.14159).' },
    {
      type: 'an1_math_fn',
      message0: '%1 of %2',
      args0: [{ type: 'field_dropdown', name: 'OP', options: MATH_FN_OPTIONS }, numberShadow('VALUE')],
      extensions: [numberShadowExtension(['VALUE'])],
      inputsInline: true,
      output: null,
      colour: COLOUR.operators,
      tooltip: 'Trig functions are in radians, matching this whole engine’s rotation unit.',
    },
    {
      type: 'an1_random_int',
      message0: 'pick random %1 to %2',
      args0: [numberShadow('MIN'), numberShadow('MAX')],
      extensions: [numberShadowExtension(['MIN', 'MAX'])],
      inputsInline: true,
      output: null,
      colour: COLOUR.operators,
      tooltip: 'A whole number, inclusive of both ends (order doesn’t matter).',
    },
    {
      type: 'an1_random_float',
      message0: 'pick random float %1 to %2',
      args0: [numberShadow('MIN'), numberShadow('MAX')],
      extensions: [numberShadowExtension(['MIN', 'MAX'])],
      inputsInline: true,
      output: null,
      colour: COLOUR.operators,
      tooltip: 'Order doesn’t matter — the smaller value is always the lower end.',
    },
  ])

  // A default `an1_number` shadow for a hand-built (not JSON-array) value
  // input — the imperative-API counterpart to `numberShadow` above, for the
  // handful of blocks that need a dynamic node-list dropdown and so can't
  // be expressed as a plain JSON definition at all.
  const withNumberShadow = (input: Blockly.Input, defaultValue = 0) => {
    input.connection?.setShadowState({ type: 'an1_number', fields: { NUM: defaultValue } })
    return input
  }

  Blockly.Blocks['an1_go_to_xy'] = {
    init(this: Blockly.Block) {
      this.appendDummyInput().appendField('go to').appendField(new Blockly.FieldDropdown(() => nodeOptions), 'NODE')
      withNumberShadow(this.appendValueInput('X')).appendField('x:')
      withNumberShadow(this.appendValueInput('Y')).appendField('y:')
      this.setInputsInline(true)
      this.setPreviousStatement(true, null)
      this.setNextStatement(true, null)
      this.setColour(COLOUR.motion)
    },
  }
  Blockly.Blocks['an1_set_property'] = {
    init(this: Blockly.Block) {
      this.appendDummyInput()
        .appendField('set')
        .appendField(new Blockly.FieldDropdown(() => nodeOptions), 'NODE')
        .appendField(new Blockly.FieldDropdown(PROPERTY_OPTIONS), 'PROPERTY')
        .appendField('to')
      withNumberShadow(this.appendValueInput('VALUE'))
      this.setInputsInline(true)
      this.setPreviousStatement(true, null)
      this.setNextStatement(true, null)
      this.setColour(COLOUR.motion)
    },
  }
  Blockly.Blocks['an1_change_property'] = {
    init(this: Blockly.Block) {
      this.appendDummyInput()
        .appendField('change')
        .appendField(new Blockly.FieldDropdown(() => nodeOptions), 'NODE')
        .appendField(new Blockly.FieldDropdown(PROPERTY_OPTIONS), 'PROPERTY')
        .appendField('by')
      withNumberShadow(this.appendValueInput('VALUE'))
      this.setInputsInline(true)
      this.setPreviousStatement(true, null)
      this.setNextStatement(true, null)
      this.setColour(COLOUR.motion)
    },
  }
  Blockly.Blocks['an1_get_property'] = {
    init(this: Blockly.Block) {
      this.appendDummyInput()
        .appendField('get')
        .appendField(new Blockly.FieldDropdown(() => nodeOptions), 'NODE')
        .appendField(new Blockly.FieldDropdown(PROPERTY_OPTIONS), 'PROPERTY')
      this.setInputsInline(true)
      this.setOutput(true, null)
      this.setColour(COLOUR.motion)
    },
  }
  Blockly.Blocks['an1_create_clone_of'] = {
    init(this: Blockly.Block) {
      this.appendDummyInput().appendField('create clone of').appendField(new Blockly.FieldDropdown(() => nodeOptions), 'NODE')
      this.setInputsInline(true)
      this.setPreviousStatement(true, null)
      this.setNextStatement(true, null)
      this.setColour(COLOUR.control)
      this.setTooltip('Spawns a live copy of the chosen node and immediately runs any "when I start as a clone" script for it — pick "★ myself (clone)" here to have a clone create another clone of itself.')
    },
  }
  Blockly.common.defineBlocksWithJsonArray([
    {
      type: 'an1_delete_this_clone',
      message0: 'delete this clone',
      previousStatement: null,
      nextStatement: null,
      colour: COLOUR.control,
      tooltip: 'Removes this clone from the artboard and stops this script right here. Outside a clone\'s own "when I start as a clone" script, this just stops the script — there\'s no clone to delete.',
    },
  ])
  Blockly.Blocks['an1_show'] = {
    init(this: Blockly.Block) {
      this.appendDummyInput().appendField('show').appendField(new Blockly.FieldDropdown(() => nodeOptions), 'NODE')
      this.setInputsInline(true)
      this.setPreviousStatement(true, null)
      this.setNextStatement(true, null)
      this.setColour(COLOUR.looks)
    },
  }
  Blockly.Blocks['an1_hide'] = {
    init(this: Blockly.Block) {
      this.appendDummyInput().appendField('hide').appendField(new Blockly.FieldDropdown(() => nodeOptions), 'NODE')
      this.setInputsInline(true)
      this.setPreviousStatement(true, null)
      this.setNextStatement(true, null)
      this.setColour(COLOUR.looks)
    },
  }
  Blockly.Blocks['an1_set_color'] = {
    init(this: Blockly.Block) {
      this.appendDummyInput().appendField('set color of').appendField(new Blockly.FieldDropdown(() => nodeOptions), 'NODE')
      withNumberShadow(this.appendValueInput('R')).appendField('r:')
      withNumberShadow(this.appendValueInput('G')).appendField('g:')
      withNumberShadow(this.appendValueInput('B')).appendField('b:')
      this.setInputsInline(true)
      this.setPreviousStatement(true, null)
      this.setNextStatement(true, null)
      this.setColour(COLOUR.looks)
      this.setTooltip('Each channel is 0-1, not 0-255 — matches how fill colour is stored everywhere else in this engine.')
    },
  }
  Blockly.Blocks['an1_set_size'] = {
    init(this: Blockly.Block) {
      this.appendDummyInput().appendField('set size of').appendField(new Blockly.FieldDropdown(() => nodeOptions), 'NODE').appendField('to')
      withNumberShadow(this.appendValueInput('PERCENT'), 100).appendField('%')
      this.setInputsInline(true)
      this.setPreviousStatement(true, null)
      this.setNextStatement(true, null)
      this.setColour(COLOUR.looks)
    },
  }
  Blockly.Blocks['an1_touching'] = {
    init(this: Blockly.Block) {
      this.appendDummyInput()
        .appendField(new Blockly.FieldDropdown(() => nodeOptions), 'A')
        .appendField('touching')
        .appendField(new Blockly.FieldDropdown(() => nodeOptions), 'B')
        .appendField('?')
      this.setInputsInline(true)
      this.setOutput(true, null)
      this.setColour(COLOUR.sensing)
    },
  }
  Blockly.Blocks['an1_touching_class'] = {
    init(this: Blockly.Block) {
      this.appendDummyInput()
        .appendField(new Blockly.FieldDropdown(() => nodeOptions), 'NODE')
        .appendField('touching any')
        .appendField(new Blockly.FieldTextInput('enemy'), 'CLASS')
        .appendField('?')
      this.setInputsInline(true)
      this.setOutput(true, null)
      this.setColour(COLOUR.sensing)
      this.setTooltip('True if this node overlaps *any* node tagged with this class (Inspector’s Classes field) — one reporter for "touching any enemy/coin/etc." instead of a ForEachNode + Touching loop for every pairwise check.')
    },
  }
}

export const GAME_TOOLBOX: Blockly.utils.toolbox.ToolboxDefinition = {
  kind: 'categoryToolbox',
  contents: [
    {
      kind: 'category',
      name: 'Events',
      colour: COLOUR.events,
      contents: [
        { kind: 'block', type: 'an1_when_state_triggered' },
        { kind: 'block', type: 'an1_when_key_pressed' },
        { kind: 'block', type: 'an1_while_key_pressed' },
        { kind: 'block', type: 'an1_when_clone_started' },
      ],
    },
    {
      kind: 'category',
      name: 'Control',
      colour: COLOUR.control,
      contents: [
        { kind: 'block', type: 'an1_wait' },
        { kind: 'block', type: 'an1_if' },
        { kind: 'block', type: 'an1_if_else' },
        { kind: 'block', type: 'an1_repeat' },
        { kind: 'block', type: 'an1_forever' },
        { kind: 'block', type: 'an1_for_each_node' },
        { kind: 'block', type: 'an1_create_clone_of' },
        { kind: 'block', type: 'an1_delete_this_clone' },
      ],
    },
    {
      kind: 'category',
      name: 'Motion',
      colour: COLOUR.motion,
      contents: [
        { kind: 'block', type: 'an1_go_to_xy' },
        { kind: 'block', type: 'an1_set_property' },
        { kind: 'block', type: 'an1_change_property' },
        { kind: 'block', type: 'an1_get_property' },
      ],
    },
    {
      kind: 'category',
      name: 'Looks',
      colour: COLOUR.looks,
      contents: [
        { kind: 'block', type: 'an1_show' },
        { kind: 'block', type: 'an1_hide' },
        { kind: 'block', type: 'an1_set_color' },
        { kind: 'block', type: 'an1_set_size' },
      ],
    },
    {
      kind: 'category',
      name: 'Sensing',
      colour: COLOUR.sensing,
      contents: [
        { kind: 'block', type: 'an1_touching' },
        { kind: 'block', type: 'an1_touching_class' },
        { kind: 'block', type: 'an1_key_pressed' },
        { kind: 'block', type: 'an1_mouse_x' },
        { kind: 'block', type: 'an1_mouse_y' },
        { kind: 'block', type: 'an1_get_number_input' },
        { kind: 'block', type: 'an1_get_bool_input' },
      ],
    },
    {
      kind: 'category',
      name: 'Operators',
      colour: COLOUR.operators,
      // No `an1_number` here — it's a shadow-only default now, not
      // something an author explicitly drags in (see this module's own
      // doc comment).
      contents: [
        ...BINARY_OPERATORS.map(({ type }) => ({ kind: 'block' as const, type })),
        { kind: 'block', type: 'an1_not' },
        { kind: 'block', type: 'an1_pi' },
        { kind: 'block', type: 'an1_math_fn' },
        { kind: 'block', type: 'an1_random_int' },
        { kind: 'block', type: 'an1_random_float' },
      ],
    },
  ],
}

export const GAME_THEME = Blockly.Theme.defineTheme('an1mbyt3', {
  name: 'an1mbyt3',
  // `Zelos` (not `Classic`) — the theme actually designed to pair with the
  // `zelos` renderer's rounded-pill block shapes; `Classic`'s own block
  // styles assume `geras`'s jagged notch shape instead.
  base: Blockly.Themes.Zelos,
  componentStyles: {
    workspaceBackgroundColour: '#15151a',
    toolboxBackgroundColour: 'rgba(30, 30, 36, 0.95)',
    toolboxForegroundColour: '#e8e8ec',
    flyoutBackgroundColour: 'rgba(30, 30, 36, 0.98)',
    flyoutForegroundColour: '#e8e8ec',
    flyoutOpacity: 1,
    // Thin, translucent white — matching this app's own `::-webkit-scrollbar`
    // convention (`index.css`) exactly, not Blockly's default thick, fully-
    // opaque accent-coloured bar.
    scrollbarColour: '#ffffff',
    scrollbarOpacity: 0.14,
    insertionMarkerColour: '#6d8cff',
    insertionMarkerOpacity: 0.5,
  },
})

// ---------------------------------------------------------------------------
// Compiler: Blockly workspace -> BlockProgramDef
// ---------------------------------------------------------------------------

function exprFromBlock(block: Blockly.Block | null): Expr {
  if (!block) return { kind: 'Number', value: 0 }
  const input = (name: string) => exprFromBlock(block.getInputTargetBlock(name))
  switch (block.type) {
    case 'an1_number':
      return { kind: 'Number', value: Number(block.getFieldValue('NUM')) }
    case 'an1_mouse_x':
      return { kind: 'MouseX' }
    case 'an1_mouse_y':
      return { kind: 'MouseY' }
    case 'an1_key_pressed':
      return { kind: 'KeyPressed', value: { key: String(block.getFieldValue('KEY')) } }
    case 'an1_touching':
      return { kind: 'Touching', value: { a: decodeNodeRef(block.getFieldValue('A')), b: decodeNodeRef(block.getFieldValue('B')) } }
    case 'an1_touching_class':
      return { kind: 'TouchingClass', value: { node: decodeNodeRef(block.getFieldValue('NODE')), class: String(block.getFieldValue('CLASS')) } }
    case 'an1_add':
      return { kind: 'Add', value: [input('A'), input('B')] }
    case 'an1_subtract':
      return { kind: 'Subtract', value: [input('A'), input('B')] }
    case 'an1_multiply':
      return { kind: 'Multiply', value: [input('A'), input('B')] }
    case 'an1_divide':
      return { kind: 'Divide', value: [input('A'), input('B')] }
    case 'an1_mod':
      return { kind: 'Mod', value: [input('A'), input('B')] }
    case 'an1_greater_than':
      return { kind: 'GreaterThan', value: [input('A'), input('B')] }
    case 'an1_less_than':
      return { kind: 'LessThan', value: [input('A'), input('B')] }
    case 'an1_equals':
      return { kind: 'Equals', value: [input('A'), input('B')] }
    case 'an1_and':
      return { kind: 'And', value: [input('A'), input('B')] }
    case 'an1_or':
      return { kind: 'Or', value: [input('A'), input('B')] }
    case 'an1_not':
      return { kind: 'Not', value: [input('A')] }
    case 'an1_pi':
      return { kind: 'Pi' }
    case 'an1_math_fn':
      return { kind: 'MathFn', value: { op: block.getFieldValue('OP') as MathFn, value: input('VALUE') } }
    case 'an1_random_int':
      return { kind: 'RandomInt', value: { min: input('MIN'), max: input('MAX') } }
    case 'an1_random_float':
      return { kind: 'RandomFloat', value: { min: input('MIN'), max: input('MAX') } }
    case 'an1_get_property':
      return { kind: 'GetProperty', value: { node: decodeNodeRef(block.getFieldValue('NODE')), property: block.getFieldValue('PROPERTY') as GraphProperty } }
    case 'an1_get_number_input':
      return { kind: 'GetNumberInput', value: { name: String(block.getFieldValue('NAME')) } }
    case 'an1_get_bool_input':
      return { kind: 'GetBoolInput', value: { name: String(block.getFieldValue('NAME')) } }
    default:
      console.warn(`gameBlocks: unrecognized reporter block type "${block.type}", treating as 0`)
      return { kind: 'Number', value: 0 }
  }
}

function statementsFromBlock(block: Blockly.Block | null): Statement[] {
  const statements: Statement[] = []
  let current = block
  while (current) {
    statements.push(...statementsFromSingleBlock(current))
    current = current.getNextBlock()
  }
  return statements
}

/** One block's own IR — almost always exactly one `Statement`, but a few
 * "Looks" convenience blocks (`an1_set_color`, `an1_set_size`) are sugar
 * over *several* `SetProperty` statements against existing `GraphProperty`
 * axes (no dedicated `Statement`/`Command` needed for either): one Blockly
 * block, multiple IR statements. `loadProgramIntoWorkspace` doesn't reverse
 * this pattern-match back into the sugared block on load — a saved program
 * that happens to hold e.g. three consecutive `FillColorR`/`G`/`B` writes
 * against the same node reopens as three plain "set [property] to" blocks,
 * not one "set color" block. A deliberate, cosmetic-only gap (still
 * functionally identical), not worth the pattern-recognition complexity for
 * a convenience block. */
function statementsFromSingleBlock(block: Blockly.Block): Statement[] {
  const value = (name: string) => exprFromBlock(block.getInputTargetBlock(name))
  const node = () => decodeNodeRef(block.getFieldValue('NODE'))
  const setProperty = (property: GraphProperty, value: Expr): Statement => ({ kind: 'SetProperty', value: { node: node(), property, value } })
  switch (block.type) {
    case 'an1_go_to_xy':
      return [{ kind: 'GoToXY', value: { node: node(), x: value('X'), y: value('Y') } }]
    case 'an1_set_property':
      return [{ kind: 'SetProperty', value: { node: node(), property: block.getFieldValue('PROPERTY') as GraphProperty, value: value('VALUE') } }]
    case 'an1_change_property':
      return [{ kind: 'ChangeProperty', value: { node: node(), property: block.getFieldValue('PROPERTY') as GraphProperty, delta: value('VALUE') } }]
    case 'an1_show':
      return [setProperty('Visible', { kind: 'Number', value: 1 })]
    case 'an1_hide':
      return [setProperty('Visible', { kind: 'Number', value: 0 })]
    case 'an1_set_color':
      return [setProperty('FillColorR', value('R')), setProperty('FillColorG', value('G')), setProperty('FillColorB', value('B'))]
    case 'an1_set_size': {
      // A single "size %" (Scratch's own convention) maps onto our
      // separate ScaleX/ScaleY axes — both driven by the same percent
      // expression, evaluated once each rather than shared, so a
      // non-deterministic expression (unlikely here, but not ruled out)
      // could in principle read differently for X vs Y; harmless in
      // practice since every reporter this phase's blocks can build is
      // pure.
      const percentToScale = (): Expr => ({ kind: 'Divide', value: [value('PERCENT'), { kind: 'Number', value: 100 }] })
      return [setProperty('ScaleX', percentToScale()), setProperty('ScaleY', percentToScale())]
    }
    case 'an1_wait':
      return [{ kind: 'Wait', value: { seconds: value('SECONDS') } }]
    case 'an1_if':
      return [
        {
          kind: 'If',
          value: { condition: value('CONDITION'), thenBranch: statementsFromBlock(block.getInputTargetBlock('THEN')), elseBranch: [] },
        },
      ]
    case 'an1_if_else':
      return [
        {
          kind: 'If',
          value: { condition: value('CONDITION'), thenBranch: statementsFromBlock(block.getInputTargetBlock('THEN')), elseBranch: statementsFromBlock(block.getInputTargetBlock('ELSE')) },
        },
      ]
    case 'an1_repeat':
      return [{ kind: 'Repeat', value: { count: value('COUNT'), body: statementsFromBlock(block.getInputTargetBlock('BODY')) } }]
    case 'an1_forever':
      return [{ kind: 'Forever', value: { body: statementsFromBlock(block.getInputTargetBlock('BODY')) } }]
    case 'an1_for_each_node':
      return [
        {
          kind: 'ForEachNode',
          value: { list: { kind: 'NodesWithClass', value: { class: String(block.getFieldValue('CLASS')) } }, body: statementsFromBlock(block.getInputTargetBlock('BODY')) },
        },
      ]
    case 'an1_create_clone_of':
      return [{ kind: 'CreateCloneOf', value: { node: node() } }]
    case 'an1_delete_this_clone':
      return [{ kind: 'DeleteThisClone' }]
    default:
      console.warn(`gameBlocks: unrecognized statement block type "${block.type}", treating as a no-op wait`)
      return [{ kind: 'Wait', value: { seconds: { kind: 'Number', value: 0 } } }]
  }
}

/** Walks every top-level block into a `BlockProgramDef` — see this
 * module's own doc comment. */
export function compileWorkspace(workspace: Blockly.Workspace): BlockProgramDef {
  const scripts: BlockScript[] = []
  for (const block of workspace.getTopBlocks(true)) {
    // Captured once per hat block, regardless of which event kind it turns
    // out to be — this is what lets `loadProgramIntoWorkspace` put each
    // script back exactly where its author dragged it instead of Blockly's
    // own default cascade, the fix for scripts "moving around" on every tab
    // switch (switching away from Blocks fully unmounts and later
    // re-decompiles this same `BlockProgramDef`, same as a save/reload).
    const xy = block.getRelativeToSurfaceXY()
    const position = { x: xy.x, y: xy.y }
    if (block.type === 'an1_when_state_triggered') {
      scripts.push({ event: { kind: 'StateTriggered' }, statements: statementsFromBlock(block.getNextBlock()), position })
    } else if (block.type === 'an1_when_key_pressed') {
      scripts.push({
        event: { kind: 'KeyPressed', value: { key: String(block.getFieldValue('KEY')), overlapping: block.getFieldValue('OVERLAP') === 'TRUE' } },
        statements: statementsFromBlock(block.getNextBlock()),
        position,
      })
    } else if (block.type === 'an1_while_key_pressed') {
      scripts.push({ event: { kind: 'KeyHeld', value: { key: String(block.getFieldValue('KEY')) } }, statements: statementsFromBlock(block.getNextBlock()), position })
    } else if (block.type === 'an1_when_clone_started') {
      scripts.push({ event: { kind: 'CloneStarted' }, statements: statementsFromBlock(block.getNextBlock()), position })
    }
    // Any other top-level block (a stray reporter/statement not under a
    // hat) simply isn't part of any script — an orphaned block does
    // nothing, same as real Scratch.
  }
  return { scripts }
}

// ---------------------------------------------------------------------------
// Decompiler: BlockProgramDef -> Blockly workspace
// ---------------------------------------------------------------------------

function newBlock(workspace: Blockly.WorkspaceSvg, type: string): Blockly.BlockSvg {
  const block = workspace.newBlock(type) as Blockly.BlockSvg
  block.initSvg()
  return block
}

function connectValue(block: Blockly.Block, inputName: string, valueBlock: Blockly.BlockSvg) {
  const connection = block.getInput(inputName)?.connection
  if (connection && valueBlock.outputConnection) connection.connect(valueBlock.outputConnection)
  valueBlock.render()
}

function blockFromExpr(workspace: Blockly.WorkspaceSvg, expr: Expr): Blockly.BlockSvg {
  const binary = (type: string, [a, b]: [Expr, Expr]) => {
    const block = newBlock(workspace, type)
    connectValue(block, 'A', blockFromExpr(workspace, a))
    connectValue(block, 'B', blockFromExpr(workspace, b))
    return block
  }
  switch (expr.kind) {
    case 'Number': {
      const block = newBlock(workspace, 'an1_number')
      block.setFieldValue(String(expr.value), 'NUM')
      return block
    }
    case 'MouseX':
      return newBlock(workspace, 'an1_mouse_x')
    case 'MouseY':
      return newBlock(workspace, 'an1_mouse_y')
    case 'KeyPressed': {
      const block = newBlock(workspace, 'an1_key_pressed')
      block.setFieldValue(expr.value.key, 'KEY')
      return block
    }
    case 'Touching': {
      const block = newBlock(workspace, 'an1_touching')
      block.setFieldValue(encodeNodeRef(expr.value.a), 'A')
      block.setFieldValue(encodeNodeRef(expr.value.b), 'B')
      return block
    }
    case 'TouchingClass': {
      const block = newBlock(workspace, 'an1_touching_class')
      block.setFieldValue(encodeNodeRef(expr.value.node), 'NODE')
      block.setFieldValue(expr.value.class, 'CLASS')
      return block
    }
    case 'Add':
      return binary('an1_add', expr.value)
    case 'Subtract':
      return binary('an1_subtract', expr.value)
    case 'Multiply':
      return binary('an1_multiply', expr.value)
    case 'Divide':
      return binary('an1_divide', expr.value)
    case 'Mod':
      return binary('an1_mod', expr.value)
    case 'GreaterThan':
      return binary('an1_greater_than', expr.value)
    case 'LessThan':
      return binary('an1_less_than', expr.value)
    case 'Equals':
      return binary('an1_equals', expr.value)
    case 'And':
      return binary('an1_and', expr.value)
    case 'Or':
      return binary('an1_or', expr.value)
    case 'Not': {
      const block = newBlock(workspace, 'an1_not')
      connectValue(block, 'A', blockFromExpr(workspace, expr.value[0]))
      return block
    }
    case 'Pi':
      return newBlock(workspace, 'an1_pi')
    case 'MathFn': {
      const block = newBlock(workspace, 'an1_math_fn')
      block.setFieldValue(expr.value.op, 'OP')
      connectValue(block, 'VALUE', blockFromExpr(workspace, expr.value.value))
      return block
    }
    case 'RandomInt': {
      const block = newBlock(workspace, 'an1_random_int')
      connectValue(block, 'MIN', blockFromExpr(workspace, expr.value.min))
      connectValue(block, 'MAX', blockFromExpr(workspace, expr.value.max))
      return block
    }
    case 'RandomFloat': {
      const block = newBlock(workspace, 'an1_random_float')
      connectValue(block, 'MIN', blockFromExpr(workspace, expr.value.min))
      connectValue(block, 'MAX', blockFromExpr(workspace, expr.value.max))
      return block
    }
    case 'GetProperty': {
      const block = newBlock(workspace, 'an1_get_property')
      block.setFieldValue(encodeNodeRef(expr.value.node), 'NODE')
      block.setFieldValue(expr.value.property, 'PROPERTY')
      return block
    }
    case 'GetNumberInput': {
      const block = newBlock(workspace, 'an1_get_number_input')
      block.setFieldValue(expr.value.name, 'NAME')
      return block
    }
    case 'GetBoolInput': {
      const block = newBlock(workspace, 'an1_get_bool_input')
      block.setFieldValue(expr.value.name, 'NAME')
      return block
    }
  }
}

function blockFromStatement(workspace: Blockly.WorkspaceSvg, statement: Statement): Blockly.BlockSvg {
  switch (statement.kind) {
    case 'GoToXY': {
      const block = newBlock(workspace, 'an1_go_to_xy')
      block.setFieldValue(encodeNodeRef(statement.value.node), 'NODE')
      connectValue(block, 'X', blockFromExpr(workspace, statement.value.x))
      connectValue(block, 'Y', blockFromExpr(workspace, statement.value.y))
      return block
    }
    case 'SetProperty': {
      const block = newBlock(workspace, 'an1_set_property')
      block.setFieldValue(encodeNodeRef(statement.value.node), 'NODE')
      block.setFieldValue(statement.value.property, 'PROPERTY')
      connectValue(block, 'VALUE', blockFromExpr(workspace, statement.value.value))
      return block
    }
    case 'ChangeProperty': {
      const block = newBlock(workspace, 'an1_change_property')
      block.setFieldValue(encodeNodeRef(statement.value.node), 'NODE')
      block.setFieldValue(statement.value.property, 'PROPERTY')
      connectValue(block, 'VALUE', blockFromExpr(workspace, statement.value.delta))
      return block
    }
    case 'Wait': {
      const block = newBlock(workspace, 'an1_wait')
      connectValue(block, 'SECONDS', blockFromExpr(workspace, statement.value.seconds))
      return block
    }
    case 'If': {
      // An empty `elseBranch` round-trips as the plain `an1_if` block, not
      // `an1_if_else` with a conspicuously-empty else slot — otherwise every
      // `an1_if` the author placed reverts to its if/else sibling the
      // instant they leave this tab and come back (saving/switching tabs
      // round-trips through this exact IR, the real bug report this fixes).
      const hasElse = statement.value.elseBranch.length > 0
      const block = newBlock(workspace, hasElse ? 'an1_if_else' : 'an1_if')
      connectValue(block, 'CONDITION', blockFromExpr(workspace, statement.value.condition))
      connectStack(workspace, block, 'THEN', statement.value.thenBranch)
      if (hasElse) connectStack(workspace, block, 'ELSE', statement.value.elseBranch)
      return block
    }
    case 'Repeat': {
      const block = newBlock(workspace, 'an1_repeat')
      connectValue(block, 'COUNT', blockFromExpr(workspace, statement.value.count))
      connectStack(workspace, block, 'BODY', statement.value.body)
      return block
    }
    case 'Forever': {
      const block = newBlock(workspace, 'an1_forever')
      connectStack(workspace, block, 'BODY', statement.value.body)
      return block
    }
    case 'ForEachNode': {
      const block = newBlock(workspace, 'an1_for_each_node')
      // `ListExpr` has only one variant so far — see that type's own doc
      // comment — so this field write doesn't need a switch of its own yet.
      block.setFieldValue(statement.value.list.value.class, 'CLASS')
      connectStack(workspace, block, 'BODY', statement.value.body)
      return block
    }
    case 'CreateCloneOf': {
      const block = newBlock(workspace, 'an1_create_clone_of')
      block.setFieldValue(encodeNodeRef(statement.value.node), 'NODE')
      return block
    }
    case 'DeleteThisClone':
      return newBlock(workspace, 'an1_delete_this_clone')
  }
}

/** Builds `statements` as a connected block stack and plugs it into
 * `parent`'s named statement input — the shared tail of `If`/`Repeat`/
 * `Forever`'s decompilation. No-op for an empty list (an empty statement
 * input is simply left unconnected, matching what an author sees when they
 * haven't dropped anything into that slot themselves). */
function connectStack(workspace: Blockly.WorkspaceSvg, parent: Blockly.Block, inputName: string, statements: Statement[]) {
  if (statements.length === 0) return
  const blocks = statements.map((s) => blockFromStatement(workspace, s))
  chainAndConnect(parent.getInput(inputName)?.connection ?? null, blocks)
}

/** Same connecting logic as `connectStack`, but for a hat block's own
 * `nextConnection` instead of a named statement input — `compileWorkspace`'s
 * own read side (`getNextBlock()` walking down from a hat) is exactly what
 * this writes. */
function connectStackAfter(hat: Blockly.BlockSvg, statements: Statement[], workspace: Blockly.WorkspaceSvg) {
  if (statements.length === 0) return
  const blocks = statements.map((s) => blockFromStatement(workspace, s))
  chainAndConnect(hat.nextConnection, blocks)
}

function chainAndConnect(parentConnection: Blockly.Connection | null, blocks: Blockly.BlockSvg[]) {
  for (let i = 0; i < blocks.length - 1; i++) {
    if (blocks[i].nextConnection && blocks[i + 1].previousConnection) {
      blocks[i].nextConnection!.connect(blocks[i + 1].previousConnection!)
    }
  }
  if (parentConnection && blocks[0].previousConnection) {
    parentConnection.connect(blocks[0].previousConnection)
  }
  blocks.forEach((b) => b.render())
}

/** Rebuilds `workspace`'s contents from `program` — each script's own hat
 * block is placed back at its saved `position` (see `BlockScript.position`'s
 * own doc comment), falling back to a simple cascade only for a script
 * that's never been positioned yet. Called whenever the Blocks tab
 * (re)mounts or the underlying program changes out from under it (switching
 * to a
 * different Programmable State). */
export function loadProgramIntoWorkspace(workspace: Blockly.WorkspaceSvg, program: BlockProgramDef): void {
  workspace.clear()
  const hats: Blockly.BlockSvg[] = []
  for (const script of program.scripts) {
    const hat = (() => {
      switch (script.event.kind) {
        case 'StateTriggered':
          return newBlock(workspace, 'an1_when_state_triggered')
        case 'KeyPressed': {
          const block = newBlock(workspace, 'an1_when_key_pressed')
          block.setFieldValue(script.event.value.key, 'KEY')
          block.setFieldValue(script.event.value.overlapping ? 'TRUE' : 'FALSE', 'OVERLAP')
          return block
        }
        case 'KeyHeld': {
          const block = newBlock(workspace, 'an1_while_key_pressed')
          block.setFieldValue(script.event.value.key, 'KEY')
          return block
        }
        case 'CloneStarted':
          return newBlock(workspace, 'an1_when_clone_started')
      }
    })()
    connectStackAfter(hat, script.statements, workspace)
    hat.render()
    hats.push(hat)
  }
  // `cleanUp()` neatly re-stacks every top block — exactly right the very
  // first time a program has no saved positions at all, but it would
  // silently discard any position we're about to restore below (it
  // operates on the whole workspace, nothing selective). Once even one
  // script carries a saved `position` (this program has round-tripped
  // through the Blocks tab before), skip it entirely and place every hat
  // explicitly instead — a script that still lacks one (freshly added
  // alongside already-positioned siblings) falls back to a simple cascade
  // rather than fighting `cleanUp()` for the rest.
  const anyPositioned = program.scripts.some((s) => s.position)
  if (!anyPositioned) {
    workspace.cleanUp()
  } else {
    program.scripts.forEach((script, i) => {
      const hat = hats[i]
      const { x, y } = script.position ?? { x: 0, y: i * 160 }
      hat.moveTo(new Blockly.utils.Coordinate(x, y))
    })
  }
}
