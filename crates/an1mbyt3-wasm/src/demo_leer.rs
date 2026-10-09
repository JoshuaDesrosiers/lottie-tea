//! "Reyna's Leer" — a 3D particle + custom material showcase.
//!
//! A floating, see-through violet eye (a sphere with a shader-graph
//! material: its iris is drawn with the **View Fresnel** node, so the
//! pupil always faces the camera — it *stares at you* from any angle)
//! wrapped in four `ParticleSystem3D` layers:
//!
//! - **swirlA / swirlB** — two thick glowing bands orbiting in opposite
//!   directions on tilted orbits (additive, velocity-stretched streaks);
//! - **smoke** — dark violet "ink" drifting round it (alpha-blended, so
//!   it actually darkens what's behind);
//! - **sparks** — embers born on a wide shell that fly freely, then
//!   **converge** into the eye at the end of their life (a Converge
//!   behaviour whose strength curve ramps 0 → 1).
//!
//! Space casts the Leer: it lunges forward, flares and bursts sparks, then
//! drifts back. Drag to orbit, W/S to zoom. Everything except the 2D HUD
//! is a child of the `leer` pivot, so moving the pivot moves the effect.

use an1mbyt3_core::{
    Animation, ArtboardId, Camera3D, CameraNode, Command, Document, Emitter3DShape, FontAsset, LifeCurve, Light3D, Light3DKind, LoopMode, Material, MaterialKind, Mesh3D, Node, Node3DContent,
    NodeBehavior, NodeId, NodeKind, ParticleBehavior3D, ParticleBehaviorKind3D, ParticleBlend3D, ParticleSystem3D, PointSpace, Primitive3D, ProgramDef, ShaderDomain, ShaderEdgeDef, ShaderGraphDef,
    ShaderNodeDef, ShaderNodeKind, StateDef, StateMachineDef, Transform3D,
};
use glam::{Vec2, Vec3};

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");
const SCREEN_W: f32 = 640.0;
const SCREEN_H: f32 = 360.0;

pub(crate) struct DemoLeer {
    pub scene: ArtboardId,
}

fn node3d(parent: Option<NodeId>, id: &str, name: &str, transform: Transform3D, content: Node3DContent) -> Node {
    with_id(Node::new(name, parent, NodeKind::Node3D(an1mbyt3_core::Node3D::new(transform, content))), id)
}

/// An empty 3D node carrying a particle emitter.
fn emitter(parent: NodeId, id: &str, name: &str, at: Vec3, system: ParticleSystem3D) -> Node {
    let mut node = node3d(Some(parent), id, name, Transform3D::at(at), Node3DContent::Empty);
    node.behaviors.push(an1mbyt3_core::NodeBehaviorEntry { behavior: NodeBehavior::ParticleSystem3D(system), enabled: true });
    node
}

