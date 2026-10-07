//! "Shuriken Dungeon" — a Wolfenstein-style first-person shooter built the
//! way people build "3D" games in Scratch: **no 3D at all**, just a row of
//! 2D clones and a lot of math.
//!
//! The screen is 80 thin vertical strips (one node each — the "clones").
//! Every frame the Code program casts one ray per strip through a grid map
//! (DDA raycasting), then for each strip:
//!
//! - **height** = screen height / distance to the wall it hit (perspective),
//! - **image** = which 1-pixel-wide column of the wall texture the ray hit
//!   (the wall textures are pre-sliced into 1x16 strips, so "texturing" is
//!   just swapping a strip's image — Scratch's costume-per-column trick),
//! - **tint** = distance fog + darker on one wall axis (fake lighting),
//! - **z-index** = −distance.
//!
//! Enemies are billboards (flat sprites always facing you), projected to a
//! screen x and a size by the same camera math, and also z-sorted by
//! −distance — so a single painter's-algorithm sort of strips *and*
//! sprites gives correct occlusion: a skull half behind a pillar really is
//! half hidden. That's the whole "3D engine": 2D sprites, a sort, and trig.
//!
//! Features the engine gained for this (see `scriptRuntime.ts`):
//! `ref.zIndex` (per-frame depth sorting), `ctx.mouseDX/DY` +
//! `ctx.requestPointerLock()` (mouselook), the `"mouse"` held-key (click to
//! throw), and a Pixel Perfect camera (the whole frame renders at 320x200
//! and is nearest-upscaled, so it looks like a real DOS-era raycaster).

use std::fmt::Write as _;

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Command, Document, FontAsset, GradientDef, GradientKind, GradientStop, GroupNode, ImageId, LoopMode, Node, NodeKind,
    Path, ProgramDef, ShapeNode, StateDef, StateMachineDef, TilemapNode,
};
use glam::Vec2;

use crate::demo_ninja::{add_grid_frames, add_node, add_png, image_node, text_node, with_id};

const WALLS_BYTES: &[u8] = include_bytes!("../assets/fps/walls.png");
const MINIMAP_BYTES: &[u8] = include_bytes!("../assets/fps/minimap.png");
const SHURIKEN_BYTES: &[u8] = include_bytes!("../assets/fps/shuriken.png");
const EXPLOSION_BYTES: &[u8] = include_bytes!("../assets/fps/explosion.png");
const SKULL_BYTES: &[u8] = include_bytes!("../assets/ninja/skull.png");
const SLIME_BYTES: &[u8] = include_bytes!("../assets/ninja/slime.png");
const SKELETON_BYTES: &[u8] = include_bytes!("../assets/ninja/skeleton.png");
const HEART_BYTES: &[u8] = include_bytes!("../assets/ninja/heart.png");
const PIXEL_FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");

/// The game's reference resolution (and the scene's size).
const SCREEN_W: f32 = 320.0;
const SCREEN_H: f32 = 200.0;
/// Number of wall strips ("clones") and their width — 80 x 4px = 320px.
const STRIPS: usize = 80;
const STRIP_W: f32 = SCREEN_W / STRIPS as f32;
/// Billboard / explosion pools.
const FOES: usize = 12;
const BOOMS: usize = 4;

/// The level. `0` = floor; `1..=4` = wall texture (stone, brick, wood,
/// gold — `walls.png`'s 16px tiles in that order). Row 0 is the far north
/// edge; the script and the minimap both read this same table.
const MAP: [&str; 16] = [
    "1111111111111111",
    "1000000000000001",
    "1022200000333001",
    "1020000000003001",
    "1020000440000001",
    "1000000440000001",
    "1000000000000001",
    "1033300000222001",
    "1000300000002001",
    "1000300000002001",
    "1000000440000001",
    "1000000440000001",
    "1022000000000331",
    "1020000000000031",
    "1000000000000001",
    "1111111111111111",
];

pub(crate) struct FpsDemo {
    pub scene: ArtboardId,
    pub images: Vec<ImageId>,
}

fn rect(name: &str, parent: Option<an1mbyt3_core::NodeId>, w: f32, h: f32, color: [f32; 4], at: Vec2) -> Node {
    let mut node = Node::new(
        name,
        parent,
        NodeKind::Shape(ShapeNode { path: Path::rect(w, h), fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None }),
    );
    node.transform.translation = at;
    node
}

