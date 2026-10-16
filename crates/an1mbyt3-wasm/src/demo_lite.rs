//! **Toasty Lite components** — the small interactive vector animations the
//! docs site uses everywhere (and ships as copy-paste examples): a like
//! heart, a menu ↔ close button, a light / dark switch, a form status icon,
//! a notification bell, a download button with progress, a copy button, a
//! link arrow, a loader and the Toasty mascot.
//!
//! Every one is built only from things Toasty Lite draws — vector shapes,
//! strokes with **trim paths**, path morphs, keyframed animations and a
//! state machine driven by inputs — with **no scripts**: clicks and hovers
//! reach the state machine through On Click and Pointer Listeners actions
//! (Toggle Input / Set Input / Fire Trigger), and the page talks to it
//! through the same inputs plus a few Properties (`ink`, `progress`). So
//! each one runs identically in the full engine and in the Lite runtime,
//! and each is a template people can open, restyle and re-export.
//!
//! Conventions: a 96 × 96 scene, origin in the middle, y up, transparent
//! background, an `ink` Color property bound to the line colour (so a
//! site can match its light / dark theme), an invisible `hit` shape on top
//! carrying the UI Button that detects the pointer.

use an1mbyt3_core::{
    ActionKind, Animation, ArtboardId, BindTarget, Binding, CameraNode, Command, ConditionDef, Document, Easing, EventAction, InputDef, InputKind, Keyframe, LoopMode, Node, NodeBehavior,
    NodeBehaviorEntry, NodeId, NodeKind, Path, PathCommand, PlaybackDirection, PropKind, PropertyBindings, PropertyDef, PropertyTrack, ShapeNode, StateDef, StateMachineDef, StateMachineLayerDef,
    Stroke, StrokeCap, StrokeJoin, StrokeStyle2D, Track, TransitionDef, UiButton, UiEvents, UiListeners, UiStateStyle,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, with_id};

/// What every builder returns: the scene to switch to (and export).
pub(crate) struct LiteDemo {
    pub scene: ArtboardId,
}

// ---------------------------------------------------------------------
// Palette (straight 0..1 RGBA, as the renderer writes it)
// ---------------------------------------------------------------------

const INK: [f32; 4] = [0.11, 0.11, 0.13, 1.0];
const RED: [f32; 4] = [0.95, 0.23, 0.37, 1.0];
const AMBER: [f32; 4] = [0.96, 0.65, 0.14, 1.0];
const GREEN: [f32; 4] = [0.13, 0.63, 0.42, 1.0];
const MOON: [f32; 4] = [0.45, 0.43, 0.84, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 0.0];

