//! "Ragdoll Arena" — the bones + physics showcase.
//!
//! Two bone-rigged puppet fighters brawl in a lantern-lit cave:
//! - **Bones**: each fighter is a hips -> chest -> head spine with two-bone
//!   arms and legs; its body parts are plain Shapes parented to the bones.
//! - **Live IK** (`IkConstraint2D`): hands reach for hand-target nodes, feet
//!   for foot targets the script plants on the ground with procedural steps.
//! - **State machine + layers**: "Locomotion" keyframes the spine and head
//!   (Idle / Walk / KO, driven by the `speed` and `ko` inputs); "Combat" is
//!   masked to the player's hand targets and keyframes Guard / Jab / Cross;
//!   "Brain" runs the game code.
//! - **Ragdolls** (`HingeJoint2D` + Rigidbody 2D + capsules on every bone):
//!   fighters stand Kinematic, following animation and IK; a knockout flips
//!   the whole rig to Dynamic (`ref.ragdoll(true)`) and it goes limp with
//!   its momentum, then gets back up.
//! - **Sprite skinning** (`SpriteSkin2D`): a cave worm image bound to a
//!   five-bone chain that sways with whole-chain FABRIK IK.
//! - **Joints & props**: a hinged lantern chain carrying a swinging
//!   Light 2D, and crates the fighters shove around.

use an1mbyt3_core::{
    affine_to_array, Animation, AnimationId, ArtboardId, BoneNode, BoxCollider2D, CameraNode, CapsuleCollider2D, CapsuleDirection2D, CircleCollider2D,
    Command, ConditionDef, Document, Easing, FontAsset, GradientDef, GradientKind, GradientStop, GroupNode, HingeJoint2D, IkConstraint2D, ImageId, InputDef,
    InputKind, Keyframe, Light2DNode, LoopMode, Node, NodeBehavior, NodeBehaviorEntry, NodeId, NodeKind, Path, PlaybackDirection, ProgramDef, PropertyTrack,
    Rigidbody2D, RigidbodyType2D, ShapeNode, SpriteSkin2D, StateDef, StateMachineDef, StateMachineLayerDef, Track, TransitionDef,
};
use glam::Vec2;

use crate::demo_dungeon::{burst, glow};
use crate::demo_ninja::{add_node, add_png, image_node, text_node, with_id};

const WORM_BYTES: &[u8] = include_bytes!("../assets/rig/worm.png");
const PIXEL_FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");

const SCREEN_W: f32 = 480.0;
const SCREEN_H: f32 = 270.0;
const GROUND_Y: f32 = -100.0;
/// Hips height: thigh (20) + shin (20) + a little knee bend.
const HIPS_Y: f32 = GROUND_Y + 37.0;

pub(crate) struct RigDemo {
    pub scene: ArtboardId,
    pub images: Vec<ImageId>,
}

fn entries(behaviors: Vec<NodeBehavior>) -> Vec<NodeBehaviorEntry> {
    behaviors.into_iter().map(|behavior| NodeBehaviorEntry { behavior, enabled: true }).collect()
}

fn shape(name: &str, parent: Option<NodeId>, path: Path, color: [f32; 4], at: Vec2) -> Node {
    let mut node = Node::new(name, parent, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None }));
    node.transform.translation = at;
    node
}

fn bone(name: &str, parent: Option<NodeId>, length: f32, at: Vec2, degrees: f32) -> Node {
    let mut node = Node::new(name, parent, NodeKind::Bone(BoneNode { length }));
    node.transform.translation = at;
    node.transform.rotation = degrees.to_radians();
    node
}

fn group(name: &str, parent: Option<NodeId>, at: Vec2) -> Node {
    let mut node = Node::new(name, parent, NodeKind::Group(GroupNode::default()));
    node.transform.translation = at;
    node
}

/// Rigidbody (Kinematic: follows animation/IK until a knockout), a capsule
/// along the bone, and — for non-root bones — a hinge to the parent bone.
fn ragdoll_part(length: f32, width: f32, mass: f32, hinge: Option<(f32, f32)>) -> Vec<NodeBehavior> {
    let mut parts = vec![
        NodeBehavior::Rigidbody2D(Rigidbody2D { body_type: RigidbodyType2D::Kinematic, mass, angular_drag: 0.8, linear_drag: 0.1, ..Default::default() }),
        NodeBehavior::CapsuleCollider2D(CapsuleCollider2D {
            offset: Vec2::new(length / 2.0, 0.0),
            size: Vec2::new(length.max(width), width),
            direction: CapsuleDirection2D::Horizontal,
            ..Default::default()
        }),
    ];
    if let Some((lower, upper)) = hinge {
        parts.push(NodeBehavior::HingeJoint2D(HingeJoint2D { lower_angle: lower, upper_angle: upper, ..Default::default() }));
    }
    parts
}

