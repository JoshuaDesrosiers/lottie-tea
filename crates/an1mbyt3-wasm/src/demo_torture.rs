//! "Ragdoll Torture" — a slapstick physics sandbox with a crash-test dummy.
//!
//! The dummy is an eleven-bone ragdoll (Rigidbody 2D + Capsule Collider 2D +
//! Hinge Joint 2D per bone, the same rig recipe as the Ragdoll Arena demo)
//! saved as a **prefab**, so R can throw it away and spawn a fresh one.
//! The room is full of things to put it through:
//!
//! - **Spikes** on the floor and a spinning **saw** (a kinematic body the
//!   script rotates) hurt while touched (`ctx.collisions` start/stop).
//! - A **crusher** (kinematic, script-driven) slams down on a cycle.
//! - A **bounce pad** launches whatever lands on it.
//! - Tools (1-5 or click the buttons): GRAB a limb and fling it, PUNCH,
//!   drop a BOMB (fused prefab, radial blast), drop an ANVIL, fire the wall
//!   CANNON at the cursor.
//!
//! - Limbs **break off** (`ref.breakJoint()`) once they're battered enough
//!   and hit hard again, ripped by the grab tool, blown off by a bomb or
//!   ground down by the saw — the stump and the loose limb then bleed.
//! - **Blood**: pooled world-space Particle Systems the script moves to a
//!   wound and bursts (`ref.emit`), plus splat decals on the floor.
//!
//! The script scores PAIN from impacts — a sudden change in a limb's
//! velocity — plus contact with the hazards; limbs bruise as they take
//! damage, the face winces, and the best score is kept in `ctx.storage`.

use an1mbyt3_core::{
    Animation, ArtboardId, BoneNode, BoxCollider2D, CameraNode, CapsuleCollider2D, CapsuleDirection2D, CircleCollider2D, Command, Document, EmitterShape, FontAsset,
    GradientStop, HingeJoint2D, LoopMode, Node, NodeBehavior, NodeBehaviorEntry, NodeKind, ParticleRange, ParticleSystemNode, Path, PathCommand, PrefabDefinition,
    PrefabNode, ProgramDef, Rigidbody2D, RigidbodyType2D, ShapeNode, SimulationSpace, StateDef, StateMachineDef, Stroke,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");
const SCREEN_W: f32 = 480.0;
const SCREEN_H: f32 = 270.0;
/// Top of the floor (world y).
const FLOOR_Y: f32 = -125.0;
/// HUD bar layout (camera space): tool buttons at TOOL_X0 + k * TOOL_STEP,
/// then +DUMMY and RESET. The script hit-tests the same numbers.
const TOOL_X0: f32 = -198.0;
const TOOL_STEP: f32 = 66.0;
const ADD_X: f32 = 130.0;
const ADD_W: f32 = 54.0;
const RESET_X: f32 = 189.0;
const RESET_W: f32 = 52.0;

/// Dummy colours: crash-test yellow, dark joints, black target marks.
const YELLOW: [f32; 4] = [1.0, 0.8, 0.18, 1.0];
const JOINT: [f32; 4] = [0.22, 0.22, 0.26, 1.0];
const MARK: [f32; 4] = [0.08, 0.08, 0.1, 1.0];

pub(crate) struct TortureDemo {
    pub scene: ArtboardId,
}

fn entries(behaviors: Vec<NodeBehavior>) -> Vec<NodeBehaviorEntry> {
    behaviors.into_iter().map(|behavior| NodeBehaviorEntry { behavior, enabled: true }).collect()
}

fn shape(name: &str, path: Path, color: [f32; 4], at: Vec2) -> Node {
    let mut node = Node::new(name, None, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None }));
    node.transform.translation = at;
    node
}

fn stroked(mut node: Node, width: f32, color: [f32; 4]) -> Node {
    if let NodeKind::Shape(s) = &mut node.kind {
        s.stroke = Some(Stroke { width, color, gradient: None, image: None, cap: Default::default(), join: Default::default() });
    }
    node
}

fn with(mut node: Node, behaviors: Vec<NodeBehavior>) -> Node {
    node.behaviors = entries(behaviors);
    node
}