/// The eye's material: translucent violet glass, a swirling magenta iris
/// and a dark pupil that always face the viewer, and a hot rim.
///
/// ```text
///  d     = ViewFresnel(0.55)                 0 facing you → 1 at the edge
///  iris  = 1 - smoothstep(0.50, 0.62, d)
///  pupil = 1 - smoothstep(0.15, 0.21, d)
///  swirl = Noise(Twirl(Panner(UV)), 9)      animated streaks
///  color = mix(mix(glass, mix(deep, hot, swirl), iris), pupilColor, pupil)
///        + rimColor * d³                     (all four channels)
/// ```
fn eye_graph() -> ShaderGraphDef {
    use ShaderNodeKind as K;
    let n = |id: u32, kind: K, x: f32, y: f32, value: [f32; 4], defaults: Vec<Option<[f32; 4]>>| ShaderNodeDef { id, kind, position: Vec2::new(x, y), value, defaults };
    let f = |v: f32| Some([v; 4]);
    let e = |from: u32, from_port: u8, to: u32, to_port: u8| ShaderEdgeDef { from, from_port, to, to_port };
    ShaderGraphDef {
        domain: ShaderDomain::Canvas2D,
        nodes: vec![
            n(1, K::ViewFresnel, 0.0, 0.0, [0.0; 4], vec![f(0.55)]),
            // Iris and pupil masks (1 inside).
            n(2, K::Smoothstep, 220.0, -60.0, [0.0; 4], vec![f(0.50), f(0.62), None]),
            n(3, K::OneMinus, 420.0, -60.0, [0.0; 4], vec![]),
            n(4, K::Smoothstep, 220.0, 60.0, [0.0; 4], vec![f(0.15), f(0.21), None]),
            n(5, K::OneMinus, 420.0, 60.0, [0.0; 4], vec![]),
            // Swirling iris streaks.
            n(6, K::Panner, 0.0, 240.0, [0.0; 4], vec![None, Some([0.06, 0.025, 0.0, 0.0])]),
            n(7, K::Twirl, 220.0, 240.0, [0.0; 4], vec![None, f(7.0), None]),
            n(8, K::Noise, 420.0, 240.0, [0.0; 4], vec![None, f(9.0)]),
            n(9, K::ColorConstant, 420.0, 360.0, [0.5, 0.0, 0.85, 1.0], vec![]),
            n(10, K::ColorConstant, 420.0, 460.0, [1.0, 0.4, 0.95, 1.0], vec![]),
            n(11, K::Mix, 640.0, 300.0, [0.0; 4], vec![]),
            // Glass body → iris → pupil.
            n(12, K::ColorConstant, 640.0, 120.0, [0.16, 0.02, 0.3, 0.38], vec![]),
            n(13, K::Mix, 860.0, 120.0, [0.0; 4], vec![]),
            n(14, K::ColorConstant, 860.0, 260.0, [0.03, 0.0, 0.07, 1.0], vec![]),
            n(15, K::Mix, 1080.0, 160.0, [0.0; 4], vec![]),
            // Hot rim: d³ in every channel (alpha too, so the edge is solid).
            n(16, K::Power, 220.0, -200.0, [0.0; 4], vec![None, f(3.0)]),
            n(17, K::Combine, 420.0, -200.0, [0.0; 4], vec![]),
            n(18, K::ColorConstant, 420.0, -320.0, [0.95, 0.45, 1.0, 0.9], vec![]),
            n(19, K::Multiply, 640.0, -260.0, [0.0; 4], vec![]),
            n(20, K::Add, 1280.0, 60.0, [0.0; 4], vec![]),
            n(21, K::Saturate, 1460.0, 60.0, [0.0; 4], vec![]),
            n(22, K::Output, 1640.0, 60.0, [0.0; 4], vec![]),
        ],
        edges: vec![
            e(1, 0, 2, 2),
            e(2, 0, 3, 0),
            e(1, 0, 4, 2),
            e(4, 0, 5, 0),
            e(6, 0, 7, 0),
            e(7, 0, 8, 0),
            e(9, 0, 11, 0),
            e(10, 0, 11, 1),
            e(8, 0, 11, 2),
            e(12, 0, 13, 0),
            e(11, 0, 13, 1),
            e(3, 0, 13, 2),
            e(13, 0, 15, 0),
            e(14, 0, 15, 1),
            e(5, 0, 15, 2),
            e(1, 0, 16, 0),
            e(16, 0, 17, 0),
            e(16, 0, 17, 1),
            e(16, 0, 17, 2),
            e(16, 0, 17, 3),
            e(18, 0, 19, 0),
            e(17, 0, 19, 1),
            e(15, 0, 20, 0),
            e(19, 0, 20, 1),
            e(20, 0, 21, 0),
            e(21, 0, 22, 0),
        ],
    }
}

/// A glowing band orbiting the eye.
fn swirl(radius: f32, swirl: f32, tilt: f32, rate: f32, hot: [f32; 4], mid: [f32; 4], seed: u32) -> ParticleSystem3D {
    ParticleSystem3D {
        rate,
        max_particles: 900,
        lifetime: 1.3,
        lifetime_jitter: 0.35,
        shape: Emitter3DShape::Ring,
        radius,
        thickness: 0.14,
        speed: 0.0,
        rise: 0.0,
        swirl,
        swirl_jitter: 0.25,
        tilt,
        pull: 5.0,
        turbulence: 0.25,
        size_start: 0.24,
        size_end: 0.03,
        color_start: hot,
        color_mid: mid,
        color_end: [0.25, 0.0, 0.5, 0.0],
        // Hundreds overlap in a band: keep each one dim so it glows
        // violet instead of saturating to white.
        intensity: 0.5,
        blend: ParticleBlend3D::Additive,
        stretch: 0.09,
        seed,
        ..Default::default()
    }
}

