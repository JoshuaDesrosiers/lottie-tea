import { useEffect, useRef, useState, type ReactElement, type ReactNode } from 'react'
import type { AnimationExportOptions } from '../exportGif'
import { UI_MENU_ITEMS, UI_MENU_LABEL, type ShapePreset, type UiMenuItem } from '../ipc/wasmEditor'
import type { Light2DType, Node3DItem } from '../ipc/document'
import { useLayoutMetrics } from '../layoutMetrics'
import { FloatingPanel } from './FloatingPanel'
import { BoneIcon, CameraKindIcon, EffectsIcon, EllipsePresetIcon, GameExportIcon,
  DesktopExportIcon, GifExportIcon, ImageIcon, LightKindIcon, OpenIcon, PlusIcon, RedoIcon, SaveIcon, ShapeKindIcon, SparkleIcon, StarPresetIcon, TextKindIcon, TileGridIcon, TrailKindIcon, TrianglePresetIcon, UndoIcon } from './icons'



const SHAPE_PRESETS: { kind: ShapePreset; label: string; icon: () => ReactElement }[] = [
  { kind: 'Rectangle', label: 'Rectangle', icon: ShapeKindIcon },
  { kind: 'Ellipse', label: 'Ellipse', icon: EllipsePresetIcon },
  { kind: 'Triangle', label: 'Triangle', icon: TrianglePresetIcon },
  { kind: 'Star', label: 'Star', icon: StarPresetIcon },
]

/** One row in the + menu: an icon, a label, and either an action (a leaf)
 * or a nested flyout (`children`, a submenu opening to the left — the menu
 * hangs off the right side of the toolbar). Context-menu style: hovering a
 * submenu row opens it, clicking a leaf runs it and closes the whole menu. */
type AddMenuEntry =
  | { kind: 'item'; label: string; icon?: ReactElement; onSelect: () => void; hint?: string }
  | { kind: 'submenu'; label: string; icon?: ReactElement; children: AddMenuEntry[] }
  | { kind: 'separator' }

const MENU_PANEL = 'rounded-lg border border-panel-border bg-panel p-1 shadow-2xl backdrop-blur-panel'
const MENU_ROW = 'flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs text-ink hover:bg-overlay/8'

function AddMenuList({ entries, onDone }: { entries: AddMenuEntry[]; onDone: () => void }) {
  const [openIndex, setOpenIndex] = useState<number | null>(null)
  return (
    <div className={`w-44 ${MENU_PANEL}`}>
      {entries.map((entry, i) => {
        if (entry.kind === 'separator') return <div key={i} className="my-1 h-px bg-overlay/10" />
        if (entry.kind === 'item') {
          return (
            <button
              key={i}
              onMouseEnter={() => setOpenIndex(null)}
              onClick={() => {
                entry.onSelect()
                onDone()
              }}
              className={MENU_ROW}
              title={entry.hint}
            >
              <span className="flex w-4 justify-center text-ink-dim">{entry.icon}</span>
              {entry.label}
            </button>
          )
        }
        const open = openIndex === i
        return (
          <div key={i} className="relative" onMouseEnter={() => setOpenIndex(i)}>
            <button onClick={() => setOpenIndex(i)} className={`${MENU_ROW} ${open ? 'bg-overlay/8' : ''}`}>
              <span className="flex w-4 justify-center text-ink-dim">{entry.icon}</span>
              <span className="flex-1">{entry.label}</span>
              <span className="text-ink-dim">&lsaquo;</span>
            </button>
            {open && (
              <div className="absolute right-full top-0 z-30 mr-1">
                <AddMenuList entries={entry.children} onDone={onDone} />
              </div>
            )}
          </div>
        )
      })}
    </div>
  )
}

/**
 * The toolbar's "+" menu: everything you can add to (or load into) the
 * scene, grouped into Unity-like sections with nested flyouts instead of
 * one long list — 2D Object / Effects / Light 2D / Camera / Import / Demos.
 */
