//! "Reyna's Leer" — a 3D particle + custom material showcase, modelled on
//! Valorant's Leer: a dark ink orb with a burning magenta slit-pupil eye
//! that stares at you, wrapped in black tendrils.
//!
//! - **The eye** is a sphere with a shader-graph material built on the
//!   **Facing UV** node (a disc that always faces the camera): a vertical
//!   slit pupil, a hot-magenta iris that darkens towards its rim with
//!   dark veins drifting across it, a white specular glint, and a
//!   near-black body with a violet rim (**View Fresnel**).
//! - **Ink tendrils** — `ParticleSystem3D` with *ribbon trails* in **fluid**
//!   mode: tapering bands orbiting tight round the orb on tilted paths,
//!   drawn as one glossy liquid surface (screen-space metaballs) so they
//!   merge into thick ink where they cross, with magenta rims.
//! - **Magenta glints** ride the same orbits (thin additive ribbons) —
//!   the hot edges on the ink.
//! - **Flicks** peel off the surface and lash outwards; **droplets** of ink
//!   hang in the air round it, and a second set drifts in from further
//!   out and **converges** into the eye (a Converge behaviour whose curve
//!   holds at 0, then ramps to 1 — free, then gathered).
//! - A sunny orange courtyard for contrast.
//!
//! Space casts it (lunge, flare, a burst of flicks), drag orbits, W/S zoom.
//! Everything except the HUD rides on the `leer` pivot.

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

/// The eye's radius (a 0.5-radius sphere scaled by 1.6).
const EYE_RADIUS: f32 = 0.8;
/// Ink: near-black violet.
const INK: [f32; 3] = [0.035, 0.0, 0.06];

pub(crate) struct DemoLeer {
    pub scene: ArtboardId,
}

fn node3d(parent: Option<NodeId>, id: &str, name: &str, transform: Transform3D, content: Node3DContent) -> Node {
    with_id(Node::new(name, parent, NodeKind::Node3D(an1mbyt3_core::Node3D::new(transform, content))), id)
}

/// An emitter whose local Y points down its parent's −Z: under the `halo`
/// (which the script turns to face the camera) its orbits circle the
/// eye's silhouette instead of crossing in front of the iris.
fn facing(mut node: Node) -> Node {
    if let NodeKind::Node3D(n) = &mut node.kind {
        n.transform.rotation_degrees = Vec3::new(-90.0, 0.0, 0.0);
    }
    node
}

/// An empty 3D node carrying a particle emitter.
fn emitter(parent: NodeId, id: &str, name: &str, system: ParticleSystem3D) -> Node {
    let mut node = node3d(Some(parent), id, name, Transform3D::default(), Node3DContent::Empty);
    node.behaviors.push(an1mbyt3_core::NodeBehaviorEntry { behavior: NodeBehavior::ParticleSystem3D(system), enabled: true });
    node
}

fn solid(primitive: Primitive3D, color: [f32; 3], shininess: f32) -> Node3DContent {
    Node3DContent::Mesh(Mesh3D { shininess, ..Mesh3D::new(primitive, [color[0], color[1], color[2], 1.0]) })
}