/// Static scenery with a box collider the size of its shape.
fn wall(name: &str, w: f32, h: f32, at: Vec2, color: [f32; 4]) -> Node {
    with(stroked(shape(name, Path::rect(w, h), color, at), 1.0, [1.0, 1.0, 1.0, 0.15]), vec![NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::new(w, h), ..Default::default() })])
}

/// A row of `n` spike triangles, `w` wide and `h` tall, sitting on y = 0.
fn spike_path(n: usize, w: f32, h: f32) -> Path {
    let step = w / n as f32;
    let mut commands = vec![PathCommand::MoveTo(Vec2::new(-w / 2.0, 0.0))];
    for i in 0..n {
        let x0 = -w / 2.0 + step * i as f32;
        commands.push(PathCommand::LineTo(Vec2::new(x0 + step / 2.0, h)));
        commands.push(PathCommand::LineTo(Vec2::new(x0 + step, 0.0)));
    }
    commands.push(PathCommand::Close);
    Path { commands }
}

/// A saw blade: a star with many shallow teeth.
fn saw_path(radius: f32) -> Path {
    Path::star(14, radius, radius * 0.8)
}

// ------------------------------------------------------------------ dummy --

/// Builds the dummy prefab's node list (root first). Every bone is named
/// (the script looks parts up with `dummy.part(name)`), and every bone's
/// visible "skin" child is `<bone>Skin` so bruising can recolour it.
struct PrefabBuilder {
    nodes: Vec<PrefabNode>,
}

impl PrefabBuilder {
    fn push(&mut self, parent: Option<u32>, mut node: Node) -> u32 {
        let uid = self.nodes.len() as u32;
        node.id = String::new();
        node.parent = None;
        self.nodes.push(PrefabNode { uid, parent, node });
        uid
    }

    /// One ragdoll bone: rigidbody, capsule along the bone, hinge to its
    /// parent bone (non-root), a skin bar and a joint cap.
    #[allow(clippy::too_many_arguments)]
    fn bone(&mut self, parent: Option<u32>, name: &str, length: f32, width: f32, mass: f32, at: Vec2, degrees: f32, limits: Option<(f32, f32)>) -> u32 {
        let mut node = Node::new(name, None, NodeKind::Bone(BoneNode { length }));
        node.transform.translation = at;
        node.transform.rotation = degrees.to_radians();
        let mut behaviors = vec![
            NodeBehavior::Rigidbody2D(Rigidbody2D { body_type: RigidbodyType2D::Dynamic, mass, angular_drag: 0.6, linear_drag: 0.05, ..Default::default() }),
            NodeBehavior::CapsuleCollider2D(CapsuleCollider2D {
                offset: Vec2::new(length / 2.0, 0.0),
                size: Vec2::new(length.max(width), width),
                direction: CapsuleDirection2D::Horizontal,
                ..Default::default()
            }),
        ];
        if let Some((lower, upper)) = limits {
            behaviors.push(NodeBehavior::HingeJoint2D(HingeJoint2D { use_limits: true, lower_angle: lower, upper_angle: upper, ..Default::default() }));
        }
        node.behaviors = entries(behaviors);
        let uid = self.push(parent, node);
        self.push(Some(uid), stroked(shape(&format!("{name}Skin"), Path::rect(length + 2.0, width), YELLOW, Vec2::new(length / 2.0, 0.0)), 1.0, MARK));
        self.push(Some(uid), shape(&format!("{name}Joint"), Path::ellipse(width * 0.7, width * 0.7), JOINT, Vec2::ZERO));
        uid
    }
}

