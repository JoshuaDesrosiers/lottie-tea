/**
 * Interactive UI for Playground and exported games — the host half of the
 * engine's UI components (`an1mbyt3_core::ui`): it knows the pointer, so it
 * runs hover / press / drag, and drives the visuals by writing node
 * colours, images, visibility, positions and scales (the same untracked
 * writes scripts make — nothing here touches undo).
 *
 * - **UI Button** — Unity's `Selectable`: Normal / Highlighted / Pressed /
 *   Disabled state colours (Color Tint, faded over `fadeDuration`), or a
 *   different image per state (Sprite Swap), plus per-state scale; clicks.
 * - **UI Toggle** — click flips `isOn`; the "Checkmark" child shows while on.
 *   Toggles sharing a non-zero `group` are radio buttons.
 * - **UI Slider** — drag along the track; positions the "Fill" (stretched
 *   from the start to the value) and "Handle" (at the value) children.
 * - **UI Scrollbar** — the "Handle" child is sized / placed by `size` and
 *   `value`; drag it, or click the track to jump.
 * - **UI Scroll View** — the "Content" child scrolls (drag, mouse wheel,
 *   inertia) within the viewport; a "Scrollbar" child stays in sync.
 *
 * Pointer hits inside a scroll view only count within its viewport, so
 * scrolled-away list items can't be clicked.
 *
 * Children are found by *name* ("Fill", "Handle", "Checkmark", "Content",
 * "Scrollbar") — the same convention Unity's default UI prefabs use, and
 * what the + → UI menu builds.
 */
import type { OpaqueId, UiFillDirection, UiStateStyle } from '../ipc/document'
import type { ScriptNode } from './sceneMirror'

type Rgba = [number, number, number, number]
type Look = 'normal' | 'highlighted' | 'pressed' | 'disabled'

/** What the host gives the controls each frame. */
export interface UiFrame {
  mouse: { x: number; y: number }
  down: boolean
  /** Wheel delta this frame (pixels, +y = scroll down). */
  wheel: { x: number; y: number }
  dt: number
  /** Current local bounds of a node (scroll content changes size). */
  liveBounds: (node: OpaqueId) => [number, number, number, number]
  setFill: (node: OpaqueId, c: Rgba) => void
  setImage: (node: OpaqueId, image: OpaqueId) => void
  setVisible: (node: OpaqueId, visible: boolean) => void
  setTranslation: (node: OpaqueId, x: number, y: number) => void
  /** Scale *and* translation together (the engine's scale preview takes both). */
  setScale: (node: OpaqueId, sx: number, sy: number, tx: number, ty: number) => void
}

/** What scripts see on a control's `ref`. */
export interface UiControlState {
  hovered: boolean
  pressed: boolean
  clicked: boolean
  /** The value / isOn / scroll changed this frame (by the user or a script). */
  changed: boolean
  value: number
  isOn: boolean
  scroll: number
}

/** Per-node state (controls, and the parts they restyle). */
interface ControlState extends UiControlState {
  velocity: number
  /** A drag in progress: where it started, and where the handle was grabbed. */
  drag: { mouse: number; scroll: number; grab: number } | null
  /** A scroll view's override of its linked scrollbar's handle size. */
  sizeOverride: number | null
  /** The look before any feedback, and the look shown now. */
  baseColor: Rgba | null
  baseImage: OpaqueId | null
  baseScale: { x: number; y: number }
  color: Rgba | null
  scale: number
  look: Look | ''
  image: string
  /** The last layout written (skips identical writes). */
  layout: string
  /** Last frame's value / isOn, to detect script changes. */
  seen: string
}

const keyOf = (id: OpaqueId) => `${id.idx}:${id.version}`
const clamp01 = (v: number) => Math.max(0, Math.min(1, v))

/** A point in a node's local space (its world transform, inverted). */
function toLocal(n: ScriptNode, p: { x: number; y: number }): { x: number; y: number } {
  const t = n.worldTransform
  const dx = p.x - t.translation.x, dy = p.y - t.translation.y
  const c = Math.cos(-t.rotation), s = Math.sin(-t.rotation)
  return { x: (dx * c - dy * s) / (t.scale.x || 1) + t.anchor.x, y: (dx * s + dy * c) / (t.scale.y || 1) + t.anchor.y }
}

const inside = (b: [number, number, number, number], p: { x: number; y: number }) => p.x >= b[0] && p.x <= b[2] && p.y >= b[1] && p.y <= b[3]
const hit = (n: ScriptNode, p: { x: number; y: number }) => inside(n.bounds, toLocal(n, p))

