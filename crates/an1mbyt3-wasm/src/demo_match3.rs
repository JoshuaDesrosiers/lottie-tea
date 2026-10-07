//! "Crown Match" — a match-3 puzzle game in the style of the royal-themed
//! mobile hits, built entirely from vector shapes (no image assets).
//!
//! Three scenes, each with a one-state Code program from `demo_match3.js`:
//!
//! - **Crown Match Home** — title, a crown, drifting gems, PLAY.
//! - **Crown Match Levels** — a winding map of 20 levels: stars earned,
//!   locks, the current level pulsing. Tap one to play it.
//! - **Crown Match Game** — the 8x8 board: swap neighbours to match 3+.
//!   4 in a row makes a Rocket (clears a line), an L/T makes TNT (5x5),
//!   5 in a row makes a Light Ball (clears a color). Frost tiles break
//!   under matches. Each level has goals and a move limit; 1-3 stars.
//!
//! The board is a `clip_children` shape, so gems sliding in from above are
//! masked to it. Gem pieces are a fixed pool of 64 shape nodes whose path
//! and color the script swaps (the six gem outlines live in hidden
//! template nodes it copies from). Progress — unlocked level, best stars,
//! coins — is saved with `ctx.storage`.

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Command, Document, EmitterShape, FontAsset, FontId, GradientDef, GradientKind, GradientStop, LoopMode, Node, NodeBehavior,
    NodeKind, ParticleRange, ParticleSystemNode, Path, PathCommand, ProgramDef, ShapeNode, SimulationSpace, StateDef, StateMachineDef, Stroke, TextNode,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/RussoOne-Regular.ttf");
const SCRIPT: &str = include_str!("demo_match3.js");

const SCREEN_W: f32 = 360.0;
const SCREEN_H: f32 = 640.0;
/// Board geometry — the script uses the same numbers.
const N: usize = 8;
const CELL: f32 = 40.0;
const BOARD_Y: f32 = -36.0;

pub(crate) struct Match3Demo {
    pub scene: ArtboardId,
}

/// The six gem colors (+ the Light Ball's base), straight linear RGBA.
pub(crate) const GEM_COLORS: [[f32; 4]; 7] = [
    [0.93, 0.18, 0.3, 1.0],  // ruby
    [0.18, 0.5, 1.0, 1.0],   // sapphire
    [0.18, 0.82, 0.36, 1.0], // emerald
    [1.0, 0.8, 0.16, 1.0],   // topaz
    [0.7, 0.34, 0.98, 1.0],  // amethyst
    [1.0, 0.5, 0.12, 1.0],   // amber
    [1.0, 1.0, 1.0, 1.0],    // light ball
];

/// A rectangle with rounded corners (cubic quarter-circles).
fn rounded_rect(w: f32, h: f32, r: f32) -> Path {
    let (hw, hh) = (w / 2.0, h / 2.0);
    let r = r.min(hw).min(hh);
    let k = 0.5523 * r;
    let v = Vec2::new;
    Path {
        commands: vec![
            PathCommand::MoveTo(v(-hw + r, -hh)),
            PathCommand::LineTo(v(hw - r, -hh)),
            PathCommand::CubicTo { control1: v(hw - r + k, -hh), control2: v(hw, -hh + r - k), to: v(hw, -hh + r) },
            PathCommand::LineTo(v(hw, hh - r)),
            PathCommand::CubicTo { control1: v(hw, hh - r + k), control2: v(hw - r + k, hh), to: v(hw - r, hh) },
            PathCommand::LineTo(v(-hw + r, hh)),
            PathCommand::CubicTo { control1: v(-hw + r - k, hh), control2: v(-hw, hh - r + k), to: v(-hw, hh - r) },
            PathCommand::LineTo(v(-hw, -hh + r)),
            PathCommand::CubicTo { control1: v(-hw, -hh + r - k), control2: v(-hw + r - k, -hh), to: v(-hw + r, -hh) },
            PathCommand::Close,
        ],
    }
}

fn polyline(points: &[(f32, f32)]) -> Path {
    let mut commands = vec![PathCommand::MoveTo(Vec2::new(points[0].0, points[0].1))];
    commands.extend(points[1..].iter().map(|&(x, y)| PathCommand::LineTo(Vec2::new(x, y))));
    commands.push(PathCommand::Close);
    Path { commands }
}

