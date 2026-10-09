//! "Voxel Craft" — a Minetest / Minecraft-style block sandbox.
//!
//! - **The world** is one *streamed* `Primitive3D::Voxels` node, 80 blocks
//!   tall and endless sideways. The script generates 16 × 80 × 16 chunks
//!   from a seed as you walk (nearest first, a few milliseconds a frame) —
//!   oceans and continents, hills, snowy mountains, beaches, caves, ores,
//!   forests — hands them to the engine (`ref.setVoxelChunk`) and drops the
//!   ones far behind you (`ref.unloadVoxelChunk`). Your edits are replayed
//!   on top of regenerated chunks and saved (`ctx.storage`). The engine
//!   meshes 16³ regions nearest-first with face culling and ambient
//!   occlusion (`an1mbyt3_geometry::voxel`) and only draws regions in view.
//! - **Zombies** (a pool of blocky part-built mobs) spawn around you, chase
//!   and hit you; punch them back. Health hearts, fall damage, death and
//!   respawn; P toggles peaceful.
//! - **Play**: first-person walk / jump / swim with block collision, fly
//!   mode (F), left-click breaks, right-click places, 1–9 / the wheel pick
//!   a hotbar slot, E opens the creative inventory, N makes a new world.
//! - **Feel**: a block outline on what you're aiming at, the held block in
//!   your hand (another 1-block Voxels node) that swings, break debris,
//!   distance fog into the sky (`ctx.setEnvironment3`), simple sounds.
//! - **HUD**: HTML overlays — crosshair, hotbar with isometric block icons
//!   the script paints from the block atlas, a debug line, and the
//!   inventory grid.

use an1mbyt3_core::{
    Animation, ArtboardId, Camera3D, CameraNode, Command, Document, Light3D, Light3DKind, LoopMode, Mesh3D, Node, Node3DContent, NodeBehavior, NodeKind, Primitive3D, ProgramDef, StateDef,
    StateMachineDef, Transform3D,
};
use glam::{Vec2, Vec3};

use crate::demo_ninja::{add_node, with_id};

const SCREEN_W: f32 = 960.0;
const SCREEN_H: f32 = 540.0;
/// The block world node's size: streamed worlds only use the height (the
/// chunks are 16 × 16 columns).
const WORLD: [u32; 3] = [16, 80, 16];
/// Pooled zombies.
const MOBS: usize = 10;
/// Pooled break-debris cubes.
const DEBRIS: usize = 12;
/// The held block's centre in camera space (right, down, ahead).
const HELD_AT: Vec3 = Vec3::new(0.52, -0.44, -0.85);
const HELD_SCALE: f32 = 0.3;

pub(crate) struct VoxelDemo {
    pub scene: ArtboardId,
}

fn node3d(name: &str, parent: Option<an1mbyt3_core::NodeId>, transform: Transform3D, content: Node3DContent) -> Node {
    with_id(Node::new(name, parent, NodeKind::Node3D(an1mbyt3_core::Node3D::new(transform, content))), name)
}

fn mesh(primitive: Primitive3D, color: [f32; 4], unlit: bool) -> Node3DContent {
    Node3DContent::Mesh(Mesh3D { primitive, color, shininess: 0.0, unlit, material: None, animation: None, animation_loop: true, animation_overlay: None })
}

fn html_node(name: &str, parent: Option<an1mbyt3_core::NodeId>, html: &str, css: &str, interactive: bool) -> Node {
    let mut node = Node::with_components(
        name,
        parent,
        vec![NodeBehavior::UiHtml(an1mbyt3_core::UiHtml { html: html.into(), css: css.into(), width: 1600.0, height: 900.0, interactive })],
    );
    node.transform.scale = Vec2::splat(SCREEN_W / 1600.0);
    with_id(node, name)
}

