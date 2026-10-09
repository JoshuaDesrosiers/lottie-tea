/**
 * Shared document DTO shapes — mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s
 * `DocumentSummary`, not `an1mbyt3_core`'s internal `Document` type. See
 * that Rust file's doc comment for why: `Document`/`Artboard` hold nodes in
 * a `slotmap::SlotMap`, whose own `Serialize` impl produces its internal
 * slot array (including deleted-entry tombstones), not a clean id-keyed
 * map — the wrong shape for a frontend to consume.
 *
 * Purely type definitions now — no runtime functions here. Those used to
 * call Tauri's `invoke()` against native commands (`get_document`,
 * `set_node_translation`, etc.); the document and its editing now live in
 * the `WasmEditor` wasm module instead (`ipc/wasmEditor.ts`), which is why
 * these interfaces moved out of that removed IPC layer and into a
 * standalone types-only module. Kept the same shapes on purpose — nothing
 * consuming these types (`App.tsx`, `LayersPanel`, `InspectorPanel`) needed
 * to change.
 */

export interface Vec2 {
  x: number
  y: number
}

export interface Transform2D {
  translation: Vec2
  /** Radians. */
  rotation: number
  scale: Vec2
  /** Local-space pivot point rotation/scale pivot around — see
   * `an1mbyt3_core::Transform2D::anchor`'s doc comment. `{x: 0, y: 0}` for
   * a node whose anchor has never been moved. */
  anchor: Vec2
}

/** Opaque — a `an1mbyt3_core::NodeId`/`ArtboardId`'s own `{idx, version}`
 * serialization. The frontend never constructs or decodes one, only stores
 * whatever `WasmEditor::document_summary()` sent and echoes it back
 * verbatim as an argument to the edit/select calls in `ipc/wasmEditor.ts`. */
export interface OpaqueId {
  idx: number
  version: number
}

/** Which property a keyframe/track drives — mirrors
 * `an1mbyt3_core::PropertyKind`. */
export type PropertyName = 'Translation' | 'Rotation' | 'Scale' | 'FillColor' | 'Path' | 'MotionPath'

/** Mirrors `an1mbyt3_core::BlendMode`. */
export type BlendMode = 'Normal' | 'Multiply' | 'Screen' | 'Add'

/** Mirrors `an1mbyt3_core::GradientKind` — Affinity's gradient shapes.
 * See `GradientDef`'s own doc comment for how `start`/`end` are
 * interpreted per kind. */
export type GradientKind = 'Linear' | 'Radial' | 'Conic' | 'Bilinear'

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `GradientStopDto`. */
export interface GradientStop {
  /** Where along the gradient this stop's color sits, `0..=1`. */
  offset: number
  color: [number, number, number, number]
  /** Where the 50%-blend point sits within the segment from this stop to
   * the *next* one, `0..=1` of that segment's own span — Affinity's
   * draggable "midpoint" diamond. `0.5` is a plain, unbiased lerp. Unused
   * on the last stop (no segment follows it). See
   * `an1mbyt3_core::GradientStop::midpoint`'s own doc comment. */
  midpoint: number
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `GradientDefDto` — see
 * `an1mbyt3_core::GradientDef`'s own doc comment for the overall design
 * (an additive override of a `Shape`/`Text` node's flat `fillColor`, not a
 * restructure of it). `start`/`end` are in the node's own local (pre-
 * transform, path-space) coordinates:
 * - Linear: the axis color varies along.
 * - Radial: `start` is the center, `|end - start|` is the radius.
 * - Conic: `start` is the center, `end` marks the angle-0 reference direction.
 * - Bilinear: `start` is the corner, `|end - start|` is the radius along
 *   both the `start -> end` axis and its perpendicular (a max-norm
 *   distance) — square, corner-anchored bands ("L-shaped"). */
