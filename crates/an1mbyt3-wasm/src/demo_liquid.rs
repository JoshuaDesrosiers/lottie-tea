//! "Liquid Lab" — a showcase for the Liquid 2D component.
//!
//! - A glass tank of water with wooden crates bobbing on it and a stone
//!   block resting on the bottom: buoyancy is Archimedes' (liquid density x
//!   displaced area), so light-for-their-size bodies float and dense ones
//!   sink.
//! - A faucet pouring thick amber honey (high viscosity) into a funnel that
//!   drains into a basin, where a beach ball floats.
//! - Click to drop a crate (Shift-click: a stone) — spawned from prefabs,
//!   so the splash, the ripples and the floating all happen live.
//!
//! The containers are ordinary static Box Collider 2D shapes: Liquid 2D
//! flows around any collider, so any level geometry holds liquid.

use an1mbyt3_core::{
    Animation, ArtboardId, BoxCollider2D, CameraNode, CircleCollider2D, Command, Document, FontAsset, Liquid2D, LoopMode, Node, NodeBehavior, NodeBehaviorEntry,
    NodeKind, Path, PrefabDefinition, PrefabNode, ProgramDef, Rigidbody2D, ShapeNode, StateDef, StateMachineDef, Stroke,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");
const SCREEN_W: f32 = 480.0;
const SCREEN_H: f32 = 270.0;

pub(crate) struct LiquidDemo {
    pub scene: ArtboardId,
}

fn shape(name: &str, path: Path, color: [f32; 4], at: Vec2, rotation_degrees: f32, components: Vec<NodeBehavior>) -> Node {
    let mut node = Node::new(name, None, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None }));
    node.transform.translation = at;
    node.transform.rotation = rotation_degrees.to_radians();
    node.behaviors = components.into_iter().map(|behavior| NodeBehaviorEntry { behavior, enabled: true }).collect();
    node
}

fn with_stroke(mut node: Node, width: f32, color: [f32; 4]) -> Node {
    if let NodeKind::Shape(s) = &mut node.kind {
        s.stroke = Some(Stroke { width, color, gradient: None, image: None, cap: Default::default(), join: Default::default() });
    }
    node
}

/// Static scenery with a box collider the size of its shape.
fn wall(name: &str, w: f32, h: f32, at: Vec2, rotation_degrees: f32, color: [f32; 4]) -> Node {
    with_stroke(
        shape(name, Path::rect(w, h), color, at, rotation_degrees, vec![NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::new(w, h), ..Default::default() })]),
        1.0,
        [1.0, 1.0, 1.0, 0.25],
    )
}

fn crate_node(mass: f32, size: f32, color: [f32; 4]) -> Node {
    with_stroke(
        shape(
            if mass > 2.0 { "Stone" } else { "Crate" },
            Path::rect(size, size),
            color,
            Vec2::ZERO,
            0.0,
            vec![NodeBehavior::Rigidbody2D(Rigidbody2D { mass, ..Default::default() }), NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::splat(size), ..Default::default() })],
        ),
        1.5,
        [0.0, 0.0, 0.0, 0.45],
    )
}

fn add_prefab(document: &mut Document, inverses: &mut Vec<Command>, name: &str, mut node: Node) {
    node.id = String::new();
    let definition = PrefabDefinition { name: name.into(), nodes: vec![PrefabNode { uid: 0, parent: None, node }], next_uid: 1, deleted: false };
    inverses.push(Command::AddPrefab { definition }.apply(document));
}