fn vertical_gradient(top: [f32; 4], bottom: [f32; 4], half_h: f32) -> GradientDef {
    GradientDef {
        kind: GradientKind::Linear,
        start: Vec2::new(0.0, half_h),
        end: Vec2::new(0.0, -half_h),
        stops: vec![GradientStop { offset: 0.0, color: top, midpoint: 0.5 }, GradientStop { offset: 1.0, color: bottom, midpoint: 0.5 }],
    }
}

pub(crate) fn build_demo_fps(document: &mut Document) -> FpsDemo {
    let mut inverses = Vec::new();
    let mut images = Vec::new();

    // ---- Assets ----
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: PIXEL_FONT_BYTES.to_vec() });
    // Each 16px wall tile cut into sixteen 1x16 strips — "walls 0..63".
    let walls = add_grid_frames(document, "walls", WALLS_BYTES, 1, 16, 100);
    let skull = add_grid_frames(document, "skull", SKULL_BYTES, 16, 16, 120);
    let slime = add_grid_frames(document, "slime", SLIME_BYTES, 16, 16, 150);
    let skeleton = add_grid_frames(document, "skeleton", SKELETON_BYTES, 16, 16, 150);
    let heart = add_grid_frames(document, "heart", HEART_BYTES, 16, 16, 100);
    let shuriken = add_grid_frames(document, "shuriken", SHURIKEN_BYTES, 23, 23, 60);
    let explosion = add_grid_frames(document, "boom", EXPLOSION_BYTES, 40, 40, 60);
    let minimap_tiles = add_png(document, "minimap.png", MINIMAP_BYTES);
    for frames in [&walls, &skull, &slime, &skeleton, &heart, &shuriken, &explosion] {
        images.extend(frames.iter().copied());
    }
    images.push(minimap_tiles.image);

    // ---- Scene ----
    let inverse = document.apply(Command::AddArtboard { name: "Raycaster".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    // Ceiling and floor: two gradients meeting at the horizon (darkest at
    // the horizon = "far away").
    let mut ceiling = rect("Ceiling", None, SCREEN_W, SCREEN_H / 2.0, [1.0; 4], Vec2::new(0.0, SCREEN_H / 4.0));
    if let NodeKind::Shape(shape) = &mut ceiling.kind {
        shape.gradient = Some(vertical_gradient([0.10, 0.11, 0.17, 1.0], [0.02, 0.02, 0.04, 1.0], SCREEN_H / 4.0));
    }
    add_node(document, &mut inverses, scene, ceiling);
    let mut floor = rect("Floor", None, SCREEN_W, SCREEN_H / 2.0, [1.0; 4], Vec2::new(0.0, -SCREEN_H / 4.0));
    if let NodeKind::Shape(shape) = &mut floor.kind {
        shape.gradient = Some(vertical_gradient([0.03, 0.025, 0.02, 1.0], [0.26, 0.19, 0.13, 1.0], SCREEN_H / 4.0));
    }
    add_node(document, &mut inverses, scene, floor);

    // "View": every strip and billboard, depth-sorted together each frame
    // by their z-index (= −distance).
    let view = add_node(document, &mut inverses, scene, with_id(Node::new("View", None, NodeKind::Group(GroupNode::default())), "view"));
    for i in 0..STRIPS {
        // A strip is an Image node STRIP_W wide and 1 tall; the script
        // stretches it to the wall's on-screen height through scale.y.
        let mut strip = with_id(image_node("Strip", Some(view), walls[0], STRIP_W, 1.0), &format!("col{i}"));
        strip.transform.translation = Vec2::new(-SCREEN_W / 2.0 + STRIP_W * (i as f32 + 0.5), 0.0);
        strip.classes = vec!["col".into()];
        add_node(document, &mut inverses, scene, strip);
    }
    for i in 0..FOES {
        let mut foe = with_id(image_node("Enemy", Some(view), skull[0], 16.0, 16.0), &format!("foe{i}"));
        foe.transform.translation = Vec2::new(100_000.0, 0.0);
        foe.classes = vec!["foe".into()];
        add_node(document, &mut inverses, scene, foe);
    }
    for i in 0..BOOMS {
        let mut boom = with_id(image_node("Explosion", Some(view), explosion[0], 40.0, 40.0), &format!("boom{i}"));
        boom.transform.translation = Vec2::new(100_000.0, 0.0);
        add_node(document, &mut inverses, scene, boom);
    }

    // ---- Camera + HUD ----
    let camera = with_id(
        Node::new(
            "Main Camera",
            None,
            NodeKind::Camera(CameraNode {
                zoom: 1.0,
                background_color: [0.0, 0.0, 0.0, 1.0],
                follow_target: None,
                follow_smoothing: 0.0,
                vignette: an1mbyt3_core::CameraVignette { enabled: true, color: [0.0, 0.0, 0.0, 1.0], intensity: 0.75, radius: 1.0, blur: 0.65 },
                pixel_perfect: an1mbyt3_core::PixelPerfect {
                    enabled: true,
                    grid_snapping: an1mbyt3_core::PixelGridSnapping::UpscaleRenderTexture,
                    crop_frame: an1mbyt3_core::PixelCropFrame::Windowbox,
                },
            }),
        ),
        "camera",
    );
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));
    let hud = add_node(document, &mut inverses, scene, Node::new("HUD", Some(camera_id), NodeKind::Group(GroupNode::default())));

    // Held weapon (2x scale) and the one in flight.
    let mut weapon = with_id(image_node("Weapon", Some(hud), shuriken[0], 46.0, 46.0), "weapon");
    weapon.transform.translation = Vec2::new(96.0, -76.0);
    add_node(document, &mut inverses, scene, weapon);
    let mut thrown = with_id(image_node("Thrown", Some(hud), shuriken[0], 23.0, 23.0), "thrown");
    thrown.transform.translation = Vec2::new(100_000.0, 0.0);
    add_node(document, &mut inverses, scene, thrown);

    // Crosshair: a tiny plus.
    let white = [1.0, 1.0, 1.0, 0.85];
    add_node(document, &mut inverses, scene, rect("Crosshair H", Some(hud), 7.0, 1.0, white, Vec2::ZERO));
    add_node(document, &mut inverses, scene, rect("Crosshair V", Some(hud), 1.0, 7.0, white, Vec2::ZERO));

    for i in 0..3 {
        let mut h = with_id(image_node("Heart", Some(hud), heart[4], 14.0, 14.0), &format!("heart{i}"));
        h.transform.translation = Vec2::new(-148.0 + i as f32 * 15.0, 88.0);
        add_node(document, &mut inverses, scene, h);
    }
    add_node(document, &mut inverses, scene, text_node(Some(hud), "score", "SCORE 0", font, 8.0, [1.0, 0.9, 0.5, 1.0], Vec2::new(-104.0, 70.0)));
    add_node(document, &mut inverses, scene, text_node(Some(hud), "msg", "", font, 8.0, [1.0, 1.0, 1.0, 1.0], Vec2::new(0.0, 30.0)));
    add_node(document, &mut inverses, scene, text_node(Some(hud), "hint", "", font, 8.0, [0.75, 0.8, 0.95, 1.0], Vec2::new(0.0, -92.0)));

    // Minimap (top-right): the same MAP, as a 3px-per-cell tilemap.
    let mut minimap = TilemapNode::empty(minimap_tiles.image, minimap_tiles.width, minimap_tiles.height, 1, 1, 16, 16, 3.0);
    for (y, row) in MAP.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            minimap.tiles[y * 16 + x] = Some(ch.to_digit(10).unwrap_or(1));
        }
    }
    let mut minimap_node = Node::new("Minimap", Some(hud), NodeKind::Tilemap(minimap));
    minimap_node.transform.translation = Vec2::new(132.0, 72.0);
    add_node(document, &mut inverses, scene, minimap_node);
    for i in 0..FOES {
        add_node(document, &mut inverses, scene, with_id(rect("Minimap Enemy", Some(hud), 2.0, 2.0, [1.0, 0.3, 0.3, 1.0], Vec2::new(100_000.0, 0.0)), &format!("mmFoe{i}")));
    }
    // Player arrow: an elongated rect rotated to the view direction.
    add_node(document, &mut inverses, scene, with_id(rect("Minimap Player", Some(hud), 5.0, 2.0, [1.0, 0.95, 0.4, 1.0], Vec2::new(132.0, 72.0)), "mmPlayer"));

    // Full-screen damage flash, drawn last (transparent at rest).
    add_node(document, &mut inverses, scene, with_id(rect("Damage Flash", Some(hud), SCREEN_W, SCREEN_H, [0.85, 0.05, 0.05, 0.0], Vec2::ZERO), "flash"));

    // ---- The game ----
    let script = fps_script();
    let animation_inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("FPS filler") });
    let Command::SetAnimationDeleted { animation, .. } = animation_inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(animation_inverse);
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Raycaster".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Playing".into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(script)),
            }],
            initial_state: "Playing".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));

    inverses.reverse();
    document.commit_batch(inverses);
    FpsDemo { scene, images }
}