/** Along-axis helpers for the four fill directions. */
function axis(direction: UiFillDirection) {
  const vertical = direction === 'BottomToTop' || direction === 'TopToBottom'
  const reversed = direction === 'RightToLeft' || direction === 'TopToBottom'
  return { vertical, reversed }
}

export class UiControls {
  private states = new Map<string, ControlState>()
  private session: unknown = null
  private wasDown = false

  /** What a script sees on `ref` (`clicked`, `value`, `isOn`, …). */
  state(node: OpaqueId): UiControlState {
    const s = this.states.get(keyOf(node))
    return { hovered: !!s?.hovered, pressed: !!s?.pressed, clicked: !!s?.clicked, changed: !!s?.changed, value: s?.value ?? 0, isOn: !!s?.isOn, scroll: s?.scroll ?? 0 }
  }

  /** `ref.value = v` (sliders, scrollbars), `ref.isOn = b`, `ref.scroll = v`.
   * Applied (and `changed` raised) on the next frame. */
  set(node: OpaqueId, patch: Partial<Pick<UiControlState, 'value' | 'isOn' | 'scroll'>>): void {
    const s = this.states.get(keyOf(node))
    if (!s) return
    if (patch.value !== undefined) s.value = patch.value
    if (patch.isOn !== undefined) s.isOn = patch.isOn
    if (patch.scroll !== undefined) {
      s.scroll = patch.scroll
      s.velocity = 0
    }
  }

  private stateFor(n: ScriptNode, value = 0, isOn = false): ControlState {
    const key = keyOf(n.id)
    let s = this.states.get(key)
    if (!s) {
      s = {
        hovered: false, pressed: false, clicked: false, changed: false, value, isOn, scroll: 0,
        velocity: 0, drag: null, sizeOverride: null,
        baseColor: n.fillColor, baseImage: n.image, baseScale: { x: n.transform.scale.x, y: n.transform.scale.y },
        color: n.fillColor, scale: 1, look: '', image: '', layout: '', seen: `${value}|${isOn}`,
      }
      this.states.set(key, s)
    }
    return s
  }

  /** Raises `changed` when the value / isOn moved since last frame (from
   * the pointer *or* a script's `ref.value = …`). */
  private markChanged(s: ControlState) {
    const now = `${s.value}|${s.isOn}`
    s.changed = now !== s.seen
    s.seen = now
  }

  /**
   * Shows a state's look (`style`) on `n`: Color Tint fades its fill (and
   * optionally its children's) toward base × state colour × multiplier;
   * Sprite Swap swaps its image; both apply the per-state scale unless
   * `scaled` is off (a scrollbar handle, whose scale is its size).
   */
  private applyStyle(f: UiFrame, n: ScriptNode, s: ControlState, style: UiStateStyle, look: Look, children: ScriptNode[], scaled = true) {
    const lookChanged = look !== s.look
    s.look = look
    if (style.transition === 'ColorTint' && s.baseColor) {
      const stateColor = look === 'highlighted' ? style.highlightedColor : look === 'pressed' ? style.pressedColor : look === 'disabled' ? style.disabledColor : style.normalColor
      const m = style.colorMultiplier
      const base = s.baseColor
      const target = [base[0] * stateColor[0] * m, base[1] * stateColor[1] * m, base[2] * stateColor[2] * m, base[3] * stateColor[3]].map((v) => Math.max(0, Math.min(1, v))) as Rgba
      const current = s.color ?? target
      // Fade toward the target (instant at fadeDuration 0).
      const k = style.fadeDuration > 0 ? Math.min(1, f.dt / style.fadeDuration) : 1
      const next = current.map((v, i) => v + (target[i] - v) * k) as Rgba
      const moved = next.some((v, i) => Math.abs(v - current[i]) > 1e-4) || lookChanged
      s.color = next
      if (moved) {
        f.setFill(n.id, next)
        if (style.tintChildren) {
          // Children get the same multiply, relative to their own colour.
          const ratio = next.map((v, i) => (base[i] > 1e-4 ? v / base[i] : 1))
          for (const c of children) {
            const cs = this.stateFor(c)
            if (cs.baseColor) f.setFill(c.id, cs.baseColor.map((v, i) => Math.min(1, v * ratio[i])) as Rgba)
          }
        }
      }
    }
    if (style.transition === 'SpriteSwap') {
      const swap = look === 'highlighted' ? style.highlightedImage : look === 'pressed' ? style.pressedImage : look === 'disabled' ? style.disabledImage : null
      const image = swap ?? s.baseImage
      const key = image ? keyOf(image) : ''
      if (image && key !== s.image) {
        s.image = key
        f.setImage(n.id, image)
      }
    }
    if (scaled && style.transition !== 'None') {
      const target = look === 'pressed' ? style.pressedScale : look === 'highlighted' ? style.highlightedScale : 1
      // Scale eases a little even at fade 0, so a press reads as a "push".
      const k = Math.min(1, f.dt / Math.max(0.04, style.fadeDuration))
      const next = s.scale + (target - s.scale) * k
      if (Math.abs(next - s.scale) > 1e-4 || (lookChanged && Math.abs(target - s.scale) > 1e-4)) {
        s.scale = Math.abs(next - target) < 1e-3 ? target : next
        f.setScale(n.id, s.baseScale.x * s.scale, s.baseScale.y * s.scale, n.transform.translation.x, n.transform.translation.y)
      }
    }
  }