export interface GradientDef {
  kind: GradientKind
  start: Vec2
  end: Vec2
  /** Should stay sorted by ascending `offset` — the on-canvas handle and
   * the Inspector's stop list are both responsible for keeping this true;
   * neither the renderer nor this type enforces it. */
  stops: GradientStop[]
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `PathCommandDto` — one
 * drawing instruction in a vector path, SVG/PDF-style (move, line, cubic
 * Bézier curve, close). A discriminated union on `kind`. */
export type PathCommand =
  | { kind: 'MoveTo'; value: Vec2 }
  | { kind: 'LineTo'; value: Vec2 }
  | { kind: 'CubicTo'; value: { control1: Vec2; control2: Vec2; to: Vec2 } }
  | { kind: 'Close' }

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `PathDto` — a `Shape` node's
 * whole vector path, an ordered command list (possibly several subpaths,
 * each starting with its own `MoveTo`). */
export interface Path {
  commands: PathCommand[]
}

/** Mirrors `an1mbyt3_core::StrokeCap` — SVG's `stroke-linecap`. */
export type StrokeCap = 'Butt' | 'Square' | 'Round'

/** Mirrors `an1mbyt3_core::StrokeJoin` — SVG's `stroke-linejoin`. */
export type StrokeJoin = 'Miter' | 'Round' | 'Bevel'

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `StrokeDto` — a `Shape`
 * node's outline paint + geometry. Mirrors `fill_color`/`gradient`/`image`'s
 * own three-way "flat, or overridden by a gradient, or overridden by an
 * image" shape, for the stroke instead of the fill. */
export interface Stroke {
  width: number
  color: [number, number, number, number]
  gradient: GradientDef | null
  image: OpaqueId | null
  cap: StrokeCap
  join: StrokeJoin
}

/** Mirrors `an1mbyt3_core::ParticleRange` — a `min..=max` an author draws a
 * starting value from uniformly, each time a particle spawns. `min === max`
 * is a plain fixed value. */
export interface ParticleRange {
  min: number
  max: number
}

/** Mirrors `an1mbyt3_core::SimulationSpace`. */
export type SimulationSpace = 'Local' | 'World'

/** Mirrors `an1mbyt3_core::ParticleMain` — see that Rust type's own doc
 * comment for what each field does; this is the Main module exactly. */
export interface ParticleMain {
  duration: number
  looping: boolean
  startLifetime: ParticleRange
  startSpeed: ParticleRange
  startSize: ParticleRange
  /** Radians. */
  startRotation: ParticleRange
  startColor: [number, number, number, number]
  gravityModifier: number
  drag: number
  simulationSpace: SimulationSpace
  maxParticles: number
  playOnAwake: boolean
}

/** Mirrors `an1mbyt3_core::ParticleBurst`. */
export interface ParticleBurst {
  time: number
  count: number
  cycles: number
  interval: number
}

/** Mirrors `an1mbyt3_core::ParticleEmission`. */
export interface ParticleEmission {
  rateOverTime: number
  bursts: ParticleBurst[]
}

/** Mirrors `an1mbyt3_core::EmitterShape` — note the plain externally-tagged
 * shape (`{ Point: null }` / `{ Circle: { radius } }` / ...), not this
 * file's usual `{ kind, value }` convention — this type isn't part of the
 * Blocks/Graph scripting IR those tag/content types serve, so there was no
 * reason to add a second serde representation just for it. */
export type EmitterShape = { Point: null } | { Circle: { radius: number } } | { Cone: { angle: number; radius: number } } | { Box: { width: number; height: number } }

/** Mirrors `an1mbyt3_core::SizeKeyframe` — `sizeMultiplier` multiplies
 * `ParticleMain.startSize`, it doesn't replace it (`1.0` = unchanged). */
export interface SizeKeyframe {
  t: number
  sizeMultiplier: number
}

/** Mirrors `an1mbyt3_core::ParticleSystemNode` — the Inspector's whole
 * Particle System section, read/written in one piece via `setParticleSystem`.
 * See that Rust type's own module doc comment for this feature's overall
 * scope (the "core" module set, explicitly not Unity's full ~15). */
export interface ParticleSystemConfig {
  main: ParticleMain
  emission: ParticleEmission
  shape: EmitterShape
  /** Multiplies `main.startColor` by the gradient sampled at a particle's
   * own normalized age — reuses `GradientStop` directly (same `offset`/
   * `color`/`midpoint` shape fills already use). Empty means no tint over
   * lifetime at all. */
  colorOverLifetime: GradientStop[]
  /** Empty means every particle stays at its own fixed `startSize` for its
   * whole life. */
  sizeOverLifetime: SizeKeyframe[]
  /** Radians/second — a constant spin rate, not a full curve (see that
   * Rust field's own doc comment for why). */
  rotationSpeed: number
}

/** Mirrors `an1mbyt3_core::TilemapNode` — the Tilemap panel's whole grid,
 * read whole but (unlike `ParticleSystemConfig`) written one cell at a time
 * via `setTile`, not by sending this whole struct back. */
export interface TilemapConfig {
  tileset: OpaqueId
  tilesetWidthPx: number
  tilesetHeightPx: number
  tileWidthPx: number
  tileHeightPx: number
  mapWidth: number
  mapHeight: number
  cellSize: number
  /** Row-major, `mapWidth * mapHeight` entries — `null` is an empty cell,
   * a number is this tile's index into the tileset's own flat left-to-right,
   * top-to-bottom tile list. */
  tiles: (number | null)[]
}

/** Mirrors `an1mbyt3_core::CameraNode` — the Inspector's Camera section,
 * read and written whole (`setCamera`). */
export interface CameraConfig {
  /** Orthographic-size analog: the camera's frame is the scene's own
   * width x height divided by this. */
  zoom: number
  /** Unity's Camera "Background" — what Playground clears to while viewing
   * through this camera. Straight linear RGBA. */
  backgroundColor: [number, number, number, number]
  /** A node tracked during Playground (e.g. the player); `null` = fixed. */
  followTarget: OpaqueId | null
  /** Exponential smoothing rate per second; `0` snaps instantly. */
  followSmoothing: number
  /** URP-style vignette, applied while Playground views through this camera. */
  vignette: CameraVignette
  /** Unity's Pixel Perfect Camera. */
  pixelPerfect: PixelPerfectConfig
}

/** Unity Pixel Perfect Camera grid snapping. */
export type PixelGridSnapping = 'None' | 'PixelSnapping' | 'UpscaleRenderTexture'
/** Unity Pixel Perfect Camera crop frame. */
export type PixelCropFrame = 'None' | 'Pillarbox' | 'Letterbox' | 'Windowbox' | 'StretchFill'

/** Mirrors `dto::PixelPerfectDto`. Reference resolution = the scene's size;
 * one world unit = one art pixel. */
export interface PixelPerfectConfig {
  enabled: boolean
  gridSnapping: PixelGridSnapping
  cropFrame: PixelCropFrame
}

/** Mirrors `an1mbyt3_core::CameraVignette`. */
export interface CameraVignette {
  enabled: boolean
  /** Edge tint (alpha ignored — `intensity` is the strength). */
  color: [number, number, number, number]
  /** 0..1 — how strongly the edge color covers the image. */
  intensity: number
  /** 0..1.5 — where the clear center ends (1 ~ the frame's half-diagonal). */
  radius: number
  /** 0..1 — edge softness ("Smoothness" in URP). */
  blur: number
}

/** Unity Light 2D `Light Type`. */
export type Light2DType = 'Global' | 'Spot' | 'Freeform' | 'Sprite'
/** Unity 2D Renderer blend style: Multiply (light reveals) / Additive (light adds). */
export type LightBlendStyle = 'Multiply' | 'Additive'
/** Unity Light 2D `Overlap Operation`. */
export type LightOverlapOperation = 'Additive' | 'AlphaBlend' | 'Max'

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `Light2DDto` — Unity's
 * Light 2D component, read and written whole (`setLight2D`). */
export interface Light2DConfig {
  lightType: Light2DType
  color: [number, number, number, number]
  intensity: number
  blendStyle: LightBlendStyle
  overlapOperation: LightOverlapOperation
  lightOrder: number
  innerRadius: number
  outerRadius: number
  /** Full cone widths in degrees (360 = point light). */
  innerSpotAngle: number
  outerSpotAngle: number
  /** 0..1 — soft (0) .. hard (1) falloff. */
  falloffStrength: number
  /** Freeform polygon, node-local. */
  shapePoints: Vec2[]
  falloffSize: number
  sprite: OpaqueId | null
  spriteWidth: number
  spriteHeight: number
  shadowsEnabled: boolean
  shadowStrength: number
  shadowSoftness: number
  volumetricEnabled: boolean
  volumetricIntensity: number
  volumetricShadowsEnabled: boolean
  volumetricShadowStrength: number
  /** Crisp brightness rings in the falloff (0/1 = one solid pool). */
  falloffSteps: number
  /** Blur between the rings: 0 = crisp, 1 = smooth (Unity) gradient. */
  edgeBlur: number
  /** Pixel art: compute the light per n-unit block (0 = per pixel). */
  pixelSize: number
}

/** Mirrors `dto::TrailDto` — Unity's Trail Renderer, read and written whole. */
export interface TrailConfig {
  /** Seconds a point lives. */
  time: number
  minVertexDistance: number
  width: number
  /** Width multiplier along the trail, t 0 = head .. 1 = tail. */
  widthCurve: SizeKeyframe[]
  /** Color along the trail, offset 0 = head .. 1 = tail. */
  colorGradient: GradientStop[]
  emitting: boolean
  endCapVertices: number
  cornerVertices: number
  texture: OpaqueId | null
}

/** Unity Shadow Caster 2D `Casting Option`. */
export type ShadowCastingOption = 'CastShadow' | 'SelfShadow' | 'CastAndSelfShadow'

/** Mirrors `dto::ShadowCasterDto` — Unity's Shadow Caster 2D. */
export interface ShadowCasterConfig {
  enabled: boolean
  castingOption: ShadowCastingOption
  /** Shrinks the caster outline inward (world units). */
  trimEdge: number
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `KeyframeValueDto` — a
 * discriminated union so a `Keyframe`'s `value` shape always matches its
 * track's `property`. */
export type KeyframeValue =
  | { kind: 'Translation'; value: Vec2 }
  | { kind: 'Rotation'; value: number }
  | { kind: 'Scale'; value: Vec2 }
  | { kind: 'FillColor'; value: [number, number, number, number] }
  | { kind: 'Path'; value: Path }
  | { kind: 'MotionPath'; value: number }

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `EasingDto` — how a
 * keyframe segment's linear progress maps to eased progress. `Linear` is
 * the identity mapping; `CubicBezier` is the same two-free-handle curve
 * model as CSS's `cubic-bezier()` (fixed endpoints at `(0,0)`/`(1,1)`). */
export type Easing = { kind: 'Linear' } | { kind: 'CubicBezier'; x1: number; y1: number; x2: number; y2: number }

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `KeyframeDto` — one
 * authored keyframe. `easing` is the curve used to interpolate *out of*
 * this keyframe toward the next one — double-clicking a keyframe in the
 * Timeline opens the Curve Editor panel for the *previous* keyframe's
 * `easing`, since that's the one governing the segment leading into it. */
export interface Keyframe {
  time: number
  value: KeyframeValue
  easing: Easing
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `PropertyTrackSummary` —
 * one property's full keyframe track, for one Timeline row. */
export interface PropertyTrackSummary {
  property: PropertyName
  /** Always in ascending `time` order — the Timeline panel indexes into
   * this array directly when previewing/committing a keyframe drag. */
  keyframes: Keyframe[]
  /** Only present for `property: 'MotionPath'` — which node's `Path` this
   * track's progress keyframes are sampled against. `null` for every other
   * property kind. */
  guide: OpaqueId | null
}

export interface NodeSummary {
  id: OpaqueId
  name: string
  /** Mirrors `an1mbyt3_core::Node::id` — the author-facing "tag" a
   * Code-mode script's `getCompByID` (`scriptRuntime.ts`) looks a node up
   * by, like an HTML element's `id` attribute. Defaults to `shape(<serial>)`
   * (see that Rust field's own doc comment) but is freely renamable —
   * the Inspector's id field, `setNodeId`. Not the same thing as `id`
   * above (that's this node's opaque, never-user-facing session token). */
  elementId: string
  /** Mirrors `an1mbyt3_core::Node::classes` — `getCompByClass`'s selector
   * tags, like HTML class attributes. The Inspector's classes field,
   * `setNodeClasses`. */
  classes: string[]
  kind: 'Shape' | 'Group' | 'Bone' | 'Instance' | 'Text' | 'Image' | 'ParticleSystem' | 'Tilemap' | 'Camera' | 'Light2D' | 'Trail' | 'Node3D'
  /** `null` for a root node; the parent's id for anything nested — a bone
   * chain's second/third bone, currently the only nested nodes the editor
   * creates. */
  parent: OpaqueId | null
  /** `Bone` nodes only — `null` for every other kind. */
  boneLength: number | null
  /** `Group` nodes only — which child (if any) clips the rest of this
   * group's contents. `null` for every other kind, or an unmasked group. */
  groupMask: OpaqueId | null
  /** `Group` nodes only — `GroupNode::y_sort_enabled`, the Inspector's "Y
   * Sort" checkbox. `null` for every other kind. */
  groupYSort: boolean | null
  /** Mirrors `an1mbyt3_core::Node::clip_children` — this node's own shape
   * also clips whatever's nested inside it, no separate `groupMask` child
   * needed. Any node kind, `setNodeClipChildren`. */
  clipChildren: boolean
  /** Mirrors `an1mbyt3_core::Node::visible` — a Code-mode script's own read
   * side of `ScriptContext.setVisible`/`ComponentRef.visible`. */
  visible: boolean
  /** Mirrors `an1mbyt3_core::Node::z_index` — the Inspector's "Z Index"
   * field, any node kind. Ignored for children of a Y-sort-enabled `Group`. */
  zIndex: number
  /** `Shape` nodes only — `null` for every other kind. Mirrors
   * `an1mbyt3_core::BlendMode`. */
  blendMode: BlendMode | null
  /** `Shape`, `Text`, and `Image` nodes only — the Inspector's fill-color/
   * tint swatch. `null` for every other kind. Distinct from
   * `fillColorOverride`/`resolvedFillColor` below, which are `Instance`-only
   * concepts. */
  fillColor: [number, number, number, number] | null
  /** `Some` for a `Shape`/`Text` node whose fill is currently a gradient
   * (`WasmEditor::set_gradient`) instead of a plain flat color. */
  gradient: GradientDef | null
  /** `Shape` nodes only — `Some` when this shape is textured with an
   * already-imported raster asset instead of a flat/gradient fill
   * (`WasmEditor::set_shape_image`). `null` for every other kind, or an
   * untextured shape. */
  shapeImage: OpaqueId | null
  /** `Shape` nodes only — the path-editing overlay's own read of the
   * node's geometry, and the source for a fresh `setShapePath` commit.
   * `null` for every other kind. */
  path: Path | null
  /** `Shape` nodes only — `Some` when this shape has an outline
   * (`WasmEditor::set_shape_stroke`). `null` for every other kind, or an
   * un-stroked shape. */
  stroke: Stroke | null
  /** `Text` nodes only — the Inspector's content field. `null` for every
   * other kind. */
  textContent: string | null
  /** `Text` nodes only — the Inspector's size field. `null` for every
   * other kind. */
  textSizePx: number | null
  /** `Text` nodes only — its font (one of `DocumentSummary.fonts`). */
  textFont: OpaqueId | null
  /** `Instance` nodes only — which `ComponentDefinition` this is an
   * instance of. `null` for every other kind. */
  component: OpaqueId | null
  /** `Instance` nodes only — the Inspector's per-instance color-override
   * field. `null` means "no override," not "override to transparent." */
  fillColorOverride: [number, number, number, number] | null
  /** `Instance` nodes only — the Inspector's per-instance variant dropdown. */
  variant: string | null
  /** `Instance` nodes only — this instance's effective fill color right now
   * (override -> variant -> definition-default precedence already applied). */
  resolvedFillColor: [number, number, number, number] | null
  /** `Image` nodes only — which `ImageAsset` this node draws. `null` for
   * every other kind. */
  image: OpaqueId | null
  /** `Image` nodes only — the node's authored on-canvas display size
   * (independent of the referenced asset's own pixel dimensions). `null`
   * for every other kind. */
  imageWidth: number | null
  imageHeight: number | null
  /** `ParticleSystem` nodes only — `null` for every other kind. */
  particleSystem: ParticleSystemConfig | null
  /** `Tilemap` nodes only — `null` for every other kind. */
  tilemap: TilemapConfig | null
  /** `Camera` nodes only — `null` for every other kind. */
  camera: CameraConfig | null
  /** `Light2D` nodes only — `null` for every other kind. */
  light2d: Light2DConfig | null
  /** `Trail` nodes only — `null` for every other kind. */
  trail: TrailConfig | null
  /** `Node3D` nodes only — the 3D transform and mesh / camera / light. */
  node3d: Node3D | null
  /** Skips 2D lighting for this node and everything under it. */
  unlit: boolean
  /** Unity's Shadow Caster 2D, if attached. */
  shadowCaster: ShadowCasterConfig | null
  /** Set when this node came from a prefab — `isRoot` on the instance's
   * root (where Apply/Revert/Unpack act). */
  prefab: { prefab: OpaqueId; isRoot: boolean } | null
  /** This node's attached behaviors (Unity-"component"-style) — the
   * Inspector's "Behaviors" section, read/written as one whole list via
   * `setNodeBehaviors`. */
  behaviors: NodeBehaviorEntry[]
  transform: Transform2D
  /** This node's tracks *within the currently active animation* — see
   * `ArtboardSummary.activeAnimation`. One entry per keyed property, never
   * more than one entry per property. */
  tracks: PropertyTrackSummary[]
  /** This node's local-space (pre-transform) bounding box — real geometry
   * for `Shape` nodes, a documented best-effort approximation for every
   * other kind (see `crates/an1mbyt3-wasm/src/dto.rs`'s `local_bounds` doc
   * comment). What `TransformHandles` builds the selection outline and
   * corner/rotate handles from. */
  localMin: Vec2
  localMax: Vec2
  /** This node's own transform composed through every ancestor's, up to the
   * artboard root — what the renderer actually draws it at. `transform`
   * (above) is local-to-parent only, identical to this for a root node but
   * not for anything nested under a `Group` that itself has a non-identity
   * transform. `TransformHandles` positions its selection outline/handles
   * from this, not `transform` — using the local-only value there drew the
   * handles offset from the shape's actual rendered position for any
   * selected node nested under a moved/rotated/scaled group. */
  worldTransform: Transform2D
  /** This node's immediate parent's own world transform, or `null` for a
   * root node — see `TransformHandles`' `localFor` doc comment for why an
   * on-canvas rotate/scale/translate drag (computed against `worldTransform`
   * above) needs this to convert its result back into the *local* value
   * `Command::SetRotation`/`SetScale`/`SetTranslation` actually store. */
  parentWorldTransform: Transform2D | null
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `AnimationSummary` — one
 * entry in the Timeline's animation dropdown. */
export interface AnimationSummary {
  id: OpaqueId
  name: string
  /** The animation's fixed, authored time range (seconds) — not derived
   * from keyframes; keyframes clamp to fit inside this, not the other way
   * around (see the Rust `Animation::duration` doc comment). */
  duration: number
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `ConditionDefDto` — a
 * discriminated union (tagged on `kind`) guarding one `TransitionDefDto`. */
export type ConditionDef =
  | { kind: 'BoolIs'; input: string; value: boolean }
  | { kind: 'NumberGreaterThan'; input: string; threshold: number }
  | { kind: 'NumberLessThan'; input: string; threshold: number }
  | { kind: 'EnumIs'; input: string; value: string }
  | { kind: 'Trigger'; input: string }

/** Mirrors `an1mbyt3_core::LoopMode`. */
export type LoopMode = 'Once' | 'Loop' | 'PingPong'

/** Mirrors `an1mbyt3_core::PlaybackDirection`. */
export type PlaybackDirection = 'Forward' | 'Reverse'

/** Mirrors `an1mbyt3_core::InputKind`. */
export type InputKind = 'Bool' | 'Number' | 'Enum' | 'Trigger'

/** Mirrors `an1mbyt3_core::InputDef` — one declared state-machine input. */
export interface InputDef {
  name: string
  kind: InputKind
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `TransitionDefDto` — one
 * outgoing transition on a `StateDefDto`. */
export interface TransitionDef {
  to: string
  condition: ConditionDef
  direction: PlaybackDirection
  /** How long (seconds) this transition crossfades the outgoing state's
   * pose into the incoming one, instead of switching instantly — `0` means
   * an instant cut. See `an1mbyt3_core::TransitionDef::duration_seconds`'s
   * doc comment for the full design. */
  durationSeconds: number
  /** Rive's "Allow Exit During Transition" toggle — while `true`, a
   * different transition out of the target state can interrupt this
   * transition's own crossfade before it finishes; `false` makes it run to
   * completion first. See `an1mbyt3_core::TransitionDef::interruptible`'s
   * doc comment. */
  interruptible: boolean
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `Blend1DDefDto` — a
 * continuous blend between two animations' extremes, driven by a live
 * number input instead of a playback clock ("dynamic expression" rigs).
 * See `an1mbyt3_core::Blend1DDef`'s own doc comment. */
export interface Blend1DDef {
  input: string
  low: OpaqueId
  high: OpaqueId
  lowValue: number
  highValue: number
}

/** Mirrors `an1mbyt3_core::GraphProperty` — one scalar component of a
 * node's properties a Programmable State's graph can read/write. A `Vec2`
 * property (translation/scale/anchor) is split into independent per-axis
 * entries rather than one vector-valued socket — see that Rust type's own
 * doc comment for why. */
export type GraphProperty =
  | 'TranslationX'
  | 'TranslationY'
  | 'Rotation'
  | 'ScaleX'
  | 'ScaleY'
  | 'AnchorX'
  | 'AnchorY'
  | 'FillColorR'
  | 'FillColorG'
  | 'FillColorB'
  | 'FillColorA'
  | 'Visible'

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `GraphNodeKindDto` —
 * adjacently tagged (`kind`/`value`) since some variants carry a bare
 * scalar. Source nodes (`Time`/`MousePosition`/`Random`/`Constant`/
 * `GetNumberInput`/`GetBoolInput`) take no inputs; math and logic nodes
 * take the fixed arity their name implies (a comparison/logic node's
 * output is `0`/`1`, not a separate boolean type — see `an1mbyt3_core::program`'s
 * own module doc); `GetProperty`/`SetProperty` are the graph's only
 * connection to the rest of the artboard. */
export type GraphNodeKind =
  | { kind: 'Constant'; value: number }
  | { kind: 'Time' }
  | { kind: 'MousePosition' }
  | { kind: 'Random'; value: { seed: number } }
  | { kind: 'Add' }
  | { kind: 'Subtract' }
  | { kind: 'Multiply' }
  | { kind: 'Divide' }
  | { kind: 'Sin' }
  | { kind: 'Cos' }
  | { kind: 'Lerp' }
  | { kind: 'Clamp' }
  | { kind: 'GetProperty'; value: { node: OpaqueId; property: GraphProperty } }
  | { kind: 'SetProperty'; value: { node: OpaqueId; property: GraphProperty; global: boolean } }
  | { kind: 'Min' }
  | { kind: 'Max' }
  | { kind: 'GreaterThan' }
  | { kind: 'LessThan' }
  | { kind: 'Equals' }
  | { kind: 'And' }
  | { kind: 'Or' }
  | { kind: 'Not' }
  | { kind: 'Select' }
  | { kind: 'GetNumberInput'; value: { name: string } }
  | { kind: 'GetBoolInput'; value: { name: string } }

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `SocketOverrideDto` — an
 * authored fallback (or, with `useOverride: true`, a pin) for one of a
 * node's input sockets, addressed by position (`socketIndex`). */
export interface SocketOverride {
  socketIndex: number
  value: number
  useOverride: boolean
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `GraphNodeDefDto` — one node
 * in a Programmable State's graph. */
export interface GraphNodeDef {
  id: number
  kind: GraphNodeKind
  position: Vec2
  overrides: SocketOverride[]
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `GraphEdgeDefDto` — one wire:
 * `fromNode`'s `fromSocket`-th output feeds `toNode`'s `toSocket`-th input. */
export interface GraphEdgeDef {
  fromNode: number
  fromSocket: number
  toNode: number
  toSocket: number
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `NodeGraphDefDto` — a whole
 * authored dataflow graph. */
export interface NodeGraphDef {
  nodes: GraphNodeDef[]
  edges: GraphEdgeDef[]
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `ListExprDto` — a value-
 * producing *list of nodes* (deliberately its own small type alongside
 * `Expr`, which only ever produces a single number — see that Rust type's
 * own doc comment for why this is scoped to node lists specifically, not
 * general-purpose arrays). */
export type ListExpr = { kind: 'NodesWithClass'; value: { class: string } }

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `NodeRefDto` — something that
 * resolves to a single node at run time: an explicit pick (`Literal`), the
 * current item of the innermost enclosing `ForEachNode` loop
 * (`CurrentItem`), or a fresh random pick out of a `ListExpr`
 * (`RandomFromList`). Every place that used to hold a bare node id directly
 * (`GetProperty`'s `node`, `Touching`'s `a`/`b`, `GoToXY`/`SetProperty`/
 * `ChangeProperty`'s `node`) holds one of these instead. */
export type NodeRef = { kind: 'Literal'; value: OpaqueId } | { kind: 'CurrentItem' } | { kind: 'RandomFromList'; value: ListExpr } | { kind: 'Myself' }

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `MathFnDto` — Scratch's own
 * "[ ] of ( )" one-dropdown-many-functions operator. */
export type MathFn = 'Abs' | 'Floor' | 'Ceiling' | 'Round' | 'Sqrt' | 'Sin' | 'Cos' | 'Tan' | 'Ln' | 'Log10'

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `ExprDto` — a Scratch-style
 * Blocks program's value-producing "reporter" expressions. Recursive
 * (arithmetic/logic nest arbitrarily deep), adjacently tagged same as
 * `GraphNodeKind`. Every node reference (`GetProperty`'s `node`,
 * `Touching`'s `a`/`b`) is a `NodeRef`, not a bare node id — there's no
 * implicit "current sprite," matching `GraphNodeKind::GetProperty`'s same
 * convention (this engine's state machines aren't bound to one owning
 * node), but `NodeRef::CurrentItem` lets a `ForEachNode` body's statements
 * still refer to "whichever node this pass is on." */
export type Expr =
  | { kind: 'Number'; value: number }
  | { kind: 'Pi' }
  | { kind: 'GetProperty'; value: { node: NodeRef; property: GraphProperty } }
  | { kind: 'MouseX' }
  | { kind: 'MouseY' }
  | { kind: 'KeyPressed'; value: { key: string } }
  | { kind: 'Touching'; value: { a: NodeRef; b: NodeRef } }
  | { kind: 'TouchingClass'; value: { node: NodeRef; class: string } }
  | { kind: 'Add'; value: [Expr, Expr] }
  | { kind: 'Subtract'; value: [Expr, Expr] }
  | { kind: 'Multiply'; value: [Expr, Expr] }
  | { kind: 'Divide'; value: [Expr, Expr] }
  | { kind: 'Mod'; value: [Expr, Expr] }
  | { kind: 'GreaterThan'; value: [Expr, Expr] }
  | { kind: 'LessThan'; value: [Expr, Expr] }
  | { kind: 'Equals'; value: [Expr, Expr] }
  | { kind: 'And'; value: [Expr, Expr] }
  | { kind: 'Or'; value: [Expr, Expr] }
  | { kind: 'Not'; value: [Expr] }
  | { kind: 'MathFn'; value: { op: MathFn; value: Expr } }
  | { kind: 'RandomInt'; value: { min: Expr; max: Expr } }
  | { kind: 'RandomFloat'; value: { min: Expr; max: Expr } }
  | { kind: 'GetNumberInput'; value: { name: string } }
  | { kind: 'GetBoolInput'; value: { name: string } }

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `StatementDto` — a Scratch-
 * style Blocks program's "stack" blocks (do something, connect top/bottom
 * to the next one), as opposed to `Expr`'s "plugs into a slot." */
export type Statement =
  | { kind: 'GoToXY'; value: { node: NodeRef; x: Expr; y: Expr } }
  | { kind: 'SetProperty'; value: { node: NodeRef; property: GraphProperty; value: Expr } }
  | { kind: 'ChangeProperty'; value: { node: NodeRef; property: GraphProperty; delta: Expr } }
  | { kind: 'Wait'; value: { seconds: Expr } }
  | { kind: 'If'; value: { condition: Expr; thenBranch: Statement[]; elseBranch: Statement[] } }
  | { kind: 'Repeat'; value: { count: Expr; body: Statement[] } }
  | { kind: 'Forever'; value: { body: Statement[] } }
  | { kind: 'ForEachNode'; value: { list: ListExpr; body: Statement[] } }
  | { kind: 'CreateCloneOf'; value: { node: NodeRef } }
  | { kind: 'DeleteThisClone' }

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `EventKindDto` — what starts
 * a `BlockScript` running. `StateTriggered` fires once when the owning state
 * becomes active (the "when green flag clicked" analog); `KeyPressed` fires
 * on that key's down-edge while the state is active (`overlapping: true`
 * spawns a new run alongside any still in progress instead of ignoring the
 * retrigger — real Scratch's own hat-block behavior); `KeyHeld` runs
 * continuously for as long as the key stays down, restarting on its own if
 * the body finishes early, and aborts immediately (even mid-`Wait`) the
 * instant the key's released. */
export type EventKind =
  | { kind: 'StateTriggered' }
  | { kind: 'KeyPressed'; value: { key: string; overlapping: boolean } }
  | { kind: 'KeyHeld'; value: { key: string } }
  | { kind: 'CloneStarted' }

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `BlockScriptDto` — one event
 * and the statements it runs. */
export interface BlockScript {
  event: EventKind
  statements: Statement[]
  /** This script's own hat block's position on the Blocks tab's canvas —
   * `null`/absent for a script never explicitly positioned yet (brand new,
   * or saved before this field existed), in which case Blockly's own
   * default auto-placement applies. Round-tripped by `compileWorkspace`/
   * `loadProgramIntoWorkspace` (`gameBlocks.ts`) so switching tabs (or
   * saving/reloading) doesn't rearrange scripts the author already placed. */
  position?: { x: number; y: number } | null
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `BlockProgramDefDto` — a
 * whole authored Scratch-style block program for a Programmable State
 * (`ProgramDef`'s `Blocks` variant). Unlike `Code`, this is real Rust
 * (`an1mbyt3_runtime::block_interpreter`) and runs everywhere `Graph` does. */
export interface BlockProgramDef {
  scripts: BlockScript[]
  /** How often this program's scripts tick, independent of render frame
   * rate — `null`/`undefined` ticks every render frame (this field's own
   * absence in an older saved document reads the same way); a number
   * throttles to that many times per second. See
   * `an1mbyt3_runtime::block_interpreter::step`'s own throttle. */
  ticksPerSecond?: number | null
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `ProgramDefDto` — a
 * Programmable State's program, an author's choice of visual graph, raw
 * code, or Scratch-style blocks. A `Code` program only ever runs inside the
 * desktop editor's own frontend loop (see `scriptRuntime.ts`) — `Graph` and
 * `Blocks` are real Rust and run everywhere. Adjacently tagged, same
 * reasoning as `GraphNodeKind`. */
export type ProgramDef = { kind: 'Code'; value: string } | { kind: 'Graph'; value: NodeGraphDef } | { kind: 'Blocks'; value: BlockProgramDef }

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `StateDefDto` — one authored
 * state: which animation plays, how it loops, and its outgoing transitions. */
export interface StateDef {
  name: string
  animation: OpaqueId
  loopMode: LoopMode
  transitions: TransitionDef[]
  /** This state's box position on the graph canvas — mirrors
   * `an1mbyt3_core::StateDef::position`. */
  position: Vec2
  /** `null` for a plain played-back state; set turns this into a
   * continuous blend-tree state instead — see `Blend1DDef`. */
  blend: Blend1DDef | null
  /** `null` for a plain played-back/blend state; set turns this into a
   * Programmable State instead — see `ProgramDef`. Only ever running while
   * the State Machine tab's own toggle is on, or in the Playground tab
   * (where it always runs) — see `App.tsx`'s mode-switching. */
  program: ProgramDef | null
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `StateMachineLayerDefDto` —
 * one extra, node-masked layer's own little machine (its own `states`/
 * `initialState`, restricted to `mask`) — Unity's "Animator Layer" concept.
 * See `an1mbyt3_core::StateMachineLayerDef`'s own doc comment for the
 * "eyebrows independent of mouth, within one machine" design this enables. */
export interface StateMachineLayerDef {
  name: string
  states: StateDef[]
  initialState: string
  /** Node ids this layer's pose is allowed to write to — empty means it
   * touches nothing yet (a brand-new layer's deliberate default). */
  mask: OpaqueId[]
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `StateMachineDefSummary` —
 * a whole authored state machine, for the State Machine panel's authoring UI. */
export interface StateMachineDefSummary {
  id: OpaqueId
  name: string
  inputs: InputDef[]
  states: StateDef[]
  initialState: string
  /** Extra, node-masked layers composited on top of `states` (the base,
   * unmasked layer) — see `StateMachineLayerDef`. */
  layers: StateMachineLayerDef[]
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `ComponentVariantDto` — a
 * named, reusable override preset on a `ComponentDefinitionSummary`. */
export interface ComponentVariantDef {
  name: string
  fillColor: [number, number, number, number]
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `ComponentDefinitionSummary`
 * — what the Components panel's library listing and variant editor read. */
export interface ComponentDefinitionSummary {
  id: OpaqueId
  name: string
  defaultFillColor: [number, number, number, number]
  variants: ComponentVariantDef[]
  /** An SVG `<path d="...">` value tracing this definition's own shape —
   * a ready-made thumbnail for the Resources panel's "Drawings" section
   * (and anywhere else that wants a cheap preview), no rasterization
   * needed. Already flipped into SVG's own `+y`-down convention — see the
   * Rust `path_to_svg_d`'s own doc comment. */
  pathD: string
  /** `[minX, minY, width, height]` — a ready-to-use SVG `viewBox` matching
   * `pathD`'s own coordinate space. */
  viewBox: [number, number, number, number]
}

export interface ArtboardSummary {
  id: OpaqueId
  name: string
  width: number
  height: number
  /** Fraction of native resolution this artboard renders at (`1.0` =
   * native) — see `an1mbyt3_core::Artboard::resolution_scale`'s own doc
   * comment. The Inspector's "Canvas" section's resolution field. */
  resolutionScale: number
  /** Fixed (letterboxed) or Responsive (fills the screen) — see `ScreenSettings`. */
  screen: ScreenSettings
  nodes: NodeSummary[]
  /** Every non-deleted animation on this artboard, for the Timeline's
   * animation dropdown. */
  animations: AnimationSummary[]
  /** Which animation the Timeline is currently authoring/scrubbing —
   * `null` only if every animation was deleted. */
  activeAnimation: OpaqueId | null
  /** Every non-deleted state machine defined on this artboard, for the
   * State Machine panel's authoring UI. */
  stateMachines: StateMachineDefSummary[]
  /** Which state machine the State Machine panel is currently
   * previewing/testing — `null` if none exist. */
  activeStateMachine: OpaqueId | null
  /** This artboard's post-processing effect stack, in render order,
   * including disabled entries — the Effects panel's whole list. See
   * `an1mbyt3_core::Artboard::post_effects`'s own doc comment. */
  postEffects: PostEffectLayer[]
  /** Which `Camera` node (if any) Playground actually views this artboard
   * through — a saved scene property (`an1mbyt3_core::Artboard::active_camera`'s
   * own doc comment), not an editor-session preference like
   * `activeAnimation`/`activeStateMachine` above. `null` falls back to the
   * editor's own free-roam pan/zoom. */
  activeCamera: OpaqueId | null
  /** Physics 2D gravity (world units / s², +y up). */
  gravity: Vec2
  /** Physics 3D gravity `[x, y, z]`, m/s². */
  gravity3d: [number, number, number]
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `PostEffectDto` — one
 * whole-scene post-processing effect. A discriminated union on `kind`. */
export type PostEffect =
  | { kind: 'None' }
  | { kind: 'Blur'; radius: number }
  | { kind: 'Glow'; radius: number; threshold: number; intensity: number }
  | { kind: 'Pixelate'; blockSize: number }
  | { kind: 'ColorGrading'; brightness: number; contrast: number; saturation: number }
  | { kind: 'Posterize'; levels: number }
  | { kind: 'GradientMap'; shadow: [number, number, number, number]; highlight: [number, number, number, number] }
  | { kind: 'Vignette'; color: [number, number, number, number]; intensity: number; radius: number; blur: number }

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `PostEffectLayerDto` — one
 * entry in an artboard's post-processing stack. `enabled` lets an entry be
 * temporarily switched off without losing its place/parameters in the
 * stack, same reasoning `PostEffectLayer`'s own Rust doc comment gives. */
export interface PostEffectLayer {
  effect: PostEffect
  enabled: boolean
}

/** Every post-effect kind the Effects panel's "+ Add Effect" menu offers,
 * with the exact starting parameters `PostEffect::default_for_kind`
 * (`an1mbyt3-core`) uses — kept in sync by hand since a freshly-added
 * layer's parameters need to make sense as a *default*, not just satisfy
 * the type (a `radius: 0`/`blockSize: 0` effect would visibly do nothing
 * until a slider was first dragged). */
export const POST_EFFECT_DEFAULTS: Record<PostEffect['kind'], PostEffect> = {
  None: { kind: 'None' },
  Blur: { kind: 'Blur', radius: 4 },
  Glow: { kind: 'Glow', radius: 6, threshold: 0.7, intensity: 1 },
  Pixelate: { kind: 'Pixelate', blockSize: 8 },
  ColorGrading: { kind: 'ColorGrading', brightness: 0, contrast: 1, saturation: 1 },
  Posterize: { kind: 'Posterize', levels: 4 },
  GradientMap: { kind: 'GradientMap', shadow: [0.05, 0.0, 0.2, 1.0], highlight: [1.0, 0.8, 0.3, 1.0] },
  Vignette: { kind: 'Vignette', color: [0, 0, 0, 1], intensity: 0.45, radius: 0.75, blur: 0.5 },
}

export interface DocumentSummary {
  artboards: ArtboardSummary[]
  /** Every non-deleted component definition in the whole document (not
   * artboard-scoped), for the Components panel's library listing. */
  components: ComponentDefinitionSummary[]
  /** Every image asset in the document — the Resources panel's flat
   * listing (grouped/nested per `spriteGroups` below). */
  images: ImageAssetSummary[]
  /** Every sprite group (e.g. one imported GIF's decoded frames) — purely
   * organizational, see the Rust `SpriteGroup`'s own doc comment. */
  spriteGroups: SpriteGroupSummary[]
  /** Every non-deleted script definition in the whole document — the
   * Inspector's "Add Behavior" picker, same document-level-asset shape as
   * `components` above. */
  scripts: ScriptDefinitionSummary[]
  /** Every non-deleted prefab — the Resources panel's Prefabs folder. */
  prefabs: PrefabSummary[]
  /** Every saved Paint-mode drawing (Resources ▸ Drawings). */
  drawings: DrawingSummary[]
  /** Every non-deleted material — the Shader tab and the Material component. */
  materials: MaterialSummary[]
  /** Every imported 3D model (glTF / GLB). */
  models: { id: OpaqueId; name: string }[]
  /** Every imported sound. */
  audio: AudioSummary[]
  /** Project code / markup files (the Code tab, `@file:` references). */
  textFiles: TextFileSummary[]
  /** Every scene's properties. */
  properties: PropertySummary[]
  /** Every font on the document (Text nodes' font picker). */
  fonts: FontSummary[]
}

/** One font asset (`an1mbyt3_core::FontAsset`; its bytes stay in wasm). */
export interface FontSummary {
  id: OpaqueId
  /** The family name ("Russo One", "Material Symbols Rounded"). */
  name: string
}

// ---- 3D (mirrors `an1mbyt3_core::space3d`) ----

export type Vec3Tuple = [number, number, number]

export interface Transform3D {
  position: Vec3Tuple
  /** Euler degrees, applied yaw (Y), pitch (X), roll (Z). */
  rotationDegrees: Vec3Tuple
  scale: Vec3Tuple
}

export type Primitive3D =
  | { shape: 'Cube'; size: Vec3Tuple }
  | { shape: 'Sphere'; radius: number; segments: number; rings: number }
  | { shape: 'Plane'; size: number; subdivisions: number }
  | { shape: 'Cylinder'; radius: number; height: number; segments: number }
  | { shape: 'Cone'; radius: number; height: number; segments: number }
  | { shape: 'Torus'; radius: number; tube: number; segments: number; sides: number }
  | { shape: 'Voxels'; width: number; height: number; depth: number }
  | { shape: 'Model'; model: OpaqueId }

export type Node3DContent =
  | { type: 'Empty' }
  | {
      type: 'Mesh'
      primitive: Primitive3D
      color: [number, number, number, number]
      shininess: number
      unlit: boolean
      material: OpaqueId | null
      /** Model meshes: the clip playing, whether it loops, and an overlay clip. */
      animation: string | null
      animationLoop: boolean
      animationOverlay: string | null
    }
  | { type: 'Camera'; fovDegrees: number; near: number; far: number; current: boolean; clearColor: [number, number, number, number] | null }
  | { type: 'Light'; kind: 'Directional' | 'Point'; color: Vec3Tuple; intensity: number; range: number }

export interface Node3D {
  transform: Transform3D
  content: Node3DContent
}

/** The + menu's 3D items (`add_node3d`). */
export type Node3DItem = 'Cube' | 'Sphere' | 'Plane' | 'Cylinder' | 'Cone' | 'Torus' | 'Camera' | 'Sun' | 'PointLight' | 'Empty'

/** Unity's AudioSource component (`an1mbyt3_core::AudioSource`). */
export interface AudioSourceConfig {
  clip: OpaqueId | null
  /** 0..1 (above 1 amplifies). */
  volume: number
  /** Playback rate (1 = normal). */
  pitch: number
  loop: boolean
  /** Starts by itself when Playground / the game enters the scene. */
  playOnStart: boolean
  /** Pans / fades by the node's position relative to the camera. */
  spatial: boolean
  /** Spatial falloff distance (world units). */
  range: number
}

/** Pins a node to a point of the screen (`an1mbyt3_core::UiAnchor`). */
export interface UiAnchorConfig {
  /** `[x, y]` in 0..1 of the view (`[0, 1]` = top-left). */
  anchor: [number, number]
  /** World units from that point (+y up). */
  offset: [number, number]
}

/** Row / column layout of the children (`an1mbyt3_core::UiStack`). */
export interface UiStackConfig {
  direction: 'Vertical' | 'Horizontal'
  spacing: number
  padding: number
  align: 'Start' | 'Center' | 'End'
}

/** How a UI control shows its state (`an1mbyt3_core::UiTransition`). */
export type UiTransition = 'None' | 'ColorTint' | 'SpriteSwap'

/**
 * A colour / image / scale per interaction state — Unity's `ColorBlock` +
 * `SpriteState` (`an1mbyt3_core::UiStateStyle`).
 */
export interface UiStateStyle {
  transition: UiTransition
  normalColor: [number, number, number, number]
  highlightedColor: [number, number, number, number]
  pressedColor: [number, number, number, number]
  disabledColor: [number, number, number, number]
  colorMultiplier: number
  /** Seconds to blend between states (0 = instant). */
  fadeDuration: number
  highlightedImage: OpaqueId | null
  pressedImage: OpaqueId | null
  disabledImage: OpaqueId | null
  highlightedScale: number
  pressedScale: number
  /** Also tint the children (a button's label). */
  tintChildren: boolean
}

/** Hover / press feedback and clicks (`an1mbyt3_core::UiButton`). */
export interface UiButtonConfig {
  interactable: boolean
  style: UiStateStyle
}

export type UiFillDirection = 'LeftToRight' | 'RightToLeft' | 'BottomToTop' | 'TopToBottom'

/** A value slider; positions its "Fill" / "Handle" children (`an1mbyt3_core::UiSlider`). */
export interface UiSliderConfig {
  min: number
  max: number
  value: number
  wholeNumbers: boolean
  direction: UiFillDirection
  interactable: boolean
  style: UiStateStyle
}

/** A scrollbar; sizes / places its "Handle" child (`an1mbyt3_core::UiScrollbar`). */
export interface UiScrollbarConfig {
  value: number
  size: number
  direction: UiFillDirection
  /** 0 = continuous, else that many positions. */
  steps: number
  interactable: boolean
  style: UiStateStyle
}

/** Scrolls its "Content" child (`an1mbyt3_core::UiScrollView`). */
export interface UiScrollViewConfig {
  horizontal: boolean
  vertical: boolean
  wheelStep: number
  inertia: boolean
}

/** A checkbox / radio button; shows its "Checkmark" child while on (`an1mbyt3_core::UiToggle`). */
export interface UiToggleConfig {
  isOn: boolean
  interactable: boolean
  /** 0 = independent, else a radio group. */
  group: number
  allowSwitchOff: boolean
  style: UiStateStyle
}

/** How a scene meets the screen (`an1mbyt3_core::ScreenSettings`). */
export interface ScreenSettings {
  /** Fixed = the design size letterboxed; Responsive = fill the screen. */
  mode: 'Fixed' | 'Responsive'
  /** Responsive: 0 = keep the design's width scale, 1 = its height. */
  matchWidthOrHeight: number
  /** Screens narrower than this (CSS px) are "compact" (phones). */
  compactWidth: number
}

/** A layout box size along one axis (`an1mbyt3_core::UiSize`). */
export type UiSize = 'Hug' | 'Fill' | { Fixed: number } | { Percent: number }
export type UiFlexDirection = 'Row' | 'Column' | 'RowReverse' | 'ColumnReverse'
export type UiJustify = 'Start' | 'Center' | 'End' | 'SpaceBetween' | 'SpaceAround' | 'SpaceEvenly'
export type UiAlign = 'Start' | 'Center' | 'End' | 'Stretch'
export interface UiEdges {
  top: number
  right: number
  bottom: number
  left: number
}
/** The swappable part of a UI Layout (`an1mbyt3_core::UiFlexStyle`). */
export interface UiFlexStyle {
  direction: UiFlexDirection
  wrap: boolean
  justify: UiJustify
  align: UiAlign
  gap: number
  padding: UiEdges
}
/** A flexbox container (`an1mbyt3_core::UiLayout`). */
export interface UiLayoutConfig {
  style: UiFlexStyle
  /** Used instead on compact (phone-width) screens. */
  compact: UiFlexStyle | null
  width: UiSize
  height: UiSize
}
/** Per-child flexbox settings (`an1mbyt3_core::UiLayoutItem`). */
export interface UiLayoutItemConfig {
  width: UiSize
  height: UiSize
  grow: number
  shrink: number
  alignSelf: UiAlign | null
  margin: UiEdges
  ignore: boolean
}
export type UiScreenCondition = 'Compact' | 'Regular' | 'Portrait' | 'Landscape'
/** Show only on matching screens (`an1mbyt3_core::UiShowWhen`). */
export interface UiShowWhenConfig {
  when: UiScreenCondition
}

/** HTML + CSS shown at a node (`an1mbyt3_core::UiHtml`). */
export interface UiHtmlConfig {
  html: string
  css: string
  /** The element's box, world units. */
  width: number
  height: number
  /** Takes clicks / typing in Playground and games. */
  interactive: boolean
}

/** A 3D physics body (`an1mbyt3_core::Rigidbody3D`). */
/** One On Click action (`an1mbyt3_core::ActionKind`, externally tagged). */
export type ActionKind =
  | 'ToggleVisible'
  | { SetVisible: { visible: boolean } }
  | { SetNodeField: { field: string; value: string } }
  | { SetComponentField: { component: string; field: string; value: string } }
  | { CallMethod: { component: string; method: string; arg: string } }
  | { FireTrigger: { input: string } }
  | { SetInput: { input: string; value: string } }
  | { LoadScene: { scene: string } }
  | { PlaySound: { sound: string } }
  | { SetProperty: { property: string; value: string } }
  | { CallFunction: { function: string; arg: string } }
/** An action on a target node (`null` = the clicked node itself). */
export interface EventAction {
  target: OpaqueId | null
  action: ActionKind
}
/** `an1mbyt3_core::UiEvents` — a node's On Click list. */
export interface UiEventsConfig {
  onClick: EventAction[]
}
/** What a property binding drives. */
export type BindTarget = 'Text' | 'X' | 'Y' | 'Rotation' | 'ScaleX' | 'ScaleY' | 'Scale' | 'Visible' | 'FillColor' | { ComponentField: { component: string; field: string } }
export interface PropertyBinding {
  property: string
  target: BindTarget
  format: string
  scale: number
  offset: number
}
/** `an1mbyt3_core::PropertyBindings`. */
export interface PropertyBindingsConfig {
  bindings: PropertyBinding[]
}
/** A scene property (Rive view model value). */
export interface PropertySummary {
  id: OpaqueId
  artboard: OpaqueId
  name: string
  kind: 'Number' | 'Bool' | 'Text' | 'Color'
  /** Starting value as JSON. */
  default: string
}

/** A project code / markup file (`an1mbyt3_core::TextFile`). */
export interface TextFileSummary {
  id: OpaqueId
  /** `UI/menu.html` — `/`-separated, no leading slash. */
  path: string
  text: string
  /** Lowercased extension (`html`, `css`, `js`…) or `''`. */
  extension: string
}

/** `an1mbyt3_core::Tree2D` — a procedural tree. */
export type TreeSpecies = 'Oak' | 'Pine' | 'Birch' | 'Sakura' | 'Palm' | 'Willow'
export interface Tree2DConfig {
  species: TreeSpecies
  seed: number
  height: number
  spread: number
  density: number
  barkColor: [number, number, number, number]
  leafColor: [number, number, number, number]
  accentColor: [number, number, number, number]
  wind: number
  windSway: number
  fallingLeaves: number
  growOnPlay: boolean
  growTime: number
}

/** `an1mbyt3_core::Cloth2D` — soft fabric: flags, curtains, capes. */
export type ClothPin = 'TopEdge' | 'TopCorners' | 'LeftEdge' | 'None'
export type ClothPatternKind = 'Solid' | 'HorizontalStripes' | 'VerticalStripes' | 'Checker'
export interface Cloth2DConfig {
  width: number
  height: number
  columns: number
  rows: number
  pin: ClothPin
  color: [number, number, number, number]
  pattern: ClothPatternKind
  patternColor: [number, number, number, number]
  patternCount: number
  image: OpaqueId | null
  gravity: number
  wind: Vec2Tuple
  turbulence: number
  stiffness: number
  damping: number
  tearStretch: number
  shading: number
}

/** `an1mbyt3_core::Flipbook` — shows one child at a time on a clock. */
export interface FlipbookConfig {
  fps: number
  playing: boolean
  looping: boolean
  /** 0-based frame shown while not playing (and in Design). */
  frame: number
}

export interface Rigidbody3DConfig {
  bodyType: 'Dynamic' | 'Kinematic' | 'Static'
  mass: number
  gravityScale: number
  linearDamping: number
  angularDamping: number
  freezeRotation: boolean
  continuous: boolean
}

/** A 3D collision shape (`an1mbyt3_core::Collider3DShape`). */
export type Collider3DShape = 'Auto' | { Box: { size: [number, number, number] } } | { Sphere: { radius: number } } | { Capsule: { radius: number; height: number } }

/** `an1mbyt3_core::Collider3D`. */
export interface Collider3DConfig {
  shape: Collider3DShape
  offset: [number, number, number]
  isTrigger: boolean
  friction: number
  restitution: number
}

/** One imported sound. */
export interface AudioSummary {
  id: OpaqueId
  name: string
  byteLength: number
  /** Its file in the project folder, if linked. */
  file: string | null
}

/** One material asset (`an1mbyt3_core::Material`). */
export interface MaterialSummary {
  id: OpaqueId
  name: string
  kind: 'ShaderGraph' | 'Color' | 'Textured'
}

// ---- Shader graphs (mirrors `an1mbyt3_core::shader_graph`) ----

/** A shader-graph node type's name, e.g. `'SampleTexture'`. */
export type ShaderNodeKind = string
export type ShaderPortType = 'Float' | 'Vec2' | 'Vec3' | 'Vec4' | 'Dynamic'

export interface ShaderNodeDef {
  id: number
  kind: ShaderNodeKind
  /** `[x, y]` in graph space. */
  position: [number, number]
  /** A constant node's value (Float = `[0]`, Vec2 = `[0..1]`). */
  value: [number, number, number, number]
  /** Unwired-input constants, by input index (`null` = the port default). */
  defaults: ([number, number, number, number] | null)[]
}

export interface ShaderEdgeDef {
  from: number
  fromPort: number
  to: number
  toPort: number
}

export interface ShaderGraphDef {
  domain: 'Canvas2D'
  nodes: ShaderNodeDef[]
  edges: ShaderEdgeDef[]
}

/** One entry of `shaderNodeCatalog()` — a node type and its sockets. */
export interface ShaderNodeSpec {
  kind: ShaderNodeKind
  category: string
  hasValue: boolean
  inputs: { name: string; ty: ShaderPortType; default: [number, number, number, number] | null }[]
  outputs: { name: string; ty: ShaderPortType; default: [number, number, number, number] | null }[]
}

/** `materialStatus()` — the generated WGSL, or why it didn't compile. */
export interface MaterialStatus {
  wgsl: string | null
  error: string | null
  errorNode: number | null
}

/** One prefab asset (`an1mbyt3_core::PrefabDefinition`). */
/** One saved Paint-mode drawing — see `an1mbyt3_core::drawing`. The SVG
 * itself isn't in the summary (`drawingSvg` reads it on demand). */
export interface DrawingSummary {
  id: OpaqueId
  name: string
  /** `Animation` = frames × layers (what Paint saves now); the others are
   * older single-image drawings. */
  kind: 'Vector' | 'Bitmap' | 'Animation'
  /** An old bitmap drawing's image asset; `null` otherwise. */
  image: OpaqueId | null
  frameCount: number
  layerCount: number
}

/** Result of `addDrawing` / `addBitmapDrawing`. */
export interface DrawingResult {
  document: DocumentSummary
  drawing: OpaqueId
}

export interface PrefabSummary {
  id: OpaqueId
  name: string
  nodeCount: number
}

/** Result of `createPrefab` / `instantiatePrefab`. */
export interface PrefabResult {
  document: DocumentSummary
  prefab: OpaqueId | null
  node: OpaqueId | null
}

/** One file of `saveProject`'s listing: `text` for text files, `bytes` (an
 * index for `takeProjectFileBytes`) for binary ones. */
export interface ProjectFileListing {
  path: string
  text: string | null
  bytes: number | null
  /** An image's own source file in the project — written only if missing. */
  linked: boolean
  /** The document item the file holds (`null` for the manifest). */
  asset: { kind: 'scene' | 'prefab' | 'image' | 'audio' | 'model'; id: OpaqueId } | null
}

/** Mirrors `an1mbyt3_core::ScriptFieldKind` — a bare string-literal union,
 * same convention `GradientKind`/`LoopMode` already use for a plain
 * fieldless Rust enum. */
export type ScriptFieldKind = 'Float' | 'Int' | 'Bool' | 'Color' | 'Vec2' | 'String'

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `ScriptFieldValueDto` — a
 * tagged union so a field editor can switch on `kind` to pick the right
 * widget (number input/checkbox/color swatch/text box). */
export type ScriptFieldValue =
  | { kind: 'Float'; value: number }
  | { kind: 'Int'; value: number }
  | { kind: 'Bool'; value: boolean }
  | { kind: 'Color'; value: [number, number, number, number] }
  | { kind: 'Vec2'; value: [number, number] }
  | { kind: 'String'; value: string }

/** Mirrors `an1mbyt3_core::ScriptFieldDef`. */
export interface ScriptFieldDef {
  name: string
  default: ScriptFieldValue
  animatable: boolean
}

/** Mirrors `an1mbyt3_core::ScriptDefinition` — the Inspector's "Add
 * Behavior" picker and the script authoring UI (add/remove/retype a field)
 * both read/write this. */
export interface ScriptDefinitionSummary {
  id: OpaqueId
  name: string
  fields: ScriptFieldDef[]
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `NodeBehaviorDto` — one
 * attached behavior. `SpriteRenderer` has no editor UI yet (see the Rust
 * `behavior` module's own doc comment); `Script` is what the script
 * component UI actually authors. */
export type NodeBehavior =
  | { kind: 'SpriteRenderer'; material: OpaqueId | null; color: [number, number, number, number] }
  | { kind: 'Script'; definition: OpaqueId; values: [string, ScriptFieldValue][] }
  // Unity components — payload fields sit next to `kind`.
  | ({ kind: 'Camera' } & CameraConfig)
  | ({ kind: 'Light2D' } & Light2DConfig)
  | ({ kind: 'TrailRenderer' } & TrailConfig)
  | ({ kind: 'ParticleSystem' } & ParticleSystemConfig)
  | ({ kind: 'ShadowCaster2D' } & ShadowCasterConfig)
  | ({ kind: 'Rigidbody2D' } & Rigidbody2DConfig)
  | ({ kind: 'BoxCollider2D' } & BoxCollider2DConfig)
  | ({ kind: 'CircleCollider2D' } & CircleCollider2DConfig)
  | ({ kind: 'CapsuleCollider2D' } & CapsuleCollider2DConfig)
  | ({ kind: 'PolygonCollider2D' } & PolygonCollider2DConfig)
  | ({ kind: 'HingeJoint2D' } & HingeJoint2DConfig)
  | ({ kind: 'IkConstraint2D' } & IkConstraint2DConfig)
  | ({ kind: 'SpriteSkin2D' } & SpriteSkin2DConfig)
  | ({ kind: 'Liquid2D' } & Liquid2DConfig)
  | ({ kind: 'AudioSource' } & AudioSourceConfig)
  | ({ kind: 'UiAnchor' } & UiAnchorConfig)
  | ({ kind: 'UiStack' } & UiStackConfig)
  | ({ kind: 'UiButton' } & UiButtonConfig)
  | ({ kind: 'UiSlider' } & UiSliderConfig)
  | ({ kind: 'UiScrollbar' } & UiScrollbarConfig)
  | ({ kind: 'UiScrollView' } & UiScrollViewConfig)
  | ({ kind: 'UiToggle' } & UiToggleConfig)
  | ({ kind: 'UiLayout' } & UiLayoutConfig)
  | ({ kind: 'UiLayoutItem' } & UiLayoutItemConfig)
  | ({ kind: 'UiShowWhen' } & UiShowWhenConfig)
  | ({ kind: 'UiHtml' } & UiHtmlConfig)
  | ({ kind: 'Rigidbody3D' } & Rigidbody3DConfig)
  | ({ kind: 'Collider3D' } & Collider3DConfig)
  | ({ kind: 'Flipbook' } & FlipbookConfig)
  | ({ kind: 'Cloth2D' } & Cloth2DConfig)
  | ({ kind: 'Tree2D' } & Tree2DConfig)
  | ({ kind: 'UiEvents' } & UiEventsConfig)
  | ({ kind: 'PropertyBindings' } & PropertyBindingsConfig)

/** Every addable Unity component kind (the Add Component menu). */
export type ComponentKind = Exclude<NodeBehavior['kind'], 'SpriteRenderer' | 'Script'>

/** `[x, y]` — how the physics components' vectors travel. */
export type Vec2Tuple = [number, number]

/** Unity Physics Material 2D (per collider). */
export interface PhysicsMaterial2DConfig {
  friction: number
  bounciness: number
}

/** Unity's `Rigidbody 2D`. */
export interface Rigidbody2DConfig {
  bodyType: 'Dynamic' | 'Kinematic' | 'Static'
  mass: number
  linearDrag: number
  angularDrag: number
  gravityScale: number
  collisionDetection: 'Discrete' | 'Continuous'
  freezePositionX: boolean
  freezePositionY: boolean
  freezeRotation: boolean
}

/** Unity's `Box Collider 2D` (offset/size in the node's own art space). */
export interface BoxCollider2DConfig {
  offset: Vec2Tuple
  size: Vec2Tuple
  isTrigger: boolean
  material: PhysicsMaterial2DConfig
}

/** Unity's `Circle Collider 2D`. */
export interface CircleCollider2DConfig {
  offset: Vec2Tuple
  radius: number
  isTrigger: boolean
  material: PhysicsMaterial2DConfig
}

/** Unity's `Capsule Collider 2D` (size = overall width x height). */
export interface CapsuleCollider2DConfig {
  offset: Vec2Tuple
  size: Vec2Tuple
  direction: 'Vertical' | 'Horizontal'
  isTrigger: boolean
  material: PhysicsMaterial2DConfig
}

/** Unity's `Polygon Collider 2D` (convex hull of `points`). */
/** Unity's `Hinge Joint 2D` — ragdoll limbs, pendulums, doors. */
export interface HingeJoint2DConfig {
  /** `null` = the nearest ancestor with a Rigidbody 2D (or the world). */
  connectedBody: OpaqueId | null
  /** Hinge point in this node's local space (a bone's joint = `[0, 0]`). */
  anchor: Vec2Tuple
  useLimits: boolean
  lowerAngle: number
  upperAngle: number
  useMotor: boolean
  motorSpeed: number
  maxMotorTorque: number
  enableCollision: boolean
}

/** Live IK on the chain ending at this bone (Unity 2D IK solvers). */
export interface IkConstraint2DConfig {
  target: OpaqueId | null
  chainLength: number
  flip: boolean
  weight: number
}

/** Liquid 2D — a particle fluid (`an1mbyt3_core::Liquid2D`). */
export interface Liquid2DConfig {
  /** Starting volume, px. */
  size: Vec2Tuple
  particleSpacing: number
  color: [number, number, number, number]
  viscosity: number
  stiffness: number
  buoyancy: number
  drag: number
  /** Faucet particles / second (0 = just the starting volume). */
  emitRate: number
  emitVelocity: Vec2Tuple
  maxParticles: number
}

/** Unity's `Sprite Skin`: the image deformed by bones. */
export interface SpriteSkin2DConfig {
  bones: OpaqueId[]
  bindPoses: number[][]
  bindNode: number[]
  columns: number
  rows: number
  smoothness: number
}

export interface PolygonCollider2DConfig {
  offset: Vec2Tuple
  points: Vec2Tuple[]
  isTrigger: boolean
  material: PhysicsMaterial2DConfig
}

/** Mirrors `an1mbyt3_core::NodeBehaviorEntry`. */
export interface NodeBehaviorEntry {
  behavior: NodeBehavior
  enabled: boolean
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `ImageAssetSummary` — one
 * entry in the Resources panel's listing, plus the Inspector's asset-mode
 * fields when selected. */
export interface ImageAssetSummary {
  id: OpaqueId
  name: string
  /** The source's own decoded pixel dimensions — `null` if the asset's
   * bytes aren't valid PNG (shouldn't happen for anything imported through
   * this app). Distinct from a placed `Image` node's own authored
   * `imageWidth`/`imageHeight`, and from `processing.resize` (the baked/
   * re-sampled size). */
  width: number | null
  height: number | null
  processing: ImageProcessing
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `SpriteGroupSummary`. */
export interface SpriteGroupSummary {
  id: OpaqueId
  name: string
  /** `[frame image id, delay in milliseconds]`, in playback order. */
  frames: [OpaqueId, number][]
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `AddNodeResult` — the
 * response from `WasmEditor::add_shape_node`: the updated document plus the
 * id of the node it just created, so the caller can select it immediately. */
export interface AddNodeResult {
  document: DocumentSummary
  newNode: OpaqueId
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `AddArtboardResult` — the
 * Scenes switcher's "+ New Scene" result, carrying the new scene's id so it
 * can be switched to immediately. */
export interface AddArtboardResult {
  document: DocumentSummary
  newArtboard: OpaqueId
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `ImportSvgResult` — the
 * response from `WasmEditor::import_svg`: the updated document plus every
 * newly created shape's id (an SVG can drop in many shapes at once, unlike
 * every other `Add*Result` here). */
export interface ImportSvgResult {
  document: DocumentSummary
  newNodes: OpaqueId[]
}

/** Mirrors `an1mbyt3_core::ImageFilterMode` — resample quality, shared by an
 * asset's `resize` bake step and its live GPU sampling (a pixel-art asset
 * wants `Nearest` for both). */
export type ImageFilterMode = 'Nearest' | 'Linear'

/** Mirrors `an1mbyt3_core::ImageProcessing` — one image asset's authored
 * pre-processing params, baked once (`Renderer::bake_image`) rather than
 * costing anything per render frame. `resize: null` means "use the source's
 * own decoded pixel dimensions." */
export interface ImageProcessing {
  resize: [number, number] | null
  filter: ImageFilterMode
  effect: PostEffect
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `ImportImageResult` — the
 * response from `WasmEditor::import_image`. A still image creates one
 * canvas node (`newNode`); a GIF creates none at all (frames-only this
 * pass) but reports its decoded frames/group for the Resources panel. */
export interface ImportImageResult {
  document: DocumentSummary
  newNode: OpaqueId | null
  newImages: OpaqueId[]
  newSpriteGroup: OpaqueId | null
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `DuplicateNodesResult` — the
 * response from `WasmEditor::duplicate_nodes`: the updated document plus
 * each topmost duplicated node's own new id (not every recursed descendant),
 * in the same order the request was given. */
export interface DuplicateNodesResult {
  document: DocumentSummary
  newNodes: OpaqueId[]
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `AddAnimationResult` — the
 * response from `WasmEditor::add_animation`: the updated document plus the
 * id of the animation it just created, so the caller can select it in the
 * dropdown immediately. */
export interface AddAnimationResult {
  document: DocumentSummary
  newAnimation: OpaqueId
}

/** Mirrors `crates/an1mbyt3-wasm/src/dto.rs`'s `StateMachineSummary` — what
 * the editor's State Machine tab needs to render its states list, active-state
 * highlight, and trigger buttons. `availableTriggers` is scoped to the
 * *current* state's own transitions (see that Rust type's doc comment). */
export interface StateMachineSummary {
  states: string[]
  activeState: string
  availableTriggers: string[]
  /** Every extra layer's own live current state, keyed by layer name — see
   * `crates/an1mbyt3-wasm/src/dto.rs`'s `StateMachineSummary::layer_active_states`. */
  layerActiveStates: Record<string, string>
}