/// The map table, then the fixed game logic.
fn fps_script() -> String {
    let mut out = String::from("// Generated by build_demo_fps: the level (0 = floor, 1-4 = wall texture).\nconst MAP = [\n");
    for row in MAP {
        let _ = writeln!(out, "  '{row}',");
    }
    out.push_str("]\n");
    let _ = writeln!(out, "const STRIPS = {STRIPS}\nconst FOES = {FOES}\nconst BOOMS = {BOOMS}");
    out.push_str(FPS_SCRIPT_BODY);
    out
}

const FPS_SCRIPT_BODY: &str = r#"
// =====================================================================
// Tuning
// =====================================================================
const SCREEN_H = 200
const HALF_W = 160
const PLANE = 0.66            // camera plane length: ~66 degree field of view
const MOVE_SPEED = 3.2        // cells per second
const TURN_SPEED = 2.6        // radians per second (keys)
const MOUSE_SENS = 0.0032     // radians per pixel of mouse movement
const RADIUS = 0.22           // player collision radius (cells)
const FOG = 12                // cells until walls fade to near-black
const THROW_COOLDOWN = 0.32
const MINIMAP = { x0: 108, y0: 96, cell: 3 }   // minimap top-left + cell size (screen px)
const SPAWNS = [[1.5, 1.5], [14.5, 1.5], [1.5, 14.5], [14.5, 14.5], [8.5, 1.5], [8.5, 14.5], [1.5, 8.5], [14.5, 8.5]]
const KINDS = {
  slime: { hp: 1, speed: 0.95, size: 0.55, fps: 6, floats: false, score: 10 },
  skull: { hp: 2, speed: 1.7, size: 0.5, fps: 10, floats: true, score: 25 },
  skeleton: { hp: 3, speed: 1.25, size: 0.75, fps: 7, floats: false, score: 40 },
}