/// Standard base64 (the atlas travels to the script as text).
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub(crate) fn build_demo_voxel(document: &mut Document) -> VoxelDemo {
    let mut inverses = Vec::new();
    let inverse = document.apply(Command::AddArtboard { name: "Voxel Craft".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    // The 2D camera carries the HTML HUD; the view itself is the Camera 3D.
    let hud_camera = with_id(
        Node::with_components("HUD Camera", None, vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.5, 0.7, 0.95, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })]),
        "hudCamera",
    );
    let hud_id = add_node(document, &mut inverses, scene, hud_camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(hud_id) }));
    add_node(document, &mut inverses, scene, html_node("hud", Some(hud_id), HUD_HTML, HUD_CSS, false));
    let mut inventory = html_node("inventory", Some(hud_id), INVENTORY_HTML, INVENTORY_CSS, true);
    inventory.visible = false;
    add_node(document, &mut inverses, scene, inventory);

    let sky = [0.55, 0.74, 0.96, 1.0];
    let cam = Camera3D { fov_degrees: 72.0, near: 0.05, far: 260.0, current: true, clear_color: Some(sky) };
    let cam_id = add_node(document, &mut inverses, scene, node3d("cam3d", None, Transform3D::at(Vec3::new(0.0, 50.0, 0.0)), Node3DContent::Camera(cam)));
    add_node(
        document,
        &mut inverses,
        scene,
        node3d("sun", None, Transform3D { position: Vec3::new(0.0, 100.0, 0.0), rotation_degrees: Vec3::new(-52.0, -35.0, 0.0), scale: Vec3::ONE }, Node3DContent::Light(Light3D { kind: Light3DKind::Directional, color: [1.0, 0.95, 0.85], intensity: 0.8, range: 10.0 })),
    );

    // The world (its blocks come from the script).
    add_node(document, &mut inverses, scene, node3d("world", None, Transform3D::default(), mesh(Primitive3D::Voxels { width: WORLD[0], height: WORLD[1], depth: WORLD[2] }, [1.0; 4], false)));

    // The held block: a 1-block world riding on the camera, turned so
    // three faces show, like a hand holding it.
    let held_rot = Transform3D { position: Vec3::ZERO, rotation_degrees: Vec3::new(6.0, -32.0, 0.0), scale: Vec3::splat(HELD_SCALE) };
    let corner = HELD_AT - held_rot.rotation() * Vec3::splat(0.5 * HELD_SCALE);
    add_node(document, &mut inverses, scene, node3d("held", Some(cam_id), Transform3D { position: corner, ..held_rot }, mesh(Primitive3D::Voxels { width: 1, height: 1, depth: 1 }, [1.0; 4], false)));

    // The block outline: 12 thin dark bars around a unit cube.
    let sel = add_node(document, &mut inverses, scene, node3d("sel", None, Transform3D::at(Vec3::new(0.0, -100.0, 0.0)), Node3DContent::Empty));
    let (lo, hi, t) = (-0.004f32, 1.004f32, 0.03f32);
    let mut edge = 0;
    for axis in 0..3 {
        for a in [lo, hi] {
            for b in [lo, hi] {
                let mid = 0.5;
                let (position, scale) = match axis {
                    0 => (Vec3::new(mid, a, b), Vec3::new(1.0 + t, t, t)),
                    1 => (Vec3::new(a, mid, b), Vec3::new(t, 1.0 + t, t)),
                    _ => (Vec3::new(a, b, mid), Vec3::new(t, t, 1.0 + t)),
                };
                add_node(document, &mut inverses, scene, node3d(&format!("selEdge{edge}"), Some(sel), Transform3D { position, rotation_degrees: Vec3::ZERO, scale }, mesh(Primitive3D::cube(), [0.05, 0.05, 0.06, 1.0], true)));
                edge += 1;
            }
        }
    }

    // Break debris (hidden until a block breaks).
    for i in 0..DEBRIS {
        let mut debris = node3d(&format!("debris{i}"), None, Transform3D { position: Vec3::new(0.0, -100.0, 0.0), rotation_degrees: Vec3::ZERO, scale: Vec3::splat(0.16) }, mesh(Primitive3D::Voxels { width: 1, height: 1, depth: 1 }, [1.0; 4], false));
        debris.visible = false;
        add_node(document, &mut inverses, scene, debris);
    }

    // Zombies: a root (feet, facing) with a body, a head with eyes, and arms
    // / legs hung from pivots at the shoulders / hips so the script can
    // swing them. Facing −Z (the way the camera looks at yaw 0).
    for i in 0..MOBS {
        let mut root = node3d(&format!("mob{i}"), None, Transform3D { position: Vec3::new(0.0, -100.0, 0.0), rotation_degrees: Vec3::ZERO, scale: Vec3::splat(0.9) }, Node3DContent::Empty);
        root.visible = false;
        let root = add_node(document, &mut inverses, scene, root);
        let skin = [0.33, 0.58, 0.33, 1.0];
        let shirt = [0.13, 0.52, 0.58, 1.0];
        let pants = [0.24, 0.24, 0.55, 1.0];
        let part = |name: String, parent, at: Vec3, size: Vec3, color: [f32; 4]| node3d(&name, Some(parent), Transform3D { position: at, rotation_degrees: Vec3::ZERO, scale: size }, mesh(Primitive3D::cube(), color, false));
        add_node(document, &mut inverses, scene, part(format!("mob{i}body"), root, Vec3::new(0.0, 1.125, 0.0), Vec3::new(0.5, 0.75, 0.25), shirt));
        let head = add_node(document, &mut inverses, scene, part(format!("mob{i}head"), root, Vec3::new(0.0, 1.75, 0.0), Vec3::splat(0.5), skin));
        for (side, x) in [("L", -0.11f32), ("R", 0.11)] {
            // (Children of the head cube: in its unit space.)
            add_node(document, &mut inverses, scene, part(format!("mob{i}eye{side}"), head, Vec3::new(x / 0.5, 0.02, -0.51), Vec3::new(0.36, 0.24, 0.08), [0.05, 0.05, 0.05, 1.0]));
        }
        for (limb, x, y, color, forward) in [("armL", -0.375f32, 1.5f32, skin, true), ("armR", 0.375, 1.5, skin, true), ("legL", -0.125, 0.75, pants, false), ("legR", 0.125, 0.75, pants, false)] {
            // Zombie arms reach forward (pivot turned 90° about X).
            let pivot = add_node(
                document,
                &mut inverses,
                scene,
                node3d(&format!("mob{i}{limb}"), Some(root), Transform3D { position: Vec3::new(x, y, 0.0), rotation_degrees: Vec3::new(if forward { 90.0 } else { 0.0 }, 0.0, 0.0), scale: Vec3::ONE }, Node3DContent::Empty),
            );
            add_node(document, &mut inverses, scene, part(format!("mob{i}{limb}Mesh"), pivot, Vec3::new(0.0, -0.375, 0.0), Vec3::new(0.25, 0.75, 0.25), color));
        }
    }

    let (aw, ah, atlas) = an1mbyt3_geometry::voxel_atlas();
    let blocks = an1mbyt3_geometry::BLOCKS.iter().map(|b| format!("{{ name: {:?}, top: {}, side: {}, bottom: {} }}", b.name, b.top, b.side, b.bottom)).collect::<Vec<_>>().join(", ");
    let prefix = format!(
        "const W = {}, H = {}, D = {}\nconst DEBRIS = {DEBRIS}, MOBS = {MOBS}\nconst ATLAS_W = {aw}, ATLAS_H = {ah}, TILE = {}, ATLAS_COLS = {}\nconst ATLAS_B64 = \"{}\"\nconst BLOCKS = [{blocks}]",
        WORLD[0],
        WORLD[1],
        WORLD[2],
        an1mbyt3_geometry::voxel::TILE,
        an1mbyt3_geometry::voxel::ATLAS_COLUMNS,
        base64(&atlas),
    );

    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Voxel Craft (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Voxel Craft".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Play".into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(format!("{prefix}\n{SCRIPT}"))),
            }],
            initial_state: "Play".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));
    inverses.reverse();
    document.commit_batch(inverses);
    VoxelDemo { scene }
}