fn fade(c: [f32; 4], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

// ---------------------------------------------------------------------
// Easing presets (cubic-bezier, CSS-style)
// ---------------------------------------------------------------------

#[derive(Clone, Copy)]
enum E {
    Linear,
    Out,
    InOut,
    Back,
    In,
}

fn ease(e: E) -> Easing {
    let (x1, y1, x2, y2) = match e {
        E::Linear => return Easing::Linear,
        E::Out => (0.22, 1.0, 0.36, 1.0),
        E::InOut => (0.65, 0.0, 0.35, 1.0),
        E::Back => (0.34, 1.56, 0.64, 1.0),
        E::In => (0.55, 0.0, 1.0, 0.45),
    };
    Easing::CubicBezier { x1, y1, x2, y2 }
}

// ---------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------

fn v(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

/// An open polyline.
fn line(points: &[(f32, f32)]) -> Path {
    let mut commands = vec![PathCommand::MoveTo(v(points[0].0, points[0].1))];
    commands.extend(points[1..].iter().map(|p| PathCommand::LineTo(v(p.0, p.1))));
    Path { commands }
}

/// A circle starting at the top, running clockwise on screen (so a trim
/// path draws it like a clock hand).
fn circle_cw(r: f32) -> Path {
    const K: f32 = 0.552_284_8;
    let k = r * K;
    Path {
        commands: vec![
            PathCommand::MoveTo(v(0.0, r)),
            PathCommand::CubicTo { control1: v(k, r), control2: v(r, k), to: v(r, 0.0) },
            PathCommand::CubicTo { control1: v(r, -k), control2: v(k, -r), to: v(0.0, -r) },
            PathCommand::CubicTo { control1: v(-k, -r), control2: v(-r, -k), to: v(-r, 0.0) },
            PathCommand::CubicTo { control1: v(-r, k), control2: v(-k, r), to: v(0.0, r) },
            PathCommand::Close,
        ],
    }
}

/// A rounded rectangle `w` × `h`, corner radius `r`, starting top-centre.
fn round_rect(w: f32, h: f32, r: f32) -> Path {
    const K: f32 = 0.552_284_8;
    let (x, y) = (w / 2.0, h / 2.0);
    let k = r * K;
    Path {
        commands: vec![
            PathCommand::MoveTo(v(0.0, y)),
            PathCommand::LineTo(v(x - r, y)),
            PathCommand::CubicTo { control1: v(x - r + k, y), control2: v(x, y - r + k), to: v(x, y - r) },
            PathCommand::LineTo(v(x, -y + r)),
            PathCommand::CubicTo { control1: v(x, -y + r - k), control2: v(x - r + k, -y), to: v(x - r, -y) },
            PathCommand::LineTo(v(-x + r, -y)),
            PathCommand::CubicTo { control1: v(-x + r - k, -y), control2: v(-x, -y + r - k), to: v(-x, -y + r) },
            PathCommand::LineTo(v(-x, y - r)),
            PathCommand::CubicTo { control1: v(-x, y - r + k), control2: v(-x + r - k, y), to: v(-x + r, y) },
            PathCommand::Close,
        ],
    }
}

/// A heart about `46 × 42` (scaled by `s`), point down, centred.
fn heart(s: f32) -> Path {
    let p = |x: f32, y: f32| v(x * s, (y - 4.0) * s);
    Path {
        commands: vec![
            PathCommand::MoveTo(p(0.0, -40.0)),
            PathCommand::CubicTo { control1: p(-20.0, -25.0), control2: p(-50.0, -5.0), to: p(-50.0, 18.0) },
            PathCommand::CubicTo { control1: p(-50.0, 38.0), control2: p(-40.0, 48.0), to: p(-25.0, 48.0) },
            PathCommand::CubicTo { control1: p(-12.0, 48.0), control2: p(-3.0, 40.0), to: p(0.0, 30.0) },
            PathCommand::CubicTo { control1: p(3.0, 40.0), control2: p(12.0, 48.0), to: p(25.0, 48.0) },
            PathCommand::CubicTo { control1: p(40.0, 48.0), control2: p(50.0, 38.0), to: p(50.0, 18.0) },
            PathCommand::CubicTo { control1: p(50.0, -5.0), control2: p(20.0, -25.0), to: p(0.0, -40.0) },
            PathCommand::Close,
        ],
    }
}

/// The sun / moon body: a circle of radius `r` whose right half bulges out
/// to `a` (`a = r` is a full circle; a negative `a` carves a crescent).
/// Same command structure for every `a`, so it morphs.
fn moon(r: f32, a: f32) -> Path {
    const K: f32 = 0.552_284_8;
    Path {
        commands: vec![
            PathCommand::MoveTo(v(0.0, r)),
            PathCommand::CubicTo { control1: v(a * K, r), control2: v(a, r * K), to: v(a, 0.0) },
            PathCommand::CubicTo { control1: v(a, -r * K), control2: v(a * K, -r), to: v(0.0, -r) },
            PathCommand::CubicTo { control1: v(-r * K, -r), control2: v(-r, -r * K), to: v(-r, 0.0) },
            PathCommand::CubicTo { control1: v(-r, r * K), control2: v(-r * K, r), to: v(0.0, r) },
            PathCommand::Close,
        ],
    }
}

/// A four-point sparkle.
fn sparkle(r: f32) -> Path {
    let i = r * 0.28;
    Path {
        commands: vec![
            PathCommand::MoveTo(v(0.0, r)),
            PathCommand::CubicTo { control1: v(i * 0.3, i), control2: v(i, i * 0.3), to: v(r, 0.0) },
            PathCommand::CubicTo { control1: v(i, -i * 0.3), control2: v(i * 0.3, -i), to: v(0.0, -r) },
            PathCommand::CubicTo { control1: v(-i * 0.3, -i), control2: v(-i, -i * 0.3), to: v(-r, 0.0) },
            PathCommand::CubicTo { control1: v(-i, i * 0.3), control2: v(-i * 0.3, i), to: v(0.0, r) },
            PathCommand::Close,
        ],
    }
}

// ---------------------------------------------------------------------
// The scene kit
// ---------------------------------------------------------------------

struct Kit<'a> {
    document: &'a mut Document,
    inverses: Vec<Command>,
    scene: ArtboardId,
    animations: Vec<Animation>,
}

impl<'a> Kit<'a> {
    fn new(document: &'a mut Document, name: &str, width: f32, height: f32) -> Self {
        let mut inverses = Vec::new();
        let inverse = document.apply(Command::AddArtboard { name: name.into(), width, height });
        let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
        inverses.push(inverse);
        let mut kit = Kit { document, inverses, scene, animations: Vec::new() };
        let camera = with_id(
            Node::with_components(
                "Main Camera",
                None,
                vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: CLEAR, follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })],
            ),
            "camera",
        );
        let camera_id = kit.add(camera);
        let inverse = kit.document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) });
        kit.inverses.push(inverse);
        kit
    }

    fn add(&mut self, node: Node) -> NodeId {
        add_node(self.document, &mut self.inverses, self.scene, node)
    }

    fn group(&mut self, id: &str, parent: Option<NodeId>, at: Vec2) -> NodeId {
        let mut node = with_id(Node::new(id, parent, NodeKind::Group(Default::default())), id);
        node.transform.translation = at;
        self.add(node)
    }

    /// A filled shape (no stroke).
    fn fill(&mut self, id: &str, parent: Option<NodeId>, path: Path, color: [f32; 4], at: Vec2) -> NodeId {
        let mut node = with_id(Node::new(id, parent, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None })), id);
        node.transform.translation = at;
        self.add(node)
    }

    /// A stroked line with round caps and a Stroke Style (trim path, no
    /// dashes): `trim` is the starting (trim start, trim end).
    #[allow(clippy::too_many_arguments)]
    fn stroke(&mut self, id: &str, parent: Option<NodeId>, path: Path, color: [f32; 4], width: f32, at: Vec2, trim: (f32, f32)) -> NodeId {
        let stroke = Stroke { width, color, gradient: None, image: None, cap: StrokeCap::Round, join: StrokeJoin::Round };
        let mut node = with_id(Node::new(id, parent, NodeKind::Shape(ShapeNode { path, fill_color: CLEAR, blend_mode: Default::default(), gradient: None, image: None, stroke: Some(stroke) })), id);
        node.transform.translation = at;
        node.behaviors.push(NodeBehaviorEntry {
            behavior: NodeBehavior::StrokeStyle2D(StrokeStyle2D { dash: 0.0, gap: 0.0, dash_offset: 0.0, trim_start: trim.0, trim_end: trim.1, trim_offset: 0.0 }),
            enabled: true,
        });
        self.add(node)
    }

    /// The invisible pointer target: a UI Button (no look of its own) with
    /// On Click and Pointer Listeners actions.
    fn hit(&mut self, path: Path, on_click: Vec<ActionKind>, hover_input: Option<&str>) -> NodeId {
        let mut node = with_id(Node::new("hit", None, NodeKind::Shape(ShapeNode { path, fill_color: CLEAR, blend_mode: Default::default(), gradient: None, image: None, stroke: None })), "hit");
        let style = UiStateStyle { highlighted_color: WHITE, pressed_color: WHITE, pressed_scale: 1.0, highlighted_scale: 1.0, ..Default::default() };
        node.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::UiButton(UiButton { interactable: true, style }), enabled: true });
        let act = |action: ActionKind| EventAction { target: None, action };
        node.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::UiEvents(UiEvents { on_click: on_click.into_iter().map(act).collect() }), enabled: true });
        if let Some(input) = hover_input {
            node.behaviors.push(NodeBehaviorEntry {
                behavior: NodeBehavior::UiListeners(UiListeners {
                    on_enter: vec![act(ActionKind::SetInput { input: input.into(), value: "true".into() })],
                    on_exit: vec![act(ActionKind::SetInput { input: input.into(), value: "false".into() })],
                    on_down: Vec::new(),
                    on_up: Vec::new(),
                }),
                enabled: true,
            });
        }
        self.add(node)
    }

    fn property(&mut self, name: &str, kind: PropKind, default: &str) {
        let inverse = self.document.apply(Command::AddProperty { property: PropertyDef { artboard: self.scene, name: name.into(), kind, default: default.into(), deleted: false } });
        self.inverses.push(inverse);
    }

    /// Binds node `id`'s stroke colour (or fill colour) to the `ink` property.
    fn bind(&mut self, nodes: &[NodeId], property: &str, target: BindTarget) {
        for &id in nodes {
            let binding = Binding { property: property.into(), target: target.clone(), format: String::new(), scale: 1.0, offset: 0.0 };
            if let Some(node) = self.document.artboard_mut(self.scene).and_then(|a| a.node_mut(id)) {
                if let Some(NodeBehavior::PropertyBindings(b)) = node.behaviors.iter_mut().map(|e| &mut e.behavior).find(|b| matches!(b, NodeBehavior::PropertyBindings(_))) {
                    b.bindings.push(binding);
                } else {
                    node.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::PropertyBindings(PropertyBindings { bindings: vec![binding] }), enabled: true });
                }
            }
        }
    }

    fn ink(&mut self, strokes: &[NodeId]) {
        self.bind(strokes, "ink", BindTarget::ComponentField { component: "Shape".into(), field: "strokeColor".into() });
    }

    /// Registers an animation; returns its index for `finish`.
    fn anim(&mut self, a: Anim) -> usize {
        self.animations.push(a.0);
        self.animations.len() - 1
    }

    /// Adds the animations and the state machine, commits the build.
    fn finish(mut self, inputs: Vec<InputDef>, base: Vec<St>, layers: Vec<(&str, Vec<NodeId>, Vec<St>)>) -> LiteDemo {
        let mut ids = Vec::new();
        for animation in std::mem::take(&mut self.animations) {
            let inverse = self.document.apply(Command::AddAnimation { artboard: self.scene, animation });
            let Command::SetAnimationDeleted { animation: id, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
            self.inverses.push(inverse);
            ids.push(id);
        }
        let states = |list: Vec<St>| -> Vec<StateDef> {
            list.into_iter()
                .enumerate()
                .map(|(i, s)| StateDef {
                    name: s.name.into(),
                    animation: ids[s.anim],
                    loop_mode: s.looping,
                    transitions: s.to,
                    position: v(i as f32 * 220.0, 0.0),
                    blend: None,
                    program: None,
                })
                .collect()
        };
        let initial = base[0].name.to_string();
        let layers = layers
            .into_iter()
            .map(|(name, mask, list)| {
                let initial_state = list[0].name.to_string();
                StateMachineLayerDef { name: name.into(), states: states(list), initial_state, mask }
            })
            .collect();
        let inverse = self.document.apply(Command::AddStateMachine {
            artboard: self.scene,
            state_machine: StateMachineDef { name: "Machine".into(), inputs, states: states(base), initial_state: initial, layers, deleted: false },
        });
        self.inverses.push(inverse);
        let scene = self.scene;
        self.inverses.reverse();
        self.document.commit_batch(std::mem::take(&mut self.inverses));
        LiteDemo { scene }
    }
}

