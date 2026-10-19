/**
 * Runs Unity-style **On Click** actions (`an1mbyt3_core::UiEvents`, see
 * `crates/an1mbyt3-core/src/events.rs`), and holds the script functions
 * they can call (`ctx.on('openShop', fn)`).
 *
 * `ScriptHost` calls `runActions` when a node with a UI Button and an
 * events component is clicked; each action runs in order against its
 * target node (or the clicked node). A bad action (a missing node, a field
 * that doesn't exist) is reported in the Console and skipped — the rest
 * still run.
 *
 * `COMPONENT_METHODS` is the catalogue of callable component methods,
 * shared with the Inspector's action picker.
 */
import type { ActionKind, EventAction, OpaqueId } from '../ipc/document'
import { gameConsole, type ConsoleOrigin } from './console'
import { embedBridge } from './embedBridge'

/** Component methods an action can call: component kind → methods. */
export const COMPONENT_METHODS: Record<string, { method: string; label: string; arg?: string }[]> = {
  ParticleSystem: [{ method: 'emit', label: 'Emit particles', arg: 'Count' }],
  ParticleSystem3D: [{ method: 'burst', label: 'Burst particles', arg: 'Count' }],
  AudioSource: [
    { method: 'play', label: 'Play' },
    { method: 'stop', label: 'Stop' },
  ],
  Tree2D: [
    { method: 'shake', label: 'Shake', arg: 'Strength' },
    { method: 'regrow', label: 'Regrow' },
  ],
  Cloth2D: [{ method: 'reset', label: 'Mend (reset)' }],
  Rigidbody2D: [{ method: 'impulse', label: 'Add impulse', arg: 'x, y' }],
  UiToggle: [{ method: 'toggle', label: 'Toggle' }],
}

/** What actions do their work through (provided by `ScriptHost`). */
export interface ActionEnv {
  /** Where the clicked node is, for Console messages. */
  origin: ConsoleOrigin
  nodeName(node: OpaqueId): string | null
  isVisible(node: OpaqueId): boolean
  setVisible(node: OpaqueId, visible: boolean): void
  setNodeField(node: OpaqueId, field: string, value: unknown): boolean
  setComponentField(node: OpaqueId, component: string, field: string, value: string): boolean
  callMethod(node: OpaqueId, component: string, method: string, arg: string): boolean
  fireTrigger(input: string): void
  setInput(input: string, value: string): void
  /** Flips a Bool input. */
  toggleInput(input: string): void
  loadScene(scene: string): void
  playSound(sound: string): void
  setProperty(property: string, value: string): boolean
}

// ---- script functions (`ctx.on`) ----

type ScriptFunction = (arg?: unknown) => unknown

let functions = new Map<string, ScriptFunction>()

/** Makes `next` the active `ctx.on` registry; returns the one it replaced.
 * Each embedded player has its own (`runtimeContext.ts`), so two games on
 * one page can both register `reset` without clobbering each other. */
export function swapFunctions(next: Map<string, ScriptFunction>): Map<string, ScriptFunction> {
  const previous = functions
  functions = next
  return previous
}

/** `ctx.on(name, fn)`: makes `fn` callable from On Click "Call function". */
export function registerFunction(name: string, fn: (arg?: unknown) => unknown): void {
  if (typeof fn === 'function') functions.set(name, fn)
}

/** A new play session: forget the previous run's functions. */
/** Runs a script function registered with `ctx.on(name, fn)` — the
 * embed API's `player.call(name, arg)`. `false` if none is registered. */
export function callFunction(name: string, arg?: unknown): { found: boolean; result?: unknown } {
  const fn = functions.get(name)
  if (!fn) return { found: false }
  return { found: true, result: fn(arg) }
}

export function resetFunctions(): void {
  functions.clear()
}

function parseArg(arg: string): unknown {
  if (arg.trim() === '') return undefined
  try {
    return JSON.parse(arg)
  } catch {
    return arg
  }
}