function ShapesMenu({
  onAddShape,
  onAddParticleSystem,
  onAddText,
  onImportSvg,
  onImportImage,
  onImportModel,
  onImportAudio,
  onLoadDemo,
  onLoadGeometryDashDemo,
  onLoadNinjaDemo,
  onLoadFpsDemo,
  onLoadStressDemo,
  onLoadRigDemo,
  onLoadDungeonDemo,
  onLoadMatch3Demo,
  onLoadLiquidDemo,
  onLoadTortureDemo,
  onLoad3DDemo,
  onLoadUiDemo,
  onLoadResponsiveDemo,
  onLoadHtmlDemo,
  onLoadPhysics3DDemo,
  onLoadTreesDemo,
  onLoadLeerDemo,
  onLoadGooDemo,
  onLoadGooShapesDemo,
  onLoadClothDemo,
  onLoadLanDemo,
  onLoadBoomDemo,
  onLoadVoxelDemo,
  onLoadShatterDemo,
  onLoadFightDemo,
  onLoadTacticsDemo,
  onLoadPhysicsDemo,
  onAddTilemap,
  onAddCamera,
  onAddNode3D,
  onAddUi,
  onAddLight2D,
  onAddTrail,
  onAddBone,
}: {
  onAddShape: (kind: ShapePreset) => void
  onAddParticleSystem: () => void
  /** "Text" — a Text node using the document's font (a bundled UI font is
   * added the first time). */
  onAddText: (icon?: boolean) => void
  onImportSvg: () => void
  onImportImage: () => void
  /** Import → 3D Model: a glTF / GLB file as a new 3D node. */
  onImportModel: () => void
  /** Import → Audio: a sound file. */
  onImportAudio: () => void
  /** "Tilemap…" — picks a tileset image and builds an empty `Tilemap` node
   * from it. Select it, then paint with `TilemapPaintOverlay`. */
  onAddTilemap: () => void
  /** "Camera" — a Unity-`Camera`-style viewpoint node. */
  onAddCamera: () => void
  /** "3D" items — a Node3D mesh / camera / light / pivot. */
  onAddNode3D: (item: Node3DItem) => void
  /** "UI" items — Button / Panel / Label / Stack, under the Camera (HUD). */
  onAddUi: (item: UiMenuItem) => void
  /** "Light 2D" items — a Unity-style Light 2D node of the given type. */
  onAddLight2D: (lightType: Light2DType) => void
  /** "Trail Renderer" — attaches to the selected node (so it follows it). */
  onAddTrail: () => void
  onAddBone: () => void
  onLoadDemo: () => void
  onLoadGeometryDashDemo: () => void
  /** Top-down Ninja Adventure (two scenes, lighting, combat). */
  onLoadNinjaDemo: () => void
  /** Scratch-style "fake 3D" raycaster FPS. */
  onLoadFpsDemo: () => void
  onLoadStressDemo: () => void
  onLoadRigDemo: () => void
  onLoadDungeonDemo: () => void
  onLoadMatch3Demo: () => void
  onLoadLiquidDemo: () => void
  onLoadTortureDemo: () => void
  onLoad3DDemo: () => void
  onLoadUiDemo: () => void
  onLoadResponsiveDemo: () => void
  onLoadHtmlDemo: () => void
  onLoadPhysics3DDemo: () => void
  onLoadTreesDemo: () => void
  onLoadLeerDemo: () => void
  onLoadGooDemo: () => void
  onLoadGooShapesDemo: () => void
  onLoadClothDemo: () => void
  onLoadLanDemo: () => void
  onLoadBoomDemo: () => void
  onLoadVoxelDemo: () => void
  onLoadShatterDemo: () => void
  onLoadFightDemo: () => void
  onLoadTacticsDemo: () => void
  onLoadPhysicsDemo: () => void
}) {
  const [open, setOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    function handleOutsideClick(event: MouseEvent) {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', handleOutsideClick)
    return () => document.removeEventListener('mousedown', handleOutsideClick)
  }, [])

  const entries: AddMenuEntry[] = [
    {
      kind: 'submenu',
      label: '2D Object',
      icon: <ShapeKindIcon />,
      children: [
        ...SHAPE_PRESETS.map(({ kind, label, icon: Icon }): AddMenuEntry => ({ kind: 'item', label, icon: <Icon />, onSelect: () => onAddShape(kind) })),
        { kind: 'separator' },
        { kind: 'item', label: 'Text', icon: <TextKindIcon />, onSelect: () => onAddText() },
        { kind: 'item', label: 'Icon (Material Symbols)', icon: <SparkleIcon />, onSelect: () => onAddText(true) },
        { kind: 'item', label: 'Sprite (Image)…', icon: <ImageIcon />, onSelect: onImportImage },
        { kind: 'item', label: 'Tilemap…', icon: <TileGridIcon />, onSelect: onAddTilemap },
        { kind: 'item', label: 'Bone', icon: <BoneIcon />, onSelect: onAddBone },
      ],
    },
    {
      kind: 'submenu',
      label: 'Effects',
      icon: <EffectsIcon />,
      children: [
        { kind: 'item', label: 'Particle System', icon: <EffectsIcon />, onSelect: onAddParticleSystem },
        { kind: 'item', label: 'Trail Renderer', icon: <TrailKindIcon />, onSelect: onAddTrail, hint: 'Attaches to the selected node' },
      ],
    },
    {
      kind: 'submenu',
      label: 'Light 2D',
      icon: <LightKindIcon />,
      children: (['Global', 'Spot', 'Freeform', 'Sprite'] as const).map(
        (lightType): AddMenuEntry => ({ kind: 'item', label: `${lightType} Light 2D`, icon: <LightKindIcon />, onSelect: () => onAddLight2D(lightType) }),
      ),
    },
    { kind: 'item', label: 'Camera', icon: <CameraKindIcon />, onSelect: onAddCamera },
    {
      kind: 'submenu',
      label: 'UI',
      icon: <SparkleIcon />,
      children: [
        ...UI_MENU_ITEMS.map((item): AddMenuEntry => ({ kind: 'item', label: UI_MENU_LABEL[item] ?? item, icon: <SparkleIcon />, onSelect: () => onAddUi(item) })),
        { kind: 'item', label: 'Icon', icon: <SparkleIcon />, onSelect: () => onAddText(true) },
      ],
    },
    {
      kind: 'submenu',
      label: '3D',
      icon: <CameraKindIcon />,
      children: (
        [
          ['Cube', 'Cube'],
          ['Sphere', 'Sphere'],
          ['Plane', 'Plane'],
          ['Cylinder', 'Cylinder'],
          ['Cone', 'Cone'],
          ['Torus', 'Torus'],
          ['Camera', 'Camera 3D'],
          ['Sun', 'Sun (Directional Light)'],
          ['PointLight', 'Point Light'],
          ['Empty', 'Empty (Pivot)'],
        ] as [Node3DItem, string][]
      ).map(([item, label]): AddMenuEntry => ({ kind: 'item', label, icon: <CameraKindIcon />, onSelect: () => onAddNode3D(item) })),
    },
    { kind: 'separator' },
    {
      kind: 'submenu',
      label: 'Import',
      icon: <OpenIcon />,
      children: [
        { kind: 'item', label: 'SVG…', icon: <OpenIcon />, onSelect: onImportSvg },
        { kind: 'item', label: 'Image…', icon: <ImageIcon />, onSelect: onImportImage },
        { kind: 'item', label: '3D Model (glTF / GLB / VRM)…', icon: <CameraKindIcon />, onSelect: onImportModel },
        { kind: 'item', label: 'Audio…', icon: <SparkleIcon />, onSelect: onImportAudio },
      ],
    },
    {
      kind: 'submenu',
      label: 'Demos',
      icon: <SparkleIcon />,
      children: [
        { kind: 'item', label: 'Demo Scene', icon: <SparkleIcon />, onSelect: onLoadDemo },
        { kind: 'item', label: 'Geometry Dash', icon: <SparkleIcon />, onSelect: onLoadGeometryDashDemo },
        { kind: 'item', label: 'Ninja Adventure', icon: <SparkleIcon />, onSelect: onLoadNinjaDemo },
        { kind: 'item', label: 'Dungeon Royale', icon: <SparkleIcon />, onSelect: onLoadDungeonDemo },
        { kind: 'item', label: 'Crown Match (match-3)', icon: <SparkleIcon />, onSelect: onLoadMatch3Demo },
        { kind: 'item', label: 'Liquid Lab', icon: <SparkleIcon />, onSelect: onLoadLiquidDemo },
        { kind: 'item', label: 'Ragdoll Torture', icon: <SparkleIcon />, onSelect: onLoadTortureDemo },
        { kind: 'item', label: '3D Showroom', icon: <SparkleIcon />, onSelect: onLoad3DDemo },
        { kind: 'item', label: 'UI & Sound Lab', icon: <SparkleIcon />, onSelect: onLoadUiDemo },
        { kind: 'item', label: 'Responsive Layout', icon: <SparkleIcon />, onSelect: onLoadResponsiveDemo },
        { kind: 'item', label: 'HTML UI Lab', icon: <SparkleIcon />, onSelect: onLoadHtmlDemo },
        { kind: 'item', label: 'Physics 3D Lab', icon: <SparkleIcon />, onSelect: onLoadPhysics3DDemo },
        { kind: 'item', label: 'Tree Garden', icon: <SparkleIcon />, onSelect: onLoadTreesDemo },
        { kind: 'item', label: "Reyna's Leer", icon: <SparkleIcon />, onSelect: onLoadLeerDemo },
        { kind: 'item', label: 'Goo Lab', icon: <SparkleIcon />, onSelect: onLoadGooDemo },
        { kind: 'item', label: 'Goo Shapes', icon: <SparkleIcon />, onSelect: onLoadGooShapesDemo },
        { kind: 'item', label: 'Cloth Lab', icon: <SparkleIcon />, onSelect: onLoadClothDemo },
        { kind: 'item', label: 'LAN Party (multiplayer)', icon: <SparkleIcon />, onSelect: onLoadLanDemo },
        { kind: 'item', label: 'Toon Explosion', icon: <SparkleIcon />, onSelect: onLoadBoomDemo },
        { kind: 'item', label: 'Voxel Craft', icon: <SparkleIcon />, onSelect: onLoadVoxelDemo },
        { kind: 'item', label: 'Shatterline (glass)', icon: <SparkleIcon />, onSelect: onLoadShatterDemo },
        { kind: 'item', label: 'Fatal Clash (3D fighting)', icon: <SparkleIcon />, onSelect: onLoadFightDemo },
        { kind: 'item', label: 'Sunset Tactics (5v5)', icon: <SparkleIcon />, onSelect: onLoadTacticsDemo },
        { kind: 'item', label: 'Raycaster FPS', icon: <SparkleIcon />, onSelect: onLoadFpsDemo },
        { kind: 'item', label: 'Ragdoll Arena', icon: <SparkleIcon />, onSelect: onLoadRigDemo },
        { kind: 'item', label: 'Physics Playground', icon: <SparkleIcon />, onSelect: onLoadPhysicsDemo },
        { kind: 'item', label: 'Stress Test', icon: <SparkleIcon />, onSelect: onLoadStressDemo },
      ],
    },
  ]

  return (
    <div ref={rootRef} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        title="Add"
        className="flex h-6 w-6 items-center justify-center rounded-md text-ink-dim hover:bg-overlay/5 hover:text-ink"
      >
        <PlusIcon />
      </button>
      {open && (
        <div className="absolute right-0 top-full z-20 mt-1">
          <AddMenuList entries={entries} onDone={() => setOpen(false)} />
        </div>
      )}
    </div>
  )
}