const SCRIPT: &str = include_str!("demo_voxel.js");

/// The in-game HUD (1600×900 CSS px, scaled to the screen): crosshair,
/// hotbar, the picked block's name, a debug line and the controls hint.
const HUD_HTML: &str = r#"<div class="hud">
  <div class="debug" id="debug"></div>
  <div class="cross"></div>
  <div class="hint" id="hint">CLICK TO PLAY</div>
  <div class="toast" id="toast"></div>
  <div class="hearts" id="hearts"></div>
  <div class="bar" id="bar"></div>
  <div class="dead" id="dead"><div class="big">YOU DIED</div><div id="deadSub">Click to respawn</div></div>
</div>"#;

const HUD_CSS: &str = r#"
.hud { position: absolute; inset: 0; font-family: 'Consolas', 'Menlo', monospace; color: #fff; text-shadow: 2px 2px 0 rgb(0 0 0 / 0.55); }
.cross { position: absolute; left: 50%; top: 50%; width: 28px; height: 28px; transform: translate(-50%, -50%); }
.cross::before, .cross::after { content: ''; position: absolute; background: rgb(255 255 255 / 0.9); box-shadow: 0 0 0 1px rgb(0 0 0 / 0.35); }
.cross::before { left: 12px; top: 0; width: 4px; height: 28px; }
.cross::after { left: 0; top: 12px; width: 28px; height: 4px; }
/* (Below the editor's floating toolbar, which overlays the top.) */
.debug { position: absolute; left: 18px; top: 92px; font-size: 20px; line-height: 1.5; white-space: pre; background: rgb(0 0 0 / 0.28); padding: 6px 10px; }
.hint { position: absolute; left: 50%; top: 60%; transform: translateX(-50%); font-size: 26px; letter-spacing: 1px; background: rgb(0 0 0 / 0.45); padding: 10px 22px; white-space: pre; text-align: center; line-height: 1.5; }
.hint:empty { display: none; }
.toast { position: absolute; left: 50%; bottom: 128px; transform: translateX(-50%); font-size: 24px; transition: opacity 0.3s; }
.toast.off { opacity: 0; }
.bar { position: absolute; left: 50%; bottom: 18px; transform: translateX(-50%); display: flex; gap: 0; padding: 6px; background: rgb(20 20 24 / 0.55); border: 3px solid rgb(0 0 0 / 0.6); }
.slot { position: relative; width: 84px; height: 84px; border: 3px solid rgb(160 160 170 / 0.55); box-sizing: border-box; display: grid; place-items: center; }
.slot img { width: 62px; height: 62px; image-rendering: pixelated; }
.slot .n { position: absolute; left: 5px; top: 2px; font-size: 16px; opacity: 0.75; }
.slot.on { border: 5px solid #fff; box-shadow: 0 0 0 3px rgb(0 0 0 / 0.6); z-index: 1; }
/* Health: 10 pixel hearts above the hotbar (2 HP each). */
.hearts { position: absolute; left: 50%; bottom: 120px; transform: translateX(-370px); display: flex; gap: 4px; }
.h { width: 28px; height: 26px; position: relative; background: rgb(0 0 0 / 0.55); clip-path: polygon(0 25%, 15% 0, 40% 0, 50% 15%, 60% 0, 85% 0, 100% 25%, 100% 50%, 50% 100%, 0 50%); }
.h::after { content: ''; position: absolute; inset: 3px; background: #4a1414; clip-path: inherit; }
.h.full::after { background: linear-gradient(#ff5a5a, #c41616); }
.h.half::after { background: linear-gradient(90deg, #e33 0 50%, #4a1414 50%); }
.hearts.blink .h { animation: blink 0.15s 2; }
@keyframes blink { 50% { filter: brightness(2.2); } }
.hurt { position: absolute; inset: 0; background: radial-gradient(transparent 40%, rgb(200 0 0 / 0.55)); opacity: 0; transition: opacity 0.35s; }
.hurt.on { opacity: 1; transition: none; }
.dead { position: absolute; inset: 0; background: rgb(120 0 0 / 0.5); display: none; flex-direction: column; align-items: center; justify-content: center; gap: 24px; font-size: 30px; }
.dead.on { display: flex; }
.dead .big { font-size: 96px; letter-spacing: 6px; }
"#;

/// The creative inventory: every block, click to put it in the selected
/// hotbar slot.
const INVENTORY_HTML: &str = r#"<div class="dim">
  <div class="panel">
    <div class="title">CREATIVE INVENTORY</div>
    <div class="grid" id="grid"></div>
    <div class="foot">Click a block to put it in the selected hotbar slot  ·  1–9 pick the slot  ·  E closes</div>
  </div>
</div>"#;

const INVENTORY_CSS: &str = r#"
.dim { position: absolute; inset: 0; background: rgb(0 0 0 / 0.45); display: grid; place-items: center; font-family: 'Consolas', 'Menlo', monospace; color: #f0f0f0; }
.panel { background: #3a3a40; border: 4px solid #1c1c20; box-shadow: inset 0 0 0 4px #57575f; padding: 22px 26px; min-width: 760px; }
.title { font-size: 26px; margin-bottom: 16px; letter-spacing: 2px; text-shadow: 2px 2px 0 #000; }
.grid { display: grid; grid-template-columns: repeat(8, 84px); gap: 8px; }
.cell { width: 84px; height: 84px; background: #25252a; border: 3px solid #17171a; box-sizing: border-box; display: grid; place-items: center; cursor: pointer; position: relative; }
.cell:hover { background: #4a4a55; border-color: #fff; }
.cell img { width: 60px; height: 60px; image-rendering: pixelated; pointer-events: none; }
.cell .tip { display: none; position: absolute; bottom: -34px; left: 50%; transform: translateX(-50%); background: #111; padding: 3px 8px; font-size: 16px; white-space: nowrap; z-index: 2; pointer-events: none; }
.cell:hover .tip { display: block; }
.foot { margin-top: 18px; font-size: 17px; opacity: 0.75; }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_encoding() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }
}
