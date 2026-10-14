/**
 * The docs shipped with "Export → Website embed" (`game/exportEmbed.ts`):
 * - `README.md` — what it is + quick start, pointing to the other two;
 * - `GUIDE.md`  — a plain-language walkthrough for people;
 * - `AGENTS.md` — the exact API, rules and recipes for AI coding
 *   assistants (also copied into `src/an1mbyt3/` with a `CLAUDE.md`
 *   pointer, so it travels with the code into the host repo);
 * - `an1mbyt3.json` — this game's inputs / events as data.
 *
 * Each doc is a Markdown template (`embed/docs/*.md`, imported raw);
 * `{{TITLE}}`, `{{SURFACE}}` (this game's scenes /
 * Properties / states / inputs / events / functions) and `{{MANIFEST}}`
 * (the same as JSON) are filled per export. Keep the API descriptions in
 * step with `player/engine.ts` and `embed/*`.
 */
import type { DocumentSummary } from '../ipc/document'

import README_TEMPLATE from '../embed/docs/README.md?raw'
import GUIDE_TEMPLATE from '../embed/docs/GUIDE.md?raw'
import AGENTS_TEMPLATE from '../embed/docs/AGENTS.md?raw'

/** Names scripts pass to `ctx.emit('…')` / `ctx.on('…')`, found in the
 * document's code (state programs and project files). */
export function scriptNames(doc: DocumentSummary, call: 'emit' | 'on'): string[] {
  const text = JSON.stringify(doc)
  const found = new Set<string>()
  const pattern = new RegExp(String.raw`ctx\.${call}\(\s*\\?['"\x60]([^'"\x60\\]+)`, 'g')
  for (const m of text.matchAll(pattern)) found.add(m[1])
  return [...found].sort()
}