/// An animation under construction: `(time, value, easing out of it)` keys.
struct Anim(Animation);

impl Anim {
    fn new(name: &str, duration: f32) -> Self {
        let mut a = Animation::new(name);
        a.duration = duration;
        Anim(a)
    }
    fn pos(mut self, node: NodeId, keys: &[(f32, f32, f32, E)]) -> Self {
        self.0.tracks.push((node, PropertyTrack::Translation(Track::new(keys.iter().map(|k| Keyframe::new(k.0, v(k.1, k.2), ease(k.3))).collect()))));
        self
    }
    /// Rotation keys in degrees.
    fn rot(mut self, node: NodeId, keys: &[(f32, f32, E)]) -> Self {
        self.0.tracks.push((node, PropertyTrack::Rotation(Track::new(keys.iter().map(|k| Keyframe::new(k.0, k.1.to_radians(), ease(k.2))).collect()))));
        self
    }
    /// Uniform scale keys.
    fn scale(mut self, node: NodeId, keys: &[(f32, f32, E)]) -> Self {
        self.0.tracks.push((node, PropertyTrack::Scale(Track::new(keys.iter().map(|k| Keyframe::new(k.0, Vec2::splat(k.1), ease(k.2))).collect()))));
        self
    }
    fn scale_xy(mut self, node: NodeId, keys: &[(f32, f32, f32, E)]) -> Self {
        self.0.tracks.push((node, PropertyTrack::Scale(Track::new(keys.iter().map(|k| Keyframe::new(k.0, v(k.1, k.2), ease(k.3))).collect()))));
        self
    }
    fn fill(mut self, node: NodeId, keys: &[(f32, [f32; 4], E)]) -> Self {
        self.0.tracks.push((node, PropertyTrack::FillColor(Track::new(keys.iter().map(|k| Keyframe::new(k.0, k.1, ease(k.2))).collect()))));
        self
    }
    fn morph(mut self, node: NodeId, keys: Vec<(f32, Path, E)>) -> Self {
        self.0.tracks.push((node, PropertyTrack::Path(Track::new(keys.into_iter().map(|k| Keyframe::new(k.0, k.1, ease(k.2))).collect()))));
        self
    }
    /// Any animatable field (`behavior/StrokeStyle2D/trimEnd`, …), 1 number.
    fn field(mut self, node: NodeId, path: &str, keys: &[(f32, f32, E)]) -> Self {
        self.0.tracks.push((node, PropertyTrack::Field { path: path.into(), track: Track::new(keys.iter().map(|k| Keyframe::new(k.0, [k.1, 0.0, 0.0, 0.0], ease(k.2))).collect()) }));
        self
    }
    fn color_field(mut self, node: NodeId, path: &str, keys: &[(f32, [f32; 4], E)]) -> Self {
        self.0.tracks.push((node, PropertyTrack::Field { path: path.into(), track: Track::new(keys.iter().map(|k| Keyframe::new(k.0, k.1, ease(k.2))).collect()) }));
        self
    }
    fn trim_end(self, node: NodeId, keys: &[(f32, f32, E)]) -> Self {
        self.field(node, "behavior/StrokeStyle2D/trimEnd", keys)
    }
    fn trim_start(self, node: NodeId, keys: &[(f32, f32, E)]) -> Self {
        self.field(node, "behavior/StrokeStyle2D/trimStart", keys)
    }
    fn trim_offset(self, node: NodeId, keys: &[(f32, f32, E)]) -> Self {
        self.field(node, "behavior/StrokeStyle2D/trimOffset", keys)
    }
    fn stroke_width(self, node: NodeId, keys: &[(f32, f32, E)]) -> Self {
        self.field(node, "kind/Shape/stroke/width", keys)
    }
    fn stroke_color(self, node: NodeId, keys: &[(f32, [f32; 4], E)]) -> Self {
        self.color_field(node, "kind/Shape/stroke/color", keys)
    }
}

/// A state: name, animation index, loop mode, transitions out.
struct St {
    name: &'static str,
    anim: usize,
    looping: LoopMode,
    to: Vec<TransitionDef>,
}

fn st(name: &'static str, anim: usize, to: Vec<TransitionDef>) -> St {
    St { name, anim, looping: LoopMode::Once, to }
}

fn st_loop(name: &'static str, anim: usize, to: Vec<TransitionDef>) -> St {
    St { name, anim, looping: LoopMode::Loop, to }
}

fn go(to: &str, condition: ConditionDef, fade: f32) -> TransitionDef {
    TransitionDef { to: to.into(), condition, direction: PlaybackDirection::Forward, duration_seconds: fade, interruptible: true }
}
fn when(input: &str, value: bool) -> ConditionDef {
    ConditionDef::BoolIs { input: input.into(), value }
}
fn on_trigger(input: &str) -> ConditionDef {
    ConditionDef::Trigger { input: input.into() }
}
fn is(input: &str, value: &str) -> ConditionDef {
    ConditionDef::EnumIs { input: input.into(), value: value.into() }
}
fn input(name: &str, kind: InputKind) -> InputDef {
    InputDef { name: name.into(), kind }
}

/// The standard hover layer: `Rest ⇄ Over` on the `hover` input, scaling
/// `target` up a little.
fn hover_layer(kit: &mut Kit, target: NodeId, over: f32) -> (&'static str, Vec<NodeId>, Vec<St>) {
    let rest = kit.anim(Anim::new("Hover off", 0.2).scale(target, &[(0.0, 1.0, E::Linear)]));
    let on = kit.anim(Anim::new("Hover on", 0.2).scale(target, &[(0.0, over, E::Linear)]));
    ("Hover", vec![target], vec![st("Rest", rest, vec![go("Over", when("hover", true), 0.16)]), st("Over", on, vec![go("Rest", when("hover", false), 0.2)])])
}

const INK_JSON: &str = "[0.11,0.11,0.13,1]";

// ---------------------------------------------------------------------
// 1. Like heart
// ---------------------------------------------------------------------