/// The eye's material (all coordinates from Facing UV, so it stares back):
///
/// ```text
///  uv     = FacingUv                    (0.5, 0.5) faces the camera
///  dist   = distance(uv, 0.5)           0 centre → 0.5 silhouette
///  iris   = 1 - smoothstep(.295, .315, dist)
///  pupil  = Circle(uv stretched ×3.4 across)    a tall slit
///  veins  = thin bands where twirled noise ≈ 0.5, drifting
///  iris colour: hot pink → deep magenta towards the rim, veins darker,
///               a pale glow round the pupil
///  body   = mix(ink, violet rim, ViewFresnel²·²)
///  + a soft white glint up and left of the pupil
/// ```
fn eye_graph() -> ShaderGraphDef {
    use ShaderNodeKind as K;
    let n = |id: u32, kind: K, x: f32, y: f32, value: [f32; 4], defaults: Vec<Option<[f32; 4]>>| ShaderNodeDef { id, kind, position: Vec2::new(x, y), value, defaults };
    let f = |v: f32| Some([v; 4]);
    let v2 = |x: f32, y: f32| Some([x, y, 0.0, 0.0]);
    let c = |r: f32, g: f32, b: f32, a: f32| [r, g, b, a];
    let e = |from: u32, from_port: u8, to: u32, to_port: u8| ShaderEdgeDef { from, from_port, to, to_port };
    let z = [0.0; 4];
    ShaderGraphDef {
        domain: ShaderDomain::Canvas2D,
        nodes: vec![
            n(1, K::FacingUv, 0.0, 300.0, z, vec![]),
            n(2, K::Distance, 200.0, 0.0, z, vec![None, v2(0.5, 0.5)]),
            // Iris mask, and how far out towards its rim (for darkening).
            n(3, K::Smoothstep, 400.0, -120.0, z, vec![f(0.262), f(0.28), None]),
            n(4, K::OneMinus, 600.0, -120.0, z, vec![]),
            n(5, K::Smoothstep, 400.0, 0.0, z, vec![f(0.1), f(0.28), None]),
            // The slit pupil: the facing disc squeezed 3.4× across.
            n(6, K::TilingOffset, 200.0, 160.0, z, vec![None, v2(3.4, 1.0), v2(-1.2, 0.0)]),
            n(7, K::Circle, 400.0, 160.0, z, vec![None, f(0.19), f(0.01)]),
            // Veins: lines where drifting, twirled noise crosses 0.5.
            n(8, K::Panner, 200.0, 320.0, z, vec![None, v2(0.0, 0.015)]),
            n(9, K::Twirl, 400.0, 320.0, z, vec![None, f(2.5), v2(0.5, 0.5)]),
            n(10, K::Noise, 600.0, 320.0, z, vec![None, f(9.0)]),
            n(11, K::Subtract, 800.0, 320.0, z, vec![None, f(0.5)]),
            n(12, K::Abs, 1000.0, 320.0, z, vec![]),
            n(13, K::Smoothstep, 1200.0, 320.0, z, vec![f(0.0), f(0.07), None]),
            n(14, K::OneMinus, 1400.0, 320.0, z, vec![]),
            // Iris colour.
            n(15, K::ColorConstant, 600.0, 0.0, c(1.0, 0.24, 0.86, 1.0), vec![]),
            n(16, K::ColorConstant, 600.0, 80.0, c(0.48, 0.0, 0.55, 1.0), vec![]),
            n(17, K::Mix, 800.0, 40.0, z, vec![]),
            n(18, K::ColorConstant, 1400.0, 440.0, c(0.28, 0.0, 0.32, 1.0), vec![]),
            n(19, K::Multiply, 1600.0, 320.0, z, vec![None, f(0.65)]),
            n(20, K::Mix, 1800.0, 200.0, z, vec![]),
            // A pale glow round the pupil.
            n(21, K::Smoothstep, 400.0, 480.0, z, vec![f(0.05), f(0.16), None]),
            n(22, K::OneMinus, 600.0, 480.0, z, vec![]),
            n(23, K::ColorConstant, 1800.0, 360.0, c(1.0, 0.62, 1.0, 1.0), vec![]),
            n(24, K::Multiply, 800.0, 480.0, z, vec![None, f(0.32)]),
            n(25, K::Mix, 2000.0, 260.0, z, vec![]),
            // The ink body with a violet rim.
            n(26, K::ViewFresnel, 1600.0, -300.0, z, vec![f(3.5)]),
            n(27, K::ColorConstant, 1600.0, -420.0, c(INK[0], INK[1], INK[2], 1.0), vec![]),
            n(28, K::ColorConstant, 1600.0, -520.0, c(0.2, 0.02, 0.28, 1.0), vec![]),
            n(29, K::Mix, 1800.0, -400.0, z, vec![]),
            n(30, K::Mix, 2200.0, -100.0, z, vec![]),
            n(31, K::ColorConstant, 2200.0, 60.0, c(0.0, 0.0, 0.01, 1.0), vec![]),
            n(32, K::Mix, 2400.0, -40.0, z, vec![]),
            // The glint (alpha 0, so adding it keeps the eye opaque).
            n(33, K::TilingOffset, 200.0, 620.0, z, vec![None, v2(1.0, 1.0), v2(0.1, 0.12)]),
            n(34, K::Circle, 400.0, 620.0, z, vec![None, f(0.04), f(0.02)]),
            n(35, K::ColorConstant, 2400.0, 160.0, c(1.0, 0.95, 1.0, 0.0), vec![]),
            n(36, K::Multiply, 2600.0, 120.0, z, vec![]),
            n(37, K::Add, 2800.0, 0.0, z, vec![]),
            n(38, K::Saturate, 3000.0, 0.0, z, vec![]),
            n(39, K::Output, 3200.0, 0.0, z, vec![]),
        ],
        edges: vec![
            e(1, 0, 2, 0),
            e(2, 0, 3, 2),
            e(3, 0, 4, 0),
            e(2, 0, 5, 2),
            e(1, 0, 6, 0),
            e(6, 0, 7, 0),
            e(1, 0, 8, 0),
            e(8, 0, 9, 0),
            e(9, 0, 10, 0),
            e(10, 0, 11, 0),
            e(11, 0, 12, 0),
            e(12, 0, 13, 2),
            e(13, 0, 14, 0),
            e(15, 0, 17, 0),
            e(16, 0, 17, 1),
            e(5, 0, 17, 2),
            e(14, 0, 19, 0),
            e(17, 0, 20, 0),
            e(18, 0, 20, 1),
            e(19, 0, 20, 2),
            e(2, 0, 21, 2),
            e(21, 0, 22, 0),
            e(22, 0, 24, 0),
            e(20, 0, 25, 0),
            e(23, 0, 25, 1),
            e(24, 0, 25, 2),
            e(27, 0, 29, 0),
            e(28, 0, 29, 1),
            e(26, 0, 29, 2),
            e(29, 0, 30, 0),
            e(25, 0, 30, 1),
            e(4, 0, 30, 2),
            e(30, 0, 32, 0),
            e(31, 0, 32, 1),
            e(7, 0, 32, 2),
            e(1, 0, 33, 0),
            e(33, 0, 34, 0),
            e(35, 0, 36, 0),
            e(34, 0, 36, 1),
            e(32, 0, 37, 0),
            e(36, 0, 37, 1),
            e(37, 0, 38, 0),
            e(38, 0, 39, 0),
        ],
    }
}

