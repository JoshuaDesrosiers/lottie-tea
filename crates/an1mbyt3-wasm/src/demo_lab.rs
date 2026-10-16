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
    PostEffect, PostEffectLayer, PropertyDef, Rigidbody2D, ShapeNode, StateDef, StateMachineDef, Stroke, StrokeCap, StrokeJoin, Transform3D, TransitionDef, UiAnchor, UiHtml,
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

/// A soft round glow: a radial gradient from `alpha` at the centre to
/// nothing at radius `r` — background atmosphere with no hard edge.
fn glow_disc(id: &str, r: f32, rgb: [f32; 3], alpha: f32, at: Vec2) -> Node {
    let stop = |offset: f32, a: f32| an1mbyt3_core::GradientStop { offset, color: [rgb[0], rgb[1], rgb[2], a], midpoint: 0.5 };
    let mut node = shape(id, Path::ellipse(r * 2.0, r * 2.0), [rgb[0], rgb[1], rgb[2], alpha], at, None, Vec::new());
    if let NodeKind::Shape(shape) = &mut node.kind {
        shape.gradient = Some(an1mbyt3_core::GradientDef {
            kind: an1mbyt3_core::GradientKind::Radial,
            start: Vec2::ZERO,
            end: Vec2::new(r, 0.0),
            stops: vec![stop(0.0, alpha), stop(0.45, alpha * 0.45), stop(1.0, 0.0)],
        });
    }
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
    let camera_id = camera(document, &mut inverses, scene, [0.03, 0.035, 0.07, 1.0]);
    // Backdrop: a soft violet bloom behind the stage and a scatter of dim
    // stars, so the scene reads as a dark, glowing space.
    add_node(document, &mut inverses, scene, glow_disc("haze", 300.0, [0.3, 0.16, 0.62], 0.45, Vec2::new(60.0, 20.0)));
    add_node(document, &mut inverses, scene, glow_disc("haze2", 190.0, [0.04, 0.4, 0.62], 0.35, Vec2::new(-160.0, -50.0)));
    for i in 0..36u32 {
        let h = i.wrapping_mul(2_654_435_761);
        let x = (h % 480) as f32 - 240.0;
        let y = ((h / 480) % 230) as f32 - 100.0;
        let r = 1.2 + (h % 3) as f32 * 0.6;
        add_node(document, &mut inverses, scene, shape(&format!("star{i}"), Path::ellipse(r * 2.0, r * 2.0), [0.75, 0.82, 1.0, 0.25 + (h % 5) as f32 * 0.1], Vec2::new(x, y), None, Vec::new()));
    }
    let ground = shape("ground", Path::rect(520.0, 24.0), [0.06, 0.07, 0.13, 1.0], Vec2::new(0.0, -123.0), None, vec![NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::new(520.0, 24.0), ..Default::default() })]);
    add_node(document, &mut inverses, scene, ground);
    add_node(document, &mut inverses, scene, shape("groundEdge", Path::rect(520.0, 2.5), [0.3, 0.92, 1.0, 1.0], Vec2::new(0.0, -111.0), None, Vec::new()));
    for i in 0..5 {
        let mut coin = shape(&format!("coin{i}"), Path::ellipse(16.0, 16.0), [1.0, 0.82, 0.22, 1.0], Vec2::new(-120.0 + i as f32 * 60.0, 40.0), Some(([1.0, 0.95, 0.65, 1.0], 2.5)), Vec::new());
        coin.classes = vec!["coin".into()];
        add_node(document, &mut inverses, scene, coin);
    }
    let crate_node = shape(
        "crate",
        round_rect(34.0, 34.0, 4.0),
        [0.45, 0.25, 0.9, 1.0],
        Vec2::new(150.0, -40.0),
        Some(([0.82, 0.7, 1.0, 1.0], 3.0)),
        vec![NodeBehavior::Rigidbody2D(Rigidbody2D { mass: 2.0, ..Default::default() }), NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::new(34.0, 34.0), ..Default::default() })],
    );
    add_node(document, &mut inverses, scene, crate_node);
    let ball = shape(
        "ball",
        Path::ellipse(26.0, 26.0),
        [0.3, 0.62, 1.0, 1.0],
        Vec2::new(100.0, 80.0),
        Some(([0.75, 0.9, 1.0, 1.0], 2.5)),
        vec![NodeBehavior::Rigidbody2D(Rigidbody2D { mass: 0.5, ..Default::default() }), NodeBehavior::CircleCollider2D(CircleCollider2D { radius: 13.0, ..Default::default() })],
    );
    add_node(document, &mut inverses, scene, ball);
    let player = shape("player", round_rect(32.0, 32.0, 8.0), [1.0, 0.5, 0.12, 1.0], Vec2::new(-150.0, -95.0), Some(([1.0, 0.85, 0.55, 1.0], 2.5)), Vec::new());
    add_node(document, &mut inverses, scene, player);
    let mut label = text_node(None, "label", "Script Lab", font, 16.0, [0.86, 0.9, 1.0, 1.0], Vec2::new(0.0, 108.0));
    label.behaviors.push(NodeBehaviorEntry {
        behavior: NodeBehavior::PropertyBindings(PropertyBindings { bindings: vec![Binding { property: "message".into(), target: BindTarget::Text, format: String::new(), scale: 1.0, offset: 0.0 }] }),
        enabled: true,
    });
    add_node(document, &mut inverses, scene, label);
    let mut hud = with_id(Node::new("hud", Some(camera_id), NodeKind::Group(Default::default())), "hud");
    hud.behaviors.push(NodeBehaviorEntry {
        behavior: NodeBehavior::UiHtml(UiHtml {
            html: "<div class=\"hud\"><b id=\"title\">HUD</b><div id=\"out\">ref.dom is this box</div></div>".into(),
            css: ".hud{box-sizing:border-box;width:100%;height:100%;padding:8px 10px;font:12px/1.4 system-ui,sans-serif;color:#e6ecff;background:rgb(16 18 36 / .72);border:1px solid rgb(140 160 255 / .28);border-radius:8px;box-shadow:0 0 18px rgb(90 110 255 / .25);backdrop-filter:blur(4px)}.hud b{display:block;font-size:11px;letter-spacing:.06em;color:#8fa2ff}".into(),
            width: 150.0,
            height: 54.0,
            interactive: true,
        }),
        enabled: true,
    });
    hud.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::UiAnchor(UiAnchor { anchor: Vec2::new(1.0, 1.0), offset: Vec2::new(-88.0, -40.0) }), enabled: true });
    add_node(document, &mut inverses, scene, hud);
    post(document, &mut inverses, scene);
    machine(document, &mut inverses, scene);
    inverses.reverse();
    document.commit_batch(inverses);
    LabDemo { scene }
}