/// **Heart**: click toggles `liked` (On Click → Toggle Input). Liking pops
/// the fill, bursts eight trim-path rays and a ring; unliking crossfades
/// back. Hover (Pointer Listeners → `hover`) grows it a little. Pages
/// either let it toggle itself and listen for the `Liked` / `Unliked`
/// state, or own the value and set `liked` themselves.
pub(crate) fn build_heart(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Heart", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let burst = kit.group("burst", None, v(0.0, 0.0));
    let mut rays = Vec::new();
    for i in 0..8 {
        let a = (i as f32 * 45.0 + 22.5).to_radians();
        let (c, s) = (a.cos(), a.sin());
        rays.push(kit.stroke(&format!("ray{i}"), Some(burst), line(&[(c * 30.0, s * 30.0), (c * 42.0, s * 42.0)]), if i % 2 == 0 { RED } else { AMBER }, 4.0, Vec2::ZERO, (0.0, 0.0)));
    }
    let ring = kit.stroke("ring", None, circle_cw(26.0), RED, 0.0, Vec2::ZERO, (0.0, 1.0));
    let heart_group = kit.group("heartGroup", None, Vec2::ZERO);
    let outline = kit.stroke("outline", Some(heart_group), heart(0.5), INK, 4.5, Vec2::ZERO, (0.0, 1.0));
    let fill = kit.fill("fill", Some(heart_group), heart(0.53), RED, Vec2::ZERO);
    kit.ink(&[outline]);
    // The fill starts collapsed.
    if let Some(n) = kit.document.artboard_mut(kit.scene).and_then(|a| a.node_mut(fill)) {
        n.transform.scale = Vec2::ZERO;
    }
    kit.hit(Path::ellipse(84.0, 84.0), vec![ActionKind::ToggleInput { input: "liked".into() }], Some("hover"));

    let mut unliked = Anim::new("Unliked", 0.3).scale(fill, &[(0.0, 0.0, E::Linear)]).stroke_width(outline, &[(0.0, 4.5, E::Linear)]).stroke_width(ring, &[(0.0, 0.0, E::Linear)]);
    let mut liked = Anim::new("Liked", 0.7)
        .scale(fill, &[(0.0, 0.0, E::Out), (0.2, 1.22, E::InOut), (0.32, 0.94, E::InOut), (0.44, 1.0, E::Linear)])
        .stroke_width(outline, &[(0.0, 4.5, E::Out), (0.15, 0.0, E::Linear)])
        .scale(ring, &[(0.0, 0.5, E::Out), (0.45, 1.5, E::Linear)])
        .stroke_width(ring, &[(0.0, 10.0, E::Out), (0.45, 0.0, E::Linear)]);
    for &ray in &rays {
        unliked = unliked.trim_end(ray, &[(0.0, 0.0, E::Linear)]).trim_start(ray, &[(0.0, 0.0, E::Linear)]);
        liked = liked.trim_end(ray, &[(0.0, 0.0, E::Linear), (0.1, 0.0, E::Out), (0.3, 1.0, E::Linear)]).trim_start(ray, &[(0.0, 0.0, E::Linear), (0.22, 0.0, E::Out), (0.5, 1.0, E::Linear)]);
    }
    let unliked = kit.anim(unliked);
    let liked = kit.anim(liked);
    let hover = hover_layer(&mut kit, heart_group, 1.12);
    kit.finish(
        vec![input("liked", InputKind::Bool), input("hover", InputKind::Bool)],
        vec![st("Unliked", unliked, vec![go("Liked", when("liked", true), 0.0)]), st("Liked", liked, vec![go("Unliked", when("liked", false), 0.18)])],
        vec![hover],
    )
}

// ---------------------------------------------------------------------
// 2. Menu ↔ close
// ---------------------------------------------------------------------

/// **Menu**: three bars fold into an ✕ when `open` (click toggles it).
pub(crate) fn build_menu(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Menu", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let g = kit.group("bars", None, Vec2::ZERO);
    let bar = |_: f32| line(&[(-20.0, 0.0), (20.0, 0.0)]);
    let top = kit.stroke("top", Some(g), bar(14.0), INK, 5.0, v(0.0, 13.0), (0.0, 1.0));
    let mid = kit.stroke("middle", Some(g), bar(0.0), INK, 5.0, v(0.0, 0.0), (0.0, 1.0));
    let bot = kit.stroke("bottom", Some(g), bar(-14.0), INK, 5.0, v(0.0, -13.0), (0.0, 1.0));
    kit.ink(&[top, mid, bot]);
    kit.hit(round_rect(80.0, 80.0, 16.0), vec![ActionKind::ToggleInput { input: "open".into() }], Some("hover"));
    let closed = kit.anim(
        Anim::new("Closed", 0.3)
            .pos(top, &[(0.0, 0.0, 13.0, E::Linear)])
            .rot(top, &[(0.0, 0.0, E::Linear)])
            .pos(bot, &[(0.0, 0.0, -13.0, E::Linear)])
            .rot(bot, &[(0.0, 0.0, E::Linear)])
            .trim_start(mid, &[(0.0, 0.0, E::Linear)])
            .trim_end(mid, &[(0.0, 1.0, E::Linear)]),
    );
    let open = kit.anim(
        Anim::new("Open", 0.5)
            .pos(top, &[(0.0, 0.0, 13.0, E::InOut), (0.2, 0.0, 0.0, E::Linear)])
            .rot(top, &[(0.0, 0.0, E::Linear), (0.16, 0.0, E::Back), (0.42, 45.0, E::Linear)])
            .pos(bot, &[(0.0, 0.0, -13.0, E::InOut), (0.2, 0.0, 0.0, E::Linear)])
            .rot(bot, &[(0.0, 0.0, E::Linear), (0.16, 0.0, E::Back), (0.42, -45.0, E::Linear)])
            .trim_start(mid, &[(0.0, 0.0, E::Out), (0.2, 0.5, E::Linear)])
            .trim_end(mid, &[(0.0, 1.0, E::Out), (0.2, 0.5, E::Linear)]),
    );
    let hover = hover_layer(&mut kit, g, 1.08);
    kit.finish(
        vec![input("open", InputKind::Bool), input("hover", InputKind::Bool)],
        vec![st("Closed", closed, vec![go("Open", when("open", true), 0.0)]), st("Open", open, vec![go("Closed", when("open", false), 0.28)])],
        vec![hover],
    )
}

// ---------------------------------------------------------------------
// 3. Light / dark switch
// ---------------------------------------------------------------------

/// **Theme**: a sun whose rays retract and whose body morphs into a
/// crescent moon (with two sparkles) when `dark` (click toggles it).
pub(crate) fn build_theme(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Theme", 96.0, 96.0);
    let rays_group = kit.group("rays", None, Vec2::ZERO);
    let mut rays = Vec::new();
    for i in 0..8 {
        let a = (i as f32 * 45.0).to_radians();
        let (c, s) = (a.cos(), a.sin());
        rays.push(kit.stroke(&format!("ray{i}"), Some(rays_group), line(&[(c * 24.0, s * 24.0), (c * 32.0, s * 32.0)]), AMBER, 4.5, Vec2::ZERO, (0.0, 1.0)));
    }
    let body = kit.fill("body", None, moon(15.0, 15.0), AMBER, Vec2::ZERO);
    let s1 = kit.fill("sparkle1", None, sparkle(7.0), MOON, v(20.0, 16.0));
    let s2 = kit.fill("sparkle2", None, sparkle(4.5), MOON, v(28.0, -2.0));
    for id in [s1, s2] {
        if let Some(n) = kit.document.artboard_mut(kit.scene).and_then(|a| a.node_mut(id)) {
            n.transform.scale = Vec2::ZERO;
        }
    }
    kit.hit(Path::ellipse(84.0, 84.0), vec![ActionKind::ToggleInput { input: "dark".into() }], Some("hover"));
    let mut light = Anim::new("Light", 0.4)
        .morph(body, vec![(0.0, moon(15.0, 15.0), E::Linear)])
        .fill(body, &[(0.0, AMBER, E::Linear)])
        .rot(body, &[(0.0, 0.0, E::Linear)])
        .rot(rays_group, &[(0.0, 0.0, E::Linear)])
        .scale(s1, &[(0.0, 0.0, E::Linear)])
        .scale(s2, &[(0.0, 0.0, E::Linear)]);
    let mut dark = Anim::new("Dark", 0.7)
        .morph(body, vec![(0.0, moon(15.0, 15.0), E::InOut), (0.4, moon(18.0, -11.0), E::Linear)])
        .fill(body, &[(0.0, AMBER, E::InOut), (0.4, MOON, E::Linear)])
        .rot(body, &[(0.0, 0.0, E::InOut), (0.45, -30.0, E::Linear)])
        .rot(rays_group, &[(0.0, 0.0, E::In), (0.3, 90.0, E::Linear)])
        .scale(s1, &[(0.0, 0.0, E::Linear), (0.3, 0.0, E::Back), (0.55, 1.0, E::Linear)])
        .scale(s2, &[(0.0, 0.0, E::Linear), (0.4, 0.0, E::Back), (0.65, 1.0, E::Linear)]);
    for &ray in &rays {
        light = light.trim_start(ray, &[(0.0, 0.0, E::Linear)]);
        dark = dark.trim_start(ray, &[(0.0, 0.0, E::In), (0.25, 1.0, E::Linear)]);
    }
    let light = kit.anim(light);
    let dark = kit.anim(dark);
    let hover_target = rays_group;
    let hover = hover_layer(&mut kit, hover_target, 1.1);
    kit.finish(
        vec![input("dark", InputKind::Bool), input("hover", InputKind::Bool)],
        vec![st("Light", light, vec![go("Dark", when("dark", true), 0.0)]), st("Dark", dark, vec![go("Light", when("dark", false), 0.3)])],
        vec![hover],
    )
}