const G = ctx.globals
const dt = Math.min(ctx.deltaSeconds, 0.05)
const k = ctx.keysDown

// =====================================================================
// Helpers
// =====================================================================
function cell(x, y) {
  const row = MAP[Math.floor(y)]
  if (!row) return 1
  const c = row.charCodeAt(Math.floor(x)) - 48
  return c >= 0 && c <= 9 ? c : 1
}
function frame(name) {
  return G.frames[name] || null
}
function say(text, seconds) {
  G.msg = text
  G.msgTime = seconds || 2
}
function blip(freq, length, type, volume) {
  try {
    G.audio = G.audio || new (window.AudioContext || window.webkitAudioContext)()
    const a = G.audio
    const osc = a.createOscillator()
    const gain = a.createGain()
    osc.type = type || 'square'
    osc.frequency.setValueAtTime(freq, a.currentTime)
    osc.frequency.exponentialRampToValueAtTime(Math.max(30, freq * 0.4), a.currentTime + length)
    gain.gain.setValueAtTime(volume || 0.05, a.currentTime)
    gain.gain.exponentialRampToValueAtTime(0.001, a.currentTime + length)
    osc.connect(gain).connect(a.destination)
    osc.start()
    osc.stop(a.currentTime + length)
  } catch (e) {}
}
// Cache every write: only touch the engine when a value really changed.
function put(ref, prop, value) {
  const key = ref.id + '.' + prop
  if (G.cache[key] === value) return
  G.cache[key] = value
  ref[prop] = value
}
function putTint(ref, r, g, b, a) {
  // Quantized so a still camera produces zero writes.
  const q = (v) => Math.round(v * 64) / 64
  const key = ref.id + '.tint'
  const value = q(r) + ',' + q(g) + ',' + q(b) + ',' + q(a)
  if (G.cache[key] === value) return
  G.cache[key] = value
  ref.fillColor = [q(r), q(g), q(b), q(a)]
}
function putImage(ref, name) {
  const key = ref.id + '.img'
  if (G.cache[key] === name) return
  const id = frame(name)
  if (!id) return
  G.cache[key] = name
  ref.image = id
}
function walkable(x, y) {
  return cell(x + RADIUS, y + RADIUS) === 0 && cell(x - RADIUS, y + RADIUS) === 0 && cell(x + RADIUS, y - RADIUS) === 0 && cell(x - RADIUS, y - RADIUS) === 0
}

