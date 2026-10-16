//! **Script Lab** scenes for the docs' scripting pages: small, neutral
//! scenes with a known cast of nodes, whose program the docs page replaces
//! with the reader's code (the embed's `program` option), so every
//! scripting page can run its examples live and let the reader edit them.
//!
//! The 2D lab (`lab`, 480 × 270):
//!
//! | id | What |
//! |---|---|
//! | `player` | orange rounded square, no physics (move it from code) |
//! | `ball` | blue circle, Rigidbody 2D (dynamic) + Circle Collider |
//! | `crate` | wooden box, Rigidbody 2D + Box Collider |
//! | `ground` | static floor with a Box Collider |
//! | `coin0`…`coin4` | yellow coins, class `coin` |
//! | `label` | a text node at the top |
//! | `hud` | an HTML UI box (top-right), `ref.dom` |
//!
//! Scene Properties `score` (Number) and `message` (Text, bound to
//! `label`). State machine: `Calm` ⇄ `Excited` on Bool `excited`, trigger
//! `jump`, Number `speed` — every state runs the same (replaced) program.
//!
//! The 3D lab (`lab3d`, 480 × 270): a camera, a sun, a floor and a `cube`,
//! `sphere` and `torus`.

use an1mbyt3_core::{
    Animation, ArtboardId, BindTarget, Binding, BoxCollider2D, Camera3D, CameraNode, CircleCollider2D, Command, ConditionDef, Document, FontAsset, InputDef, InputKind, Light3D,
    Light3DKind, LoopMode, Mesh3D, Node, NodeBehavior, NodeBehaviorEntry, NodeId, NodeKind, Path, PlaybackDirection, Primitive3D, ProgramDef, PropKind, PropertyBindings,
    PropertyDef, Rigidbody2D, ShapeNode, StateDef, StateMachineDef, Stroke, StrokeCap, StrokeJoin, Transform3D, TransitionDef, UiAnchor, UiHtml,
};
use glam::{Vec2, Vec3};

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/RussoOne-Regular.ttf");

pub(crate) struct LabDemo {
    pub scene: ArtboardId,
}

/// The starting program: a comment telling the reader what's there.
const STARTER: &str = "// Script Lab. Nodes: player, ball, crate, ground, coin0..coin4, label, hud.\n// Edit this code and press Run.\nconst player = getCompByID('player')\nplayer.x = Math.sin(ctx.time) * 120\n";

fn shape(id: &str, path: Path, color: [f32; 4], at: Vec2, stroke: Option<([f32; 4], f32)>, behaviors: Vec<NodeBehavior>) -> Node {
    let stroke = stroke.map(|(color, width)| Stroke { width, color, gradient: None, image: None, cap: StrokeCap::Round, join: StrokeJoin::Round });
    let mut node = with_id(Node::new(id, None, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke })), id);
    node.transform.translation = at;
    node.behaviors = behaviors.into_iter().map(|behavior| NodeBehaviorEntry { behavior, enabled: true }).collect();
    node
}

/// A rounded rectangle centred on the origin.
fn round_rect(w: f32, h: f32, r: f32) -> Path {
    use an1mbyt3_core::PathCommand as C;
    const K: f32 = 0.552_284_8;
    let (x, y, k) = (w / 2.0, h / 2.0, r * K);
    let v = Vec2::new;
    Path {
        commands: vec![
            C::MoveTo(v(-x + r, y)),
            C::LineTo(v(x - r, y)),
            C::CubicTo { control1: v(x - r + k, y), control2: v(x, y - r + k), to: v(x, y - r) },
            C::LineTo(v(x, -y + r)),
            C::CubicTo { control1: v(x, -y + r - k), control2: v(x - r + k, -y), to: v(x - r, -y) },
            C::LineTo(v(-x + r, -y)),
            C::CubicTo { control1: v(-x + r - k, -y), control2: v(-x, -y + r - k), to: v(-x, -y + r) },
            C::LineTo(v(-x, y - r)),
            C::CubicTo { control1: v(-x, y - r + k), control2: v(-x + r - k, y), to: v(-x + r, y) },
            C::Close,
        ],
    }
}

fn camera(document: &mut Document, inverses: &mut Vec<Command>, scene: ArtboardId, bg: [f32; 4]) -> NodeId {
    let camera = with_id(
        Node::with_components("Main Camera", None, vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: bg, follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })]),
        "camera",
    );
    let id = add_node(document, inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(id) }));
    id
}