// ---------------------------------------------------------------------
// 4. Form status
// ---------------------------------------------------------------------

/// **Status**: the icon for a submit button. Enum input `status`:
/// `idle` (empty) → `loading` (spinning trim arc) → `success` (green disc,
/// check draws on) or `error` (red disc, ✕ draws on, shake).
pub(crate) fn build_status(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Status", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let g = kit.group("shake", None, Vec2::ZERO);
    let disc = kit.fill("disc", Some(g), Path::ellipse(64.0, 64.0), GREEN, Vec2::ZERO);
    let ring = kit.stroke("ring", Some(g), circle_cw(30.0), INK, 5.0, Vec2::ZERO, (0.0, 0.0));
    let check = kit.stroke("check", Some(g), line(&[(-13.0, 1.0), (-4.0, -9.0), (14.0, 10.0)]), WHITE, 6.0, Vec2::ZERO, (0.0, 0.0));
    let x1 = kit.stroke("crossA", Some(g), line(&[(-10.0, 10.0), (10.0, -10.0)]), WHITE, 6.0, Vec2::ZERO, (0.0, 0.0));
    let x2 = kit.stroke("crossB", Some(g), line(&[(10.0, 10.0), (-10.0, -10.0)]), WHITE, 6.0, Vec2::ZERO, (0.0, 0.0));
    if let Some(n) = kit.document.artboard_mut(kit.scene).and_then(|a| a.node_mut(disc)) {
        n.transform.scale = Vec2::ZERO;
    }
    let base = |name: &str, d: f32| {
        Anim::new(name, d)
            .scale(disc, &[(0.0, 0.0, E::Linear)])
            .trim_end(check, &[(0.0, 0.0, E::Linear)])
            .trim_end(x1, &[(0.0, 0.0, E::Linear)])
            .trim_end(x2, &[(0.0, 0.0, E::Linear)])
            .pos(g, &[(0.0, 0.0, 0.0, E::Linear)])
    };
    let idle = kit.anim(base("Idle", 0.3).stroke_width(ring, &[(0.0, 5.0, E::Linear)]).trim_end(ring, &[(0.0, 0.0, E::Linear)]).trim_offset(ring, &[(0.0, 0.0, E::Linear)]).rot(ring, &[(0.0, 0.0, E::Linear)]));
    let loading = kit.anim(
        base("Loading", 1.0)
            .stroke_width(ring, &[(0.0, 5.0, E::Linear)])
            .trim_end(ring, &[(0.0, 0.3, E::Linear)])
            .rot(ring, &[(0.0, 0.0, E::Linear), (1.0, -360.0, E::Linear)]),
    );
    let success = kit.anim(
        Anim::new("Success", 0.8)
            .trim_end(ring, &[(0.0, 0.3, E::Out), (0.25, 1.0, E::Linear)])
            .stroke_width(ring, &[(0.0, 5.0, E::Linear), (0.3, 5.0, E::Out), (0.45, 0.0, E::Linear)])
            .stroke_color(ring, &[(0.0, INK, E::Out), (0.2, GREEN, E::Linear)])
            .fill(disc, &[(0.0, GREEN, E::Linear)])
            .scale(disc, &[(0.0, 0.0, E::Linear), (0.18, 0.0, E::Back), (0.45, 1.0, E::Linear)])
            .trim_end(check, &[(0.0, 0.0, E::Linear), (0.35, 0.0, E::Out), (0.65, 1.0, E::Linear)])
            .trim_end(x1, &[(0.0, 0.0, E::Linear)])
            .trim_end(x2, &[(0.0, 0.0, E::Linear)])
            .pos(g, &[(0.0, 0.0, 0.0, E::Linear)]),
    );
    let error = kit.anim(
        Anim::new("Error", 0.9)
            .trim_end(ring, &[(0.0, 0.3, E::Out), (0.2, 1.0, E::Linear)])
            .stroke_width(ring, &[(0.0, 5.0, E::Linear), (0.28, 5.0, E::Out), (0.42, 0.0, E::Linear)])
            .stroke_color(ring, &[(0.0, INK, E::Out), (0.2, RED, E::Linear)])
            .fill(disc, &[(0.0, RED, E::Linear)])
            .scale(disc, &[(0.0, 0.0, E::Linear), (0.15, 0.0, E::Back), (0.4, 1.0, E::Linear)])
            .trim_end(check, &[(0.0, 0.0, E::Linear)])
            .trim_end(x1, &[(0.0, 0.0, E::Linear), (0.3, 0.0, E::Out), (0.5, 1.0, E::Linear)])
            .trim_end(x2, &[(0.0, 0.0, E::Linear), (0.42, 0.0, E::Out), (0.62, 1.0, E::Linear)])
            .pos(g, &[(0.0, 0.0, 0.0, E::Linear), (0.5, 0.0, 0.0, E::InOut), (0.57, -7.0, 0.0, E::InOut), (0.64, 6.0, 0.0, E::InOut), (0.71, -4.0, 0.0, E::InOut), (0.78, 2.0, 0.0, E::InOut), (0.85, 0.0, 0.0, E::Linear)]),
    );
    kit.ink(&[ring]);
    let all = ["idle", "loading", "success", "error"];
    let names = ["Idle", "Loading", "Success", "Error"];
    let anims = [idle, loading, success, error];
    let states = (0..4)
        .map(|i| {
            let to = (0..4).filter(|&j| j != i).map(|j| go(names[j], is("status", all[j]), if j == 0 { 0.25 } else { 0.0 })).collect();
            if i == 1 { st_loop(names[i], anims[i], to) } else { st(names[i], anims[i], to) }
        })
        .collect();
    kit.finish(vec![input("status", InputKind::Enum)], states, Vec::new())
}

// ---------------------------------------------------------------------
// 5. Notification bell
// ---------------------------------------------------------------------