/** One action's human description ("Set score to +1"). */
export function describeAction(a: ActionKind): string {
  if (a === 'ToggleVisible') return 'Toggle visible'
  if ('SetVisible' in a) return a.SetVisible.visible ? 'Show' : 'Hide'
  if ('SetNodeField' in a) return `Set ${a.SetNodeField.field} = ${a.SetNodeField.value}`
  if ('SetComponentField' in a) return `Set ${a.SetComponentField.component}.${a.SetComponentField.field} = ${a.SetComponentField.value}`
  if ('CallMethod' in a) return `${a.CallMethod.component}.${a.CallMethod.method}(${a.CallMethod.arg})`
  if ('FireTrigger' in a) return `Fire trigger ${a.FireTrigger.input}`
  if ('SetInput' in a) return `Set input ${a.SetInput.input} = ${a.SetInput.value}`
  if ('ToggleInput' in a) return `Toggle input ${a.ToggleInput.input}`
  if ('LoadScene' in a) return `Load scene ${a.LoadScene.scene}`
  if ('PlaySound' in a) return `Play sound ${a.PlaySound.sound}`
  if ('SetProperty' in a) return `Set ${a.SetProperty.property} = ${a.SetProperty.value}`
  if ('CallFunction' in a) return `Call ${a.CallFunction.function}(${a.CallFunction.arg})`
  return '?'
}

/** Runs `actions` for a click on `self`. */
export function runActions(actions: readonly EventAction[], self: OpaqueId, env: ActionEnv): void {
  for (const step of actions) {
    const target = step.target ?? self
    const a = step.action
    const fail = (why: string) => gameConsole.add('warn', `On Click: ${describeAction(a)} — ${why}`, { ...env.origin, kind: 'Event' })
    try {
      if (a !== 'ToggleVisible' && !('FireTrigger' in a) && !('SetInput' in a) && !('ToggleInput' in a) && !('LoadScene' in a) && !('PlaySound' in a) && !('SetProperty' in a) && !('CallFunction' in a) && env.nodeName(target) === null) {
        fail('its target node no longer exists')
        continue
      }
      if (a === 'ToggleVisible') env.setVisible(target, !env.isVisible(target))
      else if ('SetVisible' in a) env.setVisible(target, a.SetVisible.visible)
      else if ('SetNodeField' in a) {
        if (!env.setNodeField(target, a.SetNodeField.field, parseArg(a.SetNodeField.value))) fail(`${env.nodeName(target)} has no field "${a.SetNodeField.field}"`)
      } else if ('SetComponentField' in a) {
        const { component, field, value } = a.SetComponentField
        if (!env.setComponentField(target, component, field, value)) fail(`${env.nodeName(target)} has no ${component} with a "${field}" that takes ${value}`)
      } else if ('CallMethod' in a) {
        const { component, method, arg } = a.CallMethod
        if (!env.callMethod(target, component, method, arg)) fail(`${env.nodeName(target)} has no ${component} to call ${method} on`)
      } else if ('FireTrigger' in a) env.fireTrigger(a.FireTrigger.input)
      else if ('SetInput' in a) env.setInput(a.SetInput.input, a.SetInput.value)
      else if ('ToggleInput' in a) env.toggleInput(a.ToggleInput.input)
      else if ('LoadScene' in a) env.loadScene(a.LoadScene.scene)
      else if ('PlaySound' in a) env.playSound(a.PlaySound.sound)
      else if ('SetProperty' in a) {
        if (!env.setProperty(a.SetProperty.property, a.SetProperty.value)) fail(`this scene has no property "${a.SetProperty.property}"`)
      } else if ('CallFunction' in a) {
        const fn = functions.get(a.CallFunction.function)
        // No script handles it: on an embedded page it becomes a page
        // event (`player.on('event', …)`), so buttons can talk to the
        // site with no code in the game at all.
        if (!fn && embedBridge.embedded) embedBridge.emit(a.CallFunction.function, parseArg(a.CallFunction.arg), env.origin)
        else if (!fn) fail(`no script registered "${a.CallFunction.function}" (use ctx.on('${a.CallFunction.function}', () => { … }) in a script, or listen for it on the page when embedded)`)
        else fn(parseArg(a.CallFunction.arg))
      }
    } catch (err) {
      gameConsole.add('error', `On Click: ${describeAction(a)} threw ${err instanceof Error ? `${err.name}: ${err.message}` : String(err)}`, { ...env.origin, kind: 'Event' })
    }
  }
}