/// The gem outlines, by type (6 = Light Ball).
fn gem_path(t: usize) -> Path {
    match t {
        0 => Path::polygon(6, 16.0),
        1 => polyline(&[(0.0, 17.0), (15.0, 0.0), (0.0, -17.0), (-15.0, 0.0)]),
        2 => rounded_rect(27.0, 27.0, 7.0),
        3 => Path::star(5, 18.0, 8.5),
        4 => Path::ellipse(29.0, 29.0),
        5 => polyline(&[(0.0, 17.0), (16.0, -12.0), (-16.0, -12.0)]),
        _ => Path::star(8, 17.0, 10.0),
    }
}

fn gradient(top: [f32; 4], bottom: [f32; 4], half_h: f32) -> GradientDef {
    GradientDef {
        kind: GradientKind::Linear,
        start: Vec2::new(0.0, half_h),
        end: Vec2::new(0.0, -half_h),
        stops: vec![GradientStop { offset: 0.0, color: top, midpoint: 0.5 }, GradientStop { offset: 1.0, color: bottom, midpoint: 0.5 }],
    }
}

fn stroke(width: f32, color: [f32; 4]) -> Option<Stroke> {
    Some(Stroke { width, color, gradient: None, image: None, cap: Default::default(), join: Default::default() })
}

/// A shape node (`id` empty = no element id).
fn shape(id: &str, parent: Option<an1mbyt3_core::NodeId>, path: Path, color: [f32; 4], at: Vec2) -> Node {
    let node = Node::new(if id.is_empty() { "Shape" } else { id }, parent, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None }));
    let mut node = if id.is_empty() { node } else { with_id(node, id) };
    node.transform.translation = at;
    node
}

fn with_stroke(mut node: Node, s: Option<Stroke>) -> Node {
    if let NodeKind::Shape(shape) = &mut node.kind {
        shape.stroke = s;
    }
    node
}

fn with_gradient(mut node: Node, g: GradientDef) -> Node {
    if let NodeKind::Shape(shape) = &mut node.kind {
        shape.gradient = Some(g);
    }
    node
}

fn text(id: &str, parent: Option<an1mbyt3_core::NodeId>, content: &str, font: FontId, size: f32, color: [f32; 4], at: Vec2) -> Node {
    let mut node = with_id(Node::new(id, parent, NodeKind::Text(TextNode { content: content.into(), font, size_px: size, fill_color: color, gradient: None })), id);
    node.transform.translation = at;
    node
}

/// Gold gradient title text.
fn gold_text(id: &str, content: &str, font: FontId, size: f32, at: Vec2) -> Node {
    let mut node = text(id, None, content, font, size, [1.0; 4], at);
    if let NodeKind::Text(t) = &mut node.kind {
        t.gradient = Some(gradient([1.0, 0.95, 0.6, 1.0], [0.95, 0.6, 0.12, 1.0], size / 2.0));
    }
    node
}

/// A glossy rounded button: plate + label (the script handles hover/taps).
fn button(document: &mut Document, inverses: &mut Vec<Command>, scene: ArtboardId, id: &str, label: &str, font: FontId, w: f32, h: f32, top: [f32; 4], bottom: [f32; 4], at: Vec2, size: f32) {
    let plate = with_stroke(with_gradient(shape(&format!("{id}Btn"), None, rounded_rect(w, h, h * 0.35), [1.0; 4], at), gradient(top, bottom, h / 2.0)), stroke(2.0, [1.0, 0.92, 0.6, 1.0]));
    add_node(document, inverses, scene, plate);
    add_node(document, inverses, scene, text(&format!("{id}Label"), None, label, font, size, [1.0, 1.0, 1.0, 1.0], at + Vec2::new(0.0, -1.0)));
}