struct Palette {
    skin: [f32; 4],
    main: [f32; 4],
    dark: [f32; 4],
    accent: [f32; 4],
}

/// The node ids a fighter's animations need.
struct Fighter {
    chest: NodeId,
    head: NodeId,
    hand_near: NodeId,
    hand_far: NodeId,
}

/// One two-bone arm hanging from the chest's shoulder; returns the forearm.
#[allow(clippy::too_many_arguments)]
fn add_arm(document: &mut Document, inverses: &mut Vec<Command>, scene: ArtboardId, chest: NodeId, prefix: &str, side: &str, spread: f32, color: [f32; 4], skin: [f32; 4]) -> NodeId {
    let id = |name: &str| format!("{prefix}_{name}");
    let upper = add_node(document, inverses, scene, with_id(
        Node { behaviors: entries(ragdoll_part(17.0, 7.0, 0.8, Some((-160.0, 160.0)))), ..bone(&format!("Upper Arm {side}"), Some(chest), 17.0, Vec2::new(19.0, 0.0), 180.0 + spread) },
        &id(&format!("uarm{side}")),
    ));
    for n in limb("Upper Arm", upper, 17.0, 7.0, color) {
        add_node(document, inverses, scene, n);
    }
    let lower = add_node(document, inverses, scene, with_id(
        Node { behaviors: entries(ragdoll_part(16.0, 6.0, 0.6, Some((-150.0, 150.0)))), ..bone(&format!("Forearm {side}"), Some(upper), 16.0, Vec2::new(17.0, 0.0), 0.0) },
        &id(&format!("larm{side}")),
    ));
    for n in limb("Forearm", lower, 15.0, 6.0, color) {
        add_node(document, inverses, scene, n);
    }
    add_node(document, inverses, scene, shape("Fist", Some(lower), Path::ellipse(9.0, 9.0), skin, Vec2::new(17.0, 0.0)));
    lower
}

/// A rounded limb: a bar along the bone plus a joint cap.
fn limb(name: &str, parent: NodeId, length: f32, width: f32, color: [f32; 4]) -> Vec<Node> {
    vec![
        shape(name, Some(parent), Path::rect(length + 2.0, width), color, Vec2::new(length / 2.0, 0.0)),
        shape(&format!("{name} joint"), Some(parent), Path::ellipse(width + 1.0, width + 1.0), color, Vec2::ZERO),
    ]
}