fn dummy_prefab() -> PrefabDefinition {
    let mut b = PrefabBuilder { nodes: Vec::new() };
    // Hips point straight up (90°): a bone's +x runs along it, so the chest
    // continues upward and "+y" in a torso bone's space is the dummy's left.
    let hips = b.bone(None, "Hips", 16.0, 16.0, 3.0, Vec2::ZERO, 90.0, None);
    // Legs first so they draw behind the torso.
    for (side, y, spread) in [("L", 5.0f32, -8.0f32), ("R", -5.0, 8.0)] {
        let thigh = b.bone(Some(hips), &format!("Thigh{side}"), 19.0, 9.0, 1.5, Vec2::new(0.0, y), 180.0 + spread, Some((-100.0, 100.0)));
        let shin = b.bone(Some(thigh), &format!("Shin{side}"), 18.0, 8.0, 1.0, Vec2::new(19.0, 0.0), 0.0, Some((-130.0, 130.0)));
        b.push(Some(shin), shape(&format!("Foot{side}"), Path::rect(6.0, 11.0), JOINT, Vec2::new(19.0, 0.0)));
    }
    let chest = b.bone(Some(hips), "Chest", 22.0, 18.0, 2.5, Vec2::new(16.0, 0.0), 0.0, Some((-40.0, 40.0)));
    // The crash-test target marks on the chest.
    b.push(Some(chest), shape("ChestMark", Path::ellipse(9.0, 9.0), MARK, Vec2::new(13.0, 0.0)));
    b.push(Some(chest), shape("ChestMarkIn", Path::ellipse(4.0, 4.0), YELLOW, Vec2::new(13.0, 0.0)));
    let head = b.bone(Some(chest), "Head", 15.0, 15.0, 1.2, Vec2::new(22.0, 0.0), 0.0, Some((-45.0, 45.0)));
    // The face (head-local: +x = up, +y = the dummy's left).
    b.push(Some(head), stroked(shape("Face", Path::ellipse(17.0, 17.0), YELLOW, Vec2::new(8.0, 0.0)), 1.0, MARK));
    b.push(Some(head), shape("EyeL", Path::ellipse(3.0, 3.0), MARK, Vec2::new(10.0, 3.5)));
    b.push(Some(head), shape("EyeR", Path::ellipse(3.0, 3.0), MARK, Vec2::new(10.0, -3.5)));
    b.push(Some(head), shape("Mouth", Path::rect(2.0, 5.0), MARK, Vec2::new(4.5, 0.0)));
    for (side, y, swing) in [("L", 9.0f32, 160.0f32), ("R", -9.0, 200.0)] {
        let upper = b.bone(Some(chest), &format!("UpperArm{side}"), 15.0, 7.0, 0.8, Vec2::new(19.0, y), swing, Some((-170.0, 170.0)));
        let fore = b.bone(Some(upper), &format!("Forearm{side}"), 14.0, 6.0, 0.6, Vec2::new(15.0, 0.0), 0.0, Some((-145.0, 145.0)));
        b.push(Some(fore), shape(&format!("Hand{side}"), Path::ellipse(8.0, 8.0), JOINT, Vec2::new(15.0, 0.0)));
    }
    let next_uid = b.nodes.len() as u32;
    PrefabDefinition { name: "Dummy".into(), nodes: b.nodes, next_uid, deleted: false }
}

/// A single-node prefab (props the tools drop).
fn single_prefab(name: &str, node: Node) -> PrefabDefinition {
    let mut node = node;
    node.id = String::new();
    PrefabDefinition { name: name.into(), nodes: vec![PrefabNode { uid: 0, parent: None, node }], next_uid: 1, deleted: false }
}