// =====================================================================
// New game
// =====================================================================
if (!G.started) {
  G.started = true
  G.frames = {}
  for (const img of ctx.images) G.frames[img.name] = img.id
  G.cache = {}
  // South-west corridor, looking north up a long open lane.
  G.px = 5.5
  G.py = 13.5
  G.angle = -Math.PI / 2   // facing north (up the map)
  G.hp = 6
  G.score = 0
  G.wave = 0
  G.waveDelay = 1.2
  G.foes = []
  G.booms = []
  G.cool = 0
  G.throwT = -1
  G.flash = 0
  G.shake = 0
  G.bob = 0
  G.boost = 0
  G.dead = false
  G.clock = 0
  G.prevFire = false
  say('SHURIKEN DUNGEON', 2.5)
}
G.clock += dt

// =====================================================================
// Input: move / strafe / turn (keys + pointer-locked mouse)
// =====================================================================
ctx.requestPointerLock()
let moving = false
if (!G.dead) {
  let turn = ((k.has('right') || k.has('e') ? 1 : 0) - (k.has('left') || k.has('q') ? 1 : 0)) * TURN_SPEED * dt
  if (ctx.pointerLocked) turn += ctx.mouseDX * MOUSE_SENS
  G.angle += turn
  const dirX = Math.cos(G.angle)
  const dirY = Math.sin(G.angle)
  const fwd = (k.has('w') || k.has('up') ? 1 : 0) - (k.has('s') || k.has('down') ? 1 : 0)
  const strafe = (k.has('d') ? 1 : 0) - (k.has('a') ? 1 : 0)
  let mx = dirX * fwd - dirY * strafe
  let my = dirY * fwd + dirX * strafe
  const len = Math.hypot(mx, my)
  if (len > 0) {
    moving = true
    mx = (mx / len) * MOVE_SPEED * dt
    my = (my / len) * MOVE_SPEED * dt
    if (walkable(G.px + mx, G.py)) G.px += mx
    if (walkable(G.px, G.py + my)) G.py += my
  }
} else if (k.has('r')) {
  G.started = false
  return
}
const dirX = Math.cos(G.angle)
const dirY = Math.sin(G.angle)
// Camera plane: perpendicular to the view direction (screen-right).
const planeX = -dirY * PLANE
const planeY = dirX * PLANE
G.bob = moving ? G.bob + dt * 11 : G.bob * 0.85

// =====================================================================
// Raycasting: one ray per strip (DDA through the grid)
// =====================================================================
const zbuf = new Array(STRIPS)
const strips = getCompByClass('col')
G.boost = Math.max(0, G.boost - dt * 6)
for (const strip of strips) {
  const i = parseInt(strip.id.slice(3), 10)
  const cameraX = (2 * (i + 0.5)) / STRIPS - 1
  const rayX = dirX + planeX * cameraX
  const rayY = dirY + planeY * cameraX
  let mapX = Math.floor(G.px)
  let mapY = Math.floor(G.py)
  const deltaX = Math.abs(1 / (rayX || 1e-9))
  const deltaY = Math.abs(1 / (rayY || 1e-9))
  const stepX = rayX < 0 ? -1 : 1
  const stepY = rayY < 0 ? -1 : 1
  let sideX = rayX < 0 ? (G.px - mapX) * deltaX : (mapX + 1 - G.px) * deltaX
  let sideY = rayY < 0 ? (G.py - mapY) * deltaY : (mapY + 1 - G.py) * deltaY
  let side = 0
  let hit = 0
  for (let n = 0; n < 64 && !hit; n++) {
    if (sideX < sideY) {
      sideX += deltaX
      mapX += stepX
      side = 0
    } else {
      sideY += deltaY
      mapY += stepY
      side = 1
    }
    hit = cell(mapX, mapY)
  }
  const dist = Math.max(0.05, side === 0 ? sideX - deltaX : sideY - deltaY)
  zbuf[i] = dist
  // Where along the wall face the ray landed -> which texture column.
  let wallX = side === 0 ? G.py + dist * rayY : G.px + dist * rayX
  wallX -= Math.floor(wallX)
  let texX = Math.floor(wallX * 16)
  if ((side === 0 && rayX > 0) || (side === 1 && rayY < 0)) texX = 15 - texX
  const tex = Math.min(4, Math.max(1, hit)) - 1
  putImage(strip, 'walls ' + (tex * 16 + texX))
  put(strip, 'scaleY', Math.min(SCREEN_H * 6, SCREEN_H / dist))
  put(strip, 'zIndex', -dist)
  // Distance fog + the classic "one axis is darker" shading + muzzle boost.
  const fog = Math.max(0.08, Math.min(1, 1.15 - dist / FOG))
  const s = fog * (side === 1 ? 0.72 : 1) * (1 + G.boost * 0.6)
  putTint(strip, s, s * 0.95, s * 1.05, 1)
}

