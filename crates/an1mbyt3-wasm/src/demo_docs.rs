//! The docs site's **showcase components** — six small, self-contained
//! scenes built to be embedded in web pages and driven from page code
//! (React / plain JS). Each one is the "proof of concept" behind a live
//! demo on the ToastyCreate docs site, so every one follows the same
//! contract the docs describe:
//!
//! - **Scene Properties** are the page-facing data (`player.set(...)` /
//!   React `data={{...}}` write same-named Properties);
//! - **state machine inputs** the page can flip (`setInput` / `fire`);
//! - **states** the page hears about (`onStateChange`);
//! - **events** the game sends with `ctx.emit` (`onEvent`).
//!
//! | Builder | Scene | Properties | Page inputs | States | Events |
//! |---|---|---|---|---|---|
//! | [`build_like_button`] | Like | `count`, `liked` | `burst` | Idle, Liked | `like` |
//! | [`build_progress_ring`] | Progress | `progress`, `label`, `color` | — | Loading, Done | `done` |
//! | [`build_day_night`] | Toggle | `isNight` | `isNight` | Day, Night | `toggle` |
//! | [`build_mood_blob`] | Mood | `mood` | `jump` | Sad, Neutral, Happy | `poke` |
//! | [`build_weather_card`] | Weather | `city`, `temp`, `condition` | — | Sun, Cloud, Rain, Snow, Storm | — |
//! | [`build_bell`] | Bell | `unread` | `ring` | Quiet, Ringing | `open` |
//!
//! Every visual is a plain vector shape animated by one Code program (the
//! same script runs in every state; it branches on `ctx.state` /
//! `ctx.layer`). One-shot effects (the like burst, the blob's jump) live on
//! a second state machine layer, "FX", whose `Rest → Pop` transition a
//! page trigger fires and whose script fires a hidden trigger to return —
//! the state machine has no exit-time transitions, so code ends the pose.

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Command, ConditionDef, Document, EmitterShape, FontAsset, FontId, GradientStop, InputDef, InputKind, LoopMode, Node, NodeBehavior,
    NodeBehaviorEntry, NodeId, NodeKind, ParticleRange, ParticleSystemNode, Path, PathCommand, PlaybackDirection, ProgramDef, PropKind, PropertyDef, ShapeNode, SimulationSpace,
    SizeKeyframe, StateDef, StateMachineDef, StateMachineLayerDef, TransitionDef, UiButton,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/RussoOne-Regular.ttf");

/// What every showcase builder returns: the scene to switch to.
pub(crate) struct DocsDemo {
    pub scene: ArtboardId,
}

/// The shared scaffolding: one scene + camera + font, nodes added in
/// order, and every command's inverse collected so the whole build is one
/// undo step (same pattern as every other demo builder).
struct Kit<'a> {
    document: &'a mut Document,
    inverses: Vec<Command>,
    scene: ArtboardId,
    font: FontId,
}

impl<'a> Kit<'a> {
    fn new(document: &'a mut Document, name: &str, width: f32, height: f32, background: [f32; 4]) -> Self {
        let mut inverses = Vec::new();
        let font = document.add_font(FontAsset { name: "Russo One".into(), data: FONT_BYTES.to_vec() });
        let inverse = document.apply(Command::AddArtboard { name: name.into(), width, height });
        let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
        inverses.push(inverse);
        let mut kit = Kit { document, inverses, scene, font };
        let camera = with_id(
            Node::with_components(
                "Main Camera",
                None,
                vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: background, follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })],
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

