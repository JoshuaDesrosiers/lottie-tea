//! "Physics 3D Lab" — Rigidbody 3D + Collider 3D (Rapier 3D) in action.
//!
//! - A pyramid of crates, a ramp with balls rolling down it, a bouncy ball,
//!   all **Dynamic** Rigidbody 3Ds with **Auto** colliders (fitted to their
//!   meshes, scale included).
//! - The floor and ramp are colliders without a body: static scenery.
//! - A green goal zone is a **trigger**: crates landing in it score (read
//!   from `ctx.collisions`).
//! - **Click** fires a ball from the camera through the mouse
//!   (`ref.velocity3`); a marker shows where `ctx.raycast3` hits under the
//!   cursor. **Drag** orbits, **R** resets.

use an1mbyt3_core::{
    Animation, ArtboardId, Camera3D, CameraNode, Collider3D, Command, Document, FontAsset, Light3D, Light3DKind, LoopMode, Mesh3D, Node, Node3DContent, NodeBehavior,
    NodeBehaviorEntry, NodeKind, Primitive3D, ProgramDef, Rigidbody3D, StateDef, StateMachineDef, Transform3D,
};
use glam::{Vec2, Vec3};

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");
const SCREEN_W: f32 = 960.0;
const SCREEN_H: f32 = 540.0;
/// Pre-made balls the click cannon recycles (hidden until fired).
const BALLS: usize = 16;

pub(crate) struct Physics3DDemo {
    pub scene: ArtboardId,
}

fn node3d(id: &str, transform: Transform3D, content: Node3DContent, behaviors: Vec<NodeBehavior>) -> Node {
    let mut node = with_id(Node::new(id, None, NodeKind::Node3D(an1mbyt3_core::Node3D::new(transform, content))), id);
    for behavior in behaviors {
        node.behaviors.push(NodeBehaviorEntry { behavior, enabled: true });
    }
    node
}

fn mesh(primitive: Primitive3D, color: [f32; 4], shininess: f32) -> Node3DContent {
    Node3DContent::Mesh(Mesh3D { primitive, color, shininess, unlit: false, material: None, animation: None, animation_loop: true, animation_overlay: None })
}

fn body(mass: f32) -> NodeBehavior {
    NodeBehavior::Rigidbody3D(Rigidbody3D { mass, ..Default::default() })
}

fn collider(friction: f32, restitution: f32) -> NodeBehavior {
    NodeBehavior::Collider3D(Collider3D { friction, restitution, ..Default::default() })
}