/// A sparkle burst (pops, confetti).
fn burst(id: &str, color: [f32; 4], speed: (f32, f32), size: (f32, f32), gravity: f32, max: u32) -> Node {
    let mut config = ParticleSystemNode::default();
    config.main.looping = false;
    config.main.duration = 0.1;
    config.main.start_lifetime = ParticleRange { min: 0.35, max: 0.7 };
    config.main.start_speed = ParticleRange { min: speed.0, max: speed.1 };
    config.main.start_size = ParticleRange { min: size.0, max: size.1 };
    config.main.start_color = color;
    config.main.gravity_modifier = gravity;
    config.main.simulation_space = SimulationSpace::World;
    config.main.max_particles = max;
    config.emission.rate_over_time = 0.0;
    config.emission.bursts.clear();
    config.shape = EmitterShape::Circle { radius: 6.0 };
    config.color_over_lifetime = vec![
        GradientStop { offset: 0.0, color: [1.0, 1.0, 1.0, 1.0], midpoint: 0.5 },
        GradientStop { offset: 1.0, color: [1.0, 1.0, 1.0, 0.0], midpoint: 0.5 },
    ];
    let mut node = with_id(Node::with_components(id, None, vec![NodeBehavior::ParticleSystem(config)]), id);
    // Parked off-screen until the script moves it to a blast and emits.
    node.transform.translation = Vec2::new(100_000.0, 0.0);
    node.unlit = true;
    node
}

/// A new scene with a camera and the royal backdrop gradient.
fn new_scene(document: &mut Document, inverses: &mut Vec<Command>, name: &str, top: [f32; 4], bottom: [f32; 4]) -> ArtboardId {
    let inverse = document.apply(Command::AddArtboard { name: name.into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);
    let camera = with_id(
        Node::with_components(
            "Main Camera",
            None,
            vec![NodeBehavior::Camera(CameraNode {
                zoom: 1.0,
                background_color: bottom,
                follow_target: None,
                follow_smoothing: 0.0,
                vignette: an1mbyt3_core::CameraVignette { enabled: true, color: [0.02, 0.0, 0.08, 1.0], intensity: 0.35, radius: 1.1, blur: 0.8 },
                pixel_perfect: Default::default(),
            })],
        ),
        "camera",
    );
    let camera_id = add_node(document, inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));
    add_node(document, inverses, scene, with_gradient(shape("backdrop", None, Path::rect(SCREEN_W, SCREEN_H), [1.0; 4], Vec2::ZERO), gradient(top, bottom, SCREEN_H / 2.0)));
    // Soft light rays behind everything.
    for i in 0..5 {
        let mut ray = shape("", None, Path::rect(46.0, SCREEN_H * 1.4), [1.0, 0.9, 0.7, 0.035], Vec2::new(-150.0 + i as f32 * 75.0, 40.0));
        ray.transform.rotation = 0.35;
        add_node(document, inverses, scene, ray);
    }
    scene
}

/// The scene's program: one Code state running `demo_match3.js`'s library
/// + the `state` section.
fn add_program(document: &mut Document, inverses: &mut Vec<Command>, scene: ArtboardId, machine: &str, state: &str) {
    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Code") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    let mut sections = SCRIPT.split("//@program ");
    let library = sections.next().unwrap_or_default();
    let body = sections
        .find_map(|section| {
            let (name, body) = section.split_once('\n')?;
            (name.trim() == state).then_some(body)
        })
        .unwrap_or_else(|| panic!("demo_match3.js has no program {state:?}"));
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: machine.into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: state.into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(format!("{library}\n// ---- {state} ----\n{body}"))),
            }],
            initial_state: state.into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));
}

/// A finished gem shape (type `t`) with its shine and special markers.
fn add_gem(document: &mut Document, inverses: &mut Vec<Command>, scene: ArtboardId, parent: Option<an1mbyt3_core::NodeId>, id: &str, t: usize, at: Vec2, markers: bool) -> an1mbyt3_core::NodeId {
    let gem = with_stroke(shape(id, parent, gem_path(t), GEM_COLORS[t], at), stroke(2.0, [0.05, 0.02, 0.12, 0.45]));
    let gem_id = add_node(document, inverses, scene, gem);
    add_node(document, inverses, scene, shape(&format!("{id}Shine"), Some(gem_id), Path::ellipse(10.0, 6.0), [1.0, 1.0, 1.0, 0.55], Vec2::new(-5.0, 6.0)));
    if markers {
        let mut h = shape(&format!("{id}H"), Some(gem_id), rounded_rect(30.0, 6.0, 3.0), [1.0, 1.0, 1.0, 0.95], Vec2::ZERO);
        h.visible = false;
        add_node(document, inverses, scene, with_stroke(h, stroke(1.0, [0.1, 0.05, 0.2, 0.6])));
        let mut v = shape(&format!("{id}V"), Some(gem_id), rounded_rect(6.0, 30.0, 3.0), [1.0, 1.0, 1.0, 0.95], Vec2::ZERO);
        v.visible = false;
        add_node(document, inverses, scene, with_stroke(v, stroke(1.0, [0.1, 0.05, 0.2, 0.6])));
        let mut ring = with_stroke(shape(&format!("{id}R"), Some(gem_id), Path::ellipse(30.0, 30.0), [1.0, 0.4, 0.2, 0.25], Vec2::ZERO), stroke(3.0, [1.0, 0.95, 0.5, 1.0]));
        ring.visible = false;
        add_node(document, inverses, scene, ring);
    }
    gem_id
}