pub(crate) fn build_demo_leer(document: &mut Document) -> DemoLeer {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: FONT_BYTES.to_vec() });
    let inverse = document.apply(Command::AddArtboard { name: "Reyna's Leer".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    let inverse = document.apply(Command::AddMaterial { material: Material { name: "Leer Eye".into(), kind: MaterialKind::ShaderGraph(eye_graph()), deleted: false } });
    let Command::SetMaterialDeleted { material: eye_material, .. } = inverse else { unreachable!("AddMaterial's inverse is always a SetMaterialDeleted") };
    inverses.push(inverse);

    // A 2D camera frames the HUD (the 3D view comes from the Camera 3D).
    let camera2d = with_id(
        Node::with_components(
            "HUD Camera",
            None,
            vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.02, 0.01, 0.04, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })],
        ),
        "hudCamera",
    );
    let camera2d_id = add_node(document, &mut inverses, scene, camera2d);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera2d_id) }));

    // ---- World ----
    let cam = Camera3D { fov_degrees: 50.0, near: 0.05, far: 200.0, current: true, clear_color: Some([0.03, 0.015, 0.06, 1.0]) };
    add_node(document, &mut inverses, scene, node3d(None, "cam3d", "Camera 3D", Transform3D { position: Vec3::new(0.0, 2.2, 6.5), rotation_degrees: Vec3::new(-6.0, 0.0, 0.0), scale: Vec3::ONE }, Node3DContent::Camera(cam)));
    add_node(
        document,
        &mut inverses,
        scene,
        node3d(None, "moon", "Moonlight", Transform3D { position: Vec3::new(0.0, 6.0, 0.0), rotation_degrees: Vec3::new(-55.0, 30.0, 0.0), scale: Vec3::ONE }, Node3DContent::Light(Light3D { kind: Light3DKind::Directional, color: [0.45, 0.4, 0.75], intensity: 0.35, range: 10.0 })),
    );
    let floor = Mesh3D { shininess: 0.6, ..Mesh3D::new(Primitive3D::Plane { size: 40.0, subdivisions: 1 }, [0.07, 0.06, 0.1, 1.0]) };
    add_node(document, &mut inverses, scene, node3d(None, "floor", "Floor", Transform3D::default(), Node3DContent::Mesh(floor)));
    // Broken stone pillars for the light to fall on.
    // Outside the camera's orbit, so they never block the view.
    for (i, (x, z, h)) in [(-8.5, -6.0, 2.6), (8.0, -7.0, 1.7), (-10.0, 3.5, 1.2), (9.5, 5.0, 3.1), (0.5, -10.5, 2.2)].into_iter().enumerate() {
        let pillar = Mesh3D { shininess: 0.15, ..Mesh3D::new(Primitive3D::cylinder(), [0.2, 0.18, 0.26, 1.0]) };
        add_node(document, &mut inverses, scene, node3d(None, &format!("pillar{i}"), "Pillar", Transform3D { position: Vec3::new(x, h / 2.0, z), scale: Vec3::new(0.7, h, 0.7), ..Default::default() }, Node3DContent::Mesh(pillar)));
    }

    // ---- The Leer (everything below rides on this pivot) ----
    let leer = add_node(document, &mut inverses, scene, node3d(None, "leer", "Leer", Transform3D::at(Vec3::new(0.0, 1.7, 0.0)), Node3DContent::Empty));
    add_node(
        document,
        &mut inverses,
        scene,
        node3d(Some(leer), "glow", "Eye Light", Transform3D::default(), Node3DContent::Light(Light3D { kind: Light3DKind::Point, color: [0.75, 0.25, 1.0], intensity: 2.2, range: 7.0 })),
    );
    // Alpha below 100 % = drawn see-through; the material sets the actual look.
    let eye = Mesh3D { unlit: true, material: Some(eye_material), shininess: 0.0, ..Mesh3D::new(Primitive3D::sphere(), [1.0, 1.0, 1.0, 0.99]) };
    add_node(document, &mut inverses, scene, node3d(Some(leer), "eye", "Eye", Transform3D { scale: Vec3::splat(1.6), ..Default::default() }, Node3DContent::Mesh(eye)));

    add_node(document, &mut inverses, scene, emitter(leer, "swirlA", "Swirl Band A", Vec3::ZERO, swirl(1.25, 230.0, 32.0, 150.0, [0.95, 0.5, 1.0, 1.0], [0.65, 0.12, 1.0, 0.9], 3)));
    add_node(document, &mut inverses, scene, emitter(leer, "swirlB", "Swirl Band B", Vec3::ZERO, swirl(1.5, -170.0, 62.0, 110.0, [0.7, 0.45, 1.0, 1.0], [0.45, 0.1, 0.95, 0.85], 11)));
    add_node(
        document,
        &mut inverses,
        scene,
        emitter(
            leer,
            "smoke",
            "Dark Smoke",
            Vec3::ZERO,
            ParticleSystem3D {
                rate: 30.0,
                max_particles: 160,
                lifetime: 2.6,
                lifetime_jitter: 0.3,
                shape: Emitter3DShape::Shell,
                radius: 1.15,
                thickness: 0.25,
                swirl: 45.0,
                swirl_jitter: 0.4,
                tilt: 90.0,
                pull: 0.5,
                speed: 0.25,
                rise: 0.2,
                turbulence: 0.3,
                size_start: 0.6,
                size_end: 1.4,
                color_start: [0.12, 0.0, 0.2, 0.0],
                color_mid: [0.07, 0.0, 0.14, 0.55],
                color_end: [0.02, 0.0, 0.05, 0.0],
                intensity: 1.0,
                blend: ParticleBlend3D::Alpha,
                seed: 23,
                ..Default::default()
            },
        ),
    );
    add_node(
        document,
        &mut inverses,
        scene,
        emitter(
            leer,
            "sparks",
            "Converging Sparks",
            Vec3::ZERO,
            ParticleSystem3D {
                rate: 45.0,
                max_particles: 700,
                lifetime: 1.8,
                lifetime_jitter: 0.25,
                shape: Emitter3DShape::Shell,
                radius: 2.4,
                thickness: 0.4,
                swirl: 90.0,
                swirl_jitter: 0.5,
                tilt: 90.0,
                pull: 0.0,
                speed: 0.2,
                turbulence: 0.5,
                size_start: 0.05,
                size_end: 0.07,
                color_start: [1.0, 0.85, 1.0, 0.0],
                color_mid: [1.0, 0.6, 1.0, 1.0],
                color_end: [0.9, 0.3, 1.0, 1.0],
                intensity: 1.2,
                blend: ParticleBlend3D::Additive,
                stretch: 0.025,
                seed: 41,
                // Free for the first 35 % of life, then pulled all the way in.
                behaviors: vec![ParticleBehavior3D {
                    kind: ParticleBehaviorKind3D::Converge,
                    enabled: true,
                    strength: 1.0,
                    curve: LifeCurve { points: vec![[0.0, 0.0], [0.35, 0.0], [1.0, 1.0]] },
                    point: [0.0, 0.0, 0.0],
                    space: PointSpace::Local,
                }],
                ..Default::default()
            },
        ),
    );

    // ---- HUD (2D, over the 3D view) ----
    add_node(document, &mut inverses, scene, text_node(Some(camera2d_id), "title", "LEER", font, 18.0, [0.95, 0.7, 1.0, 1.0], Vec2::new(0.0, SCREEN_H / 2.0 - 26.0)));
    add_node(document, &mut inverses, scene, text_node(Some(camera2d_id), "hint", "SPACE: CAST    DRAG: ORBIT    W/S: ZOOM", font, 8.0, [0.8, 0.7, 0.95, 0.85], Vec2::new(0.0, -SCREEN_H / 2.0 + 16.0)));

    // ---- Script ----
    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Leer (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Leer".into(),
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
    DemoLeer { scene }
}

