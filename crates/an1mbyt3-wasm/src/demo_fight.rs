//! "Fatal Clash" — a Mortal Kombat-style 3D fighting game with VRM
//! avatars: **popi** (by rachel bethia d, CC BY) against her double,
//! **Shadow Popi**, who wears the painterly Arcane toon material.
//!
//! - A side-on 2.5D duel in a moonlit temple courtyard: torch-lit braziers,
//!   a few pillars, fog, a full moon over the wall.
//! - **Moves**: jab, kick, uppercut (↓ + punch, launches), sweep (↓ + kick,
//!   trips), flying kick (kick in the air), block / crouch-block, and the
//!   **Spirit Orb** special (back, forward + punch). Combos count up; blows
//!   spray blood (a fluid particle burst) that splatters on the floor.
//! - **Knockdowns and KOs use the VRM ragdoll**: the avatar's bones become
//!   physics limbs, she crumples, tumbles and gets back up (or doesn't).
//! - **Rounds**: best of three, 60 s timer, "ROUND 1 … FIGHT!", and on the
//!   deciding round **"FINISH HER!"** — input ↓ → ↓ + kick up close for a
//!   **FATALITY**.
//! - **1 player** against a CPU (easy / normal / hard) or **2 players**.
//! - Every sound is synthesized here (punches, kicks, blocks, whooshes, the
//!   orb, a gong, the KO boom, a taiko drum loop).
//!
//! The avatar's bytes come in from the host (`apps/editor/public/demo/
//! popi.vrm`) rather than being compiled in. All gameplay lives in
//! `demo_fight.js`; poses are bone rotations the script blends per frame.

use an1mbyt3_core::{
    Animation, ArtboardId, AudioAsset, Camera3D, CameraNode, Command, Document, Emitter3DShape, Light3D, Light3DKind, LoopMode, MaterialId, Mesh3D, ModelAsset, ModelId, Node,
    Node3DContent, NodeBehavior, NodeBehaviorEntry, NodeId, NodeKind, ParticleBlend3D, ParticleSystem3D, PostEffect, PostEffectLayer, Primitive3D, ProgramDef, Ragdoll3D, StateDef, StateMachineDef,
    Transform3D,
};
use glam::{Vec2, Vec3};

use crate::demo_ninja::{add_node, with_id};
use crate::demo_shatter::{wav, Noise, RATE};

const SCREEN_W: f32 = 960.0;
const SCREEN_H: f32 = 540.0;
/// Where the fighters can go (± metres along X).
const ARENA: f32 = 6.5;
/// Pools: floor blood splats, and blood / spark emitters (rotated through).
const DECALS: usize = 48;
const BLOOD: usize = 4;
const SPARKS: usize = 2;
/// Braziers either side of the stage.
const BRAZIERS: usize = 2;

pub(crate) struct FightDemo {
    pub scene: ArtboardId,
    pub model: ModelId,
    /// Shadow Popi's node.
    pub shadow: NodeId,
}

fn node3d(name: &str, parent: Option<NodeId>, transform: Transform3D, content: Node3DContent) -> Node {
    with_id(Node::new(name, parent, NodeKind::Node3D(an1mbyt3_core::Node3D::new(transform, content))), name)
}

fn mesh(primitive: Primitive3D, color: [f32; 4], shininess: f32, unlit: bool, material: Option<MaterialId>) -> Node3DContent {
    Node3DContent::Mesh(Mesh3D { primitive, color, shininess, unlit, material, animation: None, animation_loop: true, animation_overlay: None })
}

fn at(position: Vec3) -> Transform3D {
    Transform3D::at(position)
}

fn posed(position: Vec3, rotation_degrees: Vec3, scale: Vec3) -> Transform3D {
    Transform3D { position, rotation_degrees, scale }
}

const PARKED: Vec3 = Vec3::new(0.0, -50.0, 0.0);

fn hidden(mut node: Node) -> Node {
    node.visible = false;
    node
}

fn with(mut node: Node, behavior: NodeBehavior) -> Node {
    node.behaviors.push(NodeBehaviorEntry { behavior, enabled: true });
    node
}

fn html_node(name: &str, parent: Option<NodeId>, html: &str, css: &str, interactive: bool) -> Node {
    let mut node = Node::with_components(
        name,
        parent,
        vec![NodeBehavior::UiHtml(an1mbyt3_core::UiHtml { html: html.into(), css: css.into(), width: 1600.0, height: 900.0, interactive })],
    );
    node.transform.scale = Vec2::splat(SCREEN_W / 1600.0);
    with_id(node, name)
}

// ------------------------------------------------------------ sounds --

fn seconds(s: f32) -> usize {
    (s * RATE as f32) as usize
}

/// A blow landing: a low body thump (a falling sine) under a short slap of
/// filtered noise. `weight` 0..1 makes it heavier (lower, longer).
fn hit_sound(weight: f32, seed: u32) -> Vec<f32> {
    let n = seconds(0.32 + weight * 0.25);
    let mut rng = Noise(seed);
    let mut out = vec![0.0; n];
    let mut lp = 0.0;
    for (i, o) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        let f = (95.0 - weight * 40.0) * (1.0 + 2.5 * (-t * 30.0).exp());
        let thump = (t * f * std::f32::consts::TAU).sin() * (-t * (14.0 - weight * 6.0)).exp();
        lp += (rng.next() - lp) * (0.55 - weight * 0.25);
        let slap = lp * (-t * 45.0).exp() * 0.9;
        *o = thump + slap;
    }
    out
}