/// Builds one puppet fighter (element ids prefixed `p_` / `e_`). `facing`:
/// +1 = fights toward +x (the player), -1 = toward -x (the enemy).
fn add_fighter(document: &mut Document, inverses: &mut Vec<Command>, scene: ArtboardId, prefix: &str, x: f32, facing: f32, colors: &Palette) -> Fighter {
    let id = |name: &str| format!("{prefix}_{name}");

    let hips = add_node(document, inverses, scene, with_id(
        Node { behaviors: entries(ragdoll_part(20.0, 14.0, 3.0, None)), ..bone("Hips", None, 20.0, Vec2::new(x, HIPS_Y), 90.0) },
        &id("hips"),
    ));
    // Far leg first (drawn behind), then the torso, then the near leg.
    let mut legs = Vec::new();
    for (side, spread, color) in [("F", -10.0f32, colors.dark), ("N", 10.0, colors.main)] {
        let thigh = add_node(document, inverses, scene, with_id(
            Node { behaviors: entries(ragdoll_part(20.0, 8.0, 1.5, Some((-100.0, 100.0)))), ..bone(&format!("Thigh {side}"), Some(hips), 20.0, Vec2::ZERO, 180.0 + spread * facing) },
            &id(&format!("thigh{side}")),
        ));
        for n in limb("Thigh", thigh, 20.0, 9.0, color) {
            add_node(document, inverses, scene, n);
        }
        let shin = add_node(document, inverses, scene, with_id(
            Node { behaviors: entries(ragdoll_part(20.0, 7.0, 1.0, Some((-130.0, 130.0)))), ..bone(&format!("Shin {side}"), Some(thigh), 20.0, Vec2::new(20.0, 0.0), 0.0) },
            &id(&format!("shin{side}")),
        ));
        for n in limb("Shin", shin, 19.0, 7.0, color) {
            add_node(document, inverses, scene, n);
        }
        add_node(document, inverses, scene, shape("Boot", Some(shin), Path::rect(6.0, 10.0), colors.accent, Vec2::new(20.0, -2.0 * facing)));
        legs.push(shin);
        if side == "F" {
            // The torso sits between the two legs in draw order.
            let pelvis = shape("Pelvis", Some(hips), Path::ellipse(18.0, 20.0), colors.main, Vec2::new(6.0, 0.0));
            add_node(document, inverses, scene, pelvis);
        }
    }
    let chest = add_node(document, inverses, scene, with_id(
        Node { behaviors: entries(ragdoll_part(22.0, 16.0, 2.5, Some((-45.0, 45.0)))), ..bone("Chest", Some(hips), 22.0, Vec2::new(20.0, 0.0), 0.0) },
        &id("chest"),
    ));
    // Far arm (behind the torso).
    let forearm_far = add_arm(document, inverses, scene, chest, prefix, "F", -12.0 * facing, colors.dark, colors.skin);
    add_node(document, inverses, scene, shape("Torso", Some(chest), Path::ellipse(30.0, 24.0), colors.main, Vec2::new(10.0, 0.0)));
    add_node(document, inverses, scene, shape("Belt", Some(chest), Path::rect(4.0, 22.0), colors.accent, Vec2::new(1.0, 0.0)));
    let head = add_node(document, inverses, scene, with_id(
        Node { behaviors: entries(ragdoll_part(14.0, 16.0, 1.2, Some((-40.0, 40.0)))), ..bone("Head", Some(chest), 14.0, Vec2::new(22.0, 0.0), 0.0) },
        &id("head"),
    ));
    add_node(document, inverses, scene, shape("Face", Some(head), Path::ellipse(17.0, 17.0), colors.skin, Vec2::new(8.0, 0.0)));
    add_node(document, inverses, scene, shape("Headband", Some(head), Path::rect(4.0, 18.0), colors.accent, Vec2::new(11.0, 0.0)));
    add_node(document, inverses, scene, shape("Eye", Some(head), Path::ellipse(3.0, 3.0), [0.08, 0.06, 0.1, 1.0], Vec2::new(8.0, -4.0 * facing)));
    let forearm_near = add_arm(document, inverses, scene, chest, prefix, "N", 12.0 * facing, colors.main, colors.skin);

    // IK targets: hands live in the chest's space (they move with the body
    // and are keyframed there); feet are world-space, planted by the script.
    let guard = |near: bool| Vec2::new(if near { 10.0 } else { 12.0 }, if near { -14.0 } else { -9.0 } * facing);
    let hand_near = add_node(document, inverses, scene, with_id(group("Hand Target N", Some(chest), guard(true)), &id("handN")));
    let hand_far = add_node(document, inverses, scene, with_id(group("Hand Target F", Some(chest), guard(false)), &id("handF")));
    let foot_far = add_node(document, inverses, scene, with_id(group("Foot Target F", None, Vec2::new(x - 6.0 * facing, GROUND_Y + 2.0)), &id("footF")));
    let foot_near = add_node(document, inverses, scene, with_id(group("Foot Target N", None, Vec2::new(x + 6.0 * facing, GROUND_Y + 2.0)), &id("footN")));

    // Live IK: elbows bend down/back, knees point the way the fighter faces.
    let arm_flip = facing < 0.0;
    let leg_flip = facing > 0.0;
    for (tip, target, flip) in [(forearm_near, hand_near, arm_flip), (forearm_far, hand_far, arm_flip), (legs[1], foot_near, leg_flip), (legs[0], foot_far, leg_flip)] {
        let artboard = document.artboard(scene).expect("scene exists");
        let mut behaviors = artboard.node(tip).expect("tip exists").behaviors.clone();
        behaviors.push(NodeBehaviorEntry {
            behavior: NodeBehavior::IkConstraint2D(IkConstraint2D { target: Some(target), chain_length: 2, flip, weight: 1.0 }),
            enabled: true,
        });
        inverses.push(document.apply(Command::SetNodeBehaviors { artboard: scene, node: tip, value: behaviors }));
    }
    Fighter { chest, head, hand_near, hand_far }
}

/// A looping/one-shot rotation track.
fn rotation_track(node: NodeId, keys: &[(f32, f32)]) -> (NodeId, PropertyTrack) {
    let ease = Easing::CubicBezier { x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 };
    (node, PropertyTrack::Rotation(Track::new(keys.iter().map(|&(t, deg)| Keyframe::new(t, deg.to_radians(), ease)).collect())))
}

