//! "Physics Playground" — a small showcase for the Unity-style physics
//! components (`Rigidbody 2D`, `Box`/`Circle`/`Capsule Collider 2D`, trigger
//! colliders) and the script physics API (`ref.velocity`, `ctx.raycast`,
//! `ctx.collisions`, `spawnClone`).
//!
//! Everything here is ordinary nodes with components attached — no special
//! node kinds: the ground and ramp are Shapes with a Box Collider 2D (no
//! Rigidbody 2D, so they're static scenery), crates/balls are Shapes with a
//! Rigidbody 2D plus a collider, the player is a Shape with a frozen-rotation
//! Rigidbody 2D and a Capsule Collider 2D, and the goal is a trigger.
//!
//! Controls: A/D (or arrows) to run, Space/W to jump (only when a downward
//! raycast finds ground), click to drop a bouncy ball at the cursor.

use an1mbyt3_core::{
    Animation, ArtboardId, BoxCollider2D, CameraNode, CapsuleCollider2D, CircleCollider2D, Command, Document, FontAsset, LoopMode, Node, NodeBehavior,
    NodeBehaviorEntry, NodeKind, Path, PhysicsMaterial2D, ProgramDef, Rigidbody2D, ShapeNode, StateDef, StateMachineDef,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, text_node, with_id};

const PIXEL_FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");

const SCREEN_W: f32 = 480.0;
const SCREEN_H: f32 = 270.0;

pub(crate) struct PhysicsDemo {
    pub scene: ArtboardId,
}

/// A filled Shape (`path`) at `at`, with `components` attached.
fn body(name: &str, path: Path, color: [f32; 4], at: Vec2, components: Vec<NodeBehavior>) -> Node {
    let mut node = Node::new(name, None, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None }));
    node.transform.translation = at;
    node.behaviors = components.into_iter().map(|behavior| NodeBehaviorEntry { behavior, enabled: true }).collect();
    node
}

fn box_collider(w: f32, h: f32) -> NodeBehavior {
    NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::new(w, h), ..Default::default() })
}

/// Static scenery block: a Box Collider 2D and no Rigidbody 2D.
fn platform(name: &str, w: f32, h: f32, at: Vec2, rotation_degrees: f32) -> Node {
    let mut node = body(name, Path::rect(w, h), [0.22, 0.25, 0.33, 1.0], at, vec![box_collider(w, h)]);
    node.transform.rotation = rotation_degrees.to_radians();
    node
}

pub(crate) fn build_demo_physics(document: &mut Document) -> PhysicsDemo {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: PIXEL_FONT_BYTES.to_vec() });

    let inverse = document.apply(Command::AddArtboard { name: "Physics Playground".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    // Camera framing the whole room.
    let camera = Node::with_components(
        "Main Camera",
        None,
        vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.07, 0.08, 0.11, 1.0], ..CameraNode::default() })],
    );
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));

    // Room: floor, walls, a ramp and a ledge.
    let half_w = SCREEN_W / 2.0;
    let half_h = SCREEN_H / 2.0;
    add_node(document, &mut inverses, scene, platform("Ground", SCREEN_W, 20.0, Vec2::new(0.0, -half_h + 10.0), 0.0));
    add_node(document, &mut inverses, scene, platform("Left Wall", 16.0, SCREEN_H, Vec2::new(-half_w + 8.0, 0.0), 0.0));
    add_node(document, &mut inverses, scene, platform("Right Wall", 16.0, SCREEN_H, Vec2::new(half_w - 8.0, 0.0), 0.0));
    add_node(document, &mut inverses, scene, platform("Ramp", 150.0, 12.0, Vec2::new(120.0, -88.0), 20.0));
    add_node(document, &mut inverses, scene, platform("Ledge", 110.0, 12.0, Vec2::new(-150.0, -20.0), 0.0));

    // Goal: a trigger — detects the player (ctx.collisions) but doesn't push.
    let mut goal_trigger = BoxCollider2D { size: Vec2::new(40.0, 40.0), ..Default::default() };
    goal_trigger.is_trigger = true;
    add_node(
        document,
        &mut inverses,
        scene,
        with_id(body("Goal", Path::rect(40.0, 40.0), [0.95, 0.8, 0.2, 0.35], Vec2::new(-150.0, 6.0), vec![NodeBehavior::BoxCollider2D(goal_trigger)]), "goal"),
    );

    // Stack of crates.
    for (i, at) in [Vec2::new(10.0, -110.0), Vec2::new(36.0, -110.0), Vec2::new(23.0, -84.0), Vec2::new(-60.0, 40.0)].into_iter().enumerate() {
        let mut crate_node = body(
            "Crate",
            Path::rect(24.0, 24.0),
            [0.72, 0.48, 0.26, 1.0],
            at,
            vec![NodeBehavior::Rigidbody2D(Rigidbody2D { mass: 2.0, ..Default::default() }), box_collider(24.0, 24.0)],
        );
        crate_node.classes = vec!["crate".into()];
        add_node(document, &mut inverses, scene, with_id(crate_node, &format!("crate{i}")));
    }

    // The ball template the script clones on click (starts as a real ball).
    let bouncy = PhysicsMaterial2D { friction: 0.2, bounciness: 0.7 };
    add_node(
        document,
        &mut inverses,
        scene,
        with_id(
            body(
                "Ball",
                Path::ellipse(16.0, 16.0),
                [0.35, 0.75, 1.0, 1.0],
                Vec2::new(150.0, 80.0),
                vec![
                    NodeBehavior::Rigidbody2D(Rigidbody2D { mass: 0.5, ..Default::default() }),
                    NodeBehavior::CircleCollider2D(CircleCollider2D { radius: 8.0, material: bouncy, ..Default::default() }),
                ],
            ),
            "ball",
        ),
    );

    // Player: capsule body that never tips over.
    add_node(
        document,
        &mut inverses,
        scene,
        with_id(
            body(
                "Player",
                Path::rect(16.0, 28.0),
                [0.45, 0.95, 0.55, 1.0],
                Vec2::new(-180.0, -100.0),
                vec![
                    NodeBehavior::Rigidbody2D(Rigidbody2D { mass: 1.0, freeze_rotation: true, ..Default::default() }),
                    NodeBehavior::CapsuleCollider2D(CapsuleCollider2D { size: Vec2::new(16.0, 28.0), material: PhysicsMaterial2D { friction: 0.0, bounciness: 0.0 }, ..Default::default() }),
                ],
            ),
            "player",
        ),
    );

    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "hint", "A/D RUN  SPACE JUMP  CLICK BALL", font, 8.0, [0.75, 0.8, 0.95, 1.0], Vec2::new(0.0, half_h - 14.0)));
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "status", "", font, 8.0, [1.0, 0.85, 0.35, 1.0], Vec2::new(0.0, half_h - 30.0)));

    let animation_inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Physics filler") });
    let Command::SetAnimationDeleted { animation, .. } = animation_inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(animation_inverse);
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Physics".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Playing".into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(PHYSICS_SCRIPT.to_string())),
            }],
            initial_state: "Playing".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));

    inverses.reverse();
    document.commit_batch(inverses);
    PhysicsDemo { scene }
}