fn machine(document: &mut Document, inverses: &mut Vec<Command>, scene: ArtboardId) {
    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Lab (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    let go = |to: &str, value: bool| TransitionDef { to: to.into(), condition: ConditionDef::BoolIs { input: "excited".into(), value }, direction: PlaybackDirection::Forward, duration_seconds: 0.0, interruptible: true };
    let state = |name: &str, to: TransitionDef, x: f32| StateDef { name: name.into(), animation, loop_mode: LoopMode::Once, transitions: vec![to], position: Vec2::new(x, 0.0), blend: None, program: Some(ProgramDef::Code(STARTER.into())) };
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Lab".into(),
            inputs: vec![
                InputDef { name: "excited".into(), kind: InputKind::Bool },
                InputDef { name: "jump".into(), kind: InputKind::Trigger },
                InputDef { name: "speed".into(), kind: InputKind::Number },
            ],
            states: vec![state("Calm", go("Excited", true), 0.0), state("Excited", go("Calm", false), 220.0)],
            initial_state: "Calm".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));
}

pub(crate) fn build_lab(document: &mut Document) -> LabDemo {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Russo One".into(), data: FONT_BYTES.to_vec() });
    let inverse = document.apply(Command::AddArtboard { name: "Script Lab".into(), width: 480.0, height: 270.0 });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);
    for (name, kind, default) in [("score", PropKind::Number, "0"), ("message", PropKind::Text, "\"Script Lab\"")] {
        inverses.push(document.apply(Command::AddProperty { property: PropertyDef { artboard: scene, name: name.into(), kind, default: default.into(), deleted: false } }));
    }
    let camera_id = camera(document, &mut inverses, scene, [0.96, 0.96, 0.95, 1.0]);
    let ground = shape("ground", Path::rect(520.0, 24.0), [0.82, 0.81, 0.78, 1.0], Vec2::new(0.0, -123.0), None, vec![NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::new(520.0, 24.0), ..Default::default() })]);
    add_node(document, &mut inverses, scene, ground);
    for i in 0..5 {
        let mut coin = shape(&format!("coin{i}"), Path::ellipse(16.0, 16.0), [0.98, 0.76, 0.18, 1.0], Vec2::new(-120.0 + i as f32 * 60.0, 40.0), Some(([0.85, 0.58, 0.1, 1.0], 2.5)), Vec::new());
        coin.classes = vec!["coin".into()];
        add_node(document, &mut inverses, scene, coin);
    }
    let crate_node = shape(
        "crate",
        round_rect(34.0, 34.0, 4.0),
        [0.78, 0.55, 0.32, 1.0],
        Vec2::new(150.0, -40.0),
        Some(([0.55, 0.36, 0.18, 1.0], 3.0)),
        vec![NodeBehavior::Rigidbody2D(Rigidbody2D { mass: 2.0, ..Default::default() }), NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::new(34.0, 34.0), ..Default::default() })],
    );
    add_node(document, &mut inverses, scene, crate_node);
    let ball = shape(
        "ball",
        Path::ellipse(26.0, 26.0),
        [0.25, 0.47, 0.95, 1.0],
        Vec2::new(100.0, 80.0),
        None,
        vec![NodeBehavior::Rigidbody2D(Rigidbody2D { mass: 0.5, ..Default::default() }), NodeBehavior::CircleCollider2D(CircleCollider2D { radius: 13.0, ..Default::default() })],
    );
    add_node(document, &mut inverses, scene, ball);
    let player = shape("player", round_rect(32.0, 32.0, 8.0), [0.96, 0.55, 0.15, 1.0], Vec2::new(-150.0, -95.0), None, Vec::new());
    add_node(document, &mut inverses, scene, player);
    let mut label = text_node(None, "label", "Script Lab", font, 16.0, [0.2, 0.2, 0.24, 1.0], Vec2::new(0.0, 108.0));
    label.behaviors.push(NodeBehaviorEntry {
        behavior: NodeBehavior::PropertyBindings(PropertyBindings { bindings: vec![Binding { property: "message".into(), target: BindTarget::Text, format: String::new(), scale: 1.0, offset: 0.0 }] }),
        enabled: true,
    });
    add_node(document, &mut inverses, scene, label);
    let mut hud = with_id(Node::new("hud", Some(camera_id), NodeKind::Group(Default::default())), "hud");
    hud.behaviors.push(NodeBehaviorEntry {
        behavior: NodeBehavior::UiHtml(UiHtml {
            html: "<div class=\"hud\"><b id=\"title\">HUD</b><div id=\"out\">ref.dom is this box</div></div>".into(),
            css: ".hud{box-sizing:border-box;width:100%;height:100%;padding:8px 10px;font:12px/1.4 system-ui,sans-serif;color:#1c1c20;background:#fff;border:1px solid #e3e2de;border-radius:8px}.hud b{display:block;font-size:11px;letter-spacing:.06em;color:#8a8984}".into(),
            width: 150.0,
            height: 54.0,
            interactive: true,
        }),
        enabled: true,
    });
    hud.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::UiAnchor(UiAnchor { anchor: Vec2::new(1.0, 1.0), offset: Vec2::new(-88.0, -40.0) }), enabled: true });
    add_node(document, &mut inverses, scene, hud);
    machine(document, &mut inverses, scene);
    inverses.reverse();
    document.commit_batch(inverses);
    LabDemo { scene }
}