fn translation_track(node: NodeId, keys: &[(f32, Vec2)]) -> (NodeId, PropertyTrack) {
    let ease = Easing::CubicBezier { x1: 0.3, y1: 0.0, x2: 0.3, y2: 1.0 };
    (node, PropertyTrack::Translation(Track::new(keys.iter().map(|&(t, v)| Keyframe::new(t, v, ease)).collect())))
}

fn animation(name: &str, duration: f32, tracks: Vec<(NodeId, PropertyTrack)>) -> Animation {
    let mut a = Animation::new(name);
    a.duration = duration;
    a.tracks = tracks;
    a
}

pub(crate) fn build_demo_rig(document: &mut Document) -> RigDemo {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: PIXEL_FONT_BYTES.to_vec() });
    let worm_art = add_png(document, "worm.png", WORM_BYTES);
    let images = vec![worm_art.image];

    let inverse = document.apply(Command::AddArtboard { name: "Ragdoll Arena".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    let camera = with_id(
        Node::with_components(
            "Main Camera",
            None,
            vec![NodeBehavior::Camera(CameraNode {
                zoom: 1.0,
                background_color: [0.06, 0.05, 0.08, 1.0],
                follow_target: None,
                follow_smoothing: 0.0,
                vignette: an1mbyt3_core::CameraVignette { enabled: true, color: [0.02, 0.01, 0.03, 1.0], intensity: 0.5, radius: 1.05, blur: 0.8 },
                pixel_perfect: Default::default(),
            })],
        ),
        "camera",
    );
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));
    let ambient = Light2DNode { color: [0.7, 0.66, 0.8, 1.0], intensity: 0.7, ..Light2DNode::global() };
    add_node(document, &mut inverses, scene, Node::with_components("Global Light 2D", None, vec![NodeBehavior::Light2D(ambient)]));

    // ---- Cave ----
    let mut backdrop = shape("Backdrop", None, Path::rect(SCREEN_W, SCREEN_H), [1.0; 4], Vec2::ZERO);
    if let NodeKind::Shape(s) = &mut backdrop.kind {
        s.gradient = Some(GradientDef {
            kind: GradientKind::Linear,
            start: Vec2::new(0.0, SCREEN_H / 2.0),
            end: Vec2::new(0.0, -SCREEN_H / 2.0),
            stops: vec![
                GradientStop { offset: 0.0, color: [0.1, 0.08, 0.14, 1.0], midpoint: 0.5 },
                GradientStop { offset: 1.0, color: [0.22, 0.17, 0.24, 1.0], midpoint: 0.5 },
            ],
        });
    }
    add_node(document, &mut inverses, scene, backdrop);
    // Distant rock pillars.
    for (x, w, h) in [(-205.0f32, 34.0f32, 150.0f32), (-95.0, 22.0, 110.0), (60.0, 28.0, 130.0), (215.0, 40.0, 170.0)] {
        add_node(document, &mut inverses, scene, shape("Pillar", None, Path::rect(w, h), [0.15, 0.12, 0.18, 1.0], Vec2::new(x, GROUND_Y + h / 2.0)));
    }
    let ground = Node {
        behaviors: entries(vec![NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::new(SCREEN_W + 80.0, 40.0), ..Default::default() })]),
        ..shape("Ground", None, Path::rect(SCREEN_W + 80.0, 40.0), [0.3, 0.24, 0.3, 1.0], Vec2::new(0.0, GROUND_Y - 20.0))
    };
    add_node(document, &mut inverses, scene, ground);
    add_node(document, &mut inverses, scene, shape("Ground Lip", None, Path::rect(SCREEN_W + 80.0, 3.0), [0.45, 0.36, 0.42, 1.0], Vec2::new(0.0, GROUND_Y - 1.5)));
    for (name, x) in [("Left Wall", -SCREEN_W / 2.0 - 10.0), ("Right Wall", SCREEN_W / 2.0 + 10.0)] {
        add_node(document, &mut inverses, scene, Node {
            behaviors: entries(vec![NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::new(20.0, SCREEN_H * 2.0), ..Default::default() })]),
            ..group(name, None, Vec2::new(x, 0.0))
        });
    }

    // ---- Skinned cave worm: an image bound to a 5-bone chain (FABRIK IK) ----
    let worm_x = 190.0;
    let mut worm_bones = Vec::new();
    let mut parent = None;
    for i in 0..5 {
        let at = if i == 0 { Vec2::new(worm_x, GROUND_Y - 4.0) } else { Vec2::new(17.0, 0.0) };
        let id = add_node(document, &mut inverses, scene, bone(&format!("Worm {i}"), parent, 17.0, at, if i == 0 { 90.0 } else { 0.0 }));
        worm_bones.push(id);
        parent = Some(id);
    }
    let mut worm = with_id(image_node("Worm", None, worm_art.image, 28.0, 84.0), "worm");
    worm.transform.translation = Vec2::new(worm_x, GROUND_Y - 4.0 + 42.0);
    let worm_id = add_node(document, &mut inverses, scene, worm);
    {
        let artboard = document.artboard(scene).expect("scene exists");
        let skin = SpriteSkin2D {
            bind_poses: worm_bones.iter().map(|b| affine_to_array(artboard.world_transform(*b).to_affine2())).collect(),
            bones: worm_bones.clone(),
            bind_node: affine_to_array(artboard.world_transform(worm_id).to_affine2()),
            columns: 2,
            rows: 16,
            smoothness: 10.0,
        };
        inverses.push(document.apply(Command::SetNodeBehaviors { artboard: scene, node: worm_id, value: entries(vec![NodeBehavior::SpriteSkin2D(skin)]) }));
    }
    let worm_target = add_node(document, &mut inverses, scene, with_id(group("Worm Target", None, Vec2::new(worm_x, GROUND_Y + 80.0)), "wormTarget"));
    inverses.push(document.apply(Command::SetNodeBehaviors {
        artboard: scene,
        node: worm_bones[4],
        value: entries(vec![NodeBehavior::IkConstraint2D(IkConstraint2D { target: Some(worm_target), chain_length: 5, flip: false, weight: 1.0 })]),
    }));

    // ---- Lantern on a hinged chain (pinned to the ceiling) ----
    let mut parent = None;
    let mut last = None;
    for i in 0..5 {
        let at = if i == 0 { Vec2::new(-150.0, SCREEN_H / 2.0) } else { Vec2::new(13.0, 0.0) };
        let mut link = bone(&format!("Chain {i}"), parent, 13.0, at, if i == 0 { -90.0 } else { 0.0 });
        link.behaviors = entries(vec![
            NodeBehavior::Rigidbody2D(Rigidbody2D { mass: 0.3, angular_drag: 0.3, linear_drag: 0.05, ..Default::default() }),
            NodeBehavior::CapsuleCollider2D(CapsuleCollider2D { offset: Vec2::new(6.5, 0.0), size: Vec2::new(13.0, 3.0), direction: CapsuleDirection2D::Horizontal, ..Default::default() }),
            NodeBehavior::HingeJoint2D(HingeJoint2D { use_limits: false, ..Default::default() }),
        ]);
        let id = add_node(document, &mut inverses, scene, link);
        add_node(document, &mut inverses, scene, shape("Link", Some(id), Path::rect(14.0, 2.5), [0.55, 0.5, 0.45, 1.0], Vec2::new(6.5, 0.0)));
        parent = Some(id);
        last = Some(id);
    }
    let lantern = Node {
        behaviors: entries(vec![
            NodeBehavior::CircleCollider2D(CircleCollider2D { radius: 7.0, ..Default::default() }),
            // Smooth (not pixel-stepped) falloff: this scene is vector art.
            NodeBehavior::Light2D(Light2DNode { pixel_size: 0.0, edge_blur: 1.0, ..glow([1.0, 0.78, 0.45], 1.5, 150.0) }),
        ]),
        ..with_id(shape("Lantern", last, Path::rect(12.0, 10.0), [1.0, 0.82, 0.45, 1.0], Vec2::new(19.0, 0.0)), "lantern")
    };
    let lantern_id = add_node(document, &mut inverses, scene, lantern);
    let mut glass = shape("Lantern Glow", Some(lantern_id), Path::ellipse(7.0, 7.0), [1.0, 0.95, 0.75, 1.0], Vec2::ZERO);
    glass.unlit = true;
    add_node(document, &mut inverses, scene, glass);

    // ---- Crates ----
    for (i, (x, y)) in [(120.0f32, GROUND_Y + 11.0), (146.0, GROUND_Y + 11.0), (133.0, GROUND_Y + 33.0)].into_iter().enumerate() {
        add_node(document, &mut inverses, scene, Node {
            behaviors: entries(vec![
                NodeBehavior::Rigidbody2D(Rigidbody2D { mass: 1.5, ..Default::default() }),
                NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::new(22.0, 22.0), ..Default::default() }),
            ]),
            ..with_id(shape("Crate", None, Path::rect(22.0, 22.0), [0.62, 0.42, 0.24, 1.0], Vec2::new(x, y)), &format!("crate{i}"))
        });
    }

    // ---- Fighters ----
    let enemy_colors = Palette { skin: [0.95, 0.78, 0.66, 1.0], main: [0.82, 0.24, 0.26, 1.0], dark: [0.5, 0.13, 0.16, 1.0], accent: [0.15, 0.12, 0.14, 1.0] };
    let player_colors = Palette { skin: [0.95, 0.8, 0.68, 1.0], main: [0.22, 0.62, 0.86, 1.0], dark: [0.12, 0.34, 0.52, 1.0], accent: [0.98, 0.85, 0.3, 1.0] };
    add_fighter(document, &mut inverses, scene, "e", 70.0, -1.0, &enemy_colors);
    let player = add_fighter(document, &mut inverses, scene, "p", -70.0, 1.0, &player_colors);

    // ---- FX + HUD ----
    add_node(document, &mut inverses, scene, burst("fxHit", [1.0, 0.92, 0.6, 1.0], (40.0, 110.0), (0.15, 0.35), (1.5, 3.0), 0.0, 3.0, false, 2.0));
    add_node(document, &mut inverses, scene, burst("fxDust", [0.75, 0.68, 0.62, 0.8], (10.0, 40.0), (0.3, 0.7), (2.0, 4.0), -0.2, 2.0, true, 6.0));
    let hud = |node: Node| {
        let mut node = node;
        node.unlit = true;
        node
    };
    add_node(document, &mut inverses, scene, hud(text_node(Some(camera_id), "title", "RAGDOLL ARENA", font, 8.0, [1.0, 0.86, 0.55, 1.0], Vec2::new(0.0, SCREEN_H / 2.0 - 12.0))));
    add_node(document, &mut inverses, scene, hud(text_node(Some(camera_id), "help", "A/D MOVE  J JAB  K CROSS  R RESET", font, 8.0, [0.78, 0.76, 0.86, 1.0], Vec2::new(0.0, -SCREEN_H / 2.0 + 10.0))));
    add_node(document, &mut inverses, scene, hud(text_node(Some(camera_id), "msg", "", font, 16.0, [1.0, 0.9, 0.55, 1.0], Vec2::new(0.0, 40.0))));
    for (prefix, x, color) in [("p", -170.0f32, player_colors.main), ("e", 170.0, enemy_colors.main)] {
        add_node(document, &mut inverses, scene, hud(shape("HP Back", Some(camera_id), Path::rect(104.0, 8.0), [0.05, 0.03, 0.07, 0.85], Vec2::new(x, SCREEN_H / 2.0 - 28.0))));
        add_node(document, &mut inverses, scene, hud(with_id(shape("HP", Some(camera_id), Path::rect(100.0, 4.0), color, Vec2::new(x, SCREEN_H / 2.0 - 28.0)), &format!("{prefix}_hp"))));
    }

    // ---- State machine ----
    let mut add_animation = |document: &mut Document, a: Animation| -> AnimationId {
        let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: a });
        let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
        inverses.push(inverse);
        animation
    };
    let idle = add_animation(document, animation("Idle", 2.0, vec![rotation_track(player.chest, &[(0.0, 0.0), (1.0, 3.0), (2.0, 0.0)]), rotation_track(player.head, &[(0.0, 0.0), (1.0, -4.0), (2.0, 0.0)])]));
    let walk = add_animation(document, animation("Walk", 0.5, vec![rotation_track(player.chest, &[(0.0, -7.0), (0.25, -4.0), (0.5, -7.0)]), rotation_track(player.head, &[(0.0, 4.0), (0.25, 7.0), (0.5, 4.0)])]));
    let ko = add_animation(document, animation("KO (physics)", 1.0, Vec::new()));
    let guard_n = Vec2::new(10.0, -14.0);
    let guard_f = Vec2::new(12.0, -9.0);
    let guard = add_animation(document, animation("Guard", 1.2, vec![
        translation_track(player.hand_near, &[(0.0, guard_n), (0.6, guard_n + Vec2::new(1.5, 1.0)), (1.2, guard_n)]),
        translation_track(player.hand_far, &[(0.0, guard_f), (0.6, guard_f + Vec2::new(1.5, 1.0)), (1.2, guard_f)]),
    ]));
    let jab = add_animation(document, animation("Jab", 0.28, vec![
        translation_track(player.hand_near, &[(0.0, guard_n), (0.09, Vec2::new(16.0, -40.0)), (0.16, Vec2::new(15.0, -38.0)), (0.28, guard_n)]),
        translation_track(player.hand_far, &[(0.0, guard_f), (0.28, guard_f)]),
    ]));
    let cross = add_animation(document, animation("Cross", 0.34, vec![
        translation_track(player.hand_far, &[(0.0, guard_f), (0.12, Vec2::new(18.0, -42.0)), (0.2, Vec2::new(17.0, -40.0)), (0.34, guard_f)]),
        translation_track(player.hand_near, &[(0.0, guard_n), (0.12, Vec2::new(6.0, -6.0)), (0.34, guard_n)]),
        rotation_track(player.chest, &[(0.0, -4.0), (0.12, -14.0), (0.34, -4.0)]),
    ]));

    let anim_state = |name: &str, animation, loop_mode, at: Vec2, transitions: Vec<TransitionDef>| StateDef {
        name: name.into(),
        animation,
        loop_mode,
        transitions,
        position: at,
        blend: None,
        program: None,
    };
    let fade = |to: &str, condition: ConditionDef, seconds: f32| TransitionDef { to: to.into(), condition, direction: PlaybackDirection::Forward, duration_seconds: seconds, interruptible: true };
    let ko_on = || ConditionDef::BoolIs { input: "ko".into(), value: true };
    let locomotion = vec![
        anim_state("Idle", idle, LoopMode::Loop, Vec2::new(0.0, 0.0), vec![fade("KO", ko_on(), 0.0), fade("Walk", ConditionDef::NumberGreaterThan { input: "speed".into(), threshold: 8.0 }, 0.2)]),
        anim_state("Walk", walk, LoopMode::Loop, Vec2::new(220.0, 0.0), vec![fade("KO", ko_on(), 0.0), fade("Idle", ConditionDef::NumberLessThan { input: "speed".into(), threshold: 8.0 }, 0.25)]),
        anim_state("KO", ko, LoopMode::Once, Vec2::new(110.0, 160.0), vec![fade("Idle", ConditionDef::BoolIs { input: "ko".into(), value: false }, 0.3)]),
    ];
    let combat = StateMachineLayerDef {
        name: "Combat".into(),
        initial_state: "Guard".into(),
        states: vec![
            anim_state("Guard", guard, LoopMode::Loop, Vec2::new(0.0, 0.0), vec![
                fade("Jab", ConditionDef::Trigger { input: "jab".into() }, 0.04),
                fade("Cross", ConditionDef::Trigger { input: "cross".into() }, 0.04),
            ]),
            anim_state("Jab", jab, LoopMode::Once, Vec2::new(220.0, -60.0), vec![fade("Guard", ConditionDef::Trigger { input: "guard".into() }, 0.08)]),
            anim_state("Cross", cross, LoopMode::Once, Vec2::new(220.0, 60.0), vec![fade("Guard", ConditionDef::Trigger { input: "guard".into() }, 0.08)]),
        ],
        // Only the player's hand targets (and the chest twist on Cross).
        mask: vec![player.hand_near, player.hand_far, player.chest],
    };
    let filler = add_animation(document, Animation::new("Brain (code)"));
    let brain = StateMachineLayerDef {
        name: "Brain".into(),
        initial_state: "Fight".into(),
        states: vec![StateDef {
            name: "Fight".into(),
            animation: filler,
            loop_mode: LoopMode::Once,
            transitions: Vec::new(),
            position: Vec2::ZERO,
            blend: None,
            program: Some(ProgramDef::Code(RIG_SCRIPT.to_string())),
        }],
        mask: Vec::new(),
    };
    let input = |name: &str, kind: InputKind| InputDef { name: name.into(), kind };
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Ragdoll Arena".into(),
            inputs: vec![
                input("speed", InputKind::Number),
                input("ko", InputKind::Bool),
                input("jab", InputKind::Trigger),
                input("cross", InputKind::Trigger),
                input("guard", InputKind::Trigger),
            ],
            states: locomotion,
            initial_state: "Idle".into(),
            layers: vec![combat, brain],
            deleted: false,
        },
    }));

    inverses.reverse();
    document.commit_batch(inverses);
    RigDemo { scene, images }
}