/** What the File menu needs from `App` — project-folder actions (see
 * `project/projectSession.ts`) plus the legacy single-file `.tea`/`.teab`
 * import/export. */
export interface FileMenuActions {
  /** The open project's name, or `null` for an unsaved, folder-less document. */
  projectName: string | null
  /** Unsaved changes since the last save (drives the Save button's dot). */
  dirty: boolean
  recent: string[]
  onNewProject: () => void
  onOpenProject: () => void
  onOpenRecent: (folder: string) => void
  onShowInExplorer: () => void
  onImportDocument: () => void
  onExportDocument: () => void
}

function FileMenu({ actions, onSave }: { actions: FileMenuActions; onSave: () => void }) {
  const [open, setOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    function handleOutsideClick(event: MouseEvent) {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', handleOutsideClick)
    return () => document.removeEventListener('mousedown', handleOutsideClick)
  }, [])
  const shortName = (folder: string) => folder.split('/').filter(Boolean).slice(-2).join('/')
  const entries: AddMenuEntry[] = [
    { kind: 'item', label: 'New Project…', icon: <PlusIcon />, onSelect: actions.onNewProject, hint: 'Pick an empty folder — the project is saved there as files' },
    { kind: 'item', label: 'Open Project…', icon: <OpenIcon />, onSelect: actions.onOpenProject },
    ...(actions.recent.length > 0
      ? [
          {
            kind: 'submenu' as const,
            label: 'Open Recent',
            icon: <OpenIcon />,
            children: actions.recent.map((folder): AddMenuEntry => ({ kind: 'item', label: shortName(folder), onSelect: () => actions.onOpenRecent(folder), hint: folder })),
          },
        ]
      : []),
    { kind: 'separator' },
    { kind: 'item', label: actions.projectName ? 'Save  (Ctrl+S)' : 'Save as Project…', icon: <SaveIcon />, onSelect: onSave },
    ...(actions.projectName ? [{ kind: 'item' as const, label: 'Show in Explorer', icon: <OpenIcon />, onSelect: actions.onShowInExplorer }] : []),
    { kind: 'separator' },
    { kind: 'item', label: 'Import .tea / .teab…', icon: <OpenIcon />, onSelect: actions.onImportDocument, hint: 'Open a single-file document (replaces the current one)' },
    { kind: 'item', label: 'Export .tea / .teab…', icon: <SaveIcon />, onSelect: actions.onExportDocument },
  ]
  return (
    <div ref={rootRef} className="relative">
      <button
        onClick={() => setOpen((v) => !v)}
        title={actions.projectName ? `Project: ${actions.projectName}` : 'File'}
        className="flex h-6 items-center gap-1 rounded-md px-1.5 text-[11px] font-medium text-ink-dim hover:bg-overlay/5 hover:text-ink"
      >
        <OpenIcon />
        <span className="max-w-28 truncate">{actions.projectName ?? 'File'}</span>
      </button>
      {open && (
        <div className="absolute left-0 top-full z-20 mt-1 [&>div]:w-56">
          <AddMenuList entries={entries} onDone={() => setOpen(false)} />
        </div>
      )}
    </div>
  )
}

