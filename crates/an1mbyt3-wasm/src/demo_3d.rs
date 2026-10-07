//! "3D Showroom" — the first 3D demo: every primitive on a lit stage.
//!
//! - A floor, five pedestals with a cube, sphere, torus, cone and cylinder,
//!   a sun and two coloured point lights that circle the stage.
//! - Drag the mouse (or use the arrow keys / A-D) to orbit the camera,
//!   W/S or the wheel-ish +/- keys to zoom; the torus spins, the sphere
//!   bobs, the lights orbit — all from a Code script using the 3D ref API
//!   (`ref.position3`, `ref.rotation3`, `ref.lookAt3`).
//! - The title and hints are ordinary 2D text, drawn over the 3D world.

use an1mbyt3_core::{
    Animation, ArtboardId, Camera3D, CameraNode, Command, Document, FontAsset, Light3D, Light3DKind, LoopMode, Mesh3D, Node, NodeBehavior, NodeKind, Primitive3D,
    ProgramDef, StateDef, StateMachineDef, Transform3D,
};
use glam::{Vec2, Vec3};

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");
const SCREEN_W: f32 = 640.0;
const SCREEN_H: f32 = 360.0;

pub(crate) struct Demo3D {
    pub scene: ArtboardId,
}

fn node3d(id: &str, name: &str, transform: Transform3D, content: an1mbyt3_core::Node3DContent) -> Node {
    with_id(Node::new(name, None, NodeKind::Node3D(an1mbyt3_core::Node3D::new(transform, content))), id)
}

fn mesh(primitive: Primitive3D, color: [f32; 4], shininess: f32) -> an1mbyt3_core::Node3DContent {
    an1mbyt3_core::Node3DContent::Mesh(Mesh3D { primitive, color, shininess, unlit: false, material: None, animation: None, animation_loop: true, animation_overlay: None })
}

pub(crate) fn build_demo_3d(document: &mut Document) -> Demo3D {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: FONT_BYTES.to_vec() });
    let inverse = document.apply(Command::AddArtboard { name: "3D Showroom".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    // A 2D camera frames the HUD (the 3D view comes from the Camera 3D).
    let camera2d = with_id(
        Node::with_components(
            "HUD Camera",
            None,
            vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.1, 0.1, 0.12, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })],
        ),
        "hudCamera",
    );
    let camera2d_id = add_node(document, &mut inverses, scene, camera2d);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera2d_id) }));

    // ---- The 3D world ----
    let cam = Camera3D { fov_degrees: 55.0, near: 0.05, far: 200.0, current: true, clear_color: Some([0.36, 0.5, 0.72, 1.0]) };
    add_node(document, &mut inverses, scene, node3d("cam3d", "Camera 3D", Transform3D { position: Vec3::new(0.0, 3.0, 8.0), rotation_degrees: Vec3::new(-18.0, 0.0, 0.0), scale: Vec3::ONE }, an1mbyt3_core::Node3DContent::Camera(cam)));
    add_node(
        document,
        &mut inverses,
        scene,
        node3d("sun", "Sun", Transform3D { position: Vec3::new(0.0, 6.0, 0.0), rotation_degrees: Vec3::new(-50.0, -30.0, 0.0), scale: Vec3::ONE }, an1mbyt3_core::Node3DContent::Light(Light3D { kind: Light3DKind::Directional, color: [1.0, 0.95, 0.86], intensity: 0.85, range: 10.0 })),
    );
    for (i, color) in [[1.0, 0.35, 0.2], [0.25, 0.55, 1.0]].into_iter().enumerate() {
        add_node(
            document,
            &mut inverses,
            scene,
            node3d(&format!("lamp{i}"), &format!("Point Light {}", i + 1), Transform3D::at(Vec3::new(0.0, 1.5, 0.0)), an1mbyt3_core::Node3DContent::Light(Light3D { kind: Light3DKind::Point, color, intensity: 2.2, range: 6.0 })),
        );
        // A small glowing bulb that rides with each light (moved by the script).
        add_node(document, &mut inverses, scene, node3d(&format!("bulb{i}"), "Bulb", Transform3D { scale: Vec3::splat(0.18), ..Transform3D::at(Vec3::new(0.0, 1.5, 0.0)) }, an1mbyt3_core::Node3DContent::Mesh(Mesh3D { primitive: Primitive3D::sphere(), color: [color[0], color[1], color[2], 1.0], shininess: 0.0, unlit: true, material: None, animation: None, animation_loop: true, animation_overlay: None })));
    }
    add_node(document, &mut inverses, scene, node3d("floor", "Floor", Transform3D::default(), mesh(Primitive3D::Plane { size: 24.0, subdivisions: 1 }, [0.32, 0.34, 0.38, 1.0], 0.1)));

    // Five pedestals in an arc, each with a primitive on top.
    let items: [(&str, Primitive3D, [f32; 4]); 5] = [
        ("cube", Primitive3D::cube(), [0.9, 0.3, 0.3, 1.0]),
        ("sphere", Primitive3D::sphere(), [0.3, 0.6, 1.0, 1.0]),
        ("torus", Primitive3D::torus(), [1.0, 0.8, 0.25, 1.0]),
        ("cone", Primitive3D::cone(), [0.4, 0.9, 0.5, 1.0]),
        ("cylinder", Primitive3D::cylinder(), [0.8, 0.45, 1.0, 1.0]),
    ];
    for (i, (id, primitive, color)) in items.into_iter().enumerate() {
        let angle = (i as f32 - 2.0) * 0.55;
        let at = Vec3::new(angle.sin() * 4.0, 0.0, -angle.cos() * 4.0 + 3.0);
        add_node(document, &mut inverses, scene, node3d(&format!("ped{i}"), "Pedestal", Transform3D { position: at + Vec3::Y * 0.3, scale: Vec3::new(1.2, 0.6, 1.2), ..Default::default() }, mesh(Primitive3D::cylinder(), [0.55, 0.55, 0.6, 1.0], 0.2)));
        add_node(document, &mut inverses, scene, node3d(id, &capitalize(id), Transform3D::at(at + Vec3::Y * 1.15), mesh(primitive, color, 0.6)));
    }

    // ---- HUD (2D, over the 3D view) ----
    add_node(document, &mut inverses, scene, text_node(Some(camera2d_id), "title", "3D SHOWROOM", font, 16.0, [1.0, 1.0, 1.0, 1.0], Vec2::new(0.0, SCREEN_H / 2.0 - 24.0)));
    add_node(document, &mut inverses, scene, text_node(Some(camera2d_id), "hint", "DRAG / ARROWS: ORBIT    W/S: ZOOM    SPACE: PAUSE", font, 8.0, [0.85, 0.9, 1.0, 0.9], Vec2::new(0.0, -SCREEN_H / 2.0 + 16.0)));

    // ---- Script ----
    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Showroom (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "3D Showroom".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Play".into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(SCRIPT.into())),
            }],
            initial_state: "Play".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));

    inverses.reverse();
    document.commit_batch(inverses);
    Demo3D { scene }
}