const RIG_SCRIPT: &str = include_str!("demo_rig.js");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_two_rigged_fighters_with_ik_ragdolls_and_a_skinned_worm() {
        let mut document = Document::new();
        let demo = build_demo_rig(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let nodes: Vec<&Node> = artboard.node_ids().filter_map(|id| artboard.node(id)).collect();
        let bones = nodes.iter().filter(|n| matches!(n.kind, NodeKind::Bone(_))).count();
        // 2 fighters x 11 bones + 5 worm + 5 chain links.
        assert_eq!(bones, 32);
        assert_eq!(nodes.iter().filter(|n| n.ik_constraint().is_some()).count(), 9);
        assert_eq!(nodes.iter().filter(|n| n.hinge_joint().is_some()).count(), 2 * 10 + 5);
        assert!(nodes.iter().any(|n| n.sprite_skin().is_some_and(|s| s.bones.len() == 5)));
        let (_, machine) = artboard.state_machines().next().unwrap();
        assert_eq!(machine.layers.len(), 2);
        assert_eq!(machine.layers[0].mask.len(), 3);
    }

    #[test]
    fn a_knocked_out_fighter_ragdolls_onto_the_floor_in_one_piece() {
        let mut document = Document::new();
        let demo = build_demo_rig(&mut document);
        let artboard = document.artboard_mut(demo.scene).unwrap();
        let find = |a: &an1mbyt3_core::Artboard, id: &str| a.node_ids().find(|n| a.node(*n).is_some_and(|x| x.id == id)).unwrap();
        let mut world = an1mbyt3_physics::PhysicsWorld::new();
        for _ in 0..10 {
            an1mbyt3_constraints::solve_ik_constraints(artboard);
            world.sync(artboard);
            world.step(artboard, 1.0 / 60.0);
        }
        // Knock the player out: every body in the rig goes Dynamic.
        let hips = find(artboard, "p_hips");
        let ids: Vec<NodeId> = artboard.node_ids().filter(|id| artboard.node(*id).is_some_and(|n| n.id.starts_with("p_") && n.rigidbody().is_some())).collect();
        for id in &ids {
            for e in artboard.node_mut(*id).unwrap().behaviors.iter_mut() {
                if let NodeBehavior::Rigidbody2D(rb) = &mut e.behavior {
                    rb.body_type = RigidbodyType2D::Dynamic;
                }
            }
        }
        for _ in 0..180 {
            an1mbyt3_constraints::solve_ik_constraints(artboard);
            world.sync(artboard);
            world.step(artboard, 1.0 / 60.0);
        }
        let hips_pos = artboard.world_transform(hips).translation;
        assert!(hips_pos.is_finite() && hips_pos.x.abs() < 240.0, "hips stayed in the arena: {hips_pos:?}");
        assert!(hips_pos.y > GROUND_Y - 12.0 && hips_pos.y < GROUND_Y + 40.0, "lying on the floor: {hips_pos:?}");
        // Joints held: each forearm still starts at its upper arm's elbow.
        for side in ["N", "F"] {
            let upper = find(artboard, &format!("p_uarm{side}"));
            let lower = find(artboard, &format!("p_larm{side}"));
            let elbow = artboard.world_transform(upper).to_affine2().transform_point2(Vec2::new(17.0, 0.0));
            let joint = artboard.world_transform(lower).translation;
            assert!(elbow.distance(joint) < 3.0, "elbow {side} together: {elbow:?} vs {joint:?}");
        }
    }

    #[test]
    fn standing_fighters_reach_their_feet_targets() {
        let mut document = Document::new();
        let demo = build_demo_rig(&mut document);
        let artboard = document.artboard_mut(demo.scene).unwrap();
        an1mbyt3_constraints::solve_ik_constraints(artboard);
        let find = |a: &an1mbyt3_core::Artboard, id: &str| a.node_ids().find(|n| a.node(*n).is_some_and(|x| x.id == id)).unwrap();
        let shin = find(artboard, "p_shinN");
        let foot = find(artboard, "p_footN");
        let tip = artboard.world_transform(shin).to_affine2().transform_point2(Vec2::new(20.0, 0.0));
        let goal = artboard.world_transform(foot).translation;
        assert!(tip.distance(goal) < 1.0, "foot planted: {tip:?} vs {goal:?}");
        // Knee points forward (+x) for the right-facing player.
        let knee = artboard.world_transform(shin).translation;
        let hips = artboard.world_transform(find(artboard, "p_hips")).translation;
        assert!(knee.x > hips.x - 1.0, "knee forward: {knee:?} hips {hips:?}");
    }
}