// =====================================================================
// Waves
// =====================================================================
const alive = G.foes.filter((f) => f.hp > 0)
if (!G.dead && alive.length === 0) {
  G.waveDelay -= dt
  if (G.waveDelay <= 0) {
    G.wave += 1
    const count = Math.min(FOES, 2 + G.wave * 2)
    const kinds = G.wave < 2 ? ['slime'] : G.wave < 4 ? ['slime', 'skull'] : ['slime', 'skull', 'skeleton']
    G.foes = []
    for (let i = 0; i < count; i++) {
      let spawn = SPAWNS[(i + G.wave) % SPAWNS.length]
      // Never spawn right next to the player.
      if (Math.hypot(spawn[0] - G.px, spawn[1] - G.py) < 4) spawn = SPAWNS[(i + G.wave + 3) % SPAWNS.length]
      const kind = kinds[i % kinds.length]
      G.foes.push({ slot: i, kind, x: spawn[0] + (Math.random() - 0.5) * 0.4, y: spawn[1] + (Math.random() - 0.5) * 0.4, hp: KINDS[kind].hp, hurt: 0, cd: 1 + Math.random(), phase: Math.random() * 10 })
    }
    G.waveDelay = 2
    say('WAVE ' + G.wave, 2)
    blip(220, 0.5, 'triangle', 0.06)
  }
}

// =====================================================================
// Enemies: chase, slide along walls, hit you up close
// =====================================================================
for (const f of G.foes) {
  if (f.hp <= 0) continue
  const spec = KINDS[f.kind]
  f.hurt = Math.max(0, f.hurt - dt)
  const toX = G.px - f.x
  const toY = G.py - f.y
  const d = Math.hypot(toX, toY) || 1
  if (!G.dead && d > 0.7 && f.hurt <= 0) {
    const sx = (toX / d) * spec.speed * dt
    const sy = (toY / d) * spec.speed * dt
    if (cell(f.x + sx + Math.sign(sx) * 0.25, f.y) === 0) f.x += sx
    if (cell(f.x, f.y + sy + Math.sign(sy) * 0.25) === 0) f.y += sy
  }
  f.cd -= dt
  if (!G.dead && d <= 0.85 && f.cd <= 0) {
    f.cd = 1.1
    G.hp -= 1
    G.flash = 0.55
    G.shake = 6
    blip(110, 0.25, 'sawtooth', 0.07)
    if (G.hp <= 0) {
      G.dead = true
      say('YOU DIED  -  PRESS R', 999)
      blip(70, 1.2, 'sawtooth', 0.08)
    }
  }
}