/// A block: a hard, bright clack.
fn block_sound() -> Vec<f32> {
    let mut rng = Noise(77);
    (0..seconds(0.18))
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let ring = (t * 1850.0 * std::f32::consts::TAU).sin() * 0.5 + (t * 2630.0 * std::f32::consts::TAU).sin() * 0.3;
            (ring + rng.next() * 0.6) * (-t * 38.0).exp()
        })
        .collect()
}

/// A swing through the air: noise swept up then down through a band.
fn whoosh_sound(seed: u32, length: f32) -> Vec<f32> {
    let n = seconds(length);
    let mut rng = Noise(seed);
    let (mut lp, mut bp) = (0.0, 0.0);
    (0..n)
        .map(|i| {
            let p = i as f32 / n as f32;
            let env = (p * std::f32::consts::PI).sin().powf(1.5);
            let cut = 0.04 + 0.25 * env;
            lp += (rng.next() - lp) * cut;
            bp += (lp - bp) * 0.3;
            (lp - bp) * env * 2.0
        })
        .collect()
}

/// The Spirit Orb: a rising, shimmering hum.
fn orb_sound() -> Vec<f32> {
    let n = seconds(0.7);
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let p = i as f32 / n as f32;
            let f = 260.0 + 620.0 * p;
            let env = (p * 6.0).min(1.0) * (1.0 - p).powf(1.3);
            ((t * f * std::f32::consts::TAU).sin() + 0.4 * (t * f * 2.01 * std::f32::consts::TAU).sin() + 0.25 * (t * f * 3.03 * std::f32::consts::TAU).sin() * (t * 23.0 * std::f32::consts::TAU).sin()) * env
        })
        .collect()
}

/// A temple gong: inharmonic partials ringing out.
fn gong_sound() -> Vec<f32> {
    let partials = [(72.0, 1.0, 0.9), (149.0, 0.7, 1.4), (211.0, 0.5, 1.9), (287.0, 0.4, 2.6), (403.0, 0.25, 3.4), (577.0, 0.15, 4.5)];
    (0..seconds(3.2))
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let swell = (t * 30.0).min(1.0);
            partials.iter().map(|(f, a, d)| (t * f * std::f32::consts::TAU + (t * 3.0).sin() * 0.3).sin() * a * (-t * d).exp()).sum::<f32>() * swell
        })
        .collect()
}

/// A KO / fatality boom: a deep falling sine and a roar of noise.
fn boom_sound() -> Vec<f32> {
    let mut rng = Noise(901);
    let mut lp = 0.0;
    (0..seconds(1.8))
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let f = 30.0 + 70.0 * (-t * 3.0).exp();
            lp += (rng.next() - lp) * 0.08;
            (t * f * std::f32::consts::TAU).sin() * (-t * 1.8).exp() + lp * 1.6 * (-t * 2.5).exp()
        })
        .collect()
}

/// A body hitting the floor.
fn fall_sound() -> Vec<f32> {
    let mut rng = Noise(313);
    let mut lp = 0.0;
    (0..seconds(0.45))
        .map(|i| {
            let t = i as f32 / RATE as f32;
            lp += (rng.next() - lp) * 0.2;
            (t * (70.0 + 50.0 * (-t * 20.0).exp()) * std::f32::consts::TAU).sin() * (-t * 9.0).exp() + lp * (-t * 18.0).exp()
        })
        .collect()
}

/// A taiko loop (4 s, 120 bpm): big drums, rim clicks, a low drone.
fn drums_sound() -> Vec<f32> {
    let n = seconds(4.0);
    let mut out = vec![0.0f32; n];
    let beat = 0.25f32;
    // 16 steps: 2 = big drum, 1 = small drum, 3 = rim.
    let pattern = [2, 0, 0, 1, 0, 0, 2, 0, 3, 0, 2, 1, 0, 3, 1, 0];
    let mut rng = Noise(55);
    for (step, kind) in pattern.iter().enumerate() {
        if *kind == 0 {
            continue;
        }
        let start = seconds(step as f32 * beat);
        for i in 0..seconds(0.6) {
            let Some(o) = out.get_mut(start + i) else { break };
            let t = i as f32 / RATE as f32;
            *o += match kind {
                2 => (t * (58.0 + 60.0 * (-t * 25.0).exp()) * std::f32::consts::TAU).sin() * (-t * 6.0).exp() * 1.0,
                1 => (t * (110.0 + 80.0 * (-t * 30.0).exp()) * std::f32::consts::TAU).sin() * (-t * 10.0).exp() * 0.55,
                _ => rng.next() * (-t * 60.0).exp() * 0.35,
            };
        }
    }
    for (i, o) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        *o += ((t * 55.0 * std::f32::consts::TAU).sin() * 0.06 + (t * 82.4 * std::f32::consts::TAU).sin() * 0.04) * (0.7 + 0.3 * (t * 0.5 * std::f32::consts::TAU).sin());
    }
    out
}