    /// A filled shape node with a stable script id.
    fn shape(&mut self, id: &str, parent: Option<NodeId>, path: Path, color: [f32; 4], at: Vec2) -> NodeId {
        let mut node = with_id(Node::new(id, parent, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None })), id);
        node.transform.translation = at;
        self.add(node)
    }

    /// A shape the script reads clicks from (`ref.clicked`).
    fn button(&mut self, id: &str, parent: Option<NodeId>, path: Path, color: [f32; 4], at: Vec2) -> NodeId {
        let mut node = with_id(Node::new(id, parent, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None })), id);
        node.transform.translation = at;
        node.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::UiButton(UiButton::default()), enabled: true });
        self.add(node)
    }

    fn text(&mut self, id: &str, parent: Option<NodeId>, content: &str, size: f32, color: [f32; 4], at: Vec2) -> NodeId {
        let font = self.font;
        self.add(text_node(parent, id, content, font, size, color, at))
    }

    /// A world-space burst emitter the script fires with `ref.emit(n)`.
    #[allow(clippy::too_many_arguments)]
    fn emitter(&mut self, id: &str, color: [f32; 4], speed: (f32, f32), lifetime: (f32, f32), size: (f32, f32), gravity: f32, shape: EmitterShape, at: Vec2) -> NodeId {
        let mut config = ParticleSystemNode::default();
        config.main.looping = false;
        config.main.duration = 0.05;
        config.main.start_speed = ParticleRange { min: speed.0, max: speed.1 };
        config.main.start_lifetime = ParticleRange { min: lifetime.0, max: lifetime.1 };
        config.main.start_size = ParticleRange { min: size.0, max: size.1 };
        config.main.start_color = color;
        config.main.gravity_modifier = gravity;
        config.main.drag = 0.6;
        config.main.simulation_space = SimulationSpace::World;
        config.main.max_particles = 400;
        config.emission.rate_over_time = 0.0;
        config.emission.bursts.clear();
        config.shape = shape;
        config.color_over_lifetime = vec![
            GradientStop { offset: 0.0, color: [1.0; 4], midpoint: 0.5 },
            GradientStop { offset: 0.7, color: [1.0; 4], midpoint: 0.5 },
            GradientStop { offset: 1.0, color: [1.0, 1.0, 1.0, 0.0], midpoint: 0.5 },
        ];
        config.size_over_lifetime = vec![SizeKeyframe { t: 0.0, size_multiplier: 1.0 }, SizeKeyframe { t: 1.0, size_multiplier: 0.5 }];
        let mut node = with_id(Node::new(id, None, NodeKind::ParticleSystem(config)), id);
        node.transform.translation = at;
        node.unlit = true;
        self.add(node)
    }

    /// A scene Property (page-facing data). `default` is JSON.
    fn property(&mut self, name: &str, kind: PropKind, default: &str) {
        let inverse = self.document.apply(Command::AddProperty { property: PropertyDef { artboard: self.scene, name: name.into(), kind, default: default.into(), deleted: false } });
        self.inverses.push(inverse);
    }

    /// Adds the state machine — `states` on the base layer, plus optional
    /// extra layers — every state running `script`, then commits the build.
    fn finish(mut self, name: &str, inputs: Vec<InputDef>, states: Vec<(String, Vec<TransitionDef>)>, layers: Vec<(String, Vec<(String, Vec<TransitionDef>)>)>, script: &str) -> DocsDemo {
        let inverse = self.document.apply(Command::AddAnimation { artboard: self.scene, animation: Animation::new(&format!("{name} (code)")) });
        let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
        self.inverses.push(inverse);
        let to_states = |list: Vec<(String, Vec<TransitionDef>)>| -> Vec<StateDef> {
            list.into_iter()
                .enumerate()
                .map(|(i, (name, transitions))| StateDef {
                    name,
                    animation,
                    loop_mode: LoopMode::Once,
                    transitions,
                    position: Vec2::new(i as f32 * 220.0, 0.0),
                    blend: None,
                    program: Some(ProgramDef::Code(script.into())),
                })
                .collect()
        };
        let initial_state = states[0].0.clone();
        let layers = layers
            .into_iter()
            .map(|(name, states)| {
                let initial_state = states[0].0.clone();
                StateMachineLayerDef { name, states: to_states(states), initial_state, mask: Vec::new() }
            })
            .collect();
        let inverse = self.document.apply(Command::AddStateMachine {
            artboard: self.scene,
            state_machine: StateMachineDef { name: name.into(), inputs, states: to_states(states), initial_state, layers, deleted: false },
        });
        self.inverses.push(inverse);
        let scene = self.scene;
        self.inverses.reverse();
        self.document.commit_batch(std::mem::take(&mut self.inverses));
        DocsDemo { scene }
    }
}

fn input(name: &str, kind: InputKind) -> InputDef {
    InputDef { name: name.into(), kind }
}

fn go(to: &str, condition: ConditionDef) -> TransitionDef {
    TransitionDef { to: to.into(), condition, direction: PlaybackDirection::Forward, duration_seconds: 0.0, interruptible: true }
}

fn trigger(name: &str) -> ConditionDef {
    ConditionDef::Trigger { input: name.into() }
}

fn state(name: &str, transitions: Vec<TransitionDef>) -> (String, Vec<TransitionDef>) {
    (name.to_string(), transitions)
}

/// The one-shot "FX" layer: `Rest --fire--> Pop --end--> Rest`.
fn fx_layer(fire: &str, end: &str) -> (String, Vec<(String, Vec<TransitionDef>)>) {
    ("FX".to_string(), vec![state("Rest", vec![go("Pop", trigger(fire))]), state("Pop", vec![go("Rest", trigger(end))])])
}

fn rgb(hex: u32) -> [f32; 4] {
    [((hex >> 16) & 255) as f32 / 255.0, ((hex >> 8) & 255) as f32 / 255.0, (hex & 255) as f32 / 255.0, 1.0]
}

