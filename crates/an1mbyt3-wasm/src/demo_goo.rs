//! "Goo Lab" — 2D particles drawn as liquid (`ParticleFluid2D`) and steered
//! by behaviours with life curves (`ParticleBehaviors2D`). Four stations:
//!
//! - **Lava Lamp** — big blobs float up (negative gravity), merge, stretch
//!   apart, and an Attract with a late curve pulls them back down.
//! - **Slime Fountain** — a jet of small drops that fuse into streams and
//!   splash apart as they fall.
//! - **Converging Swarm** — drops scattered round a circle drift freely,
//!   then a Converge (curve 0 → 1 late in life) gathers them into one
//!   pulsing ball.
//! - **Ink Orb** — a 2D Leer: dark ink drops attracted to a staring
//!   magenta eye, bubbling round it with a magenta rim.

use an1mbyt3_core::{
    ArtboardId, CameraNode, Command, Document, EmitterShape, FontAsset, GradientStop, LifeCurve, Node, NodeBehavior, NodeKind, ParticleBehavior3D, ParticleBehaviorKind3D,
    ParticleBehaviors2D, ParticleFluid2D, ParticleRange, ParticleSystemNode, Path, PointSpace, ShapeNode, SimulationSpace,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");
const SCREEN_W: f32 = 960.0;
const SCREEN_H: f32 = 540.0;

pub(crate) struct GooDemo {
    pub scene: ArtboardId,
}

/// A behaviour whose strength follows `curve` (life 0 → 1).
fn behavior(kind: ParticleBehaviorKind3D, strength: f32, curve: &[[f32; 2]], point: Vec2) -> ParticleBehavior3D {
    ParticleBehavior3D { kind, enabled: true, strength, curve: LifeCurve { points: curve.to_vec() }, point: [point.x, point.y, 0.0], space: PointSpace::Local }
}

struct Station<'a> {
    id: &'a str,
    name: &'a str,
    at: Vec2,
    shape: EmitterShape,
    rate: f32,
    life: (f32, f32),
    speed: (f32, f32),
    size: (f32, f32),
    gravity: f32,
    drag: f32,
    max: u32,
    fluid: ParticleFluid2D,
    behaviors: Vec<ParticleBehavior3D>,
}

fn station(s: Station) -> Node {
    let mut config = ParticleSystemNode::default();
    config.main.looping = true;
    config.main.duration = 5.0;
    config.main.start_lifetime = ParticleRange { min: s.life.0, max: s.life.1 };
    config.main.start_speed = ParticleRange { min: s.speed.0, max: s.speed.1 };
    config.main.start_size = ParticleRange { min: s.size.0, max: s.size.1 };
    config.main.start_color = [1.0, 1.0, 1.0, 1.0];
    config.main.gravity_modifier = s.gravity;
    config.main.drag = s.drag;
    config.main.simulation_space = SimulationSpace::Local;
    config.main.max_particles = s.max;
    config.emission.rate_over_time = s.rate;
    config.emission.bursts.clear();
    config.shape = s.shape;
    // Alpha = how much liquid a drop adds: swell in, hold, melt away (so
    // drops join and part smoothly instead of popping).
    config.color_over_lifetime = [(0.0, 0.0), (0.12, 1.0), (0.85, 1.0), (1.0, 0.0)]
        .into_iter()
        .map(|(offset, a)| GradientStop { offset, color: [1.0, 1.0, 1.0, a], midpoint: 0.5 })
        .collect();
    let mut components = vec![NodeBehavior::ParticleSystem(config), NodeBehavior::ParticleFluid2D(s.fluid)];
    if !s.behaviors.is_empty() {
        components.push(NodeBehavior::ParticleBehaviors2D(ParticleBehaviors2D { behaviors: s.behaviors }));
    }
    let mut node = with_id(Node::with_components(s.name, None, components), s.id);
    node.transform.translation = s.at;
    node
}

fn shape(id: &str, path: Path, color: [f32; 4], at: Vec2) -> Node {
    let mut node = with_id(Node::new(id, None, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None })), id);
    node.transform.translation = at;
    node
}

