//! "Goo Shapes" — `Goo2D` on ordinary nodes: shapes, text and particles in
//! the same liquid group melt together.
//!
//! - **GOO** — a text node in group 1 with a hot gradient fill and a dark
//!   gradient rim. Two balls (circle shapes, also group 1) orbit it and
//!   merge into the letters as they pass; a particle system under the
//!   letters (group 1 too) drips drops that stretch off and fall away.
//! - **Squares** — two rounded squares in group 2 slide together, fuse with
//!   a neck, and pull apart again (teal → violet gradient).
//!
//! A Code script moves the balls and squares (`ref.x` / `ref.y`).

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Command, Document, EmitterShape, FontAsset, GradientDef, GradientKind, GradientStop, Goo2D, LoopMode, Node, NodeBehavior, NodeKind, ParticleRange,
    ParticleSystemNode, Path, ProgramDef, ShapeNode, SimulationSpace, StateDef, StateMachineDef, TextNode,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/RussoOne-Regular.ttf");
const SCREEN_W: f32 = 960.0;
const SCREEN_H: f32 = 540.0;

pub(crate) struct GooShapesDemo {
    pub scene: ArtboardId,
}

fn gradient(start: Vec2, end: Vec2, colors: &[[f32; 4]]) -> GradientDef {
    let n = (colors.len().max(2) - 1) as f32;
    GradientDef { kind: GradientKind::Linear, start, end, stops: colors.iter().enumerate().map(|(i, c)| GradientStop { offset: i as f32 / n, color: *c, midpoint: 0.5 }).collect() }
}

fn goo(group: u32, fill: GradientDef, rim: GradientDef, edge_width: f32, reach: f32) -> NodeBehavior {
    NodeBehavior::Goo2D(Goo2D { group, color: [1.0; 4], gradient: Some(fill), edge_color: [0.0, 0.0, 0.0, 1.0], edge_gradient: Some(rim), edge_width, reach, gloss: 0.55, detail: 0.16, hide_source: true })
}

fn shape(id: &str, path: Path, at: Vec2, components: Vec<NodeBehavior>) -> Node {
    let mut node = with_id(Node::new(id, None, NodeKind::Shape(ShapeNode { path, fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None })), id);
    node.behaviors.extend(components.into_iter().map(|behavior| an1mbyt3_core::NodeBehaviorEntry { behavior, enabled: true }));
    node.transform.translation = at;
    node
}

/// A square with rounded corners (cubic arcs).
fn rounded_square(size: f32, r: f32) -> Path {
    use an1mbyt3_core::PathCommand as C;
    let h = size / 2.0;
    let k = r * 0.5523;
    Path {
        commands: vec![
            C::MoveTo(Vec2::new(-h + r, -h)),
            C::LineTo(Vec2::new(h - r, -h)),
            C::CubicTo { control1: Vec2::new(h - r + k, -h), control2: Vec2::new(h, -h + r - k), to: Vec2::new(h, -h + r) },
            C::LineTo(Vec2::new(h, h - r)),
            C::CubicTo { control1: Vec2::new(h, h - r + k), control2: Vec2::new(h - r + k, h), to: Vec2::new(h - r, h) },
            C::LineTo(Vec2::new(-h + r, h)),
            C::CubicTo { control1: Vec2::new(-h + r - k, h), control2: Vec2::new(-h, h - r + k), to: Vec2::new(-h, h - r) },
            C::LineTo(Vec2::new(-h, -h + r)),
            C::CubicTo { control1: Vec2::new(-h, -h + r - k), control2: Vec2::new(-h + r - k, -h), to: Vec2::new(-h + r, -h) },
            C::Close,
        ],
    }
}