/// Bloom on the bright shapes, a touch of grade, a vignette and fine grain.
fn post(document: &mut Document, inverses: &mut Vec<Command>, scene: ArtboardId) {
    inverses.push(document.apply(Command::SetPostEffects {
        artboard: scene,
        value: vec![
            PostEffectLayer { effect: PostEffect::Glow { radius: 5.0, threshold: 0.55, intensity: 0.9 }, enabled: true },
            PostEffectLayer { effect: PostEffect::ColorGrading { brightness: 0.0, contrast: 1.06, saturation: 1.15 }, enabled: true },
            PostEffectLayer { effect: PostEffect::Vignette { color: [0.0, 0.0, 0.03, 1.0], intensity: 0.5, radius: 0.72, blur: 0.55 }, enabled: true },
            PostEffectLayer { effect: PostEffect::FilmGrain { amount: 0.02, size: 1.0 }, enabled: true },
        ],
    }));
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
    let cam = Camera3D { fov_degrees: 50.0, near: 0.05, far: 200.0, current: true, clear_color: Some([0.03, 0.035, 0.07, 1.0]) };
    add_node(document, &mut inverses, scene, node3d("cam3d", Transform3D { position: Vec3::new(0.0, 2.6, 7.0), rotation_degrees: Vec3::new(-16.0, 0.0, 0.0), scale: Vec3::ONE }, an1mbyt3_core::Node3DContent::Camera(cam)));
    add_node(document, &mut inverses, scene, node3d("sun", Transform3D { position: Vec3::new(0.0, 6.0, 0.0), rotation_degrees: Vec3::new(-50.0, -30.0, 0.0), scale: Vec3::ONE }, an1mbyt3_core::Node3DContent::Light(Light3D { kind: Light3DKind::Directional, color: [1.0, 0.97, 0.9], intensity: 0.95, range: 10.0 })));
    add_node(document, &mut inverses, scene, node3d("floor", Transform3D::default(), mesh(Primitive3D::Plane { size: 20.0, subdivisions: 1 }, [0.1, 0.11, 0.17, 1.0])));
    add_node(document, &mut inverses, scene, node3d("cube", Transform3D::at(Vec3::new(-2.2, 0.6, 0.0)), mesh(Primitive3D::cube(), [0.95, 0.5, 0.18, 1.0])));
    add_node(document, &mut inverses, scene, node3d("sphere", Transform3D::at(Vec3::new(0.0, 0.6, 0.0)), mesh(Primitive3D::sphere(), [0.25, 0.47, 0.95, 1.0])));
    add_node(document, &mut inverses, scene, node3d("torus", Transform3D::at(Vec3::new(2.2, 0.7, 0.0)), mesh(Primitive3D::torus(), [0.95, 0.76, 0.2, 1.0])));
    post(document, &mut inverses, scene);
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