  update(f: UiFrame, nodes: ScriptNode[], session: unknown): void {
    if (session !== this.session) {
      this.states.clear()
      this.session = session
    }
    const pressedNow = f.down && !this.wasDown
    const releasedNow = !f.down && this.wasDown
    this.wasDown = f.down
    // Most game scenes have no UI controls: skip building the per-frame
    // indexes below (they cost a few ms a frame in big scenes).
    if (!nodes.some((n) => n.button || n.toggle || n.slider || n.scrollbar || n.scrollView)) return
    const byKey = new Map(nodes.map((n) => [keyOf(n.id), n]))
    const kids = new Map<string, ScriptNode[]>()
    for (const n of nodes) {
      if (!n.parent) continue
      const k = keyOf(n.parent)
      const list = kids.get(k)
      if (list) list.push(n)
      else kids.set(k, [n])
    }
    const childrenOf = (n: ScriptNode) => kids.get(keyOf(n.id)) ?? []
    const child = (n: ScriptNode, name: string) => childrenOf(n).find((c) => c.name === name)
    // Pointer hits only count inside every scroll-view viewport above.
    const reachable = (n: ScriptNode) => {
      let p = n.parent ? byKey.get(keyOf(n.parent)) : undefined
      while (p) {
        if (p.scrollView && !hit(p, f.mouse)) return false
        p = p.parent ? byKey.get(keyOf(p.parent)) : undefined
      }
      return true
    }
    const lookOf = (interactable: boolean, s: ControlState) => (!interactable ? 'disabled' : s.pressed ? 'pressed' : s.hovered ? 'highlighted' : 'normal')
    /** Press / release / click for a simple clickable (button, toggle). */
    const clickable = (s: ControlState, interactable: boolean, over: boolean) => {
      s.clicked = false
      if (interactable) {
        if (pressedNow && over) s.pressed = true
        if (releasedNow) {
          s.clicked = s.pressed && over
          s.pressed = false
        }
      } else s.pressed = false
      s.hovered = over
    }

    // Toggles first, so radio groups settle before anything reads them.
    const groups = new Map<number, ScriptNode[]>()
    for (const n of nodes) if (n.toggle && n.toggle.group > 0) {
      const list = groups.get(n.toggle.group)
      if (list) list.push(n)
      else groups.set(n.toggle.group, [n])
    }

    for (const n of nodes) {
      if (!n.visible) continue

      // ---- Button ----
      if (n.button) {
        const cfg = n.button
        const s = this.stateFor(n)
        clickable(s, cfg.interactable, cfg.interactable && reachable(n) && hit(n, f.mouse))
        this.applyStyle(f, n, s, cfg.style, lookOf(cfg.interactable, s), childrenOf(n))
      }

      // ---- Toggle ----
      if (n.toggle) {
        const cfg = n.toggle
        const s = this.stateFor(n, 0, cfg.isOn)
        // The box or its label (any child) is the hit area.
        const over = cfg.interactable && reachable(n) && (hit(n, f.mouse) || childrenOf(n).some((c) => c.visible && hit(c, f.mouse)))
        clickable(s, cfg.interactable, over)
        if (s.clicked) {
          if (cfg.group > 0 && s.isOn && !cfg.allowSwitchOff) {
            // Radio: clicking the on one keeps it on.
          } else s.isOn = !s.isOn
        }
        if (cfg.group > 0 && s.isOn) {
          // Radio: this one on turns the rest of its group off.
          for (const other of groups.get(cfg.group) ?? []) {
            if (other === n) continue
            const os = this.stateFor(other, 0, other.toggle!.isOn)
            if (os.isOn && (s.clicked || s.seen !== `${s.value}|${s.isOn}`)) os.isOn = false
          }
        }
        this.applyStyle(f, n, s, cfg.style, lookOf(cfg.interactable, s), [])
      }

      // ---- Slider ----
      if (n.slider) {
        const cfg = n.slider
        const s = this.stateFor(n, cfg.value)
        s.clicked = false
        const { vertical, reversed } = axis(cfg.direction)
        const [x0, y0, x1, y1] = n.bounds
        const lo = vertical ? y0 : x0, hi = vertical ? y1 : x1
        const local = toLocal(n, f.mouse)
        const handle = child(n, 'Handle')
        const over = cfg.interactable && reachable(n) && (inside(n.bounds, local) || (!!handle && hit(handle, f.mouse)))
        if (cfg.interactable && pressedNow && over) s.pressed = true
        if (!f.down) s.pressed = false
        s.hovered = over
        if (s.pressed) {
          let t = clamp01(((vertical ? local.y : local.x) - lo) / Math.max(1e-4, hi - lo))
          if (reversed) t = 1 - t
          let v = cfg.min + t * (cfg.max - cfg.min)
          if (cfg.wholeNumbers) v = Math.round(v)
          s.value = v
        }
        const span = cfg.max - cfg.min
        let t = Math.abs(span) < 1e-6 ? 0 : clamp01((s.value - cfg.min) / span)
        if (reversed) t = 1 - t
        const at = lo + t * (hi - lo)
        // Fill: stretched from the start edge to the value.
        const fill = child(n, 'Fill')
        if (fill) {
          const fs = this.stateFor(fill)
          const from = reversed ? hi : lo
          const k = Math.max(0.0001, Math.abs(at - from) / Math.max(1e-4, hi - lo))
          const mid = (from + at) / 2
          const key = `${k.toFixed(4)}|${mid.toFixed(2)}`
          if (fs.layout !== key) {
            fs.layout = key
            if (vertical) f.setScale(fill.id, fs.baseScale.x, fs.baseScale.y * k, fill.transform.translation.x, mid)
            else f.setScale(fill.id, fs.baseScale.x * k, fs.baseScale.y, mid, fill.transform.translation.y)
          }
        }
        if (handle) {
          const hs = this.stateFor(handle)
          // Style first: its scale write also sets translation, which the
          // position write below must then override.
          this.applyStyle(f, handle, hs, cfg.style, lookOf(cfg.interactable, s), [])
          const hx = vertical ? handle.transform.translation.x : at
          const hy = vertical ? at : handle.transform.translation.y
          const key = `${hx.toFixed(2)}|${hy.toFixed(2)}`
          if (hs.layout !== key || Math.abs(hx - handle.transform.translation.x) > 1e-3 || Math.abs(hy - handle.transform.translation.y) > 1e-3) {
            hs.layout = key
            f.setTranslation(handle.id, hx, hy)
          }
        }
      }

      // ---- Scrollbar ----
      if (n.scrollbar) this.updateScrollbar(f, n, child(n, 'Handle'), pressedNow, reachable(n))

      // ---- Scroll view ----
      if (n.scrollView) {
        const cfg = n.scrollView
        const s = this.stateFor(n)
        const content = child(n, 'Content')
        if (!content) continue
        const vb = n.bounds
        const cb = f.liveBounds(content.id)
        const vertical = cfg.vertical || !cfg.horizontal
        const viewLen = vertical ? vb[3] - vb[1] : vb[2] - vb[0]
        const contentLen = vertical ? cb[3] - cb[1] : cb[2] - cb[0]
        const range = Math.max(0, contentLen - viewLen)
        const local = toLocal(n, f.mouse)
        const over = reachable(n) && inside(vb, local)
        s.hovered = over
        const bar = child(n, 'Scrollbar')
        const barState = bar ? this.states.get(keyOf(bar.id)) : undefined
        const overBar = !!bar && hit(bar, f.mouse)
        // Wheel and drag.
        if (over && (f.wheel.y || f.wheel.x)) {
          s.scroll += ((vertical ? f.wheel.y : f.wheel.x) / 100) * cfg.wheelStep
          s.velocity = 0
        }
        if (pressedNow && over && !overBar) s.drag = { mouse: vertical ? local.y : local.x, scroll: s.scroll, grab: 0 }
        if (s.drag && f.down) {
          const now = vertical ? local.y : local.x
          // Content follows the pointer (drag up = scroll down, vertically).
          const next = s.drag.scroll + (vertical ? now - s.drag.mouse : s.drag.mouse - now)
          s.velocity = (next - s.scroll) / Math.max(f.dt, 1e-3)
          s.scroll = next
        } else {
          s.drag = null
          if (cfg.inertia && Math.abs(s.velocity) > 1) {
            s.scroll += s.velocity * f.dt
            s.velocity *= Math.pow(0.04, f.dt)
          } else s.velocity = 0
        }
        // A dragged linked scrollbar drives the view instead.
        if (barState?.pressed) s.scroll = barState.value * range
        if (s.scroll <= 0 || s.scroll >= range) s.velocity = 0
        s.scroll = Math.max(0, Math.min(range, s.scroll))
        // Content's top (or left) edge sits at the viewport's, minus the scroll.
        const tx = vertical ? content.transform.translation.x : vb[0] - cb[0] - s.scroll
        const ty = vertical ? vb[3] - cb[3] + s.scroll : content.transform.translation.y
        if (Math.abs(tx - content.transform.translation.x) > 1e-3 || Math.abs(ty - content.transform.translation.y) > 1e-3) f.setTranslation(content.id, tx, ty)
        // Keep the scrollbar in sync (value + handle size).
        if (barState && !barState.pressed) {
          barState.value = range > 0 ? s.scroll / range : 0
          barState.sizeOverride = contentLen > 0 ? Math.min(1, viewLen / contentLen) : 1
        }
        s.value = range > 0 ? s.scroll / range : 0
      }
    }

    // Toggle checkmarks, and `changed` for everything.
    for (const n of nodes) {
      const s = this.states.get(keyOf(n.id))
      if (!s) continue
      if (n.toggle) {
        const check = child(n, 'Checkmark')
        if (check && check.visible !== s.isOn) f.setVisible(check.id, s.isOn)
      }
      if (n.toggle || n.slider || n.scrollbar || n.scrollView) this.markChanged(s)
    }
  }