/// **Bell**: trigger `ring` swings it; Bool `unread` pops a badge dot.
/// Click sends the page an `open` event.
pub(crate) fn build_bell(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Bell", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let tilt = kit.group("tilt", None, v(0.0, 24.0));
    let swing = kit.group("swing", Some(tilt), Vec2::ZERO);
    // A simpler, cleaner outline: dome + rim as one stroke.
    let dome = Path {
        commands: vec![
            PathCommand::MoveTo(v(-21.0, -34.0)),
            PathCommand::LineTo(v(21.0, -34.0)),
            PathCommand::CubicTo { control1: v(17.0, -29.0), control2: v(16.0, -24.0), to: v(16.0, -16.0) },
            PathCommand::CubicTo { control1: v(16.0, -5.0), control2: v(10.0, 0.0), to: v(0.0, 0.0) },
            PathCommand::CubicTo { control1: v(-10.0, 0.0), control2: v(-16.0, -5.0), to: v(-16.0, -16.0) },
            PathCommand::CubicTo { control1: v(-16.0, -24.0), control2: v(-17.0, -29.0), to: v(-21.0, -34.0) },
            PathCommand::Close,
        ],
    };
    let outline = kit.stroke("bell", Some(swing), dome, INK, 4.5, Vec2::ZERO, (0.0, 1.0));
    let knob = kit.stroke("knob", Some(swing), line(&[(0.0, 0.0), (0.0, 4.0)]), INK, 4.5, Vec2::ZERO, (0.0, 1.0));
    let clapper = kit.stroke("clapper", Some(swing), line(&[(-5.0, -41.0), (5.0, -41.0)]), INK, 5.0, Vec2::ZERO, (0.0, 1.0));
    kit.ink(&[outline, knob, clapper]);
    let badge = kit.fill("badge", None, Path::ellipse(16.0, 16.0), RED, v(16.0, 18.0));
    if let Some(n) = kit.document.artboard_mut(kit.scene).and_then(|a| a.node_mut(badge)) {
        n.transform.scale = Vec2::ZERO;
    }
    kit.hit(round_rect(80.0, 80.0, 16.0), vec![ActionKind::CallFunction { function: "open".into(), arg: String::new() }, ActionKind::FireTrigger { input: "ring".into() }], Some("hover"));
    let ring_keys: Vec<(f32, f32, E)> = vec![(0.0, 0.0, E::InOut), (0.1, 18.0, E::InOut), (0.22, -16.0, E::InOut), (0.34, 12.0, E::InOut), (0.46, -8.0, E::InOut), (0.58, 4.0, E::InOut), (0.7, 0.0, E::Linear)];
    let still = kit.anim(Anim::new("Still", 0.3).rot(swing, &[(0.0, 0.0, E::Linear)]).pos(clapper, &[(0.0, 0.0, 0.0, E::Linear)]));
    let ring = kit.anim(
        Anim::new("Ring", 0.8)
            .rot(swing, &ring_keys)
            .pos(clapper, &[(0.0, 0.0, 0.0, E::InOut), (0.12, -5.0, 0.0, E::InOut), (0.24, 5.0, 0.0, E::InOut), (0.36, -4.0, 0.0, E::InOut), (0.48, 3.0, 0.0, E::InOut), (0.62, 0.0, 0.0, E::Linear)]),
    );
    let hidden = kit.anim(Anim::new("Badge hidden", 0.2).scale(badge, &[(0.0, 0.0, E::Linear)]));
    let shown = kit.anim(Anim::new("Badge shown", 0.4).scale(badge, &[(0.0, 0.0, E::Back), (0.3, 1.0, E::Linear)]));
    let tilt_rest = kit.anim(Anim::new("Hover off", 0.2).rot(tilt, &[(0.0, 0.0, E::Linear)]));
    let tilt_over = kit.anim(Anim::new("Hover on", 0.2).rot(tilt, &[(0.0, -10.0, E::Linear)]));
    kit.finish(
        vec![input("ring", InputKind::Trigger), input("unread", InputKind::Bool), input("hover", InputKind::Bool)],
        vec![
            st("Still", still, vec![go("Ringing", on_trigger("ring"), 0.0)]),
            st("Ringing", ring, vec![go("Ringing again", on_trigger("ring"), 0.0)]),
            st("Ringing again", ring, vec![go("Ringing", on_trigger("ring"), 0.0)]),
        ],
        vec![
            ("Badge", vec![badge], vec![st("Read", hidden, vec![go("Unread", when("unread", true), 0.0)]), st("Unread", shown, vec![go("Read", when("unread", false), 0.15)])]),
            ("Hover", vec![tilt], vec![st("Rest", tilt_rest, vec![go("Over", when("hover", true), 0.15)]), st("Over", tilt_over, vec![go("Rest", when("hover", false), 0.2)])]),
        ],
    )
}

// ---------------------------------------------------------------------
// 6. Download
// ---------------------------------------------------------------------

/// **Download**: Enum `state` (`idle` / `downloading` / `done`) and a
/// `progress` Number property (0..1) bound to the ring's trim end. Click
/// sends the page a `download` event — the page starts the download and
/// drives the rest.
pub(crate) fn build_download(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Download", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    kit.property("progress", PropKind::Number, "0");
    let track = kit.stroke("track", None, circle_cw(36.0), fade(INK, 0.12), 4.0, Vec2::ZERO, (0.0, 1.0));
    let bar = kit.stroke("progressRing", None, circle_cw(36.0), AMBER, 4.0, Vec2::ZERO, (0.0, 0.0));
    kit.bind(&[bar], "progress", BindTarget::ComponentField { component: "StrokeStyle2D".into(), field: "trimEnd".into() });
    let arrow = kit.group("arrow", None, v(0.0, 3.0));
    let shaft = kit.stroke("shaft", Some(arrow), line(&[(0.0, 14.0), (0.0, -10.0)]), INK, 5.0, Vec2::ZERO, (0.0, 1.0));
    let head = kit.stroke("head", Some(arrow), line(&[(-10.0, 0.0), (0.0, -10.0), (10.0, 0.0)]), INK, 5.0, Vec2::ZERO, (0.0, 1.0));
    let tray = kit.stroke("tray", None, line(&[(-17.0, -8.0), (-17.0, -18.0), (17.0, -18.0), (17.0, -8.0)]), INK, 5.0, Vec2::ZERO, (0.0, 1.0));
    let check = kit.stroke("check", None, line(&[(-12.0, 0.0), (-4.0, -8.0), (13.0, 9.0)]), GREEN, 5.5, v(0.0, 2.0), (0.0, 0.0));
    kit.ink(&[shaft, head, tray]);
    let _ = track;
    kit.hit(Path::ellipse(84.0, 84.0), vec![ActionKind::CallFunction { function: "download".into(), arg: String::new() }], Some("hover"));
    let idle = kit.anim(
        Anim::new("Idle", 0.3)
            .pos(arrow, &[(0.0, 0.0, 3.0, E::Linear)])
            .scale(arrow, &[(0.0, 1.0, E::Linear)])
            .trim_end(tray, &[(0.0, 1.0, E::Linear)])
            .trim_end(check, &[(0.0, 0.0, E::Linear)])
            .stroke_color(bar, &[(0.0, AMBER, E::Linear)]),
    );
    let downloading = kit.anim(
        Anim::new("Downloading", 0.9)
            .pos(arrow, &[(0.0, 0.0, 6.0, E::InOut), (0.45, 0.0, -2.0, E::InOut), (0.9, 0.0, 6.0, E::Linear)])
            .scale(arrow, &[(0.0, 0.85, E::Linear)])
            .trim_end(tray, &[(0.0, 1.0, E::Linear)])
            .trim_end(check, &[(0.0, 0.0, E::Linear)])
            .stroke_color(bar, &[(0.0, AMBER, E::Linear)]),
    );
    let done = kit.anim(
        Anim::new("Done", 0.8)
            .pos(arrow, &[(0.0, 0.0, 3.0, E::In), (0.2, 0.0, -14.0, E::Linear)])
            .scale(arrow, &[(0.0, 0.85, E::In), (0.2, 0.0, E::Linear)])
            .trim_end(tray, &[(0.0, 1.0, E::In), (0.2, 0.0, E::Linear)])
            .trim_end(check, &[(0.0, 0.0, E::Linear), (0.22, 0.0, E::Out), (0.5, 1.0, E::Linear)])
            .stroke_color(bar, &[(0.0, AMBER, E::Linear), (0.2, GREEN, E::Linear)]),
    );
    let hover_off = kit.anim(Anim::new("Hover off", 0.2).scale(tray, &[(0.0, 1.0, E::Linear)]));
    let hover_on = kit.anim(Anim::new("Hover on", 0.2).scale_xy(tray, &[(0.0, 1.08, 1.0, E::Linear)]));
    let names = ["Idle", "Downloading", "Done"];
    let values = ["idle", "downloading", "done"];
    let anims = [idle, downloading, done];
    let states = (0..3)
        .map(|i| {
            let to = (0..3).filter(|&j| j != i).map(|j| go(names[j], is("state", values[j]), if i == 2 { 0.3 } else { 0.15 })).collect();
            if i == 1 { st_loop(names[i], anims[i], to) } else { st(names[i], anims[i], to) }
        })
        .collect();
    kit.finish(
        vec![input("state", InputKind::Enum), input("hover", InputKind::Bool)],
        states,
        vec![("Hover", vec![tray], vec![st("Rest", hover_off, vec![go("Over", when("hover", true), 0.15)]), st("Over", hover_on, vec![go("Rest", when("hover", false), 0.2)])])],
    )
}