pub(crate) fn build_demo_physics3d(document: &mut Document) -> Physics3DDemo {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: FONT_BYTES.to_vec() });
    let inverse = document.apply(Command::AddArtboard { name: "Physics 3D Lab".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    // The HUD camera (2D text) — the 3D view comes from the Camera 3D.
    let hud_camera = with_id(
        Node::with_components("HUD Camera", None, vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.1, 0.1, 0.12, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })]),
        "hudCamera",
    );
    let hud_id = add_node(document, &mut inverses, scene, hud_camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(hud_id) }));
    add_node(document, &mut inverses, scene, text_node(Some(hud_id), "title", "PHYSICS 3D LAB", font, 16.0, [1.0, 1.0, 1.0, 1.0], Vec2::new(0.0, 238.0)));
    add_node(document, &mut inverses, scene, text_node(Some(hud_id), "hint", "CLICK FIRE  -  DRAG ORBIT  -  R RESET", font, 8.0, [0.85, 0.9, 1.0, 1.0], Vec2::new(0.0, -246.0)));
    add_node(document, &mut inverses, scene, text_node(Some(hud_id), "score", "IN THE ZONE: 0", font, 10.0, [0.55, 1.0, 0.6, 1.0], Vec2::new(-330.0, 200.0)));
    add_node(document, &mut inverses, scene, text_node(Some(hud_id), "aim", "AIM: -", font, 8.0, [1.0, 0.85, 0.5, 1.0], Vec2::new(330.0, 200.0)));

    let cam = Camera3D { fov_degrees: 55.0, near: 0.05, far: 200.0, current: true, clear_color: Some([0.42, 0.56, 0.78, 1.0]) };
    add_node(document, &mut inverses, scene, node3d("cam3d", Transform3D::at(Vec3::new(0.0, 6.0, 14.0)), Node3DContent::Camera(cam), Vec::new()));
    add_node(
        document,
        &mut inverses,
        scene,
        node3d("sun", Transform3D { position: Vec3::new(0.0, 10.0, 0.0), rotation_degrees: Vec3::new(-55.0, -35.0, 0.0), scale: Vec3::ONE }, Node3DContent::Light(Light3D { kind: Light3DKind::Directional, color: [1.0, 0.96, 0.88], intensity: 0.95, range: 10.0 }), Vec::new()),
    );

    // Static scenery: floor and a ramp (colliders, no body).
    add_node(document, &mut inverses, scene, node3d("floor", Transform3D::default(), mesh(Primitive3D::Plane { size: 40.0, subdivisions: 1 }, [0.36, 0.38, 0.42, 1.0], 0.1), vec![collider(0.7, 0.0)]));
    add_node(
        document,
        &mut inverses,
        scene,
        node3d(
            "ramp",
            Transform3D { position: Vec3::new(-6.5, 1.3, -1.0), rotation_degrees: Vec3::new(0.0, 0.0, -18.0), scale: Vec3::new(6.0, 0.3, 3.0) },
            mesh(Primitive3D::cube(), [0.55, 0.42, 0.3, 1.0], 0.2),
            vec![collider(0.4, 0.0)],
        ),
    );

    // The goal zone: a trigger (no body, sensor collider), drawn as a flat disc.
    add_node(
        document,
        &mut inverses,
        scene,
        node3d(
            "goal",
            Transform3D { position: Vec3::new(5.5, 0.05, 2.5), scale: Vec3::new(1.0, 1.0, 1.0), ..Default::default() },
            Node3DContent::Mesh(Mesh3D { primitive: Primitive3D::Cylinder { radius: 1.8, height: 0.1, segments: 40 }, color: [0.3, 0.95, 0.45, 1.0], shininess: 0.0, unlit: true, material: None, animation: None, animation_loop: true, animation_overlay: None }),
            vec![NodeBehavior::Collider3D(Collider3D { shape: an1mbyt3_core::Collider3DShape::Box { size: Vec3::new(3.6, 1.2, 3.6) }, offset: Vec3::new(0.0, 0.6, 0.0), is_trigger: true, ..Default::default() })],
        ),
    );

    // A pyramid of crates (5-4-3-2-1).
    let crate_colors = [[0.85, 0.55, 0.25, 1.0], [0.75, 0.45, 0.2, 1.0]];
    let mut n = 0;
    for layer in 0..5 {
        let count = 5 - layer;
        for i in 0..count {
            let x = (i as f32 - (count as f32 - 1.0) / 2.0) * 0.82 + 1.5;
            let y = 0.4 + layer as f32 * 0.8;
            let color = crate_colors[(i + layer) % 2];
            add_node(
                document,
                &mut inverses,
                scene,
                node3d(&format!("crate{n}"), Transform3D { position: Vec3::new(x, y, -1.5), scale: Vec3::splat(0.8), ..Default::default() }, mesh(Primitive3D::cube(), color, 0.25), vec![body(1.0), collider(0.6, 0.05)]),
            );
            n += 1;
        }
    }

    // Balls at the top of the ramp, and one bouncy ball.
    for (i, color) in [[0.95, 0.3, 0.3, 1.0], [0.3, 0.6, 1.0, 1.0], [0.95, 0.85, 0.3, 1.0]].into_iter().enumerate() {
        add_node(
            document,
            &mut inverses,
            scene,
            node3d(&format!("roller{i}"), Transform3D { position: Vec3::new(-8.8, 3.0 + i as f32 * 0.9, -1.8 + i as f32 * 0.8), scale: Vec3::splat(0.7), ..Default::default() }, mesh(Primitive3D::sphere(), color, 0.6), vec![body(0.8), collider(0.3, 0.2)]),
        );
    }
    add_node(
        document,
        &mut inverses,
        scene,
        node3d("bouncy", Transform3D { position: Vec3::new(-2.0, 5.0, 2.5), scale: Vec3::splat(0.6), ..Default::default() }, mesh(Primitive3D::sphere(), [0.9, 0.35, 0.95, 1.0], 0.8), vec![body(0.5), collider(0.2, 0.92)]),
    );

    // The cannon's balls (hidden = out of the simulation until fired).
    for i in 0..BALLS {
        let mut ball = node3d(&format!("shot{i}"), Transform3D { position: Vec3::new(0.0, -50.0, 0.0), scale: Vec3::splat(0.45), ..Default::default() }, mesh(Primitive3D::sphere(), [0.95, 0.95, 1.0, 1.0], 0.9), vec![NodeBehavior::Rigidbody3D(Rigidbody3D { mass: 3.0, continuous: true, ..Default::default() }), collider(0.4, 0.3)]);
        ball.visible = false;
        add_node(document, &mut inverses, scene, ball);
    }
    // The aim marker (unlit, no collider).
    add_node(
        document,
        &mut inverses,
        scene,
        node3d("marker", Transform3D { scale: Vec3::splat(0.18), ..Transform3D::at(Vec3::new(0.0, -50.0, 0.0)) }, Node3DContent::Mesh(Mesh3D { primitive: Primitive3D::sphere(), color: [1.0, 0.85, 0.3, 1.0], shininess: 0.0, unlit: true, material: None, animation: None, animation_loop: true, animation_overlay: None }), Vec::new()),
    );

    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Lab (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Physics 3D Lab".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Play".into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(SCRIPT.replace("__BALLS__", &BALLS.to_string()).replace("__W__", &SCREEN_W.to_string()).replace("__H__", &SCREEN_H.to_string()))),
            }],
            initial_state: "Play".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));
    inverses.reverse();
    document.commit_batch(inverses);
    Physics3DDemo { scene }
}