/**
 * The toolbar's Export menu: the active animation as a GIF (opaque or
 * transparent), a transparent WebM or an MP4 video; or the whole game as
 * a web page or a desktop app.
 */
function ExportMenu({ onExportAnimation, onExportGame, onExportDesktopGame }: { onExportAnimation: (options: AnimationExportOptions) => void; onExportGame: () => void; onExportDesktopGame: () => void }) {
  const [open, setOpen] = useState(false)
  const rootRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    function handleOutsideClick(event: MouseEvent) {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', handleOutsideClick)
    return () => document.removeEventListener('mousedown', handleOutsideClick)
  }, [])
  const entries: AddMenuEntry[] = [
    { kind: 'item', label: 'Animation → GIF', icon: <GifExportIcon />, onSelect: () => onExportAnimation({ format: 'gif', transparent: false }) },
    { kind: 'item', label: 'Animation → GIF (transparent)', icon: <GifExportIcon />, onSelect: () => onExportAnimation({ format: 'gif', transparent: true }), hint: '1-bit transparency: pixels are fully clear or fully solid' },
    { kind: 'item', label: 'Animation → WebM (transparent)', icon: <GifExportIcon />, onSelect: () => onExportAnimation({ format: 'webm', transparent: true }), hint: 'VP9 video with a real alpha channel — overlays for OBS, video editors, the web' },
    { kind: 'item', label: 'Animation → MP4', icon: <GifExportIcon />, onSelect: () => onExportAnimation({ format: 'mp4', transparent: false }), hint: 'H.264 video (MP4 has no transparency)' },
    { kind: 'separator' },
    { kind: 'item', label: 'Game → Web page (.html)', icon: <GameExportIcon />, onSelect: onExportGame, hint: 'One self-contained page — runs in any browser' },
    { kind: 'item', label: 'Game → Desktop app (.exe)', icon: <DesktopExportIcon />, onSelect: onExportDesktopGame, hint: 'One file, double-click to play, F11 for fullscreen' },
  ]
  return (
    <div ref={rootRef} className="relative">
      <button
        onClick={() => setOpen((v) => !v)}
        title="Export"
        className="flex h-6 w-6 items-center justify-center rounded-md text-ink-dim hover:bg-overlay/5 hover:text-ink"
      >
        <GameExportIcon />
      </button>
      {open && (
        <div className="absolute right-0 top-full z-20 mt-1 [&>div]:w-60">
          <AddMenuList entries={entries} onDone={() => setOpen(false)} />
        </div>
      )}
    </div>
  )
}