fn prop_prefabs() -> Vec<PrefabDefinition> {
    let dynamic = |mass: f32| NodeBehavior::Rigidbody2D(Rigidbody2D { mass, ..Default::default() });
    // Bomb: a round body with a fuse; the script lights it and blows it up.
    let mut bomb_root = with(stroked(shape("Bomb", Path::ellipse(14.0, 14.0), [0.12, 0.12, 0.15, 1.0], Vec2::ZERO), 1.0, [0.5, 0.5, 0.6, 1.0]), vec![
        dynamic(1.0),
        NodeBehavior::CircleCollider2D(CircleCollider2D { radius: 7.0, ..Default::default() }),
    ]);
    bomb_root.id = String::new();
    let mut fuse = shape("Fuse", Path::rect(2.0, 5.0), [0.75, 0.6, 0.4, 1.0], Vec2::new(0.0, 9.0));
    fuse.id = String::new();
    let mut spark = shape("Spark", Path::star(5, 3.5, 1.5), [1.0, 0.8, 0.2, 1.0], Vec2::new(0.0, 12.0));
    spark.id = String::new();
    spark.unlit = true;
    let bomb = PrefabDefinition {
        name: "Bomb".into(),
        nodes: vec![PrefabNode { uid: 0, parent: None, node: bomb_root }, PrefabNode { uid: 1, parent: Some(0), node: fuse }, PrefabNode { uid: 2, parent: Some(0), node: spark }],
        next_uid: 3,
        deleted: false,
    };
    // Anvil: a heavy flat block (the cartoon classic).
    let anvil = with(stroked(shape("Anvil", Path::rect(32.0, 16.0), [0.3, 0.32, 0.38, 1.0], Vec2::ZERO), 1.0, [0.65, 0.68, 0.75, 1.0]), vec![
        dynamic(25.0),
        NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::new(32.0, 16.0), ..Default::default() }),
    ]);
    // Cannonball.
    let ball = with(stroked(shape("Ball", Path::ellipse(12.0, 12.0), [0.18, 0.18, 0.22, 1.0], Vec2::ZERO), 1.0, [0.55, 0.55, 0.62, 1.0]), vec![
        dynamic(4.0),
        NodeBehavior::CircleCollider2D(CircleCollider2D { radius: 6.0, ..Default::default() }),
    ]);
    vec![dummy_prefab(), bomb, single_prefab("Anvil", anvil), single_prefab("Ball", ball)]
}

/// How many blood particle systems / floor splats the script can use.
const BLOOD_EMITTERS: usize = 6;
const BLOOD_SPLATS: usize = 40;

/// A blood burst: fast red droplets that fall and fade.
fn blood_emitter(id: &str) -> Node {
    let mut config = ParticleSystemNode::default();
    config.main.looping = false;
    config.main.duration = 0.1;
    config.main.start_lifetime = ParticleRange { min: 0.45, max: 1.0 };
    config.main.start_speed = ParticleRange { min: 50.0, max: 190.0 };
    config.main.start_size = ParticleRange { min: 1.5, max: 3.8 };
    config.main.start_color = [0.78, 0.04, 0.08, 1.0];
    config.main.gravity_modifier = 1.0;
    config.main.drag = 0.4;
    config.main.simulation_space = SimulationSpace::World;
    config.main.max_particles = 260;
    config.emission.rate_over_time = 0.0;
    config.emission.bursts.clear();
    config.shape = EmitterShape::Circle { radius: 3.0 };
    config.color_over_lifetime = vec![
        GradientStop { offset: 0.0, color: [1.0, 1.0, 1.0, 1.0], midpoint: 0.5 },
        GradientStop { offset: 0.7, color: [0.75, 0.6, 0.6, 1.0], midpoint: 0.5 },
        GradientStop { offset: 1.0, color: [0.6, 0.4, 0.4, 0.0], midpoint: 0.5 },
    ];
    let mut node = with_id(Node::with_components(id, None, vec![NodeBehavior::ParticleSystem(config)]), id);
    node.transform.translation = Vec2::new(100_000.0, 0.0);
    node
}

// ------------------------------------------------------------------ build --