/** Keys scripts read from `ctx.data` (`ctx.data.key` / `ctx.data['key']`). */
export function dataKeys(doc: DocumentSummary): string[] {
  const text = JSON.stringify(doc)
  const found = new Set<string>()
  for (const m of text.matchAll(/ctx\.data\.([A-Za-z_$][\w$]*)/g)) found.add(m[1])
  for (const m of text.matchAll(/ctx\.data\[\s*\\?['"\x60]([^'"\x60\\]+)/g)) found.add(m[1])
  return [...found].sort()
}

type Condition = DocumentSummary['artboards'][number]['stateMachines'][number]['states'][number]['transitions'][number]['condition']

/** A transition's condition in words, with the call that triggers it. */
function describeCondition(c: Condition): string {
  switch (c.kind) {
    case 'Trigger':
      return `trigger \`${c.input}\` fires — \`fire('${c.input}')\``
    case 'BoolIs':
      return `\`${c.input}\` is ${c.value} — \`setInput('${c.input}', ${c.value})\``
    case 'NumberGreaterThan':
      return `\`${c.input}\` > ${c.threshold} — \`setInput('${c.input}', n)\``
    case 'NumberLessThan':
      return `\`${c.input}\` < ${c.threshold} — \`setInput('${c.input}', n)\``
    case 'EnumIs':
      return `\`${c.input}\` is "${c.value}" — \`setInput('${c.input}', '${c.value}')\``
  }
}

/** Everything a host page can use, as data. */
export function gameManifest(doc: DocumentSummary, title: string, startScene: string) {
  return {
    title,
    startScene,
    scenes: doc.artboards.map((scene) => ({
      name: scene.name,
      properties: (doc.properties ?? [])
        .filter((p) => p.artboard.idx === scene.id.idx && p.artboard.version === scene.id.version)
        .map((p) => ({ name: p.name, kind: p.kind, default: p.default })),
      stateMachines: scene.stateMachines.map((m) => ({
        name: m.name,
        initialState: m.initialState,
        states: m.states.map((s) => s.name),
        transitions: [
          ...m.states.flatMap((s) => s.transitions.map((t) => ({ layer: null as string | null, from: s.name, to: t.to, when: describeCondition(t.condition) }))),
          ...m.layers.flatMap((l) => l.states.flatMap((s) => s.transitions.map((t) => ({ layer: l.name as string | null, from: s.name, to: t.to, when: describeCondition(t.condition) })))),
        ],
        layers: m.layers.map((l) => ({ name: l.name, initialState: l.initialState, states: l.states.map((s) => s.name) })),
        inputs: m.inputs.map((i) => ({ name: i.name, kind: i.kind })),
      })),
    })),
    dataKeys: dataKeys(doc),
    events: scriptNames(doc, 'emit'),
    functions: scriptNames(doc, 'on'),
  }
}

type Manifest = ReturnType<typeof gameManifest>

/** The manifest as readable Markdown. */
function surface(m: Manifest): string {
  const code = (s: string) => '`' + s + '`'
  const lines: string[] = []
  for (const scene of m.scenes) {
    lines.push(`### Scene ${code(scene.name)}${scene.name === m.startScene ? ' — opens first' : ''}`, '')
    if (scene.properties.length) {
      lines.push('**Properties** (send with `set` / `data`; the game reports its own changes as `prop` events):', '')
      for (const p of scene.properties) lines.push(`- ${code(p.name)} — ${p.kind}, starts at ${code(p.default)}`)
      lines.push('')
    }
    for (const sm of scene.stateMachines) {
      lines.push(`**State machine ${code(sm.name)}** — starts in ${code(sm.initialState)}; states reported by \`state\` events: ${sm.states.map(code).join(', ') || '(none)'}`)
      for (const l of sm.layers) lines.push(`- layer ${code(l.name)} (starts in ${code(l.initialState)}): ${l.states.map(code).join(', ')}`)
      if (sm.transitions.length) {
        lines.push('', 'How to move between states:', '')
        for (const t of sm.transitions) lines.push(`- ${t.layer ? `[${t.layer}] ` : ''}${code(t.from)} → ${code(t.to)} when ${t.when}`)
      }
      if (sm.inputs.length) {
        lines.push('', 'Inputs (`setInput` / `inputs`; Triggers also with `fire`):', '')
        for (const i of sm.inputs) lines.push(`- ${code(i.name)} — ${i.kind}`)
      }
      lines.push('')
    }
    if (!scene.properties.length && !scene.stateMachines.length) lines.push('(no Properties or state machine)', '')
  }
  lines.push(`**Data the game reads** (\`ctx.data.…\` — send with \`set\` / \`data\`): ${m.dataKeys.length ? m.dataKeys.map(code).join(', ') : 'none found in its scripts'}`, '')
  lines.push(`**Events the game sends** (\`ctx.emit\`): ${m.events.length ? m.events.map(code).join(', ') : 'none found in its scripts'}`, '')
  lines.push(`**Functions you can call** (\`player.call\`, registered with \`ctx.on\`): ${m.functions.length ? m.functions.map(code).join(', ') : 'none found in its scripts'}`)
  lines.push('', "_Data keys, events and functions are found by scanning the game's scripts; names built at run time aren't listed. On Click \"Call script function\" names reach the page as events too._")
  return lines.join('\n')
}

function fill(template: string, m: Manifest): string {
  return template
    .split('{{TITLE}}')
    .join(m.title)
    .split('{{SURFACE}}')
    .join(surface(m))
    .split('{{MANIFEST}}')
    .join(JSON.stringify(m, null, 2))
}

/** Every doc for one export. */
export function embedDocs(doc: DocumentSummary, title: string, startScene: string) {
  const m = gameManifest(doc, title, startScene)
  return {
    readme: fill(README_TEMPLATE, m),
    guide: fill(GUIDE_TEMPLATE, m),
    agents: fill(AGENTS_TEMPLATE, m),
    /** `src/an1mbyt3/CLAUDE.md`: Claude Code reads CLAUDE.md, and it
     * imports the AI guide next to it. */
    claude: '# an1mbyt3 embed\n\nThis folder is an exported an1mbyt3 game player. Before writing code that embeds or drives it, read @AGENTS.md — the API, the rules, and this game\'s exact inputs and events.\n',
    manifest: JSON.stringify(m, null, 2) + '\n',
  }
}