// ------------------------------------------------------------- build --

/// Builds the Fatal Clash scene around a VRM avatar (`vrm`, the file's
/// bytes) — both fighters wear it; the second gets the Arcane material.
pub(crate) fn build_demo_fight(document: &mut Document, vrm: Vec<u8>, name: &str) -> FightDemo {
    let mut inverses = Vec::new();
    let inverse = document.apply(Command::AddArtboard { name: "Fatal Clash".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    for (sound, samples) in [
        ("hitLight", hit_sound(0.15, 11)),
        ("hitMid", hit_sound(0.5, 23)),
        ("hitHeavy", hit_sound(1.0, 37)),
        ("block", block_sound()),
        ("whoosh", whoosh_sound(5, 0.22)),
        ("whooshBig", whoosh_sound(9, 0.38)),
        ("orb", orb_sound()),
        ("gong", gong_sound()),
        ("boom", boom_sound()),
        ("fall", fall_sound()),
        ("drums", drums_sound()),
    ] {
        document.add_audio(AudioAsset { name: sound.into(), source: wav(&samples), file: None, deleted: false });
    }

    let model = document.add_model(ModelAsset { name: name.into(), source: vrm, files: Vec::new(), deleted: false });

    // The 2D camera carries the HTML HUD; the view is the Camera 3D.
    let hud_camera = with_id(
        Node::with_components("HUD Camera", None, vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.02, 0.02, 0.04, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })]),
        "hudCamera",
    );
    let hud_id = add_node(document, &mut inverses, scene, hud_camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(hud_id) }));
    add_node(document, &mut inverses, scene, html_node("hud", Some(hud_id), HUD_HTML, HUD_CSS, false));
    add_node(document, &mut inverses, scene, html_node("menu", Some(hud_id), MENU_HTML, MENU_CSS, true));

    let night = [0.035, 0.03, 0.065, 1.0];
    let cam = Camera3D { fov_degrees: 36.0, near: 0.05, far: 160.0, current: true, clear_color: Some(night) };
    add_node(document, &mut inverses, scene, node3d("cam", None, posed(Vec3::new(0.0, 1.45, 7.5), Vec3::new(-4.0, 0.0, 0.0), Vec3::ONE), Node3DContent::Camera(cam)));
    add_node(
        document,
        &mut inverses,
        scene,
        node3d("moonlight", None, posed(Vec3::new(0.0, 20.0, 0.0), Vec3::new(-52.0, 28.0, 0.0), Vec3::ONE), Node3DContent::Light(Light3D { kind: Light3DKind::Directional, color: [0.66, 0.72, 1.0], intensity: 1.25, range: 10.0 })),
    );

    // The courtyard, kept quiet: dark stone, a low temple wall with a few
    // pillars, mountains and the moon in the fog behind.
    let stone = [0.11, 0.105, 0.12, 1.0];
    // The stone sits a few cm under y = 0 so soles never sink into it; the
    // real ground is an invisible collider at y = 0 (ragdolls land on it).
    add_node(document, &mut inverses, scene, node3d("floor", None, at(Vec3::new(0.0, -0.16, 0.0)), mesh(Primitive3D::Cube { size: Vec3::new(44.0, 0.2, 18.0) }, stone, 0.4, false, None)));
    add_node(
        document,
        &mut inverses,
        scene,
        with(node3d("ground", None, at(Vec3::new(0.0, -0.1, 0.0)), Node3DContent::Empty), NodeBehavior::Collider3D(an1mbyt3_core::Collider3D { shape: an1mbyt3_core::Collider3DShape::Box { size: Vec3::new(44.0, 0.2, 18.0) }, friction: 0.9, ..Default::default() })),
    );
    add_node(document, &mut inverses, scene, node3d("wall", None, at(Vec3::new(0.0, 1.6, -6.4)), mesh(Primitive3D::Cube { size: Vec3::new(44.0, 3.2, 1.0) }, [0.075, 0.07, 0.085, 1.0], 0.1, false, None)));
    add_node(document, &mut inverses, scene, node3d("wallTop", None, at(Vec3::new(0.0, 3.3, -6.2)), mesh(Primitive3D::Cube { size: Vec3::new(44.0, 0.25, 1.4) }, [0.09, 0.085, 0.1, 1.0], 0.15, false, None)));
    for (i, x) in [-9.5f32, -4.8, 4.8, 9.5].into_iter().enumerate() {
        add_node(document, &mut inverses, scene, node3d(&format!("pillar{i}"), None, at(Vec3::new(x, 2.4, -5.6)), mesh(Primitive3D::Cylinder { radius: 0.38, height: 4.8, segments: 20 }, [0.12, 0.115, 0.13, 1.0], 0.25, false, None)));
    }
    // Far mountains and the moon.
    for (i, (x, h, w)) in [(-26.0f32, 12.0f32, 16.0f32), (-9.0, 16.0, 18.0), (8.0, 13.0, 14.0), (24.0, 17.0, 20.0)].into_iter().enumerate() {
        add_node(
            document,
            &mut inverses,
            scene,
            node3d(&format!("peak{i}"), None, posed(Vec3::new(x, 0.0, -48.0), Vec3::new(0.0, 0.0, 45.0), Vec3::ONE), mesh(Primitive3D::Cube { size: Vec3::new(w * 0.6, h * 0.6, 2.0) }, [0.055, 0.055, 0.085, 1.0], 0.0, true, None)),
        );
    }
    add_node(document, &mut inverses, scene, node3d("moon", None, at(Vec3::new(7.0, 11.0, -60.0)), mesh(Primitive3D::Sphere { radius: 3.0, segments: 40, rings: 20 }, [1.25, 1.18, 1.05, 1.0], 0.0, true, None)));

    // Braziers: pedestal, bowl, a flickering flame and its light.
    for i in 0..BRAZIERS {
        let x = if i == 0 { -6.2 } else { 6.2 };
        let z = -4.2;
        let root = add_node(document, &mut inverses, scene, node3d(&format!("brazier{i}"), None, at(Vec3::new(x, 0.0, z)), Node3DContent::Empty));
        add_node(document, &mut inverses, scene, node3d(&format!("brazier{i}p"), Some(root), at(Vec3::new(0.0, 0.55, 0.0)), mesh(Primitive3D::Cylinder { radius: 0.28, height: 1.1, segments: 16 }, [0.2, 0.18, 0.2, 1.0], 0.4, false, None)));
        add_node(document, &mut inverses, scene, node3d(&format!("brazier{i}b"), Some(root), at(Vec3::new(0.0, 1.2, 0.0)), mesh(Primitive3D::Cylinder { radius: 0.5, height: 0.22, segments: 20 }, [0.28, 0.2, 0.12, 1.0], 0.7, false, None)));
        add_node(document, &mut inverses, scene, node3d(&format!("flame{i}"), Some(root), posed(Vec3::new(0.0, 1.3, 0.0), Vec3::ZERO, Vec3::new(1.0, 0.25, 1.0)), mesh(Primitive3D::Sphere { radius: 0.34, segments: 16, rings: 8 }, [0.9, 0.32, 0.08, 1.0], 0.0, true, None)));
        add_node(document, &mut inverses, scene, node3d(&format!("fire{i}"), Some(root), at(Vec3::new(0.0, 2.0, 0.0)), Node3DContent::Light(Light3D { kind: Light3DKind::Point, color: [1.0, 0.55, 0.2], intensity: 1.1, range: 7.0 })));
    }

    // The fighters: the same avatar twice (the second gets her own skin —
    // Material Overrides, set up by the loader once the avatar's own
    // materials exist).
    let ragdoll = Ragdoll3D { start_ragdolled: false, mass: 55.0, flexibility: 0.85, thickness: 1.0 };
    let mut fighters = Vec::new();
    for (id, x) in [("f1", -1.8f32), ("f2", 1.8)] {
        let fighter = mesh(Primitive3D::Model { model }, [1.0; 4], 0.15, false, None);
        // Foot IK keeps her feet on the stone through every stance, crouch
        // and lunge (the poses lower her hips; the knees bend for it).
        let node = with(with(node3d(id, None, at(Vec3::new(x, 0.0, 0.0)), fighter), NodeBehavior::Ragdoll3D(ragdoll)), NodeBehavior::FootIK(an1mbyt3_core::FootIK::default()));
        fighters.push(add_node(document, &mut inverses, scene, node));
    }

    // Blood, done realistically: a spray of small dark droplets that
    // streak as they fly and fall, a fine mist that hangs a moment, and
    // irregular splatter left on the stone (a few overlapping smears).
    let drops = ParticleSystem3D {
        emitting: false,
        rate: 0.0,
        max_particles: 420,
        lifetime: 0.75,
        lifetime_jitter: 0.5,
        shape: Emitter3DShape::Sphere,
        radius: 0.035,
        thickness: 0.0,
        speed: 1.9,
        rise: 1.2,
        swirl: 0.0,
        swirl_jitter: 0.0,
        tilt: 90.0,
        pull: 0.0,
        gravity: 9.8,
        turbulence: 0.15,
        size_start: 0.022,
        size_end: 0.012,
        color_start: [0.22, 0.0, 0.008, 1.0],
        color_mid: [0.16, 0.0, 0.006, 0.95],
        color_end: [0.1, 0.0, 0.004, 0.0],
        blend: ParticleBlend3D::Alpha,
        stretch: 0.012,
        local_space: false,
        hardness: 1.0,
        ..ParticleSystem3D::default()
    };
    let mist = ParticleSystem3D {
        emitting: false,
        rate: 0.0,
        max_particles: 120,
        lifetime: 0.6,
        lifetime_jitter: 0.4,
        shape: Emitter3DShape::Sphere,
        radius: 0.08,
        speed: 0.7,
        rise: 0.25,
        swirl: 0.0,
        tilt: 90.0,
        pull: 0.0,
        gravity: 0.6,
        turbulence: 0.25,
        size_start: 0.07,
        size_end: 0.22,
        color_start: [0.25, 0.0, 0.01, 0.35],
        color_mid: [0.18, 0.0, 0.01, 0.18],
        color_end: [0.12, 0.0, 0.0, 0.0],
        blend: ParticleBlend3D::Alpha,
        local_space: false,
        hardness: 0.0,
        ..ParticleSystem3D::default()
    };
    for i in 0..BLOOD {
        let p = ParticleSystem3D { seed: 100 + i as u32, ..drops.clone() };
        add_node(document, &mut inverses, scene, with(node3d(&format!("blood{i}"), None, at(PARKED), Node3DContent::Empty), NodeBehavior::ParticleSystem3D(p)));
        let p = ParticleSystem3D { seed: 150 + i as u32, ..mist.clone() };
        add_node(document, &mut inverses, scene, with(node3d(&format!("mist{i}"), None, at(PARKED), Node3DContent::Empty), NodeBehavior::ParticleSystem3D(p)));
    }
    let sparks = ParticleSystem3D {
        emitting: false,
        rate: 0.0,
        max_particles: 120,
        lifetime: 0.22,
        lifetime_jitter: 0.4,
        shape: Emitter3DShape::Sphere,
        radius: 0.05,
        speed: 5.0,
        rise: 0.5,
        swirl: 0.0,
        tilt: 90.0,
        pull: 0.0,
        gravity: 4.0,
        size_start: 0.05,
        size_end: 0.0,
        color_start: [1.0, 0.95, 0.7, 1.0],
        color_mid: [1.0, 0.6, 0.2, 0.9],
        color_end: [0.8, 0.2, 0.05, 0.0],
        intensity: 2.5,
        blend: ParticleBlend3D::Additive,
        stretch: 0.03,
        local_space: false,
        hardness: 0.6,
        ..ParticleSystem3D::default()
    };
    for i in 0..SPARKS {
        let p = ParticleSystem3D { seed: 200 + i as u32, ..sparks.clone() };
        add_node(document, &mut inverses, scene, with(node3d(&format!("spark{i}"), None, at(PARKED), Node3DContent::Empty), NodeBehavior::ParticleSystem3D(p)));
    }
    for i in 0..DECALS {
        add_node(document, &mut inverses, scene, hidden(node3d(&format!("bd{i}"), None, at(PARKED), mesh(Primitive3D::Cylinder { radius: 0.5, height: 0.004, segments: 14 }, [0.13, 0.0, 0.008, 0.82], 0.9, false, None))));
    }

    inverses.push(document.apply(Command::SetPostEffects {
        artboard: scene,
        value: vec![
            PostEffectLayer { effect: PostEffect::Glow { radius: 3.0, threshold: 1.15, intensity: 0.3 }, enabled: true },
            PostEffectLayer { effect: PostEffect::ColorGrading { brightness: 0.0, contrast: 1.12, saturation: 1.08 }, enabled: true },
            PostEffectLayer { effect: PostEffect::Vignette { color: [0.0, 0.0, 0.02, 1.0], intensity: 0.35, radius: 0.78, blur: 0.6 }, enabled: true },
        ],
    }));

    let prefix = format!("const W = {SCREEN_W}, H = {SCREEN_H}, ARENA = {ARENA}, DECALS = {DECALS}, BLOOD = {BLOOD}, SPARKS = {SPARKS}, BRAZIERS = {BRAZIERS}");
    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Fatal Clash (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Fatal Clash".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Fight".into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(format!("{prefix}\n{SCRIPT}"))),
            }],
            initial_state: "Fight".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));
    inverses.reverse();
    document.commit_batch(inverses);
    FightDemo { scene, model, shadow: fighters[1] }
}