pub(crate) fn build_demo_torture(document: &mut Document) -> TortureDemo {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: FONT_BYTES.to_vec() });
    let inverse = document.apply(Command::AddArtboard { name: "Ragdoll Torture".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    let camera = with_id(
        Node::with_components(
            "Main Camera",
            None,
            vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.1, 0.1, 0.13, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })],
        ),
        "camera",
    );
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));

    let (half_w, half_h) = (SCREEN_W / 2.0, SCREEN_H / 2.0);
    let concrete = [0.3, 0.31, 0.36, 1.0];
    // Backdrop: a test-lab wall with hazard stripes along the floor.
    add_node(document, &mut inverses, scene, shape("Back Wall", Path::rect(SCREEN_W, SCREEN_H), [0.16, 0.17, 0.21, 1.0], Vec2::ZERO));
    for i in 0..8 {
        let x = -half_w + 30.0 + i as f32 * 60.0;
        add_node(document, &mut inverses, scene, shape("Panel Seam", Path::rect(2.0, SCREEN_H), [0.13, 0.14, 0.17, 1.0], Vec2::new(x, 0.0)));
    }
    add_node(document, &mut inverses, scene, text_node(None, "labTitle", "CRASH TEST LAB 7", font, 8.0, [0.3, 0.32, 0.4, 1.0], Vec2::new(0.0, 40.0)));

    // ---- The room ----
    add_node(document, &mut inverses, scene, wall("Floor", SCREEN_W, 20.0, Vec2::new(0.0, FLOOR_Y - 10.0), concrete));
    add_node(document, &mut inverses, scene, wall("Ceiling", SCREEN_W, 10.0, Vec2::new(0.0, half_h - 26.0), concrete));
    add_node(document, &mut inverses, scene, wall("Left Wall", 12.0, SCREEN_H, Vec2::new(-half_w + 6.0, 0.0), concrete));
    add_node(document, &mut inverses, scene, wall("Right Wall", 12.0, SCREEN_H, Vec2::new(half_w - 6.0, 0.0), concrete));
    for i in 0..24 {
        let x = -half_w + 10.0 + i as f32 * 20.0;
        let mut stripe = shape("Hazard Stripe", Path::rect(8.0, 4.0), [0.95, 0.75, 0.1, 0.8], Vec2::new(x, FLOOR_Y - 3.0));
        stripe.transform.rotation = 0.6;
        add_node(document, &mut inverses, scene, stripe);
    }

    // ---- Hazards ----
    // Bounce pad (left): the script launches anything that touches it.
    add_node(document, &mut inverses, scene, shape("Pad Base", Path::rect(56.0, 6.0), [0.25, 0.25, 0.3, 1.0], Vec2::new(-170.0, FLOOR_Y + 3.0)));
    add_node(document, &mut inverses, scene, with_id(wall("Bounce Pad", 52.0, 5.0, Vec2::new(-170.0, FLOOR_Y + 8.5), [0.25, 0.85, 0.55, 1.0]), "pad"));
    // Spikes (centre-left).
    let spike_w = 70.0;
    let spikes = with(shape("Spikes", spike_path(7, spike_w, 12.0), [0.75, 0.78, 0.85, 1.0], Vec2::new(-60.0, FLOOR_Y)), vec![NodeBehavior::BoxCollider2D(BoxCollider2D {
        offset: Vec2::new(0.0, 5.0),
        size: Vec2::new(spike_w, 10.0),
        ..Default::default()
    })]);
    add_node(document, &mut inverses, scene, with_id(spikes, "spikes"));
    // Crusher (centre-right): a kinematic block the script slams down.
    add_node(document, &mut inverses, scene, shape("Crusher Rail", Path::rect(6.0, 200.0), [0.2, 0.2, 0.24, 1.0], Vec2::new(60.0, 10.0)));
    let crusher = with(stroked(shape("Crusher", Path::rect(54.0, 30.0), [0.55, 0.2, 0.2, 1.0], Vec2::new(60.0, 60.0)), 2.0, [0.95, 0.75, 0.1, 1.0]), vec![
        NodeBehavior::Rigidbody2D(Rigidbody2D { body_type: RigidbodyType2D::Kinematic, mass: 50.0, ..Default::default() }),
        NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::new(54.0, 30.0), ..Default::default() }),
    ]);
    add_node(document, &mut inverses, scene, with_id(crusher, "crusher"));
    // Saw (right): a kinematic disc the script spins, on a post.
    add_node(document, &mut inverses, scene, shape("Saw Post", Path::rect(8.0, 70.0), [0.22, 0.22, 0.27, 1.0], Vec2::new(172.0, FLOOR_Y + 35.0)));
    let saw = with(stroked(shape("Saw", saw_path(24.0), [0.78, 0.8, 0.86, 1.0], Vec2::new(172.0, FLOOR_Y + 72.0)), 1.0, [0.4, 0.42, 0.5, 1.0]), vec![
        NodeBehavior::Rigidbody2D(Rigidbody2D { body_type: RigidbodyType2D::Kinematic, mass: 10.0, ..Default::default() }),
        NodeBehavior::CircleCollider2D(CircleCollider2D { radius: 22.0, ..Default::default() }),
    ]);
    let saw_id = add_node(document, &mut inverses, scene, with_id(saw, "saw"));
    let mut hub = shape("Saw Hub", Path::ellipse(10.0, 10.0), [0.3, 0.3, 0.36, 1.0], Vec2::ZERO);
    hub.parent = Some(saw_id);
    add_node(document, &mut inverses, scene, hub);
    // Wall cannon (left wall): its barrel turns toward the cursor.
    add_node(document, &mut inverses, scene, stroked(shape("Cannon Mount", Path::rect(14.0, 22.0), [0.25, 0.25, 0.3, 1.0], Vec2::new(-half_w + 16.0, 40.0)), 1.0, [0.5, 0.5, 0.6, 1.0]));
    let mut barrel = with_id(stroked(shape("Cannon", Path::rect(26.0, 10.0), [0.35, 0.35, 0.42, 1.0], Vec2::new(-half_w + 22.0, 40.0)), 1.0, [0.6, 0.6, 0.7, 1.0]), "cannon");
    barrel.transform.anchor = Vec2::new(-13.0, 0.0);
    add_node(document, &mut inverses, scene, barrel);

    // ---- Effects pools (the script reuses these) ----
    // Floor splats: dark red decals the script drops where blood lands.
    for k in 0..BLOOD_SPLATS {
        let mut splat = with_id(shape("Splat", Path::ellipse(10.0, 3.0), [0.45, 0.02, 0.05, 0.9], Vec2::ZERO), &format!("splat{k}"));
        splat.visible = false;
        add_node(document, &mut inverses, scene, splat);
    }
    // Blood bursts: world-space particle systems, parked off-screen until
    // the script moves one to a wound and emits.
    for k in 0..BLOOD_EMITTERS {
        add_node(document, &mut inverses, scene, blood_emitter(&format!("blood{k}")));
    }
    for k in 0..6 {
        let mut ring = with_id(stroked(shape("Boom", Path::ellipse(20.0, 20.0), [1.0, 0.7, 0.2, 0.5], Vec2::ZERO), 3.0, [1.0, 0.95, 0.6, 0.9]), &format!("boom{k}"));
        ring.visible = false;
        ring.unlit = true;
        add_node(document, &mut inverses, scene, ring);
    }
    for k in 0..10 {
        let mut pop = text_node(None, &format!("pop{k}"), "", font, 8.0, [1.0, 1.0, 1.0, 1.0], Vec2::ZERO);
        pop.visible = false;
        add_node(document, &mut inverses, scene, pop);
    }
    let mut grab_line = with_id(shape("Grab Line", Path::rect(1.0, 2.0), [1.0, 1.0, 1.0, 0.7], Vec2::ZERO), "grabLine");
    grab_line.visible = false;
    add_node(document, &mut inverses, scene, grab_line);

    // ---- Prefabs ----
    for definition in prop_prefabs() {
        inverses.push(Command::AddPrefab { definition }.apply(document));
    }

    // ---- HUD (camera children) ----
    let hud_y = half_h - 10.0;
    add_node(document, &mut inverses, scene, shape("HUD Bar", Path::rect(SCREEN_W, 20.0), [0.05, 0.05, 0.07, 0.9], Vec2::new(0.0, hud_y)).with_parent(camera_id));
    // (The script hit-tests these at the same x: TOOL_X0 + k * TOOL_STEP.)
    for (k, label) in ["1 GRAB", "2 PUNCH", "3 BOMB", "4 ANVIL", "5 CANNON"].into_iter().enumerate() {
        let x = TOOL_X0 + k as f32 * TOOL_STEP;
        add_node(document, &mut inverses, scene, with_id(stroked(shape("Tool Button", Path::rect(TOOL_STEP - 4.0, 14.0), [0.16, 0.16, 0.2, 1.0], Vec2::new(x, hud_y)), 1.0, [0.45, 0.45, 0.55, 1.0]), &format!("tool{k}")).with_parent(camera_id));
        add_node(document, &mut inverses, scene, text_node(Some(camera_id), &format!("toolLabel{k}"), label, font, 8.0, [1.0, 1.0, 1.0, 1.0], Vec2::new(x, hud_y)));
    }
    // +DUMMY: another ragdoll in a random colour.
    add_node(document, &mut inverses, scene, with_id(stroked(shape("Add Dummy Button", Path::rect(ADD_W, 14.0), [0.25, 0.27, 0.55, 1.0], Vec2::new(ADD_X, hud_y)), 1.0, [0.7, 0.75, 1.0, 1.0]), "addBtn").with_parent(camera_id));
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "addLabel", "+DUMMY", font, 8.0, [1.0, 1.0, 1.0, 1.0], Vec2::new(ADD_X, hud_y)));
    // RESET: fresh dummy, no props, no blood, PAIN back to 0.
    add_node(document, &mut inverses, scene, with_id(stroked(shape("Reset Button", Path::rect(RESET_W, 14.0), [0.2, 0.45, 0.3, 1.0], Vec2::new(RESET_X, hud_y)), 1.0, [0.6, 0.95, 0.7, 1.0]), "resetBtn").with_parent(camera_id));
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "resetLabel", "RESET", font, 8.0, [1.0, 1.0, 1.0, 1.0], Vec2::new(RESET_X, hud_y)));
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "pain", "PAIN 0", font, 8.0, [1.0, 0.45, 0.4, 1.0], Vec2::new(170.0, half_h - 34.0)));
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "best", "BEST 0", font, 8.0, [0.7, 0.7, 0.8, 1.0], Vec2::new(170.0, half_h - 46.0)));
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "hint", "R: RESET   N: ADD DUMMY   C: CLEAR PROPS", font, 8.0, [0.55, 0.56, 0.65, 1.0], Vec2::new(0.0, -half_h + 6.0)));
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "status", "", font, 8.0, [1.0, 0.85, 0.5, 1.0], Vec2::new(-150.0, half_h - 34.0)));

    // ---- Script ----
    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Torture (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    let prefix = format!(
        "const FLOOR_Y = {FLOOR_Y:.1}, HALF_W = {half_w:.1}, HALF_H = {half_h:.1}\nconst TOOL_X0 = {TOOL_X0:.1}, TOOL_STEP = {TOOL_STEP:.1}, RESET_X = {RESET_X:.1}, RESET_W = {RESET_W:.1}, ADD_X = {ADD_X:.1}, ADD_W = {ADD_W:.1}\nconst BLOOD_EMITTERS = {BLOOD_EMITTERS}, BLOOD_SPLATS = {BLOOD_SPLATS}\n"
    );
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Ragdoll Torture".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Play".into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(prefix + TORTURE_SCRIPT)),
            }],
            initial_state: "Play".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));

    inverses.reverse();
    document.commit_batch(inverses);
    TortureDemo { scene }
}