const PHYSICS_SCRIPT: &str = r#"// Physics Playground — Rigidbody 2D + colliders driven from script.
const g = ctx.globals
if (g.goals === undefined) { g.goals = 0; g.balls = []; g.jumpHeld = false; g.clickHeld = false }
const keys = ctx.keysDown
const player = getCompByID('player')

if (player && player.velocity) {
  // Run: steer horizontal velocity toward the target speed, keep gravity's y.
  const dir = ((keys.has('d') || keys.has('right')) ? 1 : 0) - ((keys.has('a') || keys.has('left')) ? 1 : 0)
  const v = player.velocity
  const k = Math.min(1, ctx.deltaSeconds * 12)
  player.velocity = { x: v.x + (dir * 170 - v.x) * k, y: v.y }

  // Jump only when a short ray straight down finds something to stand on.
  const ground = ctx.raycast(player.x, player.y, 0, -1, 18, player)
  const jump = keys.has('space') || keys.has('w') || keys.has('up')
  if (jump && !g.jumpHeld && ground) player.addImpulse(0, 340)
  g.jumpHeld = jump
}

// Click: drop a bouncy ball at the cursor (keep the newest 25).
const click = keys.has('mouse')
if (click && !g.clickHeld) {
  const template = getCompByID('ball')
  const ball = template && spawnClone(template)
  if (ball) {
    ball.x = ctx.mouseX
    ball.y = ctx.mouseY
    ball.fillColor = [0.3 + Math.random() * 0.7, 0.4 + Math.random() * 0.6, 1, 1]
    g.balls.push(ball)
    if (g.balls.length > 25) deleteNode(g.balls.shift())
  }
}
g.clickHeld = click

// Trigger: count every time the player enters the yellow goal box.
for (const c of ctx.collisions) {
  const ids = [c.a && c.a.id, c.b && c.b.id]
  if (c.started && c.trigger && ids.includes('player') && ids.includes('goal')) g.goals += 1
}
const status = getCompByID('status')
const text = g.goals > 0 ? 'GOAL x' + g.goals : 'REACH THE YELLOW BOX'
if (status && status.textContent !== text) status.textContent = text
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_scene_with_physics_components() {
        let mut document = Document::new();
        let demo = build_demo_physics(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let bodies = artboard.node_ids().filter(|id| artboard.node(*id).is_some_and(|n| n.rigidbody().is_some())).count();
        // 4 crates + ball + player.
        assert_eq!(bodies, 6);
        let colliders: usize = artboard.node_ids().filter_map(|id| artboard.node(id)).map(|n| n.colliders().count()).sum();
        // 5 platforms + goal + 4 crates + ball + player.
        assert_eq!(colliders, 12);
        assert!(artboard.active_camera().is_some());
    }

    #[test]
    fn player_lands_on_ground() {
        let mut document = Document::new();
        let demo = build_demo_physics(&mut document);
        let artboard = document.artboard_mut(demo.scene).unwrap();
        let player = artboard.node_ids().find(|id| artboard.node(*id).is_some_and(|n| n.id == "player")).unwrap();
        let mut world = an1mbyt3_physics::PhysicsWorld::new();
        for _ in 0..180 {
            world.sync(artboard);
            world.step(artboard, 1.0 / 60.0);
        }
        let y = artboard.node(player).unwrap().transform.translation.y;
        // Ground (center -125, 20 tall) tops out at -115; the 28px player rests
        // with its center 14 above that.
        assert!((y - (-101.0)).abs() < 2.0, "player y = {y}");
    }
}