/// `"cube"` -> `"Cube"`.
fn capitalize(word: &str) -> String {
    word[..1].to_uppercase() + &word[1..]
}

const SCRIPT: &str = r#"// 3D Showroom — orbit camera + animated props, via the 3D ref API.
const g = ctx.globals.showroom || (ctx.globals.showroom = { yaw: 0, pitch: 22, dist: 8.5, t: 0, paused: false })
const keys = ctx.keysDown
const dt = Math.min(ctx.deltaSeconds || 1 / 60, 1 / 20)
const ref = (id) => getCompByID(id)

// ---- orbit input ----
const down = keys.has('mouse')
if (down && g.wasDown) {
  g.yaw -= (ctx.mouseX - g.lastX) * 0.4
  g.pitch = Math.max(5, Math.min(80, g.pitch + (ctx.mouseY - g.lastY) * 0.3))
}
g.wasDown = down
g.lastX = ctx.mouseX; g.lastY = ctx.mouseY
if (keys.has('left') || keys.has('a')) g.yaw += 70 * dt
if (keys.has('right') || keys.has('d')) g.yaw -= 70 * dt
if (keys.has('up')) g.pitch = Math.min(80, g.pitch + 50 * dt)
if (keys.has('down')) g.pitch = Math.max(5, g.pitch - 50 * dt)
if (keys.has('w')) g.dist = Math.max(3, g.dist - 6 * dt)
if (keys.has('s')) g.dist = Math.min(20, g.dist + 6 * dt)
const space = keys.has(' ') || keys.has('space')
if (space && !g.spaceWas) g.paused = !g.paused
g.spaceWas = space
if (!g.paused) g.t += dt
if (!down && !keys.size) g.yaw += 6 * dt   // idle: a slow turntable

// ---- camera: orbit the stage centre, looking at it ----
const target = { x: 0, y: 1, z: 1.5 }
const yr = g.yaw * Math.PI / 180, pr = g.pitch * Math.PI / 180
const cam = ref('cam3d')
cam.position3 = {
  x: target.x + Math.sin(yr) * Math.cos(pr) * g.dist,
  y: target.y + Math.sin(pr) * g.dist,
  z: target.z + Math.cos(yr) * Math.cos(pr) * g.dist,
}
cam.lookAt3(target.x, target.y, target.z)

// ---- props ----
const t = g.t
ref('torus').rotation3 = { x: t * 70, y: t * 40, z: 0 }
ref('cube').rotation3 = { x: 0, y: t * 50, z: 0 }
const s = ref('sphere'), sp = s.position3
s.position3 = { x: sp.x, y: 1.15 + Math.abs(Math.sin(t * 3)) * 0.6, z: sp.z }
ref('cone').rotation3 = { x: 0, y: -t * 60, z: Math.sin(t * 2) * 12 }
const cyl = ref('cylinder')
cyl.scale3 = { x: 1, y: 1 + Math.sin(t * 2.5) * 0.25, z: 1 }

// ---- the two coloured lights circle the stage ----
for (let i = 0; i < 2; i++) {
  const a = t * 0.8 + i * Math.PI
  const p = { x: Math.cos(a) * 3.2, y: 1.6 + Math.sin(t * 1.3 + i) * 0.5, z: 1.5 + Math.sin(a) * 3.2 }
  ref('lamp' + i).position3 = p
  ref('bulb' + i).position3 = p
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_lit_scene_with_a_camera_and_every_primitive() {
        let mut document = Document::new();
        let demo = build_demo_3d(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let contents: Vec<_> = artboard.node_ids().filter_map(|id| match &artboard.node(id)?.kind {
            NodeKind::Node3D(n) => Some(n.content),
            _ => None,
        }).collect();
        assert_eq!(contents.iter().filter(|c| matches!(c, an1mbyt3_core::Node3DContent::Camera(_))).count(), 1);
        assert_eq!(contents.iter().filter(|c| matches!(c, an1mbyt3_core::Node3DContent::Light(_))).count(), 3);
        assert!(contents.iter().filter(|c| matches!(c, an1mbyt3_core::Node3DContent::Mesh(_))).count() >= 11);
    }
}