/// A heart, point down, about 100 × 90, centred on the origin (y up).
fn heart_path(s: f32) -> Path {
    let p = |x: f32, y: f32| Vec2::new(x * s, y * s);
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

/// A crescent "smile" (`dir` = 1) or "frown" (`dir` = -1), `w` wide.
fn mouth_path(w: f32, dir: f32) -> Path {
    let h = w * 0.55 * dir;
    let v = |x: f32, y: f32| Vec2::new(x, y);
    Path {
        commands: vec![
            PathCommand::MoveTo(v(-w / 2.0, 0.0)),
            PathCommand::CubicTo { control1: v(-w / 2.0, -h), control2: v(w / 2.0, -h), to: v(w / 2.0, 0.0) },
            PathCommand::CubicTo { control1: v(w * 0.35, -h * 0.45), control2: v(-w * 0.35, -h * 0.45), to: v(-w / 2.0, 0.0) },
            PathCommand::Close,
        ],
    }
}

/// A horizontal pill (capsule) `w` × `h` (needs `w >= h`), as one path.
fn pill_path(w: f32, h: f32) -> Path {
    let r = h / 2.0;
    let k = r * 0.5523;
    let x = w / 2.0 - r;
    let v = Vec2::new;
    Path {
        commands: vec![
            PathCommand::MoveTo(v(-x, -r)),
            PathCommand::LineTo(v(x, -r)),
            PathCommand::CubicTo { control1: v(x + k, -r), control2: v(x + r, -k), to: v(x + r, 0.0) },
            PathCommand::CubicTo { control1: v(x + r, k), control2: v(x + k, r), to: v(x, r) },
            PathCommand::LineTo(v(-x, r)),
            PathCommand::CubicTo { control1: v(-x - k, r), control2: v(-x - r, k), to: v(-x - r, 0.0) },
            PathCommand::CubicTo { control1: v(-x - r, -k), control2: v(-x - k, -r), to: v(-x, -r) },
            PathCommand::Close,
        ],
    }
}

// ---------------------------------------------------------------------
// Like button
// ---------------------------------------------------------------------

pub(crate) fn build_like_button(document: &mut Document) -> DocsDemo {
    let mut kit = Kit::new(document, "Like", 360.0, 240.0, rgb(0x16141f));
    kit.property("count", PropKind::Number, "128");
    kit.property("liked", PropKind::Bool, "false");
    kit.shape("glow", None, Path::ellipse(130.0, 130.0), [0.95, 0.25, 0.45, 0.0], Vec2::new(0.0, 22.0));
    kit.emitter("burst", rgb(0xff5c8a), (60.0, 140.0), (0.5, 0.9), (3.0, 6.0), 0.0, EmitterShape::Circle { radius: 28.0 }, Vec2::new(0.0, 22.0));
    kit.emitter("burst2", rgb(0xffc857), (40.0, 110.0), (0.4, 0.8), (2.0, 4.0), 0.0, EmitterShape::Circle { radius: 20.0 }, Vec2::new(0.0, 22.0));
    kit.button("heart", None, heart_path(1.0), rgb(0x4a4560), Vec2::new(0.0, 22.0));
    kit.text("count", None, "128 likes", 20.0, rgb(0xf3eefc), Vec2::new(0.0, -62.0));
    kit.text("hint", None, "click the heart", 10.0, rgb(0x8a83a3), Vec2::new(0.0, -92.0));
    kit.finish(
        "Like",
        vec![input("isLiked", InputKind::Bool), input("burst", InputKind::Trigger), input("popEnd", InputKind::Trigger)],
        vec![
            state("Idle", vec![go("Liked", ConditionDef::BoolIs { input: "isLiked".into(), value: true })]),
            state("Liked", vec![go("Idle", ConditionDef::BoolIs { input: "isLiked".into(), value: false })]),
        ],
        vec![fx_layer("burst", "popEnd")],
        LIKE_SCRIPT,
    )
}

const LIKE_SCRIPT: &str = r#"// Like button. Page data: count, liked (scene Properties).
// Page trigger: burst. Event: like { liked, count }.
const g = ctx.globals.like || (ctx.globals.like = { pop: 0, popping: false, fill: 0, shown: '', synced: null })

if (ctx.layer === 'FX') {
  // One-shot burst: fired by a click (or by the page: player.fire('burst')).
  if (ctx.state === 'Pop' && !g.popping) {
    g.popping = true
    g.pop = 1
    getCompByID('burst').emit(26)
    getCompByID('burst2').emit(14)
  }
  if (ctx.state === 'Pop' && ctx.time > 0.45) { g.popping = false; ctx.fireTrigger('popEnd') }
} else {
  const heart = getCompByID('heart')
  if (heart.clicked) {
    const liked = !ctx.props.liked
    ctx.props.liked = liked
    ctx.props.count = Math.max(0, ctx.props.count + (liked ? 1 : -1))
    if (liked) ctx.fireTrigger('burst')
    else g.pop = 0.5
    ctx.emit('like', { liked, count: ctx.props.count })
  }
  // Keep the state machine (Idle / Liked) in step with the Property.
  const liked = !!ctx.props.liked
  if (g.synced !== liked) { g.synced = liked; ctx.setInput('isLiked', liked) }

  // Animate: squash-and-stretch pop, colour fades between grey and pink.
  const dt = ctx.deltaSeconds
  g.pop = Math.max(0, g.pop - dt * 2.4)
  g.fill += ((liked ? 1 : 0) - g.fill) * Math.min(1, dt * 10)
  const wobble = Math.sin(g.pop * Math.PI * 2) * 0.22 * g.pop
  heart.scaleX = 1 + wobble
  heart.scaleY = 1 - wobble * 0.7
  const mix = (a, b) => a + (b - a) * g.fill
  heart.fillColor = [mix(0.29, 1.0), mix(0.27, 0.27), mix(0.38, 0.45), 1]
  getCompByID('glow').fillColor = [1, 0.3, 0.5, 0.08 * g.fill + 0.22 * g.pop]

  const text = ctx.props.count + (ctx.props.count === 1 ? ' like' : ' likes')
  if (g.shown !== text) { g.shown = text; getCompByID('count').textContent = text }
}
"#;

// ---------------------------------------------------------------------
// Progress ring
// ---------------------------------------------------------------------

const RING_SEGMENTS: usize = 48;

pub(crate) fn build_progress_ring(document: &mut Document) -> DocsDemo {
    let mut kit = Kit::new(document, "Progress", 300.0, 300.0, rgb(0x0f1220));
    kit.property("progress", PropKind::Number, "0.35");
    kit.property("label", PropKind::Text, "\"Uploading\"");
    kit.property("color", PropKind::Color, "[0.486,0.361,1,1]");
    kit.shape("halo", None, Path::ellipse(250.0, 250.0), [0.49, 0.36, 1.0, 0.05], Vec2::ZERO);
    for i in 0..RING_SEGMENTS {
        let angle = std::f32::consts::FRAC_PI_2 - i as f32 / RING_SEGMENTS as f32 * std::f32::consts::TAU;
        let mut node = with_id(
            Node::new(
                &format!("seg{i}"),
                None,
                NodeKind::Shape(ShapeNode { path: pill_path(20.0, 7.0), fill_color: rgb(0x262a40), blend_mode: Default::default(), gradient: None, image: None, stroke: None }),
            ),
            &format!("seg{i}"),
        );
        node.transform.translation = Vec2::new(angle.cos(), angle.sin()) * 100.0;
        // The pill lies along +x, so rotating by `angle` points it outward.
        node.transform.rotation = angle;
        kit.add(node);
    }
    kit.emitter("sparks", rgb(0xb9a6ff), (40.0, 120.0), (0.5, 1.0), (2.0, 4.0), 0.0, EmitterShape::Circle { radius: 90.0 }, Vec2::ZERO);
    kit.text("pct", None, "35%", 38.0, rgb(0xf2f0ff), Vec2::new(0.0, 8.0));
    kit.text("label", None, "Uploading", 12.0, rgb(0x9b97b8), Vec2::new(0.0, -30.0));
    kit.finish(
        "Progress",
        vec![input("pct", InputKind::Number)],
        vec![
            state("Loading", vec![go("Done", ConditionDef::NumberGreaterThan { input: "pct".into(), threshold: 0.999 })]),
            state("Done", vec![go("Loading", ConditionDef::NumberLessThan { input: "pct".into(), threshold: 0.999 })]),
        ],
        Vec::new(),
        &RING_SCRIPT.replace("__SEGMENTS__", &RING_SEGMENTS.to_string()),
    )
}

const RING_SCRIPT: &str = r#"// Progress ring. Page data: progress (0..1), label, color.
// States: Loading / Done. Event: done (once, when progress reaches 1).
const N = __SEGMENTS__
const g = ctx.globals.ring || (ctx.globals.ring = { shown: 0, pct: '', label: null, lit: -1, color: '', pulse: 0, inputPct: -1 })
const target = Math.max(0, Math.min(1, Number(ctx.props.progress) || 0))
if (g.inputPct !== target) { g.inputPct = target; ctx.setInput('pct', target) }

// Ease the drawn value towards the data, so jumps animate smoothly.
g.shown += (target - g.shown) * Math.min(1, ctx.deltaSeconds * 8)
if (Math.abs(target - g.shown) < 0.001) g.shown = target

const c = ctx.props.color || [0.49, 0.36, 1, 1]
const lit = Math.round(g.shown * N)
const key = c.join(',')
if (lit !== g.lit || key !== g.color) {
  g.lit = lit
  g.color = key
  for (let i = 0; i < N; i++) {
    // Lit segments fade slightly along the arc for a sweep feel.
    const k = 0.75 + 0.25 * (i / N)
    getCompByID('seg' + i).fillColor = i < lit ? [c[0] * k, c[1] * k, c[2] * k, 1] : [0.15, 0.16, 0.25, 1]
  }
}

const pct = Math.round(g.shown * 100) + '%'
if (g.pct !== pct) { g.pct = pct; getCompByID('pct').textContent = pct }

const done = ctx.state === 'Done'
const label = done ? 'Done!' : String(ctx.props.label ?? '')
if (g.label !== label) { g.label = label; getCompByID('label').textContent = label }

// Entering Done: celebrate once and tell the page.
if (done && !g.celebrated) {
  g.celebrated = true
  g.pulse = 1
  getCompByID('sparks').emit(60)
  ctx.emit('done', {})
}
if (!done) g.celebrated = false
g.pulse = Math.max(0, g.pulse - ctx.deltaSeconds * 1.5)
const halo = getCompByID('halo')
halo.fillColor = [c[0], c[1], c[2], 0.05 + 0.25 * g.pulse + (done ? 0.06 : 0)]
halo.scaleX = halo.scaleY = 1 + 0.12 * g.pulse
"#;

// ---------------------------------------------------------------------
// Day / night toggle
// ---------------------------------------------------------------------

pub(crate) fn build_day_night(document: &mut Document) -> DocsDemo {
    let mut kit = Kit::new(document, "Toggle", 360.0, 200.0, rgb(0x12141c));
    kit.property("isNight", PropKind::Bool, "false");
    let track = kit.shape("track", None, pill_path(200.0, 84.0), rgb(0x7cc6ff), Vec2::ZERO);
    let _ = track;
    // Clouds (day) and stars (night) live inside the track.
    for (i, (x, y, w)) in [(40.0, -14.0, 46.0), (62.0, -6.0, 34.0), (18.0, -20.0, 30.0)].iter().enumerate() {
        kit.shape(&format!("cloud{i}"), None, Path::ellipse(*w, *w * 0.62), [1.0, 1.0, 1.0, 0.95], Vec2::new(*x, *y));
    }
    for (i, (x, y, r)) in [(-58.0, 18.0, 6.0), (-34.0, -16.0, 4.0), (-70.0, -12.0, 3.5), (-20.0, 22.0, 3.0), (-46.0, 2.0, 2.5)].iter().enumerate() {
        kit.shape(&format!("star{i}"), None, Path::star(4, *r, *r * 0.38), [1.0, 1.0, 0.9, 0.0], Vec2::new(*x, *y));
    }
    let knob = kit.shape("knob", None, Path::ellipse(66.0, 66.0), rgb(0xffd23f), Vec2::new(-58.0, 0.0));
    for (i, (x, y, r)) in [(-10.0, 10.0, 14.0), (12.0, -8.0, 10.0), (8.0, 16.0, 7.0)].iter().enumerate() {
        kit.shape(&format!("crater{i}"), Some(knob), Path::ellipse(*r, *r), [0.62, 0.64, 0.72, 0.0], Vec2::new(*x, *y));
    }
    // An invisible hit area over the whole pill.
    kit.button("hit", None, pill_path(200.0, 84.0), [1.0, 1.0, 1.0, 0.0], Vec2::ZERO);
    kit.text("caption", None, "DAY", 12.0, rgb(0x8d93ad), Vec2::new(0.0, -66.0));
    kit.finish(
        "Toggle",
        vec![input("isNight", InputKind::Bool)],
        vec![
            state("Day", vec![go("Night", ConditionDef::BoolIs { input: "isNight".into(), value: true })]),
            state("Night", vec![go("Day", ConditionDef::BoolIs { input: "isNight".into(), value: false })]),
        ],
        Vec::new(),
        DAY_NIGHT_SCRIPT,
    )
}

const DAY_NIGHT_SCRIPT: &str = r#"// Day / night toggle. Page data: isNight (Property) — or flip the
// isNight state machine input directly. Event: toggle { isNight }.
const g = ctx.globals.dn || (ctx.globals.dn = { t: 0, prop: null, caption: '' })
const night = ctx.state === 'Night'

// The page changed the Property: forward it to the state machine.
const prop = !!ctx.props.isNight
if (g.prop !== prop) { g.prop = prop; ctx.setInput('isNight', prop) }

if (getCompByID('hit').clicked) {
  const next = !night
  ctx.setInput('isNight', next)
  ctx.props.isNight = next
  g.prop = next
  ctx.emit('toggle', { isNight: next })
}

// t eases 0 (day) → 1 (night); everything is drawn from t.
const goal = night ? 1 : 0
g.t += (goal - g.t) * Math.min(1, ctx.deltaSeconds * 7)
const t = g.t
const mix = (a, b) => a + (b - a) * t
const ease = t * t * (3 - 2 * t)

getCompByID('track').fillColor = [mix(0.49, 0.1), mix(0.78, 0.12), mix(1.0, 0.3), 1]
const knob = getCompByID('knob')
knob.x = -58 + 116 * ease
knob.rotation = -180 * ease
knob.fillColor = [mix(1, 0.86), mix(0.82, 0.87), mix(0.25, 0.93), 1]
for (let i = 0; i < 3; i++) getCompByID('crater' + i).fillColor = [0.62, 0.64, 0.72, t]
for (let i = 0; i < 3; i++) {
  const cloud = getCompByID('cloud' + i)
  cloud.fillColor = [1, 1, 1, 0.95 * (1 - t)]
  cloud.y = [-14, -6, -20][i] - 30 * t
}
for (let i = 0; i < 5; i++) {
  const star = getCompByID('star' + i)
  const twinkle = 0.75 + 0.25 * Math.sin(ctx.time * 3 + i * 1.7)
  star.fillColor = [1, 1, 0.9, t * twinkle]
  star.scaleX = star.scaleY = 0.5 + 0.5 * t
}
const caption = night ? 'NIGHT' : 'DAY'
if (g.caption !== caption) { g.caption = caption; getCompByID('caption').textContent = caption }
"#;

// ---------------------------------------------------------------------
// Mood blob
// ---------------------------------------------------------------------

pub(crate) fn build_mood_blob(document: &mut Document) -> DocsDemo {
    let mut kit = Kit::new(document, "Mood", 360.0, 260.0, rgb(0x14131c));
    kit.property("mood", PropKind::Number, "0");
    kit.shape("shadow", None, Path::ellipse(120.0, 18.0), [0.0, 0.0, 0.0, 0.35], Vec2::new(0.0, -82.0));
    kit.emitter("tears", rgb(0x7fb4ff), (10.0, 30.0), (0.6, 1.0), (2.5, 4.0), 3.0, EmitterShape::Point, Vec2::new(-24.0, 0.0));
    kit.emitter("sparkles", rgb(0xffe07a), (40.0, 90.0), (0.5, 0.9), (2.0, 4.0), 0.0, EmitterShape::Circle { radius: 70.0 }, Vec2::ZERO);
    let body = kit.button("body", None, Path::ellipse(150.0, 136.0), rgb(0x9b8cf2), Vec2::new(0.0, -6.0));
    kit.shape("cheekL", Some(body), Path::ellipse(22.0, 12.0), [1.0, 0.45, 0.55, 0.0], Vec2::new(-42.0, -6.0));
    kit.shape("cheekR", Some(body), Path::ellipse(22.0, 12.0), [1.0, 0.45, 0.55, 0.0], Vec2::new(42.0, -6.0));
    kit.shape("eyeL", Some(body), Path::ellipse(16.0, 22.0), rgb(0x1d1a2b), Vec2::new(-24.0, 16.0));
    kit.shape("eyeR", Some(body), Path::ellipse(16.0, 22.0), rgb(0x1d1a2b), Vec2::new(24.0, 16.0));
    kit.shape("browL", Some(body), Path::rect(20.0, 4.0), [0.11, 0.1, 0.17, 0.0], Vec2::new(-24.0, 36.0));
    kit.shape("browR", Some(body), Path::rect(20.0, 4.0), [0.11, 0.1, 0.17, 0.0], Vec2::new(24.0, 36.0));
    kit.shape("smile", Some(body), mouth_path(44.0, 1.0), rgb(0x1d1a2b), Vec2::new(0.0, -14.0));
    kit.shape("frown", Some(body), mouth_path(40.0, -1.0), rgb(0x1d1a2b), Vec2::new(0.0, -26.0));
    kit.text("caption", None, "NEUTRAL", 11.0, rgb(0x8b86a6), Vec2::new(0.0, -108.0));
    kit.finish(
        "Mood",
        vec![input("moodValue", InputKind::Number), input("jump", InputKind::Trigger), input("jumpEnd", InputKind::Trigger)],
        vec![
            state(
                "Neutral",
                vec![
                    go("Happy", ConditionDef::NumberGreaterThan { input: "moodValue".into(), threshold: 0.33 }),
                    go("Sad", ConditionDef::NumberLessThan { input: "moodValue".into(), threshold: -0.33 }),
                ],
            ),
            state("Sad", vec![go("Neutral", ConditionDef::NumberGreaterThan { input: "moodValue".into(), threshold: -0.33 })]),
            state("Happy", vec![go("Neutral", ConditionDef::NumberLessThan { input: "moodValue".into(), threshold: 0.33 })]),
        ],
        vec![fx_layer("jump", "jumpEnd")],
        MOOD_SCRIPT,
    )
}

const MOOD_SCRIPT: &str = r#"// Mood blob. Page data: mood (-1 sad .. 1 happy). Trigger: jump.
// States: Sad / Neutral / Happy. Event: poke { mood } when clicked.
const g = ctx.globals.mood || (ctx.globals.mood = { m: 0, input: null, jumpT: -1, tear: 0, caption: '' })

if (ctx.layer === 'FX') {
  if (ctx.state === 'Pop') {
    if (g.jumpT < 0) g.jumpT = 0
    g.jumpT += ctx.deltaSeconds
    if (g.jumpT > 0.6) { g.jumpT = -1; ctx.fireTrigger('jumpEnd') }
  }
} else {
  const target = Math.max(-1, Math.min(1, Number(ctx.props.mood) || 0))
  if (g.input !== target) { g.input = target; ctx.setInput('moodValue', target) }
  g.m += (target - g.m) * Math.min(1, ctx.deltaSeconds * 5)
  const m = g.m
  const body = getCompByID('body')
  if (body.clicked) {
    ctx.fireTrigger('jump')
    ctx.emit('poke', { mood: target })
  }

  // Colour: blue (sad) → lilac (neutral) → warm yellow (happy).
  const lerp = (a, b, t) => a + (b - a) * t
  const sad = [0.36, 0.52, 0.95], mid = [0.61, 0.55, 0.95], joy = [1.0, 0.76, 0.32]
  const from = m < 0 ? mid : mid, to = m < 0 ? sad : joy, k = Math.abs(m)
  body.fillColor = [lerp(from[0], to[0], k), lerp(from[1], to[1], k), lerp(from[2], to[2], k), 1]

  // Breathing wobble (faster when happy), plus the jump arc.
  const speed = 2 + 2 * Math.max(0, m)
  const breathe = Math.sin(ctx.time * speed) * 0.04
  let hop = 0, squash = 0
  if (g.jumpT >= 0) {
    const p = g.jumpT / 0.6
    hop = Math.sin(p * Math.PI) * 46
    squash = p < 0.15 ? -0.18 * Math.sin(p / 0.15 * Math.PI) : 0
  }
  body.y = -6 + hop - Math.max(0, -m) * 6
  body.scaleX = 1 + breathe - squash
  body.scaleY = 1 - breathe + squash - Math.max(0, -m) * 0.06
  const shadow = getCompByID('shadow')
  shadow.scaleX = shadow.scaleY = 1 - hop / 120

  // Face: smile / frown grow with the mood; brows tilt when sad.
  const smile = getCompByID('smile'), frown = getCompByID('frown')
  smile.scaleY = Math.max(0.12, m)
  smile.scaleX = 0.8 + 0.4 * Math.max(0, m)
  frown.scaleY = Math.max(0.01, -m)
  frown.visible = m < -0.05
  smile.visible = m > -0.05
  const sadK = Math.max(0, -m)
  getCompByID('browL').fillColor = [0.11, 0.1, 0.17, sadK]
  getCompByID('browR').fillColor = [0.11, 0.1, 0.17, sadK]
  getCompByID('browL').rotation = -18 * sadK
  getCompByID('browR').rotation = 18 * sadK
  const blush = Math.max(0, m)
  getCompByID('cheekL').fillColor = [1, 0.45, 0.55, 0.6 * blush]
  getCompByID('cheekR').fillColor = [1, 0.45, 0.55, 0.6 * blush]
  const blink = (ctx.time % 3.4) < 0.12 ? 0.15 : 1
  getCompByID('eyeL').scaleY = blink * (1 - 0.3 * sadK)
  getCompByID('eyeR').scaleY = blink * (1 - 0.3 * sadK)

  // Tears when sad, sparkles when happy.
  g.tear += ctx.deltaSeconds
  if (m < -0.5 && g.tear > 0.5) {
    g.tear = 0
    const tears = getCompByID('tears')
    tears.x = (Math.random() < 0.5 ? -24 : 24)
    tears.y = body.y + 2
    tears.emit(1)
  }
  if (m > 0.6 && g.tear > 0.35) {
    g.tear = 0
    const s = getCompByID('sparkles')
    s.y = body.y
    s.emit(3)
  }

  const caption = ctx.state.toUpperCase()
  if (g.caption !== caption) { g.caption = caption; getCompByID('caption').textContent = caption }
}
"#;

// ---------------------------------------------------------------------
// Weather card
// ---------------------------------------------------------------------

const SKIES: [&str; 5] = ["sun", "cloud", "rain", "snow", "storm"];

pub(crate) fn build_weather_card(document: &mut Document) -> DocsDemo {
    let mut kit = Kit::new(document, "Weather", 420.0, 260.0, rgb(0x3d8bfd));
    kit.property("city", PropKind::Text, "\"Lisbon\"");
    kit.property("temp", PropKind::Number, "21");
    kit.property("condition", PropKind::Text, "\"sun\"");
    kit.shape("sky", None, Path::rect(440.0, 280.0), rgb(0x3d8bfd), Vec2::ZERO);
    kit.shape("flash", None, Path::rect(440.0, 280.0), [1.0, 1.0, 1.0, 0.0], Vec2::ZERO);
    let sun = kit.shape("sunRays", None, Path::star(12, 64.0, 46.0), rgb(0xffd54a), Vec2::new(-100.0, 30.0));
    let _ = sun;
    kit.shape("sun", None, Path::ellipse(76.0, 76.0), rgb(0xffe27a), Vec2::new(-100.0, 30.0));
    for (i, (x, y, w)) in [(-128.0, 4.0, 70.0), (-86.0, 18.0, 84.0), (-52.0, 0.0, 64.0), (-96.0, -8.0, 110.0)].iter().enumerate() {
        kit.shape(&format!("cloud{i}"), None, Path::ellipse(*w, *w * 0.62), [1.0, 1.0, 1.0, 0.0], Vec2::new(*x, *y));
    }
    let bolt = Path {
        commands: vec![
            PathCommand::MoveTo(Vec2::new(4.0, 24.0)),
            PathCommand::LineTo(Vec2::new(-12.0, -2.0)),
            PathCommand::LineTo(Vec2::new(0.0, -2.0)),
            PathCommand::LineTo(Vec2::new(-6.0, -26.0)),
            PathCommand::LineTo(Vec2::new(12.0, 6.0)),
            PathCommand::LineTo(Vec2::new(0.0, 6.0)),
            PathCommand::Close,
        ],
    };
    kit.shape("bolt", None, bolt, [1.0, 0.92, 0.4, 0.0], Vec2::new(-92.0, -40.0));
    kit.emitter("rain", [0.75, 0.85, 1.0, 0.9], (140.0, 180.0), (0.6, 0.8), (1.5, 2.5), 6.0, EmitterShape::Box { width: 150.0, height: 4.0 }, Vec2::new(-92.0, -16.0));
    kit.emitter("snow", [1.0, 1.0, 1.0, 1.0], (8.0, 20.0), (2.0, 3.0), (2.5, 4.5), 0.25, EmitterShape::Box { width: 160.0, height: 4.0 }, Vec2::new(-92.0, -16.0));
    kit.shape("panel", None, pill_path(170.0, 150.0), [1.0, 1.0, 1.0, 0.14], Vec2::new(100.0, 0.0));
    kit.text("city", None, "LISBON", 14.0, [1.0, 1.0, 1.0, 0.9], Vec2::new(100.0, 46.0));
    kit.text("temp", None, "21°", 46.0, [1.0; 4], Vec2::new(100.0, 4.0));
    kit.text("cond", None, "Sunny", 13.0, [1.0, 1.0, 1.0, 0.85], Vec2::new(100.0, -42.0));
    let mut transitions = Vec::new();
    for from in SKIES {
        let to: Vec<TransitionDef> = SKIES.iter().filter(|s| **s != from).map(|s| go(&title(s), ConditionDef::EnumIs { input: "sky".into(), value: (*s).into() })).collect();
        transitions.push(state(&title(from), to));
    }
    kit.finish("Weather", vec![input("sky", InputKind::Enum)], transitions, Vec::new(), WEATHER_SCRIPT)
}

fn title(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

const WEATHER_SCRIPT: &str = r#"// Weather card. Page data: city, temp, condition (sun | cloud | rain |
// snow | storm). States: Sun / Cloud / Rain / Snow / Storm.
const g = ctx.globals.wx || (ctx.globals.wx = { sky: null, texts: {}, mix: { sun: 1, cloud: 0, rain: 0, snow: 0, storm: 0 }, acc: 0, flash: 0, next: 2 })
const known = ['sun', 'cloud', 'rain', 'snow', 'storm']
let sky = String(ctx.props.condition || 'sun').toLowerCase()
if (!known.includes(sky)) sky = 'cloud'
if (g.sky !== sky) { g.sky = sky; ctx.setInput('sky', sky) }

const set = (id, text) => { if (g.texts[id] !== text) { g.texts[id] = text; getCompByID(id).textContent = text } }
set('city', String(ctx.props.city || '').toUpperCase())
set('temp', Math.round(Number(ctx.props.temp) || 0) + '°')
set('cond', { sun: 'Sunny', cloud: 'Cloudy', rain: 'Rain', snow: 'Snow', storm: 'Thunderstorm' }[sky])

// Cross-fade every layer of the illustration towards the current sky.
const dt = ctx.deltaSeconds
const k = Math.min(1, dt * 4)
for (const s of known) g.mix[s] += ((s === sky ? 1 : 0) - g.mix[s]) * k
const m = g.mix
const sunny = m.sun, clouds = Math.min(1, m.cloud + m.rain + m.storm + m.snow * 0.8)
const dark = m.rain * 0.4 + m.storm * 0.75 + m.cloud * 0.2

// Sky colour: blue → grey → stormy slate; snow is pale.
const base = [0.24 - 0.1 * dark + 0.4 * m.snow, 0.55 - 0.3 * dark + 0.2 * m.snow, 0.99 - 0.45 * dark - 0.05 * m.snow]
getCompByID('sky').fillColor = [base[0], base[1], base[2], 1]

const rays = getCompByID('sunRays'), sun = getCompByID('sun')
rays.rotation = ctx.time * 12
rays.fillColor = [1, 0.84, 0.29, sunny]
sun.fillColor = [1, 0.89, 0.48, Math.max(sunny, 0.0)]
const pulse = 1 + Math.sin(ctx.time * 2) * 0.04
rays.scaleX = rays.scaleY = pulse
const grey = 1 - 0.45 * (m.rain + m.storm)
for (let i = 0; i < 4; i++) {
  const c = getCompByID('cloud' + i)
  c.fillColor = [grey, grey, grey * 1.02, 0.97 * clouds]
  c.x = [-128, -86, -52, -96][i] + Math.sin(ctx.time * 0.6 + i) * 5
}

// Precipitation: emit at a rate proportional to how "rainy" the sky is.
g.acc += dt
while (g.acc > 1 / 30) {
  g.acc -= 1 / 30
  if (m.rain + m.storm > 0.3) getCompByID('rain').emit(m.storm > 0.5 ? 3 : 2)
  if (m.snow > 0.3 && Math.random() < 0.5) getCompByID('snow').emit(1)
}

// Lightning: occasional double flash during storms.
g.next -= dt
if (m.storm > 0.6 && g.next <= 0) { g.flash = 1; g.next = 1.8 + Math.random() * 2.5 }
g.flash = Math.max(0, g.flash - dt * 3.5)
const f = g.flash > 0.5 ? g.flash : g.flash * (g.flash > 0.25 ? 0.2 : 1)
getCompByID('flash').fillColor = [1, 1, 1, 0.55 * f]
getCompByID('bolt').fillColor = [1, 0.92, 0.4, g.flash > 0.3 ? 1 : 0]
"#;

// ---------------------------------------------------------------------
// Notification bell
// ---------------------------------------------------------------------

pub(crate) fn build_bell(document: &mut Document) -> DocsDemo {
    let mut kit = Kit::new(document, "Bell", 300.0, 240.0, rgb(0x171a21));
    kit.property("unread", PropKind::Number, "3");
    let v = Vec2::new;
    let bell_body = Path {
        commands: vec![
            PathCommand::MoveTo(v(-48.0, -30.0)),
            PathCommand::CubicTo { control1: v(-34.0, -18.0), control2: v(-40.0, 44.0), to: v(0.0, 46.0) },
            PathCommand::CubicTo { control1: v(40.0, 44.0), control2: v(34.0, -18.0), to: v(48.0, -30.0) },
            PathCommand::Close,
        ],
    };
    // The bell swings from its top knob: children sit 50 below the pivot.
    let mut pivot_node = with_id(Node::new("bellPivot", None, NodeKind::Group(Default::default())), "bellPivot");
    pivot_node.transform.translation = v(0.0, 50.0);
    let pivot = kit.add(pivot_node);
    kit.shape("clapper", Some(pivot), Path::ellipse(18.0, 18.0), rgb(0xd99a1e), v(0.0, -86.0));
    kit.button("bell", Some(pivot), bell_body, rgb(0xffc23d), v(0.0, -50.0));
    kit.shape("rim", Some(pivot), pill_path(104.0, 12.0), rgb(0xe8a923), v(0.0, -80.0));
    kit.shape("knob", Some(pivot), Path::ellipse(14.0, 14.0), rgb(0xe8a923), v(0.0, 0.0));
    kit.shape("shine", Some(pivot), Path::ellipse(10.0, 26.0), [1.0, 1.0, 1.0, 0.35], v(-18.0, -36.0));
    let badge = kit.shape("badge", None, Path::ellipse(34.0, 34.0), rgb(0xff3b5c), v(40.0, 40.0));
    let _ = badge;
    kit.text("badgeText", None, "3", 14.0, [1.0; 4], v(40.0, 40.0));
    kit.text("caption", None, "3 NEW", 11.0, rgb(0x8990a3), v(0.0, -82.0));
    kit.finish(
        "Bell",
        vec![input("ring", InputKind::Trigger), input("ringEnd", InputKind::Trigger)],
        vec![state("Quiet", vec![go("Ringing", trigger("ring"))]), state("Ringing", vec![go("Quiet", trigger("ringEnd"))])],
        Vec::new(),
        BELL_SCRIPT,
    )
}

const BELL_SCRIPT: &str = r#"// Notification bell. Page data: unread. Trigger: ring.
// States: Quiet / Ringing. Event: open { unread } when clicked.
const g = ctx.globals.bell || (ctx.globals.bell = { last: null, texts: {}, badge: 0 })
const unread = Math.max(0, Math.round(Number(ctx.props.unread) || 0))

// More unread than before: ring by itself.
if (g.last !== null && unread > g.last && ctx.state === 'Quiet') ctx.fireTrigger('ring')
g.last = unread

const bell = getCompByID('bell')
if (bell.clicked) ctx.emit('open', { unread })

// Ringing: a decaying swing, then back to Quiet.
const pivot = getCompByID('bellPivot'), clapper = getCompByID('clapper')
if (ctx.state === 'Ringing') {
  const t = ctx.time
  const decay = Math.max(0, 1 - t / 1.2)
  pivot.rotation = Math.sin(t * 22) * 22 * decay
  clapper.x = -Math.sin(t * 22 - 0.6) * 9 * decay
  if (t > 1.2) ctx.fireTrigger('ringEnd')
} else {
  pivot.rotation *= 0.8
  clapper.x *= 0.8
}

// Badge pops in/out with the count.
const show = unread > 0 ? 1 : 0
g.badge += (show - g.badge) * Math.min(1, ctx.deltaSeconds * 12)
const badge = getCompByID('badge'), text = getCompByID('badgeText')
const pop = ctx.state === 'Ringing' ? 1 + Math.max(0, 0.25 - ctx.time) * 1.2 : 1
badge.scaleX = badge.scaleY = g.badge * pop
text.visible = g.badge > 0.5
const set = (id, s) => { if (g.texts[id] !== s) { g.texts[id] = s; getCompByID(id).textContent = s } }
set('badgeText', unread > 9 ? '9+' : String(unread))
set('caption', unread === 0 ? 'ALL CAUGHT UP' : unread + ' NEW')
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_showcase_builds_with_its_properties_and_states() {
        type Builder = fn(&mut Document) -> DocsDemo;
        let cases: [(Builder, &[&str], &[&str]); 6] = [
            (build_like_button, &["count", "liked"], &["Idle", "Liked"]),
            (build_progress_ring, &["progress", "label", "color"], &["Loading", "Done"]),
            (build_day_night, &["isNight"], &["Day", "Night"]),
            (build_mood_blob, &["mood"], &["Neutral", "Sad", "Happy"]),
            (build_weather_card, &["city", "temp", "condition"], &["Sun", "Cloud", "Rain", "Snow", "Storm"]),
            (build_bell, &["unread"], &["Quiet", "Ringing"]),
        ];
        for (build, props, states) in cases {
            let mut document = Document::new();
            let demo = build(&mut document);
            let names: Vec<String> = document.properties().filter(|(_, p)| p.artboard == demo.scene).map(|(_, p)| p.name.clone()).collect();
            for p in props {
                assert!(names.iter().any(|n| n == p), "missing property {p}");
            }
            let artboard = document.artboard(demo.scene).unwrap();
            let (_, machine) = artboard.state_machines().next().unwrap();
            let got: Vec<&str> = machine.states.iter().map(|s| s.name.as_str()).collect();
            assert_eq!(got, states.to_vec());
        }
    }
}
