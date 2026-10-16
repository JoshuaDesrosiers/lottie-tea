//! Two small, original games for the docs — deliberately short, one Code
//! program each, built from plain shapes, so they read as templates:
//!
//! - **Toast Catch**: move a plate (mouse or ← →) to catch falling toast;
//!   burnt slices cost a life. Shows the frame loop, input, an object pool,
//!   collision by distance, a score Property and `ctx.emit` to the page.
//! - **Hop**: a one-button runner — space / tap to jump the hurdles. Shows
//!   simple physics by hand (velocity + gravity), spawning by distance,
//!   a best score in `ctx.storage`, and game-over events.

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Command, Document, FontAsset, FontId, LoopMode, Node, NodeBehavior, NodeId, NodeKind, Path, ProgramDef, PropKind, PropertyDef, ShapeNode, StateDef,
    StateMachineDef, Stroke, StrokeCap, StrokeJoin,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/RussoOne-Regular.ttf");

pub(crate) struct GameDemo {
    pub scene: ArtboardId,
}

struct Kit<'a> {
    document: &'a mut Document,
    inverses: Vec<Command>,
    scene: ArtboardId,
    font: FontId,
}

impl<'a> Kit<'a> {
    fn new(document: &'a mut Document, name: &str, w: f32, h: f32, bg: [f32; 4]) -> Self {
        let mut inverses = Vec::new();
        let font = document.add_font(FontAsset { name: "Russo One".into(), data: FONT_BYTES.to_vec() });
        let inverse = document.apply(Command::AddArtboard { name: name.into(), width: w, height: h });
        let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
        inverses.push(inverse);
        let mut kit = Kit { document, inverses, scene, font };
        let camera = with_id(
            Node::with_components("Main Camera", None, vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: bg, follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })]),
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
    fn shape(&mut self, id: &str, path: Path, color: [f32; 4], at: Vec2, stroke: Option<([f32; 4], f32)>) -> NodeId {
        let stroke = stroke.map(|(color, width)| Stroke { width, color, gradient: None, image: None, cap: StrokeCap::Round, join: StrokeJoin::Round });
        let mut node = with_id(Node::new(id, None, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke })), id);
        node.transform.translation = at;
        self.add(node)
    }
    fn text(&mut self, id: &str, content: &str, size: f32, color: [f32; 4], at: Vec2) -> NodeId {
        let font = self.font;
        self.add(text_node(None, id, content, font, size, color, at))
    }
    fn property(&mut self, name: &str, kind: PropKind, default: &str) {
        let inverse = self.document.apply(Command::AddProperty { property: PropertyDef { artboard: self.scene, name: name.into(), kind, default: default.into(), deleted: false } });
        self.inverses.push(inverse);
    }
    fn finish(mut self, script: &str) -> GameDemo {
        let inverse = self.document.apply(Command::AddAnimation { artboard: self.scene, animation: Animation::new("Game (code)") });
        let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
        self.inverses.push(inverse);
        let inverse = self.document.apply(Command::AddStateMachine {
            artboard: self.scene,
            state_machine: StateMachineDef {
                name: "Game".into(),
                inputs: Vec::new(),
                states: vec![StateDef { name: "Play".into(), animation, loop_mode: LoopMode::Once, transitions: Vec::new(), position: Vec2::ZERO, blend: None, program: Some(ProgramDef::Code(script.into())) }],
                initial_state: "Play".into(),
                layers: Vec::new(),
                deleted: false,
            },
        });
        self.inverses.push(inverse);
        let scene = self.scene;
        self.inverses.reverse();
        self.document.commit_batch(std::mem::take(&mut self.inverses));
        GameDemo { scene }
    }
}