// =====================================================================
// Throwing: hitscan down the crosshair, with a flying-shuriken animation
// =====================================================================
const fire = !G.dead && (k.has('space') || k.has('mouse'))
G.cool -= dt
if (fire && !G.prevFire && G.cool <= 0) {
  G.cool = THROW_COOLDOWN
  G.throwT = 0
  G.boost = 1
  blip(900, 0.08, 'square', 0.035)
  // Nearest enemy whose billboard covers the screen center and isn't
  // behind the wall the center ray hit.
  const invDet = 1 / (planeX * dirY - dirX * planeY)
  let best = null
  let bestDepth = zbuf[STRIPS / 2]
  for (const f of G.foes) {
    if (f.hp <= 0) continue
    const rx = f.x - G.px
    const ry = f.y - G.py
    const tx = invDet * (dirY * rx - dirX * ry)
    const depth = invDet * (-planeY * rx + planeX * ry)
    if (depth <= 0.2 || depth >= bestDepth) continue
    const halfWidth = (KINDS[f.kind].size * SCREEN_H) / depth / 2
    if (Math.abs((tx / depth) * HALF_W) < halfWidth * 0.8) {
      best = f
      bestDepth = depth
    }
  }
  if (best) {
    best.hp -= 1
    best.hurt = 0.18
    // Knock it back a little.
    const kx = best.x - G.px
    const ky = best.y - G.py
    const kd = Math.hypot(kx, ky) || 1
    if (cell(best.x + (kx / kd) * 0.3, best.y + (ky / kd) * 0.3) === 0) {
      best.x += (kx / kd) * 0.3
      best.y += (ky / kd) * 0.3
    }
    blip(300, 0.1, 'triangle', 0.05)
    if (best.hp <= 0) {
      G.score += KINDS[best.kind].score
      G.booms.push({ x: best.x, y: best.y, t: 0 })
      if (G.booms.length > BOOMS) G.booms.shift()
      G.shake = Math.max(G.shake, 3)
      blip(80, 0.4, 'sawtooth', 0.06)
    }
  }
}
G.prevFire = fire

// =====================================================================
// Billboards: enemies and explosions, projected + z-sorted with the strips
// =====================================================================
const invDet = 1 / (planeX * dirY - dirX * planeY)
function project(x, y, size) {
  const rx = x - G.px
  const ry = y - G.py
  const tx = invDet * (dirY * rx - dirX * ry)
  const depth = invDet * (-planeY * rx + planeX * ry)
  if (depth <= 0.15) return null
  const h = (size * SCREEN_H) / depth
  const wallH = SCREEN_H / depth
  // Feet on the floor line for this depth.
  return { x: (tx / depth) * HALF_W, y: -wallH / 2 + h / 2, scale: h / 16, depth }
}
function park(ref) {
  if (ref.x < 50000) ref.x = 100000
}

const foeRefs = getCompByClass('foe')
const mm = MINIMAP
for (const ref of foeRefs) {
  const slot = parseInt(ref.id.slice(3), 10)
  const f = G.foes.find((e) => e.slot === slot && e.hp > 0)
  const dot = getCompByID('mmFoe' + slot)
  if (!f) {
    park(ref)
    if (dot) park(dot)
    continue
  }
  const spec = KINDS[f.kind]
  const p = project(f.x, f.y, spec.size)
  if (!p) {
    park(ref)
  } else {
    const hover = spec.floats ? (0.12 + Math.sin(G.clock * 3 + f.phase) * 0.05) * (SCREEN_H / p.depth) : 0
    ref.x = p.x
    ref.y = p.y + hover
    put(ref, 'scaleX', p.scale)
    put(ref, 'scaleY', p.scale)
    put(ref, 'zIndex', -p.depth + 0.01)
    const step = Math.floor(G.clock * spec.fps + f.phase) % 4
    putImage(ref, f.kind + ' ' + step * 4)
    const fog = Math.max(0.15, Math.min(1, 1.2 - p.depth / FOG))
    if (f.hurt > 0) putTint(ref, 1, 0.3, 0.3, 1)
    else putTint(ref, fog, fog, fog, 1)
  }
  if (dot) {
    dot.x = mm.x0 + f.x * mm.cell
    dot.y = mm.y0 - f.y * mm.cell
  }
}

for (let i = 0; i < BOOMS; i++) {
  const ref = getCompByID('boom' + i)
  if (!ref) continue
  const b = G.booms[i]
  if (!b) {
    park(ref)
    continue
  }
  b.t += dt
  const idx = Math.floor(b.t * 18)
  const p = project(b.x, b.y, 0.9)
  if (idx >= 9 || !p) {
    park(ref)
    if (idx >= 9) G.booms[i] = null
    continue
  }
  ref.x = p.x
  ref.y = p.y
  put(ref, 'scaleX', (p.scale * 16) / 40)
  put(ref, 'scaleY', (p.scale * 16) / 40)
  put(ref, 'zIndex', -p.depth + 0.02)
  putImage(ref, 'boom ' + idx)
}
G.booms = G.booms.filter((b) => b)