// ---------------------------------------------------------------------
// 7. Copy
// ---------------------------------------------------------------------

/// **Copy**: click (or trigger `copy`) draws the two sheets out, a check
/// in, then back after a beat. Click also sends a `copy` event.
pub(crate) fn build_copy(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Copy", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let back = kit.stroke("back", None, round_rect(28.0, 28.0, 6.0), INK, 4.5, v(-6.0, 6.0), (0.0, 1.0));
    let front = kit.stroke("front", None, round_rect(28.0, 28.0, 6.0), INK, 4.5, v(6.0, -6.0), (0.0, 1.0));
    let check = kit.stroke("check", None, line(&[(-14.0, 0.0), (-4.0, -10.0), (15.0, 10.0)]), GREEN, 5.5, Vec2::ZERO, (0.0, 0.0));
    kit.ink(&[back, front]);
    kit.hit(round_rect(80.0, 80.0, 16.0), vec![ActionKind::FireTrigger { input: "copy".into() }, ActionKind::CallFunction { function: "copy".into(), arg: String::new() }], Some("hover"));
    let idle = kit.anim(Anim::new("Idle", 0.3).trim_end(back, &[(0.0, 1.0, E::Linear)]).trim_end(front, &[(0.0, 1.0, E::Linear)]).trim_end(check, &[(0.0, 0.0, E::Linear)]));
    let copied = kit.anim(
        Anim::new("Copied", 1.9)
            .trim_end(back, &[(0.0, 1.0, E::In), (0.18, 0.0, E::Linear), (1.45, 0.0, E::Out), (1.8, 1.0, E::Linear)])
            .trim_end(front, &[(0.0, 1.0, E::In), (0.22, 0.0, E::Linear), (1.4, 0.0, E::Out), (1.75, 1.0, E::Linear)])
            .trim_end(check, &[(0.0, 0.0, E::Linear), (0.2, 0.0, E::Out), (0.45, 1.0, E::Linear)])
            .trim_start(check, &[(0.0, 0.0, E::Linear), (1.2, 0.0, E::In), (1.45, 1.0, E::Linear)]),
    );
    let hover = {
        let g = front;
        let rest = kit.anim(Anim::new("Hover off", 0.2).pos(g, &[(0.0, 6.0, -6.0, E::Linear)]));
        let over = kit.anim(Anim::new("Hover on", 0.2).pos(g, &[(0.0, 8.0, -8.0, E::Linear)]));
        ("Hover", vec![g], vec![st("Rest", rest, vec![go("Over", when("hover", true), 0.15)]), st("Over", over, vec![go("Rest", when("hover", false), 0.2)])])
    };
    kit.finish(
        vec![input("copy", InputKind::Trigger), input("hover", InputKind::Bool)],
        vec![
            st("Idle", idle, vec![go("Copied", on_trigger("copy"), 0.0)]),
            st("Copied", copied, vec![go("Copied again", on_trigger("copy"), 0.0)]),
            st("Copied again", copied, vec![go("Copied", on_trigger("copy"), 0.0)]),
        ],
        vec![hover],
    )
}

// ---------------------------------------------------------------------
// 8. Link arrow
// ---------------------------------------------------------------------

/// **Arrow**: the "→" for links and calls to action. `hover` slides it
/// forward and redraws the shaft from behind. Drive `hover` from the
/// surrounding link's own hover so the whole button reacts.
pub(crate) fn build_arrow(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Arrow", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let g = kit.group("arrow", None, Vec2::ZERO);
    let shaft = kit.stroke("shaft", Some(g), line(&[(-18.0, 0.0), (16.0, 0.0)]), INK, 5.0, Vec2::ZERO, (0.0, 1.0));
    let head = kit.stroke("head", Some(g), line(&[(4.0, 12.0), (16.0, 0.0), (4.0, -12.0)]), INK, 5.0, Vec2::ZERO, (0.0, 1.0));
    kit.ink(&[shaft, head]);
    kit.hit(round_rect(80.0, 80.0, 16.0), Vec::new(), Some("hover"));
    let rest = kit.anim(Anim::new("Rest", 0.3).pos(g, &[(0.0, 0.0, 0.0, E::Linear)]).trim_start(shaft, &[(0.0, 0.0, E::Linear)]));
    let over = kit.anim(Anim::new("Over", 0.45).pos(g, &[(0.0, 0.0, 0.0, E::Out), (0.3, 10.0, 0.0, E::Linear)]).trim_start(shaft, &[(0.0, 0.0, E::Linear), (0.08, 0.6, E::Out), (0.4, 0.0, E::Linear)]));
    kit.finish(
        vec![input("hover", InputKind::Bool)],
        vec![st("Rest", rest, vec![go("Over", when("hover", true), 0.0)]), st("Over", over, vec![go("Rest", when("hover", false), 0.25)])],
        Vec::new(),
    )
}

// ---------------------------------------------------------------------
// 9. Loader
// ---------------------------------------------------------------------

/// **Loader**: the chasing-arc spinner — trim start and end chase each
/// other while the arc turns. Loops forever; no inputs.
pub(crate) fn build_loader(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Loader", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let track = kit.stroke("track", None, circle_cw(28.0), [0.5, 0.5, 0.55, 0.22], 5.0, Vec2::ZERO, (0.0, 1.0));
    let arc = kit.stroke("arc", None, circle_cw(28.0), AMBER, 5.0, Vec2::ZERO, (0.0, 0.1));
    let _ = track;
    let spin = kit.anim(
        Anim::new("Spin", 1.4)
            .trim_end(arc, &[(0.0, 0.05, E::InOut), (0.7, 0.75, E::Linear), (1.4, 0.75, E::Linear)])
            .trim_start(arc, &[(0.0, 0.0, E::Linear), (0.7, 0.0, E::InOut), (1.4, 0.7, E::Linear)])
            .trim_offset(arc, &[(0.0, 0.0, E::Linear), (1.4, 0.3, E::Linear)])
            .rot(arc, &[(0.0, 0.0, E::Linear), (1.4, -360.0, E::Linear)]),
    );
    kit.finish(Vec::new(), vec![st_loop("Spin", spin, Vec::new())], Vec::new())
}

// ---------------------------------------------------------------------
// 10. Toasty, the mascot
// ---------------------------------------------------------------------