/// A rounded rectangle centred on the origin.
fn round_rect(w: f32, h: f32, r: f32) -> Path {
    const K: f32 = 0.552_284_8;
    let (x, y, k) = (w / 2.0, h / 2.0, r * K);
    let v = Vec2::new;
    use an1mbyt3_core::PathCommand as C;
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

const INK: [f32; 4] = [0.15, 0.12, 0.1, 1.0];
const TOAST: [f32; 4] = [0.97, 0.76, 0.38, 1.0];
const CRUST: [f32; 4] = [0.74, 0.42, 0.12, 1.0];
const BURNT: [f32; 4] = [0.22, 0.15, 0.11, 1.0];

// ---------------------------------------------------------------------
// Toast Catch
// ---------------------------------------------------------------------

const TOAST_POOL: usize = 6;

pub(crate) fn build_toast_catch(document: &mut Document) -> GameDemo {
    let mut kit = Kit::new(document, "Toast Catch", 640.0, 360.0, [0.98, 0.95, 0.9, 1.0]);
    kit.property("score", PropKind::Number, "0");
    kit.property("lives", PropKind::Number, "3");
    kit.shape("counter", Path::rect(700.0, 70.0), [0.86, 0.8, 0.72, 1.0], Vec2::new(0.0, -150.0), None);
    kit.shape("counterEdge", Path::rect(700.0, 6.0), [0.78, 0.71, 0.62, 1.0], Vec2::new(0.0, -114.0), None);
    for i in 0..TOAST_POOL {
        kit.shape(&format!("toast{i}"), round_rect(38.0, 40.0, 10.0), TOAST, Vec2::new(0.0, 400.0), Some((CRUST, 5.0)));
    }
    kit.shape("plate", round_rect(96.0, 14.0, 7.0), [1.0, 1.0, 1.0, 1.0], Vec2::new(0.0, -100.0), Some(([0.8, 0.78, 0.76, 1.0], 3.0)));
    kit.text("scoreText", "0", 28.0, INK, Vec2::new(0.0, 140.0));
    for i in 0..3 {
        kit.shape(&format!("life{i}"), Path::ellipse(14.0, 14.0), [0.93, 0.3, 0.35, 1.0], Vec2::new(-290.0 + i as f32 * 22.0, 150.0), None);
    }
    kit.text("message", "CLICK TO PLAY", 22.0, INK, Vec2::new(0.0, 30.0));
    kit.text("hint", "MOUSE OR ARROW KEYS TO MOVE - CATCH THE TOAST, DODGE THE BURNT ONES", 11.0, [0.45, 0.4, 0.36, 1.0], Vec2::new(0.0, -150.0));
    kit.finish(&TOAST_CATCH.replace("__POOL__", &TOAST_POOL.to_string()))
}

const TOAST_CATCH: &str = r#"// Toast Catch — one Code program, runs every frame.
// Page data in:  ctx.data.speed (optional, 1 = normal)
// Events out:    ctx.emit('score', { score }), ctx.emit('gameover', { score })
const POOL = __POOL__
const g = ctx.globals.catch || (ctx.globals.catch = {
  mode: 'ready',            // 'ready' | 'play' | 'over'
  toasts: [],               // { x, y, vy, burnt, live }
  spawnIn: 0,
  plateX: 0,
  text: {},
})
const dt = ctx.deltaSeconds
const get = (id) => getCompByID(id)
const say = (id, text) => { if (g.text[id] !== text) { g.text[id] = text; get(id).textContent = text } }

function start() {
  g.mode = 'play'
  g.toasts = []
  g.spawnIn = 0.4
  ctx.props.score = 0
  ctx.props.lives = 3
  for (let i = 0; i < POOL; i++) get('toast' + i).visible = false
  say('message', '')
}

if (g.mode !== 'play') {
  if (ctx.tap || ctx.keysDown.has('space')) start()
} else {
  // Plate: follow the mouse, or the arrow keys.
  const keys = (ctx.keysDown.has('right') ? 1 : 0) - (ctx.keysDown.has('left') ? 1 : 0)
  if (keys) g.plateX += keys * 420 * dt
  else g.plateX += (ctx.mouseX - g.plateX) * Math.min(1, dt * 14)
  g.plateX = Math.max(-270, Math.min(270, g.plateX))
  get('plate').x = g.plateX

  // Spawn a slice now and then — faster as the score climbs.
  const speed = (Number(ctx.data.speed) || 1) * (1 + ctx.props.score / 40)
  g.spawnIn -= dt
  if (g.spawnIn <= 0 && g.toasts.filter((t) => t.live).length < POOL) {
    g.spawnIn = 0.9 / speed + Math.random() * 0.4
    const free = g.toasts.find((t) => !t.live) || (g.toasts.push({}), g.toasts[g.toasts.length - 1])
    Object.assign(free, { x: -260 + Math.random() * 520, y: 210, vy: 0, burnt: Math.random() < 0.25, live: true, spin: (Math.random() - 0.5) * 200 })
  }

  // Fall, catch, miss.
  g.toasts.forEach((t, i) => {
    const node = get('toast' + i)
    if (!t.live) { node.visible = false; return }
    t.vy -= 520 * speed * dt
    t.y += t.vy * dt
    const caught = t.y < -80 && t.y > -112 && Math.abs(t.x - g.plateX) < 56
    if (caught) {
      t.live = false
      if (t.burnt) {
        ctx.props.lives -= 1
        ctx.playTone({ freq: 180, to: 90, duration: 0.25, type: 'sawtooth', volume: 0.2 })
      } else {
        ctx.props.score += 1
        ctx.playTone({ freq: 660 + ctx.props.score * 8, duration: 0.08, type: 'triangle', volume: 0.25 })
        ctx.emit('score', { score: ctx.props.score })
      }
    } else if (t.y < -200) {
      t.live = false
      if (!t.burnt) ctx.props.lives -= 1
    }
    node.visible = t.live
    node.x = t.x
    node.y = t.y
    node.rotation += t.spin * dt
    node.fillColor = t.burnt ? [0.22, 0.15, 0.11, 1] : [0.97, 0.76, 0.38, 1]
  })

  if (ctx.props.lives <= 0) {
    g.mode = 'over'
    say('message', 'GAME OVER - ' + ctx.props.score + ' TOAST - CLICK TO RETRY')
    ctx.emit('gameover', { score: ctx.props.score })
  }
}

say('scoreText', String(ctx.props.score))
for (let i = 0; i < 3; i++) get('life' + i).visible = i < ctx.props.lives
"#;

// ---------------------------------------------------------------------
// Hop
// ---------------------------------------------------------------------

const HURDLES: usize = 4;

pub(crate) fn build_hop(document: &mut Document) -> GameDemo {
    let mut kit = Kit::new(document, "Hop", 640.0, 240.0, [0.94, 0.95, 0.98, 1.0]);
    kit.property("score", PropKind::Number, "0");
    kit.shape("ground", Path::rect(700.0, 4.0), [0.2, 0.22, 0.3, 1.0], Vec2::new(0.0, -70.0), None);
    for i in 0..HURDLES {
        kit.shape(&format!("hurdle{i}"), round_rect(18.0, 36.0, 4.0), [0.36, 0.42, 0.95, 1.0], Vec2::new(800.0, -50.0), None);
    }
    kit.shape("hopper", round_rect(30.0, 30.0, 8.0), [0.96, 0.62, 0.15, 1.0], Vec2::new(-200.0, -53.0), None);
    kit.text("scoreText", "0", 22.0, [0.2, 0.22, 0.3, 1.0], Vec2::new(250.0, 90.0));
    kit.text("bestText", "", 11.0, [0.45, 0.48, 0.58, 1.0], Vec2::new(250.0, 66.0));
    kit.text("message", "SPACE OR TAP TO HOP", 18.0, [0.2, 0.22, 0.3, 1.0], Vec2::new(0.0, 40.0));
    kit.finish(&HOP.replace("__HURDLES__", &HURDLES.to_string()))
}

const HOP: &str = r#"// Hop — a one-button runner. Space / tap jumps.
// Events out: ctx.emit('gameover', { score, best })
const N = __HURDLES__
const GROUND = -53, GRAVITY = -1900, JUMP = 640
const g = ctx.globals.hop || (ctx.globals.hop = { mode: 'ready', y: GROUND, vy: 0, dist: 0, speed: 260, hurdles: [], next: 0, text: {}, best: Number(ctx.storage.get('hop.best')) || 0 })
const dt = ctx.deltaSeconds
const get = (id) => getCompByID(id)
const say = (id, text) => { if (g.text[id] !== text) { g.text[id] = text; get(id).textContent = text } }
const jumpPressed = ctx.tap || ctx.keysDown.has('space') || ctx.keysDown.has('up')

if (g.mode !== 'play') {
  if (jumpPressed && !g.held) {
    Object.assign(g, { mode: 'play', y: GROUND, vy: 0, dist: 0, speed: 260, hurdles: [], next: 300 })
    say('message', '')
  }
} else {
  // Jump only from the ground; gravity pulls back down.
  const grounded = g.y <= GROUND
  if (jumpPressed && grounded && !g.held) { g.vy = JUMP; ctx.playTone({ freq: 520, to: 780, duration: 0.09, type: 'square', volume: 0.12 }) }
  g.vy += GRAVITY * dt
  g.y = Math.max(GROUND, g.y + g.vy * dt)
  if (g.y === GROUND && g.vy < 0) g.vy = 0

  // World scrolls left; hurdles spawn by distance.
  g.speed += 6 * dt
  g.dist += g.speed * dt
  g.next -= g.speed * dt
  if (g.next <= 0 && g.hurdles.length < N) { g.hurdles.push({ x: 360 }); g.next = 220 + Math.random() * 260 }
  for (const h of g.hurdles) h.x -= g.speed * dt
  g.hurdles = g.hurdles.filter((h) => h.x > -360)

  // Hit test: the hopper (30×30 at x = -200) against each 18×36 hurdle.
  if (g.hurdles.some((h) => Math.abs(h.x + 200) < 22 && g.y - GROUND < 34)) {
    g.mode = 'over'
    const score = Math.floor(g.dist / 40)
    if (score > g.best) { g.best = score; ctx.storage.set('hop.best', score) }
    say('message', 'OUCH - SPACE OR TAP TO GO AGAIN')
    ctx.playTone({ freq: 220, to: 70, duration: 0.3, type: 'sawtooth', volume: 0.18 })
    ctx.emit('gameover', { score, best: g.best })
  }
}
g.held = jumpPressed

const hopper = get('hopper')
hopper.y = g.y
hopper.rotation = g.y > GROUND ? (hopper.rotation - 360 * dt) : 0
for (let i = 0; i < N; i++) {
  const h = g.hurdles[i], node = get('hurdle' + i)
  node.visible = !!h
  if (h) node.x = h.x
}
ctx.props.score = Math.floor(g.dist / 40)
say('scoreText', String(ctx.props.score))
say('bestText', g.best ? 'BEST ' + g.best : '')
"#;

/// Builds the named game (`toast-catch`, `hop`).
pub(crate) fn build(document: &mut Document, kind: &str) -> Option<GameDemo> {
    Some(match kind {
        "toast-catch" => build_toast_catch(document),
        "hop" => build_hop(document),
        _ => return None,
    })
}