/** The editor's top-level modes — the toolbar's segmented buttons.
 * Animate/State Machine are bottom-dock tabs, not modes (see
 * `TopToolbar`'s own doc comment). */
export type Mode = 'Design' | 'Playground' | 'Paint' | 'Code'

const MODES: Mode[] = ['Design', 'Playground', 'Paint', 'Code']

/**
 * Mode switch (Design/Playground/Paint) plus undo/redo, since both are
 * global, always-available document actions with no more natural home yet
 * than "the persistent top bar." `onUndo`/`onRedo` call straight through to
 * `App.tsx`'s handlers, which call the real `undo`/`redo` Tauri commands
 * (§4's `Command`/undo-stack architecture) — this component has no opinion
 * on whether undo is currently possible (no disabled state yet), since the
 * document doesn't expose "is there anything to undo" over IPC yet.
 *
 * Animate/State Machine used to be 2 more modes here — they're a dockable,
 * tabbed bottom area now (`App.tsx`'s `bottomDockTab`/`bottomDockOpen`,
 * toggled from `IconRail`, same "+ add panel" convention the Layers/
 * Inspector/etc. dockable panels already use), not a canvas-wide mode
 * switch — a designer legitimately wants both the canvas (Design) and the
 * Timeline/state graph open and visible together, which modeling them as
 * mutually-exclusive top-level modes never actually allowed.
 *
 * `mode`/`onModeChange` are controlled (not local `useState`) now — `App`
 * needs to know which mode is active to tell `WasmEditor` to start/stop
 * driving the demo state machine (`setStateMachineActive`). This mode switch
 * deliberately does *not* touch the right-dock Inspector at all anymore —
 * an earlier revision swapped the whole right dock over to a
 * State-Machine-only panel while that mode was active, which meant picking
 * "State Machine" cost you your Inspector entirely; Inspector (and Layers)
 * are always-present fixtures now, the same way every competitor's editor
 * keeps its properties panel around regardless of which secondary tool
 * you're using.
 *
 * Full-width, fixed chrome right below the title bar — not draggable. An
 * earlier pass gave this panel free-floating drag-with-snap behavior
 * (`useTopBarDrag`), but there's no real scenario where a designer wants the
 * mode switch anywhere other than "the full-width bar under the title bar",
 * so that complexity was removed rather than kept unused — same reasoning
 * as `TimelinePanel` being permanently bottom-docked with no drag logic.
 *
 * `onOpen`/`onSave` are the file-menu equivalent of `onUndo`/`onRedo`: global,
 * always-available document actions with the same "no more natural home yet"
 * reasoning, now that a document can actually be saved/opened at all
 * (`App.tsx`'s handlers call through `ipc/nativeFile.ts`'s dialog/disk-I/O
 * wrappers plus `WasmEditor`'s `save_tea`/`save_teab`/`load_tea`/`load_teab`).
 * `onExportAnimation` sits right next to `onSave` for the same "global document
 * action" reasoning — renders the active animation to a `.gif` (see
 * `exportGif.ts`) so it can be dropped next to a reference clip for
 * comparison, a fidelity-debugging aid rather than a "share this clip"
 * feature.
 *
 * `onAddShape` is Phase 4's node-creation entry point (AN1MBYT3_BUILD_PLAN.md
 * §10: "Designer can create/edit/save a `.tea` file entirely through the
 * UI") — calls straight through to `WasmEditor::add_shape_node`, a real
 * `Command::AddNode`, undoable exactly like every other edit here. Exposed
 * through the `ShapesMenu` popover below (Rectangle/Ellipse/Triangle/Star),
 * not a single bare button — see that component's own doc comment.
 *
 * `onLoadDemo` is the Shapes menu's "Load Demo Scene" action — see
 * `demoScene.ts`'s doc comment for why it's a real SVG run through the same
 * `import_svg` pipeline a user's own file goes through, not a hand-built
 * document, and `App.tsx`'s `handleLoadDemo` for what it wires up to.
 */