pub(crate) fn build_demo_goo(document: &mut Document) -> GooDemo {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: FONT_BYTES.to_vec() });
    let inverse = document.apply(Command::AddArtboard { name: "Goo Lab".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);
    let camera = with_id(
        Node::with_components("Main Camera", None, vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.07, 0.07, 0.11, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })]),
        "camera",
    );
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));

    // Lava lamp glass behind its station.
    add_node(document, &mut inverses, scene, shape("lampGlass", Path::rect(170.0, 370.0), [0.16, 0.07, 0.12, 1.0], Vec2::new(-345.0, 20.0)));
    add_node(document, &mut inverses, scene, shape("lampBase", Path::rect(200.0, 36.0), [0.3, 0.3, 0.36, 1.0], Vec2::new(-345.0, -180.0)));
    add_node(document, &mut inverses, scene, shape("lampCap", Path::rect(140.0, 24.0), [0.3, 0.3, 0.36, 1.0], Vec2::new(-345.0, 215.0)));

    let stations = [
        Station {
            id: "lava",
            name: "Lava Lamp",
            at: Vec2::new(-345.0, -140.0),
            shape: EmitterShape::Box { width: 100.0, height: 10.0 },
            rate: 1.5,
            life: (7.0, 9.0),
            speed: (10.0, 25.0),
            size: (30.0, 44.0),
            gravity: -18.0,
            drag: 0.35,
            max: 14,
            fluid: ParticleFluid2D { color: [1.0, 0.36, 0.08, 1.0], edge_color: [0.75, 0.06, 0.04, 1.0], edge_width: 0.3, stickiness: 2.4, threshold: 0.5, gloss: 0.6, detail: 0.18 },
            // Rise, then sink back to the bottom late in life.
            behaviors: vec![behavior(ParticleBehaviorKind3D::Attract, 70.0, &[[0.0, 0.0], [0.5, 0.0], [0.8, 1.0]], Vec2::new(0.0, -10.0))],
        },
        Station {
            id: "slime",
            name: "Slime Fountain",
            at: Vec2::new(-110.0, -200.0),
            shape: EmitterShape::Cone { angle: 0.3, radius: 6.0 },
            rate: 30.0,
            life: (1.6, 2.0),
            speed: (300.0, 380.0),
            size: (16.0, 24.0),
            gravity: 420.0,
            drag: 0.0,
            max: 120,
            fluid: ParticleFluid2D { color: [0.4, 0.95, 0.25, 1.0], edge_color: [0.08, 0.42, 0.1, 1.0], edge_width: 0.25, stickiness: 2.2, threshold: 0.5, gloss: 0.7, detail: 0.2 },
            behaviors: Vec::new(),
        },
        Station {
            id: "swarm",
            name: "Converging Swarm",
            at: Vec2::new(120.0, 10.0),
            shape: EmitterShape::Circle { radius: 120.0 },
            rate: 26.0,
            life: (2.6, 3.2),
            speed: (15.0, 40.0),
            size: (18.0, 26.0),
            gravity: 0.0,
            drag: 0.0,
            max: 100,
            fluid: ParticleFluid2D { color: [0.58, 0.22, 1.0, 1.0], edge_color: [0.95, 0.65, 1.0, 1.0], edge_width: 0.18, stickiness: 2.3, threshold: 0.5, gloss: 0.5, detail: 0.2 },
            // Drift free, then gather into one ball.
            behaviors: vec![behavior(ParticleBehaviorKind3D::Converge, 1.0, &[[0.0, 0.0], [0.35, 0.0], [0.85, 1.0]], Vec2::ZERO)],
        },
        Station {
            id: "ink",
            name: "Ink Orb",
            at: Vec2::new(355.0, 10.0),
            shape: EmitterShape::Circle { radius: 70.0 },
            rate: 16.0,
            life: (2.0, 3.0),
            speed: (40.0, 90.0),
            size: (28.0, 38.0),
            gravity: 0.0,
            drag: 0.0,
            max: 60,
            fluid: ParticleFluid2D { color: [0.06, 0.0, 0.1, 1.0], edge_color: [0.95, 0.12, 0.78, 1.0], edge_width: 0.22, stickiness: 2.4, threshold: 0.5, gloss: 0.25, detail: 0.18 },
            behaviors: vec![
                behavior(ParticleBehaviorKind3D::Attract, 700.0, &[[0.0, 1.0], [1.0, 1.0]], Vec2::ZERO),
                behavior(ParticleBehaviorKind3D::Drag, 1.6, &[[0.0, 1.0], [1.0, 1.0]], Vec2::ZERO),
            ],
        },
    ];
    for s in stations {
        add_node(document, &mut inverses, scene, station(s));
    }

    // The ink orb's eye, on top of its goo.
    let eye = Vec2::new(355.0, 10.0);
    add_node(document, &mut inverses, scene, shape("irisRim", Path::ellipse(92.0, 92.0), [0.55, 0.02, 0.6, 1.0], eye));
    add_node(document, &mut inverses, scene, shape("iris", Path::ellipse(80.0, 80.0), [1.0, 0.3, 0.86, 1.0], eye));
    add_node(document, &mut inverses, scene, shape("irisGlow", Path::ellipse(46.0, 56.0), [1.0, 0.62, 0.98, 1.0], eye));
    add_node(document, &mut inverses, scene, shape("pupil", Path::ellipse(14.0, 50.0), [0.02, 0.0, 0.04, 1.0], eye));
    add_node(document, &mut inverses, scene, shape("glint", Path::ellipse(12.0, 12.0), [1.0, 0.95, 1.0, 1.0], eye + Vec2::new(-17.0, 16.0)));

    // Labels.
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "title", "GOO LAB", font, 18.0, [1.0, 1.0, 1.0, 1.0], Vec2::new(0.0, SCREEN_H / 2.0 - 34.0)));
    for (id, label, x) in [("labelLava", "LAVA LAMP", -345.0), ("labelSlime", "SLIME FOUNTAIN", -110.0), ("labelSwarm", "CONVERGE", 120.0), ("labelInk", "INK ORB", 355.0)] {
        add_node(document, &mut inverses, scene, text_node(Some(camera_id), id, label, font, 9.0, [0.8, 0.82, 0.95, 1.0], Vec2::new(x, -SCREEN_H / 2.0 + 30.0)));
    }
    add_node(
        document,
        &mut inverses,
        scene,
        text_node(Some(camera_id), "hint", "PARTICLE FLUID 2D + PARTICLE BEHAVIOURS 2D", font, 7.0, [0.6, 0.62, 0.75, 1.0], Vec2::new(0.0, SCREEN_H / 2.0 - 58.0)),
    );

    inverses.reverse();
    document.commit_batch(inverses);
    GooDemo { scene }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_station_is_a_fluid_particle_system() {
        let mut document = Document::new();
        let demo = build_demo_goo(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let fluids = artboard.node_ids().filter(|id| { let n = artboard.node(*id).unwrap(); n.particle_system().is_some() && n.particle_fluid().is_some() }).count();
        assert_eq!(fluids, 4);
    }
}