const SCRIPT: &str = include_str!("demo_fight.js");

const HUD_HTML: &str = r#"<div class="hud" id="root">
  <div class="flash" id="flash"></div>
  <div class="top" id="bars">
    <div class="pl l">
      <div class="crest"><span>V</span></div>
      <div class="col">
        <div class="frame"><div class="track"><i class="trail" id="g1"></i><i class="life" id="h1"></i><i class="gleam"></i></div></div>
        <div class="tag"><span class="nm" id="n1">POPI</span><span class="st">VIPER · MUAY THAI</span><span class="medals" id="w1"></span></div>
      </div>
    </div>
    <div class="clock"><div class="num" id="timer">60</div><div class="rnd" id="rnd">ROUND 1</div></div>
    <div class="pl r">
      <div class="col">
        <div class="frame"><div class="track"><i class="trail" id="g2"></i><i class="life" id="h2"></i><i class="gleam"></i></div></div>
        <div class="tag"><span class="medals" id="w2"></span><span class="st">WRAITH · KUNG FU</span><span class="nm" id="n2">SHADOW POPI</span></div>
      </div>
      <div class="crest"><span>W</span></div>
    </div>
  </div>
  <div class="combo l" id="c1"></div>
  <div class="combo r" id="c2"></div>
  <div class="callout l" id="m1"></div>
  <div class="callout r" id="m2"></div>
  <div class="banner" id="banner"><i class="slash"></i><div class="big" id="bannerBig"></div><div class="small" id="bannerSmall"></div></div>
  <div class="hint" id="hint"></div>