/// Ink colour with alpha `a`.
fn ink(a: f32) -> [f32; 4] {
    [INK[0], INK[1], INK[2], a]
}

/// Black ribbons orbiting tight round the orb.
fn tendrils() -> ParticleSystem3D {
    ParticleSystem3D {
        rate: 9.0,
        max_particles: 120,
        lifetime: 1.8,
        lifetime_jitter: 0.3,
        shape: Emitter3DShape::Ring,
        radius: EYE_RADIUS + 0.1,
        thickness: 0.05,
        swirl: 150.0,
        swirl_jitter: 0.35,
        tilt: 28.0,
        pull: 8.0,
        turbulence: 0.05,
        size_start: 0.42,
        size_end: 0.06,
        color_start: ink(1.0),
        color_mid: ink(1.0),
        color_end: ink(1.0),
        intensity: 1.0,
        blend: ParticleBlend3D::Alpha,
        hardness: 0.92,
        trail_segments: 20,
        trail_time: 0.6,
        // One liquid surface: ribbons that touch merge into thick ink.
        fluid: true,
        fluid_edge: [0.95, 0.12, 0.78, 0.7],
        fluid_threshold: 0.3,
        fluid_gloss: 0.9,
        seed: 3,
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
            vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.62, 0.78, 0.95, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })],
        ),
        "hudCamera",
    );
    let camera2d_id = add_node(document, &mut inverses, scene, camera2d);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera2d_id) }));

    // ---- A sunny courtyard (orange walls, stone tiles, open sky) ----
    let cam = Camera3D { fov_degrees: 60.0, near: 0.05, far: 200.0, current: true, clear_color: Some([0.62, 0.78, 0.95, 1.0]) };
    add_node(document, &mut inverses, scene, node3d(None, "cam3d", "Camera 3D", Transform3D { position: Vec3::new(0.0, 1.8, 4.2), rotation_degrees: Vec3::ZERO, scale: Vec3::ONE }, Node3DContent::Camera(cam)));
    add_node(
        document,
        &mut inverses,
        scene,
        node3d(None, "sun", "Sun", Transform3D { position: Vec3::new(0.0, 8.0, 0.0), rotation_degrees: Vec3::new(-50.0, 35.0, 0.0), scale: Vec3::ONE }, Node3DContent::Light(Light3D { kind: Light3DKind::Directional, color: [1.0, 0.9, 0.75], intensity: 1.05, range: 10.0 })),
    );
    add_node(document, &mut inverses, scene, node3d(None, "floor", "Tiles", Transform3D::default(), solid(Primitive3D::Plane { size: 40.0, subdivisions: 1 }, [0.8, 0.6, 0.45], 0.15)));
    add_node(document, &mut inverses, scene, node3d(None, "path", "Path Stones", Transform3D::at(Vec3::new(0.0, 0.005, -1.0)), solid(Primitive3D::Plane { size: 3.2, subdivisions: 1 }, [0.72, 0.56, 0.44], 0.1)));
    let walls: [(&str, Vec3, Vec3, [f32; 3]); 6] = [
        ("wallL", Vec3::new(-5.0, 3.0, -2.0), Vec3::new(0.6, 6.0, 16.0), [0.92, 0.5, 0.22]),
        ("wallR", Vec3::new(5.0, 3.0, -2.0), Vec3::new(0.6, 6.0, 16.0), [0.9, 0.48, 0.2]),
        ("wallBack", Vec3::new(0.0, 2.2, -8.0), Vec3::new(10.0, 4.4, 0.6), [0.93, 0.53, 0.25]),
        ("trimL", Vec3::new(-4.6, 6.1, -2.0), Vec3::new(0.5, 0.25, 16.0), [0.96, 0.86, 0.7]),
        ("trimR", Vec3::new(4.6, 6.1, -2.0), Vec3::new(0.5, 0.25, 16.0), [0.96, 0.86, 0.7]),
        ("crate", Vec3::new(-3.6, 0.35, -2.5), Vec3::new(1.4, 0.7, 0.9), [0.72, 0.6, 0.44]),
    ];
    for (id, at, size, color) in walls {
        add_node(document, &mut inverses, scene, node3d(None, id, "Wall", Transform3D { position: at, scale: size, ..Default::default() }, solid(Primitive3D::cube(), color, 0.08)));
    }

    // ---- The Leer (everything below rides on this pivot) ----
    let leer = add_node(document, &mut inverses, scene, node3d(None, "leer", "Leer", Transform3D::at(Vec3::new(0.0, 1.7, -1.0)), Node3DContent::Empty));
    add_node(
        document,
        &mut inverses,
        scene,
        node3d(Some(leer), "glow", "Eye Light", Transform3D::default(), Node3DContent::Light(Light3D { kind: Light3DKind::Point, color: [1.0, 0.2, 0.85], intensity: 1.6, range: 4.0 })),
    );
    let eye = Mesh3D { unlit: true, material: Some(eye_material), shininess: 0.0, ..Mesh3D::new(Primitive3D::Sphere { radius: 0.5, segments: 64, rings: 32 }, [1.0, 1.0, 1.0, 1.0]) };
    add_node(document, &mut inverses, scene, node3d(Some(leer), "eye", "Eye", Transform3D { scale: Vec3::splat(EYE_RADIUS * 2.0), ..Default::default() }, Node3DContent::Mesh(eye)));

    // The ink frames the eye: a pivot the script keeps facing the camera,
    // carrying the orbiting emitters.
    let halo = add_node(document, &mut inverses, scene, node3d(None, "halo", "Ink Halo", Transform3D::at(Vec3::new(0.0, 1.7, -1.0)), Node3DContent::Empty));
    add_node(document, &mut inverses, scene, facing(emitter(halo, "tendrils", "Ink Tendrils", tendrils())));
    add_node(
        document,
        &mut inverses,
        scene,
        facing(emitter(halo, "tendrils2", "Ink Tendrils (wide)", ParticleSystem3D { radius: EYE_RADIUS + 0.22, rate: 6.0, swirl: -120.0, size_start: 0.34, tilt: 40.0, trail_time: 0.75, seed: 17, ..tendrils() })),
    );
    add_node(
        document,
        &mut inverses,
        scene,
        facing(emitter(
            halo,
            "glints",
            "Magenta Glints",
            ParticleSystem3D {
                radius: EYE_RADIUS + 0.1,
                rate: 7.0,
                size_start: 0.045,
                size_end: 0.01,
                color_start: [1.0, 0.2, 0.85, 1.0],
                color_mid: [0.85, 0.1, 0.75, 0.8],
                color_end: [0.6, 0.0, 0.6, 0.0],
                intensity: 1.0,
                blend: ParticleBlend3D::Additive,
                hardness: 0.6,
                trail_segments: 14,
                trail_time: 0.4,
                fluid: false,
                seed: 29,
                ..tendrils()
            },
        )),
    );
    add_node(
        document,
        &mut inverses,
        scene,
        emitter(
            leer,
            "flicks",
            "Ink Flicks",
            ParticleSystem3D {
                rate: 5.0,
                burst: 0,
                max_particles: 80,
                lifetime: 0.8,
                lifetime_jitter: 0.3,
                shape: Emitter3DShape::Shell,
                radius: EYE_RADIUS + 0.02,
                thickness: 0.02,
                speed: 1.3,
                swirl: 140.0,
                swirl_jitter: 0.4,
                tilt: 90.0,
                pull: 0.0,
                turbulence: 0.1,
                size_start: 0.22,
                size_end: 0.02,
                color_start: ink(1.0),
                color_mid: ink(1.0),
                color_end: ink(1.0),
                fluid: true,
                fluid_edge: [0.95, 0.12, 0.78, 0.45],
                fluid_threshold: 0.42,
                fluid_gloss: 0.9,
                blend: ParticleBlend3D::Alpha,
                hardness: 0.92,
                trail_segments: 14,
                trail_time: 0.35,
                seed: 5,
                ..Default::default()
            },
        ),
    );
    let droplet = |radius: f32, rate: f32, seed: u32, behaviors: Vec<ParticleBehavior3D>| ParticleSystem3D {
        rate,
        max_particles: 60,
        lifetime: 3.5,
        lifetime_jitter: 0.3,
        shape: Emitter3DShape::Shell,
        radius,
        thickness: 0.5,
        swirl: 25.0,
        swirl_jitter: 0.6,
        tilt: 90.0,
        pull: 0.0,
        turbulence: 0.15,
        size_start: 0.22,
        size_end: 0.15,
        color_start: ink(0.0),
        color_mid: ink(1.0),
        color_end: ink(1.0),
        blend: ParticleBlend3D::Alpha,
        hardness: 1.0,
        // Glossy liquid drops (they merge if they meet).
        fluid: true,
        fluid_edge: [0.95, 0.12, 0.78, 0.25],
        fluid_threshold: 0.4,
        fluid_gloss: 1.0,
        seed,
        behaviors,
        ..Default::default()
    };
    add_node(document, &mut inverses, scene, emitter(leer, "droplets", "Ink Droplets", droplet(1.6, 3.0, 13, Vec::new())));
    add_node(
        document,
        &mut inverses,
        scene,
        emitter(
            leer,
            "gather",
            "Converging Droplets",
            droplet(
                2.6,
                4.0,
                21,
                // Drift freely for the first 55 % of life, then pulled all
                // the way into the eye's centre.
                vec![ParticleBehavior3D {
                    kind: ParticleBehaviorKind3D::Converge,
                    enabled: true,
                    strength: 1.0,
                    curve: LifeCurve { points: vec![[0.0, 0.0], [0.55, 0.0], [1.0, 1.0]] },
                    point: [0.0, 0.0, 0.0],
                    space: PointSpace::Local,
                }],
            ),
        ),
    );
    add_node(
        document,
        &mut inverses,
        scene,
        emitter(
            leer,
            "smoke",
            "Ink Smoke",
            ParticleSystem3D {
                rate: 9.0,
                max_particles: 60,
                lifetime: 2.2,
                lifetime_jitter: 0.3,
                shape: Emitter3DShape::Shell,
                radius: EYE_RADIUS + 0.05,
                thickness: 0.1,
                swirl: 35.0,
                swirl_jitter: 0.5,
                tilt: 90.0,
                pull: 0.3,
                speed: 0.15,
                rise: 0.22,
                turbulence: 0.2,
                size_start: 0.45,
                size_end: 1.0,
                color_start: ink(0.0),
                color_mid: ink(0.32),
                color_end: ink(0.0),
                blend: ParticleBlend3D::Alpha,
                hardness: 0.0,
                seed: 23,
                ..Default::default()
            },
        ),
    );

    // ---- HUD (2D, over the 3D view) ----
    add_node(document, &mut inverses, scene, text_node(Some(camera2d_id), "hint", "SPACE: CAST    DRAG: ORBIT    W/S: ZOOM", font, 8.0, [1.0, 1.0, 1.0, 0.9], Vec2::new(0.0, -SCREEN_H / 2.0 + 16.0)));

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