  private updateScrollbar(f: UiFrame, n: ScriptNode, handle: ScriptNode | undefined, pressedNow: boolean, reachable: boolean): void {
    const cfg = n.scrollbar!
    const s = this.stateFor(n, cfg.value)
    const { vertical, reversed } = axis(cfg.direction)
    const [x0, y0, x1, y1] = n.bounds
    const lo = vertical ? y0 : x0, hi = vertical ? y1 : x1
    const track = Math.max(1e-4, hi - lo)
    const size = Math.max(0.05, Math.min(1, s.sizeOverride ?? cfg.size))
    const handleLen = track * size
    const travel = track - handleLen
    const local = toLocal(n, f.mouse)
    const coord = vertical ? local.y : local.x
    // value 0 = the start end (top for TopToBottom, left for LeftToRight).
    const valueToCenter = (v: number) => lo + handleLen / 2 + (reversed ? 1 - v : v) * travel
    const centerToValue = (c: number) => {
      const t = travel > 1e-4 ? clamp01((c - lo - handleLen / 2) / travel) : 0
      return reversed ? 1 - t : t
    }
    const center = valueToCenter(clamp01(s.value))
    const over = cfg.interactable && reachable && inside(n.bounds, local)
    if (cfg.interactable && pressedNow && over) {
      s.pressed = true
      // Grab the handle where it was clicked; a track click jumps there.
      const onHandle = Math.abs(coord - center) <= handleLen / 2
      s.drag = { mouse: coord, scroll: 0, grab: onHandle ? coord - center : 0 }
    }
    if (!f.down) {
      s.pressed = false
      s.drag = null
    }
    s.hovered = over
    if (s.pressed && s.drag) {
      let v = centerToValue(coord - s.drag.grab)
      if (cfg.steps > 1) v = Math.round(v * (cfg.steps - 1)) / (cfg.steps - 1)
      s.value = v
    }
    if (!handle) return
    const hs = this.stateFor(handle)
    const handleBase = vertical ? handle.bounds[3] - handle.bounds[1] : handle.bounds[2] - handle.bounds[0]
    const k = handleBase > 1e-4 ? handleLen / handleBase : 1
    const c = valueToCenter(clamp01(s.value))
    const key = `${k.toFixed(4)}|${c.toFixed(2)}`
    if (hs.layout !== key) {
      hs.layout = key
      if (vertical) f.setScale(handle.id, hs.baseScale.x, hs.baseScale.y * k, handle.transform.translation.x, c)
      else f.setScale(handle.id, hs.baseScale.x * k, hs.baseScale.y, c, handle.transform.translation.y)
    }
    // Handle colour by state (no scale feedback — its scale is its size).
    this.applyStyle(f, handle, hs, cfg.style, !cfg.interactable ? 'disabled' : s.pressed ? 'pressed' : over ? 'highlighted' : 'normal', [], false)
  }
}