pub(crate) fn build_demo_match3(document: &mut Document) -> Match3Demo {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Russo One".into(), data: FONT_BYTES.to_vec() });
    let royal_top = [0.18, 0.12, 0.42, 1.0];
    let royal_bottom = [0.05, 0.03, 0.16, 1.0];

    // ================================================================ Home
    let home = new_scene(document, &mut inverses, "Crown Match Home", royal_top, royal_bottom);
    {
        let (document, inverses, scene) = (&mut *document, &mut inverses, home);
        // Drifting gems in the background (animated by the script).
        for i in 0..14 {
            let x = -160.0 + (i as f32 * 53.0) % 320.0;
            let y = -300.0 + (i as f32 * 97.0) % 600.0;
            add_gem(document, inverses, scene, None, &format!("drift{i}"), i % 6, Vec2::new(x, y), false);
        }
        // The crown.
        let crown = with_stroke(
            with_gradient(
                shape("crown", None, polyline(&[(-46.0, -26.0), (46.0, -26.0), (52.0, 22.0), (25.0, 2.0), (0.0, 34.0), (-25.0, 2.0), (-52.0, 22.0)]), [1.0; 4], Vec2::new(0.0, 168.0)),
                gradient([1.0, 0.93, 0.5, 1.0], [0.92, 0.55, 0.1, 1.0], 30.0),
            ),
            stroke(3.0, [0.55, 0.3, 0.05, 1.0]),
        );
        let crown_id = add_node(document, inverses, scene, crown);
        for (i, (x, y, t)) in [(-52.0f32, 24.0f32, 0usize), (0.0, 36.0, 1), (52.0, 24.0, 0), (-24.0, -12.0, 2), (24.0, -12.0, 4)].into_iter().enumerate() {
            let mut jewel = with_stroke(shape(&format!("jewel{i}"), Some(crown_id), Path::ellipse(11.0, 11.0), GEM_COLORS[t], Vec2::new(x, y)), stroke(1.5, [0.4, 0.2, 0.05, 1.0]));
            jewel.transform.scale = Vec2::splat(if i < 3 { 1.0 } else { 0.8 });
            add_node(document, inverses, scene, jewel);
        }
        add_node(document, inverses, scene, text("titleShadow", None, "CROWN MATCH", font, 40.0, [0.05, 0.0, 0.15, 0.8], Vec2::new(3.0, 87.0)));
        add_node(document, inverses, scene, gold_text("title", "CROWN MATCH", font, 40.0, Vec2::new(0.0, 90.0)));
        add_node(document, inverses, scene, text("tagline", None, "SWAP  -  MATCH  -  RULE", font, 13.0, [0.85, 0.8, 1.0, 1.0], Vec2::new(0.0, 56.0)));
        // Stats chips.
        for (id, x, label) in [("coins", -80.0f32, "0"), ("stars", 80.0, "0")] {
            add_node(document, inverses, scene, with_stroke(shape(&format!("{id}Chip"), None, rounded_rect(120.0, 32.0, 16.0), [0.08, 0.04, 0.2, 0.85], Vec2::new(x, 272.0)), stroke(2.0, [1.0, 0.85, 0.4, 0.8])));
            let icon = if id == "coins" { shape("", None, Path::ellipse(18.0, 18.0), [1.0, 0.8, 0.2, 1.0], Vec2::new(x - 40.0, 272.0)) } else { shape("", None, Path::star(5, 11.0, 5.0), [1.0, 0.85, 0.2, 1.0], Vec2::new(x - 40.0, 272.0)) };
            add_node(document, inverses, scene, with_stroke(icon, stroke(1.5, [0.6, 0.35, 0.05, 1.0])));
            add_node(document, inverses, scene, text(&format!("{id}Text"), None, label, font, 16.0, [1.0, 1.0, 1.0, 1.0], Vec2::new(x + 8.0, 271.0)));
        }
        button(document, inverses, scene, "play", "PLAY", font, 200.0, 64.0, [0.45, 0.92, 0.3, 1.0], [0.12, 0.55, 0.12, 1.0], Vec2::new(0.0, -90.0), 30.0);
        add_node(document, inverses, scene, text("levelHint", None, "LEVEL 1", font, 15.0, [1.0, 0.95, 0.75, 1.0], Vec2::new(0.0, -140.0)));
        button(document, inverses, scene, "map", "LEVEL MAP", font, 160.0, 40.0, [0.55, 0.4, 0.95, 1.0], [0.3, 0.18, 0.6, 1.0], Vec2::new(0.0, -196.0), 16.0);
        add_node(document, inverses, scene, text("footer", None, "BUILT WITH AN1MBYT3", font, 10.0, [0.6, 0.55, 0.8, 0.8], Vec2::new(0.0, -296.0)));
        add_program(document, inverses, scene, "Crown Match Home", "Home");
    }

    // ============================================================== Levels
    let levels = new_scene(document, &mut inverses, "Crown Match Levels", [0.1, 0.32, 0.28, 1.0], [0.04, 0.1, 0.14, 1.0]);
    {
        let (document, inverses, scene) = (&mut *document, &mut inverses, levels);
        // Level spots on a winding path, bottom to top, 4 per row.
        let spot = |i: usize| {
            let row = i / 4;
            let col = if row % 2 == 0 { i % 4 } else { 3 - i % 4 };
            Vec2::new(-114.0 + col as f32 * 76.0, -206.0 + row as f32 * 94.0)
        };
        // Path dots between spots.
        for i in 0..19 {
            let (a, b) = (spot(i), spot(i + 1));
            for k in 1..4 {
                let p = a + (b - a) * (k as f32 / 4.0);
                add_node(document, inverses, scene, shape(&format!("dot{i}_{k}"), None, Path::ellipse(7.0, 7.0), [1.0, 0.95, 0.8, 0.35], p));
            }
        }
        for i in 0..20 {
            let at = spot(i);
            add_node(document, inverses, scene, shape("", None, Path::ellipse(50.0, 18.0), [0.0, 0.0, 0.0, 0.25], at + Vec2::new(0.0, -20.0)));
            let node = with_stroke(shape(&format!("lvl{i}"), None, Path::ellipse(46.0, 46.0), [0.4, 0.4, 0.5, 1.0], at), stroke(3.0, [1.0, 0.9, 0.55, 1.0]));
            add_node(document, inverses, scene, node);
            add_node(document, inverses, scene, text(&format!("lvlNum{i}"), None, &format!("{}", i + 1), font, 18.0, [1.0, 1.0, 1.0, 1.0], at + Vec2::new(0.0, -1.0)));
            for k in 0..3 {
                let star = with_stroke(shape(&format!("lvlStar{i}_{k}"), None, Path::star(5, 7.0, 3.2), [0.3, 0.3, 0.35, 1.0], at + Vec2::new(-14.0 + k as f32 * 14.0, -30.0 + if k == 1 { -3.0 } else { 0.0 })), stroke(1.0, [0.15, 0.1, 0.05, 0.8]));
                add_node(document, inverses, scene, star);
            }
        }
        // Header.
        add_node(document, inverses, scene, with_stroke(shape("header", None, rounded_rect(340.0, 56.0, 18.0), [0.05, 0.12, 0.12, 0.9], Vec2::new(0.0, 284.0)), stroke(2.0, [1.0, 0.85, 0.4, 0.8])));
        add_node(document, inverses, scene, gold_text("title", "KINGDOM MAP", font, 22.0, Vec2::new(0.0, 284.0)));
        add_node(document, inverses, scene, text("starTotal", None, "0 / 60", font, 13.0, [1.0, 0.95, 0.7, 1.0], Vec2::new(122.0, 283.0)));
        button(document, inverses, scene, "back", "HOME", font, 92.0, 34.0, [0.55, 0.4, 0.95, 1.0], [0.3, 0.18, 0.6, 1.0], Vec2::new(-118.0, 284.0), 13.0);
        add_node(document, inverses, scene, text("hint", None, "TAP A LEVEL TO PLAY", font, 12.0, [0.85, 1.0, 0.95, 0.9], Vec2::new(0.0, -290.0)));
        add_program(document, inverses, scene, "Crown Match Levels", "Levels");
    }

    // ================================================================ Game
    let game = new_scene(document, &mut inverses, "Crown Match Game", royal_top, royal_bottom);
    {
        let (document, inverses, scene) = (&mut *document, &mut inverses, game);
        // Gem path templates (hidden): the script copies their outlines.
        for t in 0..7 {
            let mut tpl = shape(&format!("tpl{t}"), None, gem_path(t), GEM_COLORS[t], Vec2::new(10_000.0, 0.0));
            tpl.visible = false;
            add_node(document, inverses, scene, tpl);
        }
        // HUD panel.
        add_node(document, inverses, scene, with_stroke(with_gradient(shape("hud", None, rounded_rect(344.0, 104.0, 22.0), [1.0; 4], Vec2::new(0.0, 256.0)), gradient([0.32, 0.2, 0.62, 0.97], [0.14, 0.08, 0.32, 0.97], 52.0)), stroke(3.0, [1.0, 0.85, 0.4, 1.0])));
        add_node(document, inverses, scene, text("levelText", None, "LEVEL 1", font, 14.0, [1.0, 0.95, 0.75, 1.0], Vec2::new(-118.0, 286.0)));
        // Moves badge.
        add_node(document, inverses, scene, with_stroke(with_gradient(shape("movesBadge", None, Path::ellipse(64.0, 64.0), [1.0; 4], Vec2::new(-118.0, 246.0)), gradient([0.95, 0.4, 0.55, 1.0], [0.62, 0.12, 0.3, 1.0], 32.0)), stroke(3.0, [1.0, 0.9, 0.6, 1.0])));
        add_node(document, inverses, scene, text("movesText", None, "20", font, 26.0, [1.0, 1.0, 1.0, 1.0], Vec2::new(-118.0, 250.0)));
        add_node(document, inverses, scene, text("movesLabel", None, "MOVES", font, 9.0, [1.0, 0.9, 0.95, 1.0], Vec2::new(-118.0, 228.0)));
        // Goals: up to 3 slots (icon + count + tick).
        add_node(document, inverses, scene, text("goalsLabel", None, "GOALS", font, 11.0, [1.0, 0.9, 0.7, 1.0], Vec2::new(46.0, 290.0)));
        for k in 0..3 {
            let at = Vec2::new(-12.0 + k as f32 * 64.0, 254.0);
            add_node(document, inverses, scene, with_stroke(shape(&format!("goalSlot{k}"), None, rounded_rect(56.0, 56.0, 14.0), [0.06, 0.03, 0.16, 0.6], at), stroke(1.5, [1.0, 0.85, 0.4, 0.45])));
            let mut icon = with_stroke(shape(&format!("goalIcon{k}"), None, gem_path(k), GEM_COLORS[k], at + Vec2::new(0.0, 6.0)), stroke(2.0, [0.05, 0.02, 0.12, 0.45]));
            icon.transform.scale = Vec2::splat(0.8);
            add_node(document, inverses, scene, icon);
            add_node(document, inverses, scene, text(&format!("goalText{k}"), None, "10", font, 15.0, [1.0, 1.0, 1.0, 1.0], at + Vec2::new(0.0, -17.0)));
        }
        // Score bar with 3 star markers.
        add_node(document, inverses, scene, with_stroke(shape("scoreBack", None, rounded_rect(300.0, 12.0, 6.0), [0.05, 0.02, 0.12, 0.85], Vec2::new(0.0, 186.0)), stroke(1.5, [1.0, 0.85, 0.4, 0.6])));
        add_node(document, inverses, scene, with_gradient(shape("scoreFill", None, rounded_rect(296.0, 8.0, 4.0), [1.0; 4], Vec2::new(0.0, 186.0)), gradient([1.0, 0.9, 0.4, 1.0], [0.95, 0.55, 0.1, 1.0], 4.0)));
        for k in 0..3 {
            add_node(document, inverses, scene, with_stroke(shape(&format!("barStar{k}"), None, Path::star(5, 11.0, 5.0), [0.35, 0.32, 0.45, 1.0], Vec2::new(0.0, 186.0)), stroke(1.5, [0.2, 0.1, 0.0, 0.9])));
        }
        add_node(document, inverses, scene, text("scoreText", None, "0", font, 12.0, [1.0, 1.0, 1.0, 1.0], Vec2::new(-128.0, 168.0)));

        // The board: a rounded frame, and the clipping well holding cells,
        // frost and gems (anything outside the well is masked away).
        let side = N as f32 * CELL;
        add_node(document, inverses, scene, with_stroke(with_gradient(shape("frame", None, rounded_rect(side + 22.0, side + 22.0, 18.0), [1.0; 4], Vec2::new(0.0, BOARD_Y)), gradient([0.98, 0.82, 0.4, 1.0], [0.72, 0.45, 0.12, 1.0], side / 2.0)), stroke(2.0, [0.4, 0.22, 0.04, 1.0])));
        let mut well = shape("board", None, Path::rect(side, side), [0.1, 0.07, 0.26, 0.95], Vec2::new(0.0, BOARD_Y));
        well.clip_children = true;
        let well_id = add_node(document, inverses, scene, well);
        let local = |r: usize, c: usize| Vec2::new(-side / 2.0 + CELL / 2.0 + c as f32 * CELL, side / 2.0 - CELL / 2.0 - r as f32 * CELL);
        for r in 0..N {
            for c in 0..N {
                let i = r * N + c;
                let tint = if (r + c) % 2 == 0 { [0.26, 0.2, 0.52, 0.55] } else { [0.2, 0.15, 0.42, 0.55] };
                add_node(document, inverses, scene, shape(&format!("cell{i}"), Some(well_id), rounded_rect(CELL - 3.0, CELL - 3.0, 7.0), tint, local(r, c)));
            }
        }
        for i in 0..N * N {
            let (r, c) = (i / N, i % N);
            let mut frost = with_stroke(shape(&format!("frost{i}"), Some(well_id), rounded_rect(CELL - 5.0, CELL - 5.0, 8.0), [0.62, 0.86, 1.0, 0.5], local(r, c)), stroke(2.0, [0.85, 0.96, 1.0, 0.9]));
            frost.visible = false;
            add_node(document, inverses, scene, frost);
        }
        let mut sel = with_stroke(shape("sel", Some(well_id), rounded_rect(CELL - 2.0, CELL - 2.0, 9.0), [1.0, 1.0, 1.0, 0.12], Vec2::ZERO), stroke(3.0, [1.0, 1.0, 1.0, 0.95]));
        sel.visible = false;
        add_node(document, inverses, scene, sel);
        // The gem pool: 64 pieces (enough for a full board; cleared ones
        // are recycled as the refills that drop in).
        for i in 0..N * N {
            let (r, c) = (i / N, i % N);
            add_gem(document, inverses, scene, Some(well_id), &format!("gem{i}"), i % 6, local(r, c), true);
        }
        // Effects: sparkle pops (one per gem color), a big blast, confetti.
        for t in 0..7 {
            add_node(document, inverses, scene, burst(&format!("pop{t}"), GEM_COLORS[t], (30.0, 90.0), (2.0, 4.0), 0.4, 120));
        }
        add_node(document, inverses, scene, burst("blast", [1.0, 0.85, 0.4, 1.0], (60.0, 200.0), (2.0, 5.0), 0.2, 200));
        add_node(document, inverses, scene, burst("confetti", [1.0, 0.9, 0.5, 1.0], (80.0, 220.0), (3.0, 6.0), 1.0, 200));
        // Floating score popups.
        for k in 0..6 {
            let mut pop = text(&format!("float{k}"), None, "", font, 16.0, [1.0, 1.0, 1.0, 1.0], Vec2::new(0.0, 10_000.0));
            if let NodeKind::Text(t) = &mut pop.kind {
                t.gradient = Some(gradient([1.0, 1.0, 0.8, 1.0], [1.0, 0.75, 0.2, 1.0], 8.0));
            }
            add_node(document, inverses, scene, pop);
        }
        // Bottom: banner text + back button.
        add_node(document, inverses, scene, text("banner", None, "", font, 30.0, [1.0, 1.0, 1.0, 1.0], Vec2::new(0.0, BOARD_Y)));
        add_node(document, inverses, scene, text("bannerSub", None, "", font, 13.0, [1.0, 0.95, 0.8, 1.0], Vec2::new(0.0, BOARD_Y - 30.0)));
        button(document, inverses, scene, "quit", "MAP", font, 84.0, 34.0, [0.55, 0.4, 0.95, 1.0], [0.3, 0.18, 0.6, 1.0], Vec2::new(-128.0, -264.0), 13.0);
        add_node(document, inverses, scene, text("tip", None, "DRAG A GEM TO SWAP", font, 11.0, [0.85, 0.8, 1.0, 0.85], Vec2::new(40.0, -264.0)));

        // Result overlay (hidden until the level ends).
        let mut dim = shape("dim", None, Path::rect(SCREEN_W, SCREEN_H), [0.0, 0.0, 0.05, 0.6], Vec2::ZERO);
        dim.visible = false;
        add_node(document, inverses, scene, dim);
        let mut panel = with_stroke(with_gradient(shape("panel", None, rounded_rect(290.0, 300.0, 28.0), [1.0; 4], Vec2::new(0.0, 0.0)), gradient([0.36, 0.24, 0.7, 1.0], [0.14, 0.08, 0.34, 1.0], 150.0)), stroke(4.0, [1.0, 0.85, 0.4, 1.0]));
        panel.visible = false;
        add_node(document, inverses, scene, panel);
        let mut title = gold_text("resultTitle", "", font, 28.0, Vec2::new(0.0, 112.0));
        title.visible = false;
        add_node(document, inverses, scene, title);
        for k in 0..3 {
            let mut star = with_stroke(shape(&format!("resultStar{k}"), None, Path::star(5, 26.0, 11.0), [0.3, 0.28, 0.4, 1.0], Vec2::new(-64.0 + k as f32 * 64.0, 52.0 + if k == 1 { 12.0 } else { 0.0 })), stroke(2.5, [0.3, 0.15, 0.0, 1.0]));
            star.visible = false;
            add_node(document, inverses, scene, star);
        }
        let mut sub = text("resultSub", None, "", font, 14.0, [1.0, 0.95, 0.85, 1.0], Vec2::new(0.0, -6.0));
        sub.visible = false;
        add_node(document, inverses, scene, sub);
        for (id, label, y, top, bottom) in [("next", "NEXT", -62.0f32, [0.45, 0.92, 0.3, 1.0], [0.12, 0.55, 0.12, 1.0]), ("map2", "MAP", -114.0, [0.55, 0.4, 0.95, 1.0], [0.3, 0.18, 0.6, 1.0])] {
            button(document, inverses, scene, id, label, font, 170.0, 42.0, top, bottom, Vec2::new(0.0, y), 18.0);
        }
        add_program(document, inverses, scene, "Crown Match Game", "Game");
    }

    inverses.reverse();
    document.commit_batch(inverses);
    let _ = (levels, game);
    Match3Demo { scene: home }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_three_scenes_each_with_its_own_program() {
        let mut document = Document::new();
        let demo = build_demo_match3(&mut document);
        assert_eq!(document.artboard(demo.scene).unwrap().name, "Crown Match Home");
        for (scene, state) in [("Crown Match Home", "Home"), ("Crown Match Levels", "Levels"), ("Crown Match Game", "Game")] {
            let (_, a) = document.artboards().find(|(_, a)| a.name == scene).expect("scene exists");
            let (_, m) = a.state_machines().next().unwrap();
            let Some(ProgramDef::Code(src)) = &m.states[0].program else { panic!("expected Code") };
            assert!(src.contains(&format!("---- {state} ----")));
            for other in ["Home", "Levels", "Game"].into_iter().filter(|o| *o != state) {
                assert!(!src.contains(&format!("---- {other} ----")));
            }
        }
        let (_, game) = document.artboards().find(|(_, a)| a.name == "Crown Match Game").unwrap();
        let count = |prefix: &str| game.node_ids().filter_map(|id| game.node(id)).filter(|n| n.id.starts_with(prefix) && n.id[prefix.len()..].parse::<u32>().is_ok()).count();
        assert_eq!(count("gem"), 64);
        assert_eq!(count("cell"), 64);
        assert_eq!(count("frost"), 64);
        assert_eq!(count("tpl"), 7);
    }
}