const SCRIPT: &str = r#"// Reyna's Leer — a hovering eye in a courtyard; Space to cast.
const g = ctx.globals.leer || (ctx.globals.leer = { yaw: 0, pitch: 6, dist: 4.2, t: 0, cast: -1 })
const keys = ctx.keysDown
const dt = Math.min(ctx.deltaSeconds || 1 / 60, 1 / 20)
const ref = (id) => getCompByID(id)
g.t += dt

if (!g.envSet) {
  ctx.setEnvironment3({ fog: [0.62, 0.78, 0.95], fogDensity: 0.018, ambient: [0.5, 0.46, 0.44] })
  g.envSet = true
}

// ---- orbit input (kept inside the courtyard) ----
const down = keys.has('mouse')
if (down && g.wasDown) {
  g.yaw = Math.max(-70, Math.min(70, g.yaw - (ctx.mouseX - g.lastX) * 0.4))
  g.pitch = Math.max(-8, Math.min(45, g.pitch + (ctx.mouseY - g.lastY) * 0.3))
  g.touched = true
}
g.wasDown = down
g.lastX = ctx.mouseX; g.lastY = ctx.mouseY
if (keys.has('left') || keys.has('a')) { g.yaw = Math.min(70, g.yaw + 60 * dt); g.touched = true }
if (keys.has('right') || keys.has('d')) { g.yaw = Math.max(-70, g.yaw - 60 * dt); g.touched = true }
if (keys.has('w')) g.dist = Math.max(2.2, g.dist - 4 * dt)
if (keys.has('s')) g.dist = Math.min(7, g.dist + 4 * dt)
if (!g.touched) g.yaw = Math.sin(g.t * 0.35) * 32   // idle: a slow sway

