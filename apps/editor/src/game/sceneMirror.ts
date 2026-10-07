/**
 * The scene as Code-mode scripts see it, kept in sync cheaply — the JS half
 * of `crates/an1mbyt3-wasm/src/script_scene.rs` (read that module's doc for
 * the why). Per frame this costs one integer call (`script_revision`) and
 * one `Float32Array` copy (`script_dynamic`); the static node table is only
 * refetched when the scene's structure changes.
 *
 * The node objects are persistent `LiveNode`s whose transform / visibility /
 * fill / image getters read straight from the current frame's float block,
 * so `ctx.nodes` and refs always see this frame's values without anything
 * being rebuilt.
 */
import type { AudioSourceConfig, UiButtonConfig, UiScrollbarConfig, UiScrollViewConfig, UiSliderConfig, UiToggleConfig, ImageAssetSummary, NodeSummary, OpaqueId, Path, Transform2D } from '../ipc/document'
import type { WasmEditor } from '../wasm-pkg/an1mbyt3_wasm'

/** Floats per node — must match `script_scene::STRIDE`. */
const STRIDE = 31

/** What scripts can read about a node (a subset of `NodeSummary`). */
export type ScriptNode = Pick<
  NodeSummary,
  'id' | 'elementId' | 'name' | 'classes' | 'kind' | 'parent' | 'transform' | 'worldTransform' | 'visible' | 'fillColor' | 'textContent' | 'image' | 'path' | 'zIndex' | 'behaviors'
> & {
  /** The node's Audio Source component, if it has one. */
  audioSource: AudioSourceConfig | null
  button: UiButtonConfig | null
  slider: UiSliderConfig | null
  scrollbar: UiScrollbarConfig | null
  scrollView: UiScrollViewConfig | null
  toggle: UiToggleConfig | null
  bounds: [number, number, number, number]
  /** A Node3D's transform this frame (`null` for 2D nodes). */
  transform3d: { position: [number, number, number]; rotation: [number, number, number]; scale: [number, number, number] } | null
}

export type ScriptImage = Pick<ImageAssetSummary, 'id' | 'name'>

interface StaticNode {
  id: OpaqueId
  elementId: string
  name: string
  classes: string[]
  kind: NodeSummary['kind']
  parent: OpaqueId | null
  textContent: string | null
  path: Path | null
  bodyType: 'Dynamic' | 'Kinematic' | 'Static' | null
  audioSource: AudioSourceConfig | null
  button: UiButtonConfig | null
  slider: UiSliderConfig | null
  scrollbar: UiScrollbarConfig | null
  scrollView: UiScrollViewConfig | null
  toggle: UiToggleConfig | null
  /** Local bounds `[minX, minY, maxX, maxY]`. */
  bounds: [number, number, number, number]
}

interface StaticScene {
  revision: number
  nodes: StaticNode[]
  images: ScriptImage[]
}

class LiveNode {
  readonly id: OpaqueId
  readonly elementId: string
  readonly name: string
  readonly classes: string[]
  readonly kind: NodeSummary['kind']
  readonly parent: OpaqueId | null
  readonly textContent: string | null
  readonly path: Path | null
  readonly behaviors: NodeSummary['behaviors']
  /** The node's Audio Source component, if it has one. */
  readonly audioSource: AudioSourceConfig | null
  /** The node's UI Button component, if it has one. */
  readonly button: UiButtonConfig | null
  /** The node's other UI controls (handled by `UiControls`). */
  readonly slider: UiSliderConfig | null
  readonly scrollbar: UiScrollbarConfig | null
  readonly scrollView: UiScrollViewConfig | null
  readonly toggle: UiToggleConfig | null
  /** Local-space bounds `[minX, minY, maxX, maxY]`. */
  readonly bounds: [number, number, number, number]

  private readonly mirror: SceneMirror
  private readonly index: number

  constructor(mirror: SceneMirror, index: number, s: StaticNode) {
    this.mirror = mirror
    this.index = index
    this.id = s.id
    this.elementId = s.elementId
    this.name = s.name
    this.classes = s.classes
    this.kind = s.kind
    this.parent = s.parent
    this.textContent = s.textContent
    this.path = s.path
    this.audioSource = s.audioSource ?? null
    this.button = s.button ?? null
    this.slider = s.slider ?? null
    this.scrollbar = s.scrollbar ?? null
    this.scrollView = s.scrollView ?? null
    this.toggle = s.toggle ?? null
    this.bounds = s.bounds ?? [0, 0, 0, 0]
    // Only what scripts read (`ref.bodyType`): the Rigidbody 2D's type.
    this.behaviors = s.bodyType ? ([{ enabled: true, behavior: { kind: 'Rigidbody2D', bodyType: s.bodyType } }] as unknown as NodeSummary['behaviors']) : []
  }

  private transformAt(offset: number): Transform2D {
    const b = this.mirror.block
    const o = this.index * STRIDE + offset
    return { translation: { x: b[o], y: b[o + 1] }, rotation: b[o + 2], scale: { x: b[o + 3], y: b[o + 4] }, anchor: { x: b[o + 5], y: b[o + 6] } } as Transform2D
  }

  get transform(): Transform2D {
    return this.transformAt(0)
  }
  get worldTransform(): Transform2D {
    return this.transformAt(7)
  }
  get visible(): boolean {
    return this.mirror.block[this.index * STRIDE + 14] === 1
  }
  get zIndex(): number {
    return this.mirror.block[this.index * STRIDE + 15]
  }
  get fillColor(): [number, number, number, number] | null {
    const b = this.mirror.block
    const o = this.index * STRIDE + 16
    return Number.isNaN(b[o]) ? null : [b[o], b[o + 1], b[o + 2], b[o + 3]]
  }
  get image(): OpaqueId | null {
    const b = this.mirror.block
    const o = this.index * STRIDE + 20
    return Number.isNaN(b[o]) ? null : { idx: b[o], version: b[o + 1] }
  }
  get transform3d(): ScriptNode['transform3d'] {
    const b = this.mirror.block
    const o = this.index * STRIDE + 22
    if (Number.isNaN(b[o])) return null
    return { position: [b[o], b[o + 1], b[o + 2]], rotation: [b[o + 3], b[o + 4], b[o + 5]], scale: [b[o + 6], b[o + 7], b[o + 8]] }
  }
}

export class SceneMirror {
  /** This frame's per-node float block (read by `LiveNode`). */
  block: Float32Array = new Float32Array(0)
  nodes: ScriptNode[] = []
  images: ScriptImage[] = []
  private sceneKey = ''
  private revision = -1

  /** Brings the mirror up to date for `artboard`; returns its nodes. */
  update(editor: WasmEditor, artboard: OpaqueId): ScriptNode[] {
    const key = `${artboard.idx}:${artboard.version}`
    const revision = editor.script_revision(artboard)
    if (key !== this.sceneKey || revision !== this.revision) {
      const scene = editor.script_scene(artboard) as StaticScene
      this.sceneKey = key
      this.revision = scene.revision
      this.nodes = scene.nodes.map((s, i) => new LiveNode(this, i, s) as unknown as ScriptNode)
      this.images = scene.images
    }
    this.block = editor.script_dynamic(artboard)
    return this.nodes
  }
}