const SCRIPT: &str = r#"// Physics 3D Lab — Rigidbody 3D / Collider 3D via the 3D physics API.
const g = ctx.globals.lab3 || (ctx.globals.lab3 = { yaw: 18, pitch: 24, dist: 15, next: 0, launch: [], inZone: new Set() })
const keys = ctx.keysDown
const dt = Math.min(ctx.deltaSeconds || 1 / 60, 1 / 20)
const ref = (id) => getCompByID(id)

if (keys.has('r') && !g.rWas) { ctx.globals.lab3 = null; ctx.loadScene('Physics 3D Lab') }
g.rWas = keys.has('r')

// ---- orbit camera (drag / arrows) ----
const down = keys.has('mouse')
if (down && g.wasDown) {
  g.yaw -= (ctx.mouseX - g.lastX) * 0.3
  g.pitch = Math.max(6, Math.min(70, g.pitch + (ctx.mouseY - g.lastY) * 0.25))
}
g.wasDown = down
g.lastX = ctx.mouseX; g.lastY = ctx.mouseY
if (keys.has('left') || keys.has('a')) g.yaw += 60 * dt
if (keys.has('right') || keys.has('d')) g.yaw -= 60 * dt
if (keys.has('up') || keys.has('w')) g.dist = Math.max(6, g.dist - 8 * dt)
if (keys.has('down') || keys.has('s')) g.dist = Math.min(30, g.dist + 8 * dt)

const target = { x: 0, y: 1.5, z: 0 }
const yr = g.yaw * Math.PI / 180, pr = g.pitch * Math.PI / 180
const camPos = {
  x: target.x + Math.sin(yr) * Math.cos(pr) * g.dist,
  y: target.y + Math.sin(pr) * g.dist,
  z: target.z + Math.cos(yr) * Math.cos(pr) * g.dist,
}
const cam = ref('cam3d')
cam.position3 = camPos
cam.lookAt3(target.x, target.y, target.z)