fn node3d(id: &str, transform: Transform3D, content: an1mbyt3_core::Node3DContent) -> Node {
    with_id(Node::new(id, None, NodeKind::Node3D(an1mbyt3_core::Node3D::new(transform, content))), id)
}

fn mesh(primitive: Primitive3D, color: [f32; 4]) -> an1mbyt3_core::Node3DContent {
    an1mbyt3_core::Node3DContent::Mesh(Mesh3D { primitive, color, shininess: 0.5, unlit: false, material: None, animation: None, animation_loop: true, animation_overlay: None })
}

pub(crate) fn build_lab3d(document: &mut Document) -> LabDemo {
    let mut inverses = Vec::new();
    let inverse = document.apply(Command::AddArtboard { name: "Script Lab 3D".into(), width: 480.0, height: 270.0 });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);
    camera(document, &mut inverses, scene, [0.1, 0.1, 0.12, 1.0]);
    let cam = Camera3D { fov_degrees: 50.0, near: 0.05, far: 200.0, current: true, clear_color: Some([0.86, 0.88, 0.92, 1.0]) };
    add_node(document, &mut inverses, scene, node3d("cam3d", Transform3D { position: Vec3::new(0.0, 2.6, 7.0), rotation_degrees: Vec3::new(-16.0, 0.0, 0.0), scale: Vec3::ONE }, an1mbyt3_core::Node3DContent::Camera(cam)));
    add_node(document, &mut inverses, scene, node3d("sun", Transform3D { position: Vec3::new(0.0, 6.0, 0.0), rotation_degrees: Vec3::new(-50.0, -30.0, 0.0), scale: Vec3::ONE }, an1mbyt3_core::Node3DContent::Light(Light3D { kind: Light3DKind::Directional, color: [1.0, 0.97, 0.9], intensity: 0.95, range: 10.0 })));
    add_node(document, &mut inverses, scene, node3d("floor", Transform3D::default(), mesh(Primitive3D::Plane { size: 20.0, subdivisions: 1 }, [0.74, 0.75, 0.78, 1.0])));
    add_node(document, &mut inverses, scene, node3d("cube", Transform3D::at(Vec3::new(-2.2, 0.6, 0.0)), mesh(Primitive3D::cube(), [0.95, 0.5, 0.18, 1.0])));
    add_node(document, &mut inverses, scene, node3d("sphere", Transform3D::at(Vec3::new(0.0, 0.6, 0.0)), mesh(Primitive3D::sphere(), [0.25, 0.47, 0.95, 1.0])));
    add_node(document, &mut inverses, scene, node3d("torus", Transform3D::at(Vec3::new(2.2, 0.7, 0.0)), mesh(Primitive3D::torus(), [0.95, 0.76, 0.2, 1.0])));
    machine(document, &mut inverses, scene);
    inverses.reverse();
    document.commit_batch(inverses);
    LabDemo { scene }
}

pub(crate) fn build(document: &mut Document, kind: &str) -> Option<LabDemo> {
    Some(match kind {
        "lab" => build_lab(document),
        "lab3d" => build_lab3d(document),
        _ => return None,
    })
}