// ---- cast (Space): lunge, flare, lash out ----
const space = keys.has(' ') || keys.has('space')
if (space && !g.spaceWas && g.cast < 0) {
  g.cast = 0
  ref('flicks').burst3(40)
  ref('tendrils').burst3(30)
}
g.spaceWas = space
let lunge = 0, flare = 1
if (g.cast >= 0) {
  g.cast += dt
  const k = g.cast / 1.8
  lunge = Math.sin(Math.min(k, 1) * Math.PI)
  flare = 1 + Math.max(0, 1 - g.cast * 2) * 0.25
  if (k >= 1) g.cast = -1
}

// ---- the eye hovers and bobs; on a cast it lunges at the camera ----
const yr = g.yaw * Math.PI / 180, pr = g.pitch * Math.PI / 180
const home = { x: 0, y: 1.7 + Math.sin(g.t * 1.4) * 0.08, z: -1 }
const toCam = { x: Math.sin(yr), z: Math.cos(yr) }
const leer = ref('leer')
leer.position3 = { x: home.x + toCam.x * lunge * 1.6, y: home.y + lunge * 0.15, z: home.z + toCam.z * lunge * 1.6 }
leer.rotation3 = { x: Math.sin(g.t * 0.8) * 8, y: g.t * 20, z: Math.sin(g.t * 0.6) * 6 }
leer.scale3 = { x: flare, y: flare, z: flare }

// ---- camera orbits the eye's home ----
const target = { x: 0, y: 1.6, z: -1 }
const cam = ref('cam3d')
cam.position3 = {
  x: target.x + Math.sin(yr) * Math.cos(pr) * g.dist,
  y: target.y + Math.sin(pr) * g.dist,
  z: target.z + Math.cos(yr) * Math.cos(pr) * g.dist,
}
cam.lookAt3(target.x, target.y, target.z)

// ---- the ink halo rides with the eye and always faces the camera ----
const halo = ref('halo')
halo.position3 = leer.position3
halo.scale3 = { x: flare, y: flare, z: flare }
const cp = cam.position3
halo.lookAt3(cp.x, cp.y, cp.z)
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eye_material_compiles_and_the_effect_has_its_layers() {
        let mut document = Document::new();
        let demo = build_demo_leer(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let emitters = artboard.node_ids().filter(|id| artboard.node(*id).unwrap().behaviors.iter().any(|e| matches!(e.behavior, NodeBehavior::ParticleSystem3D(_)))).count();
        assert_eq!(emitters, 7);
        an1mbyt3_core::shader_graph::compile_wgsl(&eye_graph()).expect("eye graph compiles");
    }
}