// ---- a ray from the camera through the mouse ----
const norm = (v) => { const l = Math.hypot(v.x, v.y, v.z) || 1; return { x: v.x / l, y: v.y / l, z: v.z / l } }
const fwd = norm({ x: target.x - camPos.x, y: target.y - camPos.y, z: target.z - camPos.z })
const right = norm({ x: -fwd.z, y: 0, z: fwd.x })
const up = { x: right.y * fwd.z - right.z * fwd.y, y: right.z * fwd.x - right.x * fwd.z, z: right.x * fwd.y - right.y * fwd.x }
const tanHalf = Math.tan(55 * Math.PI / 360)
const nx = ctx.mouseX / (__W__ / 2), ny = ctx.mouseY / (__H__ / 2)
const ax = nx * tanHalf * (__W__ / __H__), ay = ny * tanHalf
const dir = norm({ x: fwd.x + right.x * ax + up.x * ay, y: fwd.y + right.y * ax + up.y * ay, z: fwd.z + right.z * ax + up.z * ay })

// Aim marker: where the ray hits (ctx.raycast3).
const hit = ctx.raycast3(camPos, dir, 100)
const marker = ref('marker')
if (hit) {
  marker.position3 = { x: hit.x + hit.normalX * 0.05, y: hit.y + hit.normalY * 0.05, z: hit.z + hit.normalZ * 0.05 }
  ref('aim').textContent = 'AIM: ' + (hit.ref ? hit.ref.name.toUpperCase().replace(/[0-9]+$/, '') : '-') + ' ' + hit.distance.toFixed(1) + 'M'
} else {
  marker.position3 = { x: 0, y: -50, z: 0 }
  ref('aim').textContent = 'AIM: SKY'
}

// ---- fire: a hidden ball appears at the camera; next frame (once its
// body exists) it gets its velocity ----
for (const shot of g.launch.splice(0)) {
  const ball = ref(shot.id)
  if (ball.velocity3) ball.velocity3 = shot.v
  else g.launch.push(shot)
}
if (ctx.tap) {
  const id = 'shot' + g.next
  g.next = (g.next + 1) % __BALLS__
  const ball = ref(id)
  ball.position3 = { x: camPos.x + dir.x * 1.2, y: camPos.y + dir.y * 1.2, z: camPos.z + dir.z * 1.2 }
  ball.visible = true
  const speed = 28
  g.launch.push({ id, v: { x: dir.x * speed, y: dir.y * speed, z: dir.z * speed } })
  ctx.playTone({ freq: 180, to: 90, duration: 0.12, type: 'square', volume: 0.15 })
}

// ---- the goal zone (a trigger): count crates inside ----
for (const c of ctx.collisions) {
  if (!c.trigger || !c.a || !c.b) continue
  const other = c.a.name === 'goal' ? c.b : c.b.name === 'goal' ? c.a : null
  if (!other || !other.name.startsWith('crate')) continue
  if (c.started) g.inZone.add(other.name)
  else g.inZone.delete(other.name)
}
ref('score').textContent = 'IN THE ZONE: ' + g.inZone.size
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_bodies_scenery_and_a_trigger() {
        let mut document = Document::new();
        let demo = build_demo_physics3d(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let count = |pred: &dyn Fn(&NodeBehavior) -> bool| artboard.node_ids().filter_map(|id| artboard.node(id)).filter(|n| n.behaviors.iter().any(|e| pred(&e.behavior))).count();
        assert_eq!(count(&|b| matches!(b, NodeBehavior::Rigidbody3D(_))), 15 + 3 + 1 + BALLS);
        assert_eq!(count(&|b| matches!(b, NodeBehavior::Collider3D(c) if c.is_trigger)), 1);
    }
}