const SCRIPT: &str = r#"// Reyna's Leer — orbit camera, a hovering eye, Space to cast.
const g = ctx.globals.leer || (ctx.globals.leer = { yaw: 0, pitch: 12, dist: 6.5, t: 0, cast: -1 })
const keys = ctx.keysDown
const dt = Math.min(ctx.deltaSeconds || 1 / 60, 1 / 20)
const ref = (id) => getCompByID(id)
g.t += dt

if (!g.envSet) {
  ctx.setEnvironment3({ fog: [0.03, 0.015, 0.06], fogDensity: 0.07, ambient: [0.16, 0.1, 0.26] })
  g.envSet = true
}

// ---- orbit input ----
const down = keys.has('mouse')
if (down && g.wasDown) {
  g.yaw -= (ctx.mouseX - g.lastX) * 0.4
  g.pitch = Math.max(-10, Math.min(70, g.pitch + (ctx.mouseY - g.lastY) * 0.3))
}
g.wasDown = down
g.lastX = ctx.mouseX; g.lastY = ctx.mouseY
if (keys.has('left') || keys.has('a')) g.yaw += 70 * dt
if (keys.has('right') || keys.has('d')) g.yaw -= 70 * dt
if (keys.has('w')) g.dist = Math.max(2.5, g.dist - 5 * dt)
if (keys.has('s')) g.dist = Math.min(16, g.dist + 5 * dt)
if (!down && !keys.size) g.yaw += 8 * dt   // idle: slow turntable