pub(crate) fn build_demo_liquid(document: &mut Document) -> LiquidDemo {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: FONT_BYTES.to_vec() });
    let inverse = document.apply(Command::AddArtboard { name: "Liquid Lab".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    let camera = with_id(
        Node::with_components(
            "Main Camera",
            None,
            vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.07, 0.09, 0.14, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })],
        ),
        "camera",
    );
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));

    let (half_w, half_h) = (SCREEN_W / 2.0, SCREEN_H / 2.0);
    let stone = [0.32, 0.34, 0.42, 1.0];
    let glass = [0.6, 0.8, 0.95, 0.35];
    // Room.
    add_node(document, &mut inverses, scene, wall("Floor", SCREEN_W, 20.0, Vec2::new(0.0, -half_h + 10.0), 0.0, stone));
    add_node(document, &mut inverses, scene, wall("Left Wall", 14.0, SCREEN_H, Vec2::new(-half_w + 7.0, 0.0), 0.0, stone));
    add_node(document, &mut inverses, scene, wall("Right Wall", 14.0, SCREEN_H, Vec2::new(half_w - 7.0, 0.0), 0.0, stone));

    // ---- The water tank (left) ----
    let tank_floor = -half_h + 20.0;
    // The glass sits flush against the room's left wall (no slot for
    // splashes to get lost in), with a rim well above the waterline.
    add_node(document, &mut inverses, scene, wall("Tank Left", 10.0, 140.0, Vec2::new(-221.0, tank_floor + 70.0), 0.0, glass));
    add_node(document, &mut inverses, scene, wall("Tank Right", 10.0, 140.0, Vec2::new(-30.0, tank_floor + 70.0), 0.0, glass));
    let water = Liquid2D { size: Vec2::new(170.0, 64.0), particle_spacing: 6.0, color: [0.18, 0.5, 0.98, 0.92], viscosity: 0.08, buoyancy: 2.5, max_particles: 1100, ..Liquid2D::default() };
    let mut water_node = with_id(Node::with_components("Water", None, vec![NodeBehavior::Liquid2D(water)]), "water");
    water_node.transform.translation = Vec2::new(-126.0, tank_floor + 34.0);
    add_node(document, &mut inverses, scene, water_node);
    for (i, x) in [-170.0f32, -112.0, -66.0].into_iter().enumerate() {
        let mut c = crate_node(1.0, 22.0, [0.72, 0.48, 0.22, 1.0]);
        c.transform.translation = Vec2::new(x, tank_floor + 86.0 + i as f32 * 12.0);
        add_node(document, &mut inverses, scene, with_id(c, &format!("crate{i}")));
    }
    let mut block = crate_node(5.0, 22.0, [0.5, 0.52, 0.58, 1.0]);
    block.transform.translation = Vec2::new(-140.0, tank_floor + 150.0);
    add_node(document, &mut inverses, scene, with_id(block, "stone"));

    // ---- Honey faucet, funnel and basin (right) ----
    let pipe = with_stroke(shape("Faucet", Path::rect(26.0, 10.0), [0.55, 0.58, 0.66, 1.0], Vec2::new(122.0, 104.0), 0.0, Vec::new()), 1.0, [0.0, 0.0, 0.0, 0.4]);
    add_node(document, &mut inverses, scene, pipe);
    add_node(document, &mut inverses, scene, shape("Faucet Pipe", Path::rect(8.0, 40.0), [0.55, 0.58, 0.66, 1.0], Vec2::new(122.0, 124.0), 0.0, Vec::new()));
    let honey = Liquid2D {
        size: Vec2::ZERO,
        particle_spacing: 6.0,
        color: [1.0, 0.62, 0.12, 0.95],
        viscosity: 0.75,
        stiffness: 0.8,
        buoyancy: 3.0,
        drag: 5.0,
        emit_rate: 70.0,
        emit_velocity: Vec2::new(0.0, -60.0),
        max_particles: 520,
    };
    let mut honey_node = with_id(Node::with_components("Honey", None, vec![NodeBehavior::Liquid2D(honey)]), "honey");
    honey_node.transform.translation = Vec2::new(122.0, 96.0);
    add_node(document, &mut inverses, scene, honey_node);
    add_node(document, &mut inverses, scene, wall("Funnel L", 70.0, 7.0, Vec2::new(94.0, 40.0), -32.0, stone));
    add_node(document, &mut inverses, scene, wall("Funnel R", 70.0, 7.0, Vec2::new(152.0, 40.0), 32.0, stone));
    add_node(document, &mut inverses, scene, wall("Basin L", 8.0, 70.0, Vec2::new(30.0, tank_floor + 35.0), 0.0, glass));
    let mut ball = shape("Beach Ball", Path::ellipse(24.0, 24.0), [1.0, 0.3, 0.35, 1.0], Vec2::new(150.0, tank_floor + 80.0), 0.0, vec![
        NodeBehavior::Rigidbody2D(Rigidbody2D { mass: 0.4, ..Default::default() }),
        NodeBehavior::CircleCollider2D(CircleCollider2D { radius: 12.0, ..Default::default() }),
    ]);
    ball = with_stroke(ball, 2.0, [1.0, 1.0, 1.0, 0.9]);
    add_node(document, &mut inverses, scene, with_id(ball, "ball"));

    // Prefabs the click spawns.
    add_prefab(document, &mut inverses, "Crate", crate_node(1.0, 22.0, [0.72, 0.48, 0.22, 1.0]));
    add_prefab(document, &mut inverses, "Stone", crate_node(5.0, 22.0, [0.5, 0.52, 0.58, 1.0]));

    // HUD.
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "hint", "CLICK: DROP CRATE   SHIFT+CLICK: STONE", font, 8.0, [0.8, 0.88, 1.0, 1.0], Vec2::new(0.0, half_h - 14.0)));
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "labels", "WATER                         HONEY", font, 8.0, [0.6, 0.7, 0.85, 0.8], Vec2::new(-10.0, half_h - 30.0)));

    // Script: click to spawn.
    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Lab (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Liquid Lab".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Play".into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(LAB_SCRIPT.into())),
            }],
            initial_state: "Play".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));

    inverses.reverse();
    document.commit_batch(inverses);
    LiquidDemo { scene }
}

const LAB_SCRIPT: &str = r#"// Liquid Lab — click to drop a crate (Shift: a stone) into the liquids.
const g = ctx.globals
const keys = ctx.keysDown
const down = keys.has('mouse')
if (down && !g.wasDown && ctx.mouseY < 110) {
  const heavy = keys.has('shift')
  const box = ctx.spawn(heavy ? 'Stone' : 'Crate', ctx.mouseX, ctx.mouseY)
  if (box) box.rotation = Math.random() * 40 - 20
}
g.wasDown = down
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_lab_with_two_liquids_and_spawnable_bodies() {
        let mut document = Document::new();
        let demo = build_demo_liquid(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let liquids = artboard.node_ids().filter_map(|id| artboard.node(id)).filter(|n| n.liquid().is_some()).count();
        assert_eq!(liquids, 2);
        assert_eq!(document.prefabs().map(|(_, p)| p.name.as_str()).collect::<Vec<_>>(), ["Crate", "Stone"]);
    }
}