// =====================================================================
// Weapon, HUD, camera
// =====================================================================
const bobX = Math.cos(G.bob * 0.5) * 3
const bobY = Math.abs(Math.sin(G.bob * 0.5)) * 4
const weapon = getCompByID('weapon')
const thrown = getCompByID('thrown')
if (G.throwT >= 0) {
  G.throwT += dt
  const t = Math.min(1, G.throwT / 0.16)
  if (thrown) {
    // From the hand to the crosshair, shrinking and spinning.
    thrown.x = 96 * (1 - t)
    thrown.y = -76 * (1 - t)
    put(thrown, 'scaleX', 2 - t * 1.6)
    put(thrown, 'scaleY', 2 - t * 1.6)
    putImage(thrown, 'shuriken ' + (Math.floor(G.throwT * 30) % 2))
  }
  if (t >= 1) {
    G.throwT = -1
    if (thrown) park(thrown)
  }
}
if (weapon) {
  // Dips away while the throw is in flight, then rises back into view.
  const reload = Math.max(0, G.cool / THROW_COOLDOWN)
  weapon.x = 96 + bobX
  weapon.y = -76 - bobY - reload * 40
  putImage(weapon, 'shuriken ' + (Math.floor(G.clock * 2) % 2))
}

const view = getCompByID('view')
if (view) view.y = -bobY * 0.5

G.flash = Math.max(0, G.flash - dt * 1.8)
const flash = getCompByID('flash')
if (flash) putTint(flash, 0.85, 0.05, 0.05, Math.min(0.6, G.flash))

const cam = getCompByID('camera')
G.shake = Math.max(0, G.shake - dt * 25)
if (cam) {
  cam.x = Math.round((Math.random() - 0.5) * G.shake)
  cam.y = Math.round((Math.random() - 0.5) * G.shake)
}

for (let i = 0; i < 3; i++) {
  const heart = getCompByID('heart' + i)
  if (heart) putImage(heart, 'heart ' + Math.min(Math.max(G.hp - i * 2, 0), 2) * 2)
}
const player = getCompByID('mmPlayer')
if (player) {
  player.x = mm.x0 + G.px * mm.cell
  player.y = mm.y0 - G.py * mm.cell
  // Map y runs down, screen y runs up: mirror the angle.
  put(player, 'rotation', (-G.angle * 180) / Math.PI)
}
G.msgTime = (G.msgTime || 0) - dt
const score = getCompByID('score')
if (score && score.textContent !== 'SCORE ' + G.score) score.textContent = 'SCORE ' + G.score
const msg = getCompByID('msg')
const msgText = G.msgTime > 0 ? G.msg : ''
if (msg && msg.textContent !== msgText) msg.textContent = msgText
const hint = getCompByID('hint')
const hintText = ctx.pointerLocked ? 'WASD MOVE  CLICK THROW' : 'CLICK TO AIM WITH MOUSE'
if (hint && hint.textContent !== hintText) hint.textContent = hintText
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_one_scene_with_80_strips_and_a_code_program() {
        let mut document = Document::new();
        let demo = build_demo_fps(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let strips = artboard.node_ids().filter(|id| artboard.node(*id).is_some_and(|n| n.classes.iter().any(|c| c == "col"))).count();
        assert_eq!(strips, STRIPS);
        assert!(artboard.active_camera().is_some());
        let (_, machine) = artboard.state_machines().next().unwrap();
        let Some(ProgramDef::Code(source)) = &machine.states[0].program else { panic!("expected Code") };
        assert!(source.contains("const MAP = ["));
        // 4 textures x 16 one-pixel strips.
        assert_eq!(document.images().filter(|(_, i)| i.name.starts_with("walls ")).count(), 64);
    }

    #[test]
    fn map_is_closed_and_rectangular() {
        assert!(MAP.iter().all(|row| row.len() == 16));
        assert!(MAP[0].chars().all(|c| c != '0') && MAP[15].chars().all(|c| c != '0'));
        assert!(MAP.iter().all(|row| !row.starts_with('0') && !row.ends_with('0')));
    }
}