export function TopToolbar({
  mode,
  onModeChange,
  onUndo,
  onRedo,
  fileMenu,
  onSave,
  onExportAnimation,
  onExportGame,
  onExportDesktopGame,
  onAddShape,
  onAddParticleSystem,
  onImportSvg,
  onImportImage,
  onImportModel,
  onImportAudio,
  onLoadDemo,
  onLoadGeometryDashDemo,
  onLoadNinjaDemo,
  onAddTilemap,
  onAddCamera,
  onAddNode3D,
  onAddUi,
  onAddLight2D,
  onAddTrail,
  onAddText,
  onLoadFpsDemo,
  onLoadStressDemo,
  onLoadRigDemo,
  onLoadDungeonDemo,
  onLoadMatch3Demo,
  onLoadLiquidDemo,
  onLoadTortureDemo,
  onLoad3DDemo,
  onLoadUiDemo,
  onLoadResponsiveDemo,
  onLoadHtmlDemo,
  onLoadPhysics3DDemo,
  onLoadTreesDemo,
  onLoadLeerDemo,
  onLoadGooDemo,
  onLoadGooShapesDemo,
  onLoadClothDemo,
  onLoadLanDemo,
  onLoadBoomDemo,
  onLoadVoxelDemo,
  onLoadShatterDemo,
  onLoadFightDemo,
  onLoadTacticsDemo,
  onLoadPhysicsDemo,
  onAddBone,
  onPaintActionBarTargetChange,
  onSaveDrawing,
  onNewDrawing,
  sceneSwitcher,
}: {
  mode: Mode
  onModeChange: (mode: Mode) => void
  onUndo: () => void
  onRedo: () => void
  fileMenu: FileMenuActions
  onSave: () => void
  /** Export menu → an animation as GIF / WebM / MP4. */
  onExportAnimation: (options: AnimationExportOptions) => void
  /** Exports the document as one self-contained, playable `.html`. */
  onExportGame: () => void
  onExportDesktopGame: () => void
  onAddShape: (kind: ShapePreset) => void
  onAddParticleSystem: () => void
  onImportSvg: () => void
  onImportImage: () => void
  /** Import → 3D Model: a glTF / GLB file as a new 3D node. */
  onImportModel: () => void
  /** Import → Audio: a sound file. */
  onImportAudio: () => void
  onLoadDemo: () => void
  onLoadGeometryDashDemo: () => void
  onLoadNinjaDemo: () => void
  onAddTilemap: () => void
  onAddCamera: () => void
  /** "3D" items — a Node3D mesh / camera / light / pivot. */
  onAddNode3D: (item: Node3DItem) => void
  /** "UI" items — Button / Panel / Label / Stack, under the Camera (HUD). */
  onAddUi: (item: UiMenuItem) => void
  /** "Light 2D" items — a Unity-style Light 2D node of the given type. */
  onAddLight2D: (lightType: Light2DType) => void
  /** "Trail Renderer" — attaches to the selected node (so it follows it). */
  onAddTrail: () => void
  onAddText: (icon?: boolean) => void
  onLoadFpsDemo: () => void
  onLoadStressDemo: () => void
  onLoadRigDemo: () => void
  onLoadDungeonDemo: () => void
  onLoadMatch3Demo: () => void
  onLoadLiquidDemo: () => void
  onLoadTortureDemo: () => void
  onLoad3DDemo: () => void
  onLoadUiDemo: () => void
  onLoadResponsiveDemo: () => void
  onLoadHtmlDemo: () => void
  onLoadPhysics3DDemo: () => void
  onLoadTreesDemo: () => void
  onLoadLeerDemo: () => void
  onLoadGooDemo: () => void
  onLoadGooShapesDemo: () => void
  onLoadClothDemo: () => void
  onLoadLanDemo: () => void
  onLoadBoomDemo: () => void
  onLoadVoxelDemo: () => void
  onLoadShatterDemo: () => void
  onLoadFightDemo: () => void
  onLoadTacticsDemo: () => void
  onLoadPhysicsDemo: () => void
  onAddBone: () => void
  /** Paint Asset Mode's own action bar (costume name/undo-redo/group-
   * ungroup/z-order/fill-stroke/current-tool-properties) portals its
   * rendered output into this element — see `PaintAssetEditor.tsx`'s own
   * doc comment for why a portal, not a plain prop, is what gets it here:
   * its real Group/Flip/Copy/etc. handlers stay wired to `onUpdateImage`
   * deep inside scratch-paint's own component tree, not reachable from a
   * true sibling like this toolbar. */
  onPaintActionBarTargetChange: (el: HTMLDivElement | null) => void
  onSaveDrawing: () => void
  /** Paint mode "New": start a blank drawing (saves as a new resource). */
  onNewDrawing: () => void
  /** `App.tsx`'s own `SceneSwitcher`, rendered right after the mode
   * buttons — passed in as a ready-built node (not a pile of scene props)
   * since every piece of scene state already lives in `App.tsx`. */
  sceneSwitcher?: ReactNode
}) {
  const m = useLayoutMetrics()
  return (
    <div
      // `z-30` — otherwise this (and the `ShapesMenu` popover nested inside
      // it) paints *behind* the docked Layers/Inspector panels, which have
      // no z-index of their own but render later in the DOM: with both at
      // the implicit stacking order, later-in-DOM wins, so the "+"
      // shapes-menu popover was getting visually clipped/covered by
      // whichever dock panel happened to sit in front of it (the bug this
      // fixes). Higher than `DockablePanel`/`DockGroup`'s resize-handle
      // `z-10`, so the toolbar (and anything it pops open) always stays on
      // top of the canvas-dominant floating panels, matching how a
      // persistent top toolbar behaves in every comparable editor.
      className="pointer-events-auto absolute z-30"
      style={{ left: m.leftDockOffset, right: m.edgeMargin, top: m.toolbarTop }}
    >
      <FloatingPanel className="flex items-center gap-0.5 px-1 py-1">
        {MODES.map((m) => (
          <button
            key={m}
            onClick={() => onModeChange(m)}
            className={`rounded-md px-2 py-1 text-xs transition-colors ${
              mode === m ? 'bg-accent/15 text-accent' : 'text-ink-dim hover:text-ink'
            }`}
          >
            {m}
          </button>
        ))}

        {sceneSwitcher && mode !== 'Paint' && mode !== 'Code' && (
          <>
            <div className="mx-1 h-4 w-px bg-overlay/10" />
            {sceneSwitcher}
          </>
        )}

        {mode === 'Paint' ? (
          <div className="ml-auto flex flex-1 items-center gap-3">
            <div ref={onPaintActionBarTargetChange} className="flex flex-1 items-center gap-3 empty:hidden" />
            <button
              onClick={onNewDrawing}
              title="New drawing (the current one stays in Resources if you saved it)"
              className="h-6 rounded-md px-2 text-[11px] text-ink-dim hover:bg-overlay/8 hover:text-ink"
            >
              New
            </button>
            <button
              onClick={onSaveDrawing}
              title="Save as Resource (Resources ▸ Drawings)"
              className="flex h-6 w-6 items-center justify-center rounded-md bg-accent text-white hover:bg-accent/90"
            >
              <SaveIcon />
            </button>
          </div>
        ) : (
          <div className="ml-auto flex items-center gap-0.5">
            <ShapesMenu
              onAddShape={onAddShape}
              onAddParticleSystem={onAddParticleSystem}
              onImportSvg={onImportSvg}
              onImportImage={onImportImage}
              onImportModel={onImportModel}
              onImportAudio={onImportAudio}
              onLoadDemo={onLoadDemo}
              onLoadGeometryDashDemo={onLoadGeometryDashDemo}
              onLoadNinjaDemo={onLoadNinjaDemo}
              onAddTilemap={onAddTilemap}
              onAddCamera={onAddCamera}
              onAddNode3D={onAddNode3D}
              onAddUi={onAddUi}
              onAddLight2D={onAddLight2D}
              onAddTrail={onAddTrail}
              onAddText={onAddText}
              onLoadFpsDemo={onLoadFpsDemo}
              onLoadStressDemo={onLoadStressDemo}
              onLoadRigDemo={onLoadRigDemo}
              onLoadDungeonDemo={onLoadDungeonDemo}
              onLoadMatch3Demo={onLoadMatch3Demo}
              onLoadLiquidDemo={onLoadLiquidDemo}
              onLoadTortureDemo={onLoadTortureDemo}
              onLoad3DDemo={onLoad3DDemo}
              onLoadUiDemo={onLoadUiDemo}
              onLoadResponsiveDemo={onLoadResponsiveDemo}
              onLoadHtmlDemo={onLoadHtmlDemo}
              onLoadPhysics3DDemo={onLoadPhysics3DDemo}
              onLoadTreesDemo={onLoadTreesDemo}
              onLoadLeerDemo={onLoadLeerDemo}
              onLoadGooDemo={onLoadGooDemo}
              onLoadGooShapesDemo={onLoadGooShapesDemo}
              onLoadClothDemo={onLoadClothDemo}
              onLoadLanDemo={onLoadLanDemo}
              onLoadBoomDemo={onLoadBoomDemo}
              onLoadVoxelDemo={onLoadVoxelDemo}
              onLoadShatterDemo={onLoadShatterDemo}
              onLoadFightDemo={onLoadFightDemo}
              onLoadTacticsDemo={onLoadTacticsDemo}
              onLoadPhysicsDemo={onLoadPhysicsDemo}
              onAddBone={onAddBone}
            />
            <button
              onClick={onAddBone}
              title="Add Bone"
              className="flex h-6 w-6 items-center justify-center rounded-md text-ink-dim hover:bg-overlay/5 hover:text-ink"
            >
              <BoneIcon />
            </button>
            <div className="mx-1 h-4 w-px bg-overlay/10" />
            <FileMenu actions={fileMenu} onSave={onSave} />
            <button
              onClick={onSave}
              title={fileMenu.dirty ? 'Save (unsaved changes)' : 'Save'}
              className="relative flex h-6 w-6 items-center justify-center rounded-md text-ink-dim hover:bg-overlay/5 hover:text-ink"
            >
              <SaveIcon />
              {fileMenu.dirty && <span className="absolute right-0.5 top-0.5 h-1.5 w-1.5 rounded-full bg-accent" />}
            </button>
            <ExportMenu onExportAnimation={onExportAnimation} onExportGame={onExportGame} onExportDesktopGame={onExportDesktopGame} />
            <div className="mx-1 h-4 w-px bg-overlay/10" />
            <button
              onClick={onUndo}
              title="Undo"
              className="flex h-6 w-6 items-center justify-center rounded-md text-ink-dim hover:bg-overlay/5 hover:text-ink"
            >
              <UndoIcon />
            </button>
            <button
              onClick={onRedo}
              title="Redo"
              className="flex h-6 w-6 items-center justify-center rounded-md text-ink-dim hover:bg-overlay/5 hover:text-ink"
            >
              <RedoIcon />
            </button>
          </div>
        )}
      </FloatingPanel>
    </div>
  )
}