/// Re-parents a freshly built node (for HUD nodes built by local helpers).
trait WithParent {
    fn with_parent(self, parent: an1mbyt3_core::NodeId) -> Self;
}
impl WithParent for Node {
    fn with_parent(mut self, parent: an1mbyt3_core::NodeId) -> Self {
        self.parent = Some(parent);
        self
    }
}

pub(crate) const TORTURE_SCRIPT: &str = include_str!("demo_torture.js");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_lab_with_an_eleven_bone_dummy_prefab_and_props() {
        let mut document = Document::new();
        let demo = build_demo_torture(&mut document);
        assert!(document.artboard(demo.scene).is_some());
        let names: Vec<_> = document.prefabs().map(|(_, p)| p.name.clone()).collect();
        assert_eq!(names, ["Dummy", "Bomb", "Anvil", "Ball"]);
        let (_, dummy) = document.prefabs().find(|(_, p)| p.name == "Dummy").unwrap();
        let bones = dummy.nodes.iter().filter(|n| matches!(n.node.kind, NodeKind::Bone(_))).count();
        assert_eq!(bones, 11);
        let hinges = dummy.nodes.iter().filter(|n| n.node.hinge_joint().is_some()).count();
        assert_eq!(hinges, 10, "every bone but the hips hinges to its parent");
    }
}