// ---- cast (Space): lunge forward, flare, burst, drift back ----
const space = keys.has(' ') || keys.has('space')
if (space && !g.spaceWas && g.cast < 0) {
  g.cast = 0
  ref('sparks').burst3(220)
  ref('swirlA').burst3(160)
}
g.spaceWas = space
let lunge = 0, flare = 1
if (g.cast >= 0) {
  g.cast += dt
  const k = g.cast / 2.2
  lunge = Math.sin(Math.min(k, 1) * Math.PI)          // out and back
  flare = 1 + Math.max(0, 1 - g.cast * 1.5) * 0.6      // a brief swell
  if (k >= 1) g.cast = -1
}

// ---- the eye hovers, bobs and leans into the cast ----
const yr = g.yaw * Math.PI / 180, pr = g.pitch * Math.PI / 180
const fwd = { x: -Math.sin(yr), z: -Math.cos(yr) }   // towards where the camera looks
const base = { x: 0, y: 1.7 + Math.sin(g.t * 1.6) * 0.14, z: 0 }
const leer = ref('leer')
leer.position3 = { x: base.x + fwd.x * lunge * -2.4, y: base.y + lunge * 0.3, z: base.z + fwd.z * lunge * -2.4 }
leer.rotation3 = { x: Math.sin(g.t * 0.7) * 6, y: g.t * 25, z: Math.sin(g.t * 0.9) * 5 }
leer.scale3 = { x: flare, y: flare, z: flare }

// ---- camera orbits the eye's resting spot ----
const target = { x: 0, y: 1.6, z: 0 }
const cam = ref('cam3d')
cam.position3 = {
  x: target.x + Math.sin(yr) * Math.cos(pr) * g.dist,
  y: target.y + Math.sin(pr) * g.dist,
  z: target.z + Math.cos(yr) * Math.cos(pr) * g.dist,
}
cam.lookAt3(target.x, target.y, target.z)
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eye_material_compiles_and_the_effect_has_four_emitters() {
        let mut document = Document::new();
        let demo = build_demo_leer(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let emitters = artboard.node_ids().filter(|id| artboard.node(*id).unwrap().behaviors.iter().any(|e| matches!(e.behavior, NodeBehavior::ParticleSystem3D(_)))).count();
        assert_eq!(emitters, 4);
        an1mbyt3_core::shader_graph::compile_wgsl(&eye_graph()).expect("eye graph compiles");
    }
}