/// **Toasty**: the slice of toast in the logo. Blinks every few seconds;
/// hover makes it hop and blush; click (trigger `boop`) squashes it.
pub(crate) fn build_mascot(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Toasty", 96.0, 96.0);
    const CRUST: [f32; 4] = [0.72, 0.4, 0.1, 1.0];
    const BREAD: [f32; 4] = [0.98, 0.78, 0.38, 1.0];
    const FACE: [f32; 4] = [0.24, 0.13, 0.05, 1.0];
    let shadow = kit.fill("shadow", None, Path::ellipse(44.0, 8.0), [0.0, 0.0, 0.0, 0.12], v(0.0, -36.0));
    let body = kit.group("body", None, v(0.0, -32.0));
    // Pivot at the feet so squash / hop read right: everything sits above.
    kit.fill("crustBottom", Some(body), round_rect(56.0, 40.0, 10.0), CRUST, v(0.0, 24.0));
    kit.fill("crustTop", Some(body), Path::ellipse(66.0, 40.0), CRUST, v(0.0, 46.0));
    kit.fill("breadBottom", Some(body), round_rect(48.0, 34.0, 8.0), BREAD, v(0.0, 25.0));
    kit.fill("breadTop", Some(body), Path::ellipse(58.0, 32.0), BREAD, v(0.0, 46.0));
    let face = kit.group("face", Some(body), v(0.0, 32.0));
    let eye_l = kit.fill("eyeL", Some(face), Path::ellipse(5.5, 8.0), FACE, v(-9.0, 4.0));
    let eye_r = kit.fill("eyeR", Some(face), Path::ellipse(5.5, 8.0), FACE, v(9.0, 4.0));
    let mouth = kit.stroke("mouth", Some(face), line(&[(-6.0, 0.0), (0.0, -3.0), (6.0, 0.0)]), FACE, 2.6, v(0.0, -5.0), (0.0, 1.0));
    let cheek_l = kit.fill("cheekL", Some(face), Path::ellipse(7.0, 4.0), [0.96, 0.45, 0.4, 0.0], v(-15.0, -3.0));
    let cheek_r = kit.fill("cheekR", Some(face), Path::ellipse(7.0, 4.0), [0.96, 0.45, 0.4, 0.0], v(15.0, -3.0));
    kit.hit(Path::ellipse(84.0, 84.0), vec![ActionKind::FireTrigger { input: "boop".into() }], Some("hover"));
    let blink = kit.anim(
        Anim::new("Idle", 3.6)
            .scale_xy(eye_l, &[(0.0, 1.0, 1.0, E::Linear), (3.3, 1.0, 1.0, E::InOut), (3.38, 1.0, 0.1, E::InOut), (3.48, 1.0, 1.0, E::Linear)])
            .scale_xy(eye_r, &[(0.0, 1.0, 1.0, E::Linear), (3.3, 1.0, 1.0, E::InOut), (3.38, 1.0, 0.1, E::InOut), (3.48, 1.0, 1.0, E::Linear)])
            .scale_xy(body, &[(0.0, 1.0, 1.0, E::InOut), (1.8, 1.015, 0.985, E::InOut), (3.6, 1.0, 1.0, E::Linear)]),
    );
    let boop = kit.anim(
        Anim::new("Boop", 0.6)
            .scale_xy(body, &[(0.0, 1.0, 1.0, E::Out), (0.08, 1.18, 0.78, E::InOut), (0.26, 0.92, 1.1, E::InOut), (0.42, 1.03, 0.97, E::InOut), (0.6, 1.0, 1.0, E::Linear)])
            .scale_xy(eye_l, &[(0.0, 1.0, 0.15, E::Linear), (0.3, 1.0, 0.15, E::Out), (0.42, 1.0, 1.0, E::Linear)])
            .scale_xy(eye_r, &[(0.0, 1.0, 0.15, E::Linear), (0.3, 1.0, 0.15, E::Out), (0.42, 1.0, 1.0, E::Linear)]),
    );
    let calm = kit.anim(
        Anim::new("Hover off", 0.3)
            .pos(face, &[(0.0, 0.0, 32.0, E::Linear)])
            .scale_xy(mouth, &[(0.0, 1.0, 1.0, E::Linear)])
            .fill(cheek_l, &[(0.0, [0.96, 0.45, 0.4, 0.0], E::Linear)])
            .fill(cheek_r, &[(0.0, [0.96, 0.45, 0.4, 0.0], E::Linear)])
            .scale(shadow, &[(0.0, 1.0, E::Linear)]),
    );
    let happy = kit.anim(
        Anim::new("Hover on", 0.7)
            .pos(face, &[(0.0, 0.0, 32.0, E::Out), (0.2, 0.0, 34.0, E::Linear)])
            .scale_xy(mouth, &[(0.0, 1.0, 1.0, E::Back), (0.25, 1.3, 2.2, E::Linear)])
            .fill(cheek_l, &[(0.0, [0.96, 0.45, 0.4, 0.0], E::Linear), (0.2, [0.96, 0.45, 0.4, 0.8], E::Linear)])
            .fill(cheek_r, &[(0.0, [0.96, 0.45, 0.4, 0.0], E::Linear), (0.2, [0.96, 0.45, 0.4, 0.8], E::Linear)])
            .scale(shadow, &[(0.0, 1.0, E::Out), (0.18, 0.75, E::In), (0.36, 1.0, E::Linear)]),
    );
    let hop_rest = kit.anim(Anim::new("Hop off", 0.2).pos(body, &[(0.0, 0.0, -32.0, E::Linear)]));
    let hop = kit.anim(Anim::new("Hop", 0.5).pos(body, &[(0.0, 0.0, -32.0, E::Out), (0.18, 0.0, -24.0, E::In), (0.36, 0.0, -32.0, E::Linear)]));
    kit.finish(
        vec![input("hover", InputKind::Bool), input("boop", InputKind::Trigger)],
        vec![st_loop("Idle", blink, vec![go("Boop", on_trigger("boop"), 0.0)]), st("Boop", boop, vec![go("Boop again", on_trigger("boop"), 0.0), go("Idle", when("hover", false), 0.3)]), st("Boop again", boop, vec![go("Boop", on_trigger("boop"), 0.0), go("Idle", when("hover", false), 0.3)])],
        vec![
            ("Face", vec![face, mouth, cheek_l, cheek_r, shadow], vec![st("Calm", calm, vec![go("Happy", when("hover", true), 0.0)]), st("Happy", happy, vec![go("Calm", when("hover", false), 0.25)])]),
            ("Hop", vec![body], vec![st("Down", hop_rest, vec![go("Up", when("hover", true), 0.0)]), st("Up", hop, vec![go("Down", when("hover", false), 0.2)])]),
        ],
    )
}

/// Builds the named component (`heart`, `menu`, `theme`, `status`, `bell`,
/// `download`, `copy`, `arrow`, `loader`, `toasty`).
pub(crate) fn build(document: &mut Document, kind: &str) -> Option<LiteDemo> {
    Some(match kind {
        "heart" => build_heart(document),
        "menu" => build_menu(document),
        "theme" => build_theme(document),
        "status" => build_status(document),
        "bell" => build_bell(document),
        "download" => build_download(document),
        "copy" => build_copy(document),
        "arrow" => build_arrow(document),
        "loader" => build_loader(document),
        "toasty" => build_mascot(document),
        _ => return None,
    })
}

/// Every component name (the test walks them all).
#[cfg(test)]
pub(crate) const ALL: [&str; 10] = ["heart", "menu", "theme", "status", "bell", "download", "copy", "arrow", "loader", "toasty"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_component_builds_a_machine_with_its_states() {
        for kind in ALL {
            let mut document = Document::new();
            let demo = build(&mut document, kind).unwrap();
            let artboard = document.artboard(demo.scene).unwrap();
            let (_, machine) = artboard.state_machines().next().unwrap();
            assert!(!machine.states.is_empty(), "{kind} has states");
            assert_eq!(artboard.animations().count(), artboard.animations().filter(|(_, a)| !a.tracks.is_empty()).count(), "{kind}: no empty animations");
        }
    }
}