</div>"#;

const HUD_CSS: &str = r#"
@import url('https://fonts.googleapis.com/css2?family=Cinzel:wght@700;900&family=Rajdhani:wght@500;600;700&display=swap');
.hud { position: absolute; inset: 0; font-family: Rajdhani, 'Segoe UI', sans-serif; color: #efe6d4; pointer-events: none; overflow: hidden; }
.top { position: absolute; left: 44px; right: 44px; top: 30px; display: flex; align-items: flex-start; gap: 18px; transition: opacity 0.5s, transform 0.5s; }
.top.off { opacity: 0; transform: translateY(-30px); }
.pl { flex: 1; display: flex; align-items: center; gap: 16px; }
.col { flex: 1; display: flex; flex-direction: column; gap: 6px; }
.crest { display: none !important; width: 62px; height: 62px; flex: none; display: grid; place-items: center; background: radial-gradient(circle at 50% 35%, #3a2a14, #0d0906 70%); border: 2px solid #b8924c; transform: rotate(45deg); box-shadow: 0 0 0 4px rgb(10 8 6 / 0.85), 0 8px 24px rgb(0 0 0 / 0.7), inset 0 0 18px rgb(255 190 90 / 0.18); }
.crest span { transform: rotate(-45deg); font-family: Cinzel, Georgia, serif; font-weight: 900; font-size: 32px; color: #f3d892; }
.frame { position: relative; height: 20px; padding: 2px; background: linear-gradient(180deg, #e9cf8c, #8a6a2c 55%, #c9a65a); clip-path: polygon(0 0, 100% 0, calc(100% - 22px) 100%, 0 100%); filter: drop-shadow(0 6px 14px rgb(0 0 0 / 0.75)); }
.r .frame { clip-path: polygon(0 0, 100% 0, 100% 100%, 22px 100%); }
.track { position: relative; height: 100%; background: linear-gradient(180deg, #150708, #2a0b0c); overflow: hidden; }
.track i { position: absolute; top: 0; bottom: 0; }
.l .track i { left: 0; }
.r .track i { right: 0; }
.trail { background: linear-gradient(180deg, #ff5a3c, #9d0f12); transition: width 0.7s 0.4s ease-out; }
.life { background: linear-gradient(180deg, #fff7c8 0%, #f3cf5c 35%, #d98f16 70%, #9c5a08); transition: width 0.09s; box-shadow: inset 0 -6px 10px rgb(120 60 0 / 0.4); }
.life.low { background: linear-gradient(180deg, #ffd3b0 0%, #ff7a3a 40%, #c8261a 75%, #7a0a08); animation: throb 0.8s infinite; }
@keyframes throb { 50% { filter: brightness(1.35); } }
.track .gleam { left: 0; right: 0; background: linear-gradient(180deg, rgb(255 255 255 / 0.32), transparent 45%); }
.tag { display: flex; align-items: baseline; gap: 12px; letter-spacing: 3px; text-shadow: 0 2px 6px #000; }
.r .tag { justify-content: flex-end; }
.nm { font-family: Cinzel, Georgia, serif; font-weight: 700; font-size: 22px; color: #efe2c2; }
.st { font-size: 13px; font-weight: 600; color: #8f7f62; letter-spacing: 4px; }
.medals { display: flex; gap: 8px; }
.medals i { width: 15px; height: 15px; transform: rotate(45deg); background: linear-gradient(135deg, #fff1c2, #d9a13a 50%, #7b4d0c); box-shadow: 0 0 10px rgb(255 180 60 / 0.8); }
.clock { width: 130px; flex: none; text-align: center; margin-top: -6px; }
.num { font-family: Cinzel, Georgia, serif; font-weight: 700; font-size: 48px; line-height: 48px; background: linear-gradient(180deg, #fff6d6, #e2b45a 55%, #8a5a1a); -webkit-background-clip: text; background-clip: text; color: transparent; filter: drop-shadow(0 4px 8px #000); font-variant-numeric: tabular-nums; }
.num.low { background: linear-gradient(180deg, #ffd1c4, #ff4a2a 55%, #8a0a0a); -webkit-background-clip: text; background-clip: text; animation: tick 1s infinite; }
@keyframes tick { 50% { transform: scale(1.1); } }
.rnd { font-size: 15px; font-weight: 700; letter-spacing: 5px; color: #b9a27a; text-shadow: 0 2px 4px #000; }
.combo { position: absolute; top: 140px; font-family: Cinzel, Georgia, serif; font-weight: 700; font-size: 40px; color: #f7e3a6; text-shadow: 0 3px 0 #4a0806, 0 0 24px rgb(255 70 30 / 0.55); opacity: 0; transition: opacity 0.25s, transform 0.25s; }
.combo.l { left: 120px; transform: translateX(-40px); }
.combo.r { right: 120px; text-align: right; transform: translateX(40px); }
.combo.on { opacity: 1; transform: none; }
.combo small { display: block; font-family: Rajdhani, sans-serif; font-weight: 700; font-size: 20px; letter-spacing: 10px; color: #d3b376; }
.callout { position: absolute; top: 236px; font-size: 16px; font-weight: 700; letter-spacing: 8px; color: #e9d7ae; opacity: 0; transition: opacity 0.4s; text-shadow: 0 2px 6px #000; }
.callout.l { left: 124px; }
.callout.r { right: 124px; }
.callout.on { opacity: 0.9; transition: none; }
.banner { position: absolute; left: 0; right: 0; top: 33%; text-align: center; opacity: 0; transform: scale(1.18); transition: opacity 0.22s, transform 0.3s cubic-bezier(0.2, 1.4, 0.4, 1); }
.banner.on { opacity: 1; transform: none; }
.banner .slash { display: none; position: absolute; left: 10%; right: 10%; top: 50%; height: 120px; margin-top: -72px; background: linear-gradient(90deg, transparent, rgb(120 10 10 / 0.75) 20%, rgb(40 0 0 / 0.85) 50%, rgb(120 10 10 / 0.75) 80%, transparent); transform: skewX(-18deg) scaleX(0); transition: transform 0.35s ease-out; }
.banner.on .slash { transform: skewX(-18deg) scaleX(1); }
.banner .big { position: relative; font-family: Cinzel, Georgia, serif; font-weight: 700; font-size: 104px; letter-spacing: 14px; background: linear-gradient(180deg, #fffbe6 0%, #f0cf7a 40%, #a6741f 62%, #f7dc93 80%, #8a5a14); -webkit-background-clip: text; background-clip: text; color: transparent; filter: drop-shadow(0 6px 10px rgb(0 0 0 / 0.9)); }
.banner.red .big { background: linear-gradient(180deg, #ffd9cf 0%, #ff4a2c 35%, #8f0606 62%, #ff3a1a 80%, #4a0000); -webkit-background-clip: text; background-clip: text; font-size: 132px; filter: drop-shadow(0 0 18px rgb(255 0 0 / 0.35)) drop-shadow(0 6px 8px #000); }
.banner .small { position: relative; margin-top: 4px; font-size: 30px; font-weight: 700; letter-spacing: 14px; color: #e9d7ae; text-shadow: 0 2px 6px #000; }
.flash { position: absolute; inset: 0; background: radial-gradient(rgb(120 0 0 / 0) 30%, rgb(110 0 0 / 0.65)); opacity: 0; transition: opacity 0.7s; }
.flash.on { opacity: 1; transition: none; }
.flash.white { background: rgb(255 240 225 / 0.9); }
.hint { position: absolute; left: 0; right: 0; bottom: 40px; text-align: center; font-size: 26px; font-weight: 700; letter-spacing: 8px; color: #ff6a4a; text-shadow: 0 0 14px rgb(255 40 20 / 0.7), 0 2px 4px #000; }
"#;

const MENU_HTML: &str = r#"<div class="menu" id="root">
  <div class="fog"></div>
  <div class="card left"><div class="who">POPI</div><div class="sty">VIPER</div><div class="art">MUAY THAI</div>
    <ul><li>Jab · Cross · Roundhouse <em>P P K</em></li><li>Teep kick <em>→ K</em></li><li>Knee strike <em>→ P</em></li><li>Low kick <em>↓ K</em></li><li>Uppercut <em>↓ P</em></li><li>Flying knee <em>air K</em></li></ul></div>
  <div class="center">
    <div class="title">FATAL<span>CLASH</span></div>
    <div class="tag">One blood. Two styles. One walks away.</div>
    <button class="b" id="p1"><b>ARCADE</b><small>versus CPU</small></button>
    <button class="b" id="p2"><b>VERSUS</b><small>two players</small></button>
    <div class="diff"><button class="d" id="dEasy">EASY</button><button class="d" id="dNormal">NORMAL</button><button class="d" id="dHard">HARD</button></div>
    <div class="keys"><span><b>P1</b> A D · W · S · J punch · K kick · L block</span><span><b>P2</b> ← → · ↑ · ↓ · 1 punch · 2 kick · 3 block</span></div>
    <div class="fat">FINISH HER — ↓ → ↓ + KICK, close</div>
  </div>
  <div class="card right"><div class="who">SHADOW POPI</div><div class="sty">WRAITH</div><div class="art">KUNG FU</div>
    <ul><li>Palm · Double palm · Spin hook kick <em>P P K</em></li><li>Crescent kick <em>K</em></li><li>Axe kick <em>→ K</em></li><li>Spinning backfist <em>→ P</em></li><li>Spinning sweep <em>↓ K</em></li><li>Dive kick <em>air K</em></li></ul></div>
  <div class="credit">popi by rachel bethia d · CC BY</div>
</div>"#;

const MENU_CSS: &str = r#"
@import url('https://fonts.googleapis.com/css2?family=Cinzel:wght@700;900&family=Rajdhani:wght@500;600;700&display=swap');
.menu { position: absolute; inset: 0; display: grid; grid-template-columns: 1fr 640px 1fr; align-items: center; font-family: Rajdhani, 'Segoe UI', sans-serif; color: #efe6d4; background: linear-gradient(90deg, rgb(4 2 3 / 0.92), rgb(10 3 4 / 0.3) 32%, rgb(10 3 4 / 0.3) 68%, rgb(4 2 3 / 0.92)); }
.fog { position: absolute; inset: 0; background: radial-gradient(ellipse at 50% 120%, rgb(140 10 10 / 0.35), transparent 60%); pointer-events: none; }
.center { position: relative; display: flex; flex-direction: column; align-items: center; gap: 14px; }
.title { font-family: Cinzel, Georgia, serif; font-weight: 900; font-size: 120px; letter-spacing: 10px; line-height: 0.95; text-align: center; background: linear-gradient(180deg, #fffbe6 0%, #ecc874 42%, #94631a 64%, #f6db96 82%, #6e4610); -webkit-background-clip: text; background-clip: text; color: transparent; filter: drop-shadow(0 8px 14px rgb(0 0 0 / 0.9)); }
.title span { display: block; font-size: 150px; background: linear-gradient(180deg, #ffd6cc 0%, #ff3d22 38%, #7a0404 64%, #ff2a12 82%, #3a0000); -webkit-background-clip: text; background-clip: text; filter: drop-shadow(0 0 26px rgb(255 0 0 / 0.45)); }
.tag { font-size: 22px; letter-spacing: 6px; color: #c9b38a; margin-bottom: 10px; }
.b { pointer-events: auto; cursor: pointer; width: 420px; padding: 12px 0 10px; display: flex; flex-direction: column; align-items: center; color: #f6e6bc; background: linear-gradient(180deg, rgb(70 12 12 / 0.9), rgb(18 4 5 / 0.95)); border: 1px solid #b8924c; clip-path: polygon(16px 0, 100% 0, calc(100% - 16px) 100%, 0 100%); transition: transform 0.12s, filter 0.12s; font: inherit; }
.b b { font-family: Cinzel, Georgia, serif; font-weight: 900; font-size: 40px; letter-spacing: 10px; }
.b small { font-size: 16px; letter-spacing: 6px; color: #c9b38a; font-weight: 600; }
.b:hover { transform: translateY(-2px) scale(1.03); filter: brightness(1.35); }
.b:focus-visible { outline: 2px solid #ffd27a; }
.diff { display: flex; gap: 10px; margin-top: 6px; }
.d { pointer-events: auto; cursor: pointer; font: inherit; font-weight: 700; font-size: 18px; letter-spacing: 5px; color: #b9a27a; padding: 6px 16px; background: rgb(20 8 8 / 0.8); border: 1px solid #5a4422; transition: color 0.12s, border-color 0.12s; }
.d.on { color: #ffe5a6; border-color: #e4b65c; box-shadow: 0 0 14px rgb(255 160 60 / 0.35); }
.d:hover { color: #ffe5a6; }
.keys { display: flex; flex-direction: column; align-items: center; gap: 4px; margin-top: 10px; font-size: 18px; letter-spacing: 2px; color: #c9b38a; }
.keys b { color: #f3d892; margin-right: 8px; letter-spacing: 3px; }
.fat { margin-top: 6px; font-size: 18px; font-weight: 700; letter-spacing: 6px; color: #ff5a3a; }
.card { position: relative; align-self: end; margin: 0 0 110px; padding: 22px 30px; }
.card.left { justify-self: start; margin-left: 50px; }
.card.right { justify-self: end; margin-right: 50px; text-align: right; }
.who { font-family: Cinzel, Georgia, serif; font-weight: 900; font-size: 44px; letter-spacing: 6px; color: #f6e6bc; text-shadow: 0 4px 10px #000; }
.sty { font-family: Cinzel, Georgia, serif; font-weight: 700; font-size: 26px; letter-spacing: 14px; color: #d8182a; margin-top: -4px; }
.art { font-size: 16px; letter-spacing: 8px; color: #b9a27a; margin-bottom: 12px; }
.card ul { list-style: none; margin: 0; padding: 0; font-size: 19px; letter-spacing: 2px; line-height: 1.75; color: #e6d8b8; text-shadow: 0 2px 6px #000; }
.card em { font-style: normal; color: #e4b65c; margin-left: 8px; font-weight: 700; }
.credit { position: absolute; left: 0; right: 0; bottom: 22px; text-align: center; font-size: 16px; letter-spacing: 4px; opacity: 0.5; }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_fighters_pools_and_sounds() {
        let mut document = Document::new();
        let demo = build_demo_fight(&mut document, vec![0u8; 16], "popi.vrm");
        let artboard = document.artboard(demo.scene).unwrap();
        let ids: Vec<String> = artboard.node_ids().filter_map(|id| artboard.node(id)).map(|n| n.id.clone()).collect();
        for id in ["f1", "f2", "cam", "hud", "menu", "blood0", "mist0", "spark0", "bd0"] {
            assert!(ids.iter().any(|i| i == id), "missing {id}");
        }
        assert!(document.audio_assets().count() >= 11);
        assert!(document.model(demo.model).is_some());
    }
}