pub(crate) fn build_demo_goo_shapes(document: &mut Document) -> GooShapesDemo {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Russo One".into(), data: FONT_BYTES.to_vec() });
    let inverse = document.apply(Command::AddArtboard { name: "Goo Shapes".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);
    let camera = with_id(
        Node::with_components("Main Camera", None, vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.96, 0.93, 0.88, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })]),
        "camera",
    );
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));

    // ---- Group 1: the GOO text, its orbiting balls and its drips ----
    // Gradients are in the group owner's (the text's) space.
    let hot = gradient(Vec2::new(-200.0, 0.0), Vec2::new(200.0, 0.0), &[[1.0, 0.2, 0.55, 1.0], [1.0, 0.45, 0.1, 1.0], [1.0, 0.85, 0.15, 1.0]]);
    let rim = gradient(Vec2::new(0.0, -80.0), Vec2::new(0.0, 120.0), &[[0.35, 0.0, 0.25, 1.0], [0.55, 0.05, 0.1, 1.0]]);
    let mut text = with_id(Node::new("GOO", None, NodeKind::Text(TextNode { content: "GOO".into(), font, size_px: 170.0, fill_color: [1.0; 4], gradient: None })), "goo");
    text.transform.translation = Vec2::new(-150.0, 20.0);
    text.behaviors.push(an1mbyt3_core::NodeBehaviorEntry { behavior: goo(1, hot.clone(), rim.clone(), 7.0, 34.0), enabled: true });
    add_node(document, &mut inverses, scene, text);
    for i in 0..2 {
        add_node(document, &mut inverses, scene, shape(&format!("ball{i}"), Path::ellipse(70.0, 70.0), Vec2::new(-150.0, 20.0), vec![goo(1, hot.clone(), rim.clone(), 7.0, 34.0)]));
    }
    // Drips: drops fall from under the letters.
    let mut drip = ParticleSystemNode::default();
    drip.main.looping = true;
    drip.main.duration = 5.0;
    drip.main.start_lifetime = ParticleRange { min: 1.4, max: 2.0 };
    drip.main.start_speed = ParticleRange { min: 0.0, max: 15.0 };
    drip.main.start_size = ParticleRange { min: 26.0, max: 38.0 };
    drip.main.gravity_modifier = 260.0;
    drip.main.drag = 0.6;
    drip.main.simulation_space = SimulationSpace::World;
    drip.main.max_particles = 40;
    drip.emission.rate_over_time = 3.0;
    drip.shape = EmitterShape::Box { width: 280.0, height: 4.0 };
    drip.color_over_lifetime = [(0.0, 0.0), (0.15, 1.0), (0.8, 1.0), (1.0, 0.0)].into_iter().map(|(offset, a)| GradientStop { offset, color: [1.0, 1.0, 1.0, a], midpoint: 0.5 }).collect();
    let mut drips = with_id(Node::with_components("Drips", None, vec![NodeBehavior::ParticleSystem(drip), goo(1, hot.clone(), rim.clone(), 7.0, 34.0)]), "drips");
    drips.transform.translation = Vec2::new(-150.0, -20.0);
    add_node(document, &mut inverses, scene, drips);

    // ---- Group 2: two squares that fuse and part ----
    let cool = gradient(Vec2::new(-90.0, -90.0), Vec2::new(90.0, 90.0), &[[0.1, 0.85, 0.75, 1.0], [0.35, 0.4, 1.0, 1.0], [0.75, 0.25, 1.0, 1.0]]);
    let cool_rim = gradient(Vec2::new(-90.0, 0.0), Vec2::new(90.0, 0.0), &[[0.02, 0.25, 0.3, 1.0], [0.2, 0.05, 0.35, 1.0]]);
    for (i, x) in [(0, 220.0), (1, 380.0)] {
        add_node(document, &mut inverses, scene, shape(&format!("square{i}"), rounded_square(110.0, 22.0), Vec2::new(x, 20.0), vec![goo(2, cool.clone(), cool_rim.clone(), 6.0, 40.0)]));
    }

    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "hint", "GOO 2D: SHAPES, TEXT AND PARTICLES IN ONE GROUP MELT TOGETHER", font, 13.0, [0.35, 0.3, 0.35, 1.0], Vec2::new(0.0, -SCREEN_H / 2.0 + 30.0)));

    // ---- Script: orbit the balls round the text, breathe the squares ----
    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Goo (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Goo".into(),
            inputs: Vec::new(),
            states: vec![StateDef { name: "Play".into(), animation, loop_mode: LoopMode::Once, transitions: Vec::new(), position: Vec2::ZERO, blend: None, program: Some(ProgramDef::Code(SCRIPT.into())) }],
            initial_state: "Play".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));

    inverses.reverse();
    document.commit_batch(inverses);
    GooShapesDemo { scene }
}

const SCRIPT: &str = r#"// Goo Shapes — balls orbit the GOO text and melt into it; squares breathe.
const t = ctx.time
const ref = (id) => getCompByID(id)
for (let i = 0; i < 2; i++) {
  const a = t * 0.9 + i * Math.PI
  const b = ref('ball' + i)
  // An ellipse that grazes the letters: they merge as they pass close.
  b.x = -150 + Math.cos(a) * 205
  b.y = 25 + Math.sin(a) * 105
}
const gap = 80 + Math.sin(t * 1.3) * 52
ref('square0').x = 300 - gap
ref('square1').x = 300 + gap
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_mix_text_shapes_and_particles() {
        let mut document = Document::new();
        let demo = build_demo_goo_shapes(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let in_group = |g: u32| artboard.node_ids().filter(|id| artboard.node(*id).unwrap().goo().is_some_and(|goo| goo.group == g)).count();
        assert_eq!(in_group(1), 4, "text + 2 balls + drips");
        assert_eq!(in_group(2), 2);
    }
}
