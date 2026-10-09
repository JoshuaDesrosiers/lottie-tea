//! "Toon Explosion" — a cartoon explosion built from layered particle
//! systems, plus a few scripted pops around them.
//!
//! **The particle layers** (all one-shot, World space, emitted by the
//! script with `ref.emit(n)` at the blast point; back to front):
//! 1. *Smoke rim* + *smoke*: grey-violet puffs that drift up, swell and
//!    shrink away. The rim layer is the same puffs a little darker and
//!    bigger, so the cloud's edge reads as an ink outline.
//! 2. *Debris*: dark chunks thrown out and falling, tumbling.
//! 3. *Fire rim* + *fire*: the fireball — orange → red puffs that pop out
//!    fast, stall (high drag) and shrink to nothing (toon puffs shrink
//!    instead of fading). The dark-red rim layer behind them is the
//!    fireball's outline.
//! 4. *Core*: a hot yellow-white centre that's gone in a blink.
//! 5. *Sparks*: tiny fast yellow embers arcing down.
//! 6. *Shockwave*: dots on a circle flying straight out at one speed — an
//!    expanding ring that thins as it grows.
//!
//! **Scripted extras**: a spiky outlined starburst flash, a comic word
//! ("BOOM!", "KA-POW!"…) that pops and wobbles, and screen shake.
//! Click anywhere to blow it up; until the first click it demos itself.

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Command, Document, EmitterShape, FontAsset, GradientStop, LoopMode, Node, NodeBehavior, NodeKind, ParticleRange, ParticleSystemNode, Path, ProgramDef,
    ShapeNode, SimulationSpace, SizeKeyframe, StateDef, StateMachineDef, Stroke,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");
const SCREEN_W: f32 = 960.0;
const SCREEN_H: f32 = 540.0;
/// Pooled starburst flashes and comic words (explosions can overlap).
const FLASHES: usize = 4;
const WORDS: usize = 4;
/// Toon ink: the outline colour everything shares.
const INK: [f32; 4] = [0.13, 0.07, 0.12, 1.0];

pub(crate) struct BoomDemo {
    pub scene: ArtboardId,
}

fn shape(name: &str, path: Path, color: [f32; 4], at: Vec2, outline: Option<f32>) -> Node {
    let stroke = outline.map(|width| Stroke { width, color: INK, gradient: None, image: None, cap: Default::default(), join: Default::default() });
    let mut node = Node::new(name, None, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke }));
    node.transform.translation = at;
    node
}

/// One particle layer of the explosion: a one-shot World-space emitter
/// (no rate, no bursts — the script calls `emit`), parked off screen.
struct Layer<'a> {
    id: &'a str,
    max: u32,
    life: (f32, f32),
    speed: (f32, f32),
    size: (f32, f32),
    color: [f32; 4],
    gravity: f32,
    drag: f32,
    shape: EmitterShape,
    /// (normalized age, size multiplier)
    sizes: &'a [(f32, f32)],
    /// (normalized age, RGBA tint) — multiplies `color`.
    colors: &'a [(f32, [f32; 4])],
    spin: f32,
}

fn layer(l: Layer) -> Node {
    let mut config = ParticleSystemNode::default();
    config.main.looping = false;
    config.main.play_on_awake = false;
    config.main.duration = 0.1;
    config.main.start_lifetime = ParticleRange { min: l.life.0, max: l.life.1 };
    config.main.start_speed = ParticleRange { min: l.speed.0, max: l.speed.1 };
    config.main.start_size = ParticleRange { min: l.size.0, max: l.size.1 };
    config.main.start_rotation = ParticleRange { min: 0.0, max: std::f32::consts::TAU };
    config.main.start_color = l.color;
    config.main.gravity_modifier = l.gravity;
    config.main.drag = l.drag;
    config.main.simulation_space = SimulationSpace::World;
    config.main.max_particles = l.max;
    config.emission.rate_over_time = 0.0;
    config.emission.bursts.clear();
    config.shape = l.shape;
    config.size_over_lifetime = l.sizes.iter().map(|&(t, size_multiplier)| SizeKeyframe { t, size_multiplier }).collect();
    config.color_over_lifetime = l.colors.iter().map(|&(offset, color)| GradientStop { offset, color, midpoint: 0.5 }).collect();
    config.rotation_speed = l.spin;
    let mut node = with_id(Node::with_components(l.id, None, vec![NodeBehavior::ParticleSystem(config)]), l.id);
    node.transform.translation = Vec2::new(0.0, -2000.0);
    node
}

pub(crate) fn build_demo_boom(document: &mut Document) -> BoomDemo {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: FONT_BYTES.to_vec() });
    let inverse = document.apply(Command::AddArtboard { name: "Toon Explosion".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    let camera = with_id(
        Node::with_components(
            "Main Camera",
            None,
            vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.55, 0.82, 0.96, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })],
        ),
        "camera",
    );
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));

    // ---- a toon backdrop: outlined clouds, hills and ground ----
    let (hw, hh) = (SCREEN_W / 2.0, SCREEN_H / 2.0);
    // Clouds: each puff drawn outlined, then all of them again without the
    // outline on top — the second pass covers the outlines' inner halves,
    // so only the cloud's silhouette keeps its ink line.
    for (x, y, s) in [(-320.0f32, 120.0f32, 1.0f32), (150.0, 60.0, 0.6), (340.0, 105.0, 1.1)] {
        let puffs = [(-38.0f32, 0.0f32, 80.0f32, 56.0f32), (0.0, 14.0, 90.0, 72.0), (38.0, 2.0, 78.0, 54.0), (0.0, -8.0, 110.0, 44.0)];
        for outline in [Some(3.5), None] {
            for (dx, dy, w, h) in puffs {
                add_node(document, &mut inverses, scene, shape("Cloud", Path::ellipse(w * s, h * s), [1.0, 1.0, 1.0, 1.0], Vec2::new(x + dx * s, y + dy * s), outline));
            }
        }
    }
    for (x, w, h, c) in [(-340.0f32, 520.0f32, 300.0f32, [0.42, 0.74, 0.38, 1.0]), (260.0, 640.0, 260.0, [0.36, 0.68, 0.34, 1.0]), (-20.0, 460.0, 200.0, [0.48, 0.8, 0.42, 1.0])] {
        add_node(document, &mut inverses, scene, shape("Hill", Path::ellipse(w, h), c, Vec2::new(x, -hh + 70.0), Some(4.0)));
    }
    add_node(document, &mut inverses, scene, shape("Ground", Path::rect(SCREEN_W + 40.0, 120.0), [0.86, 0.66, 0.4, 1.0], Vec2::new(0.0, -hh + 20.0), Some(4.0)));
    for i in 0..9 {
        let x = -hw + 60.0 + i as f32 * 110.0;
        add_node(document, &mut inverses, scene, shape("Pebble", Path::ellipse(18.0, 9.0), [0.72, 0.52, 0.32, 1.0], Vec2::new(x, -hh + 30.0 + (i % 3) as f32 * 14.0), Some(2.0)));
    }

    // ---- the explosion's particle layers (back to front) ----
    let pop = |rise: f32, hold: f32| [(0.0, 0.25), (rise, 1.2), (hold, 1.0), (1.0, 0.0)];
    let smoke_sizes = [(0.0, 0.2), (0.3, 1.0), (0.75, 0.9), (1.0, 0.0)];
    add_node(document, &mut inverses, scene, layer(Layer {
        id: "smokeRim", max: 120, life: (1.0, 1.5), speed: (40.0, 120.0), size: (46.0, 70.0), color: INK, gravity: -70.0, drag: 3.0,
        shape: EmitterShape::Circle { radius: 22.0 }, sizes: &smoke_sizes, colors: &[], spin: 0.0,
    }));
    add_node(document, &mut inverses, scene, layer(Layer {
        id: "smoke", max: 120, life: (0.95, 1.45), speed: (40.0, 120.0), size: (36.0, 58.0), color: [0.56, 0.52, 0.62, 1.0], gravity: -70.0, drag: 3.0,
        shape: EmitterShape::Circle { radius: 22.0 }, sizes: &smoke_sizes, colors: &[(0.0, [1.0, 0.8, 0.7, 1.0]), (0.35, [1.0, 1.0, 1.0, 1.0]), (1.0, [0.8, 0.8, 0.85, 1.0])], spin: 0.0,
    }));
    add_node(document, &mut inverses, scene, layer(Layer {
        id: "debris", max: 80, life: (0.9, 1.4), speed: (200.0, 380.0), size: (8.0, 14.0), color: [0.32, 0.2, 0.16, 1.0], gravity: 900.0, drag: 0.4,
        shape: EmitterShape::Circle { radius: 10.0 }, sizes: &[(0.0, 1.0), (0.8, 1.0), (1.0, 0.0)], colors: &[], spin: 9.0,
    }));
    let fire_rim_sizes = pop(0.14, 0.55);
    add_node(document, &mut inverses, scene, layer(Layer {
        id: "fireRim", max: 160, life: (0.7, 1.0), speed: (60.0, 220.0), size: (46.0, 72.0), color: [0.5, 0.06, 0.05, 1.0], gravity: -40.0, drag: 6.0,
        shape: EmitterShape::Circle { radius: 8.0 }, sizes: &fire_rim_sizes, colors: &[], spin: 0.0,
    }));
    let fire_sizes = pop(0.15, 0.5);
    add_node(document, &mut inverses, scene, layer(Layer {
        id: "fire", max: 160, life: (0.6, 0.88), speed: (50.0, 200.0), size: (36.0, 58.0), color: [1.0, 1.0, 1.0, 1.0], gravity: -40.0, drag: 6.0,
        shape: EmitterShape::Circle { radius: 8.0 }, sizes: &fire_sizes,
        colors: &[(0.0, [1.0, 0.92, 0.35, 1.0]), (0.35, [1.0, 0.6, 0.12, 1.0]), (0.75, [0.95, 0.3, 0.1, 1.0]), (1.0, [0.75, 0.15, 0.1, 1.0])], spin: 0.0,
    }));
    add_node(document, &mut inverses, scene, layer(Layer {
        id: "core", max: 80, life: (0.28, 0.42), speed: (10.0, 90.0), size: (34.0, 50.0), color: [1.0, 1.0, 1.0, 1.0], gravity: 0.0, drag: 6.0,
        shape: EmitterShape::Circle { radius: 4.0 }, sizes: &[(0.0, 0.5), (0.2, 1.2), (1.0, 0.0)], colors: &[(0.0, [1.0, 1.0, 0.92, 1.0]), (1.0, [1.0, 0.85, 0.3, 1.0])], spin: 0.0,
    }));
    add_node(document, &mut inverses, scene, layer(Layer {
        id: "sparks", max: 200, life: (0.4, 0.75), speed: (260.0, 520.0), size: (5.0, 9.0), color: [1.0, 1.0, 1.0, 1.0], gravity: 500.0, drag: 1.6,
        shape: EmitterShape::Circle { radius: 12.0 }, sizes: &[(0.0, 1.0), (1.0, 0.0)], colors: &[(0.0, [1.0, 0.98, 0.6, 1.0]), (0.5, [1.0, 0.75, 0.2, 1.0]), (1.0, [1.0, 0.4, 0.1, 1.0])], spin: 0.0,
    }));
    add_node(document, &mut inverses, scene, layer(Layer {
        id: "ring", max: 240, life: (0.3, 0.3), speed: (520.0, 520.0), size: (15.0, 15.0), color: [1.0, 1.0, 1.0, 1.0], gravity: 0.0, drag: 1.5,
        shape: EmitterShape::Circle { radius: 26.0 }, sizes: &[(0.0, 1.3), (1.0, 0.0)], colors: &[(0.0, [1.0, 1.0, 0.9, 1.0]), (1.0, [1.0, 0.95, 0.75, 1.0])], spin: 0.0,
    }));

    // ---- scripted pops: starburst flashes and comic words ----
    for i in 0..FLASHES {
        let mut flash = with_id(shape(&format!("flash{i}"), Path::star(10, 110.0, 52.0), [1.0, 0.97, 0.6, 1.0], Vec2::new(0.0, -2000.0), Some(6.0)), &format!("flash{i}"));
        flash.visible = false;
        add_node(document, &mut inverses, scene, flash);
    }
    for i in 0..WORDS {
        for (id, color, offset) in [(format!("wordShadow{i}"), INK, Vec2::new(5.0, -5.0)), (format!("word{i}"), [1.0, 0.86, 0.18, 1.0], Vec2::ZERO)] {
            let mut word = text_node(None, &id, "BOOM!", font, 34.0, color, Vec2::new(0.0, -2000.0) + offset);
            word.visible = false;
            add_node(document, &mut inverses, scene, word);
        }
    }

    // ---- HUD ----
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "titleShadow", "TOON EXPLOSION", font, 22.0, INK, Vec2::new(3.0, hh - 96.0 - 3.0)));
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "title", "TOON EXPLOSION", font, 22.0, [1.0, 0.86, 0.18, 1.0], Vec2::new(0.0, hh - 96.0)));
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "hint", "CLICK ANYWHERE TO BLOW IT UP", font, 10.0, INK, Vec2::new(0.0, hh - 126.0)));

    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Toon Explosion (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Toon Explosion".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Play".into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(format!("const FLASHES = {FLASHES}, WORDS = {WORDS}, SCREEN_W = {SCREEN_W}, SCREEN_H = {SCREEN_H}\n{SCRIPT}"))),
            }],
            initial_state: "Play".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));
    inverses.reverse();
    document.commit_batch(inverses);
    BoomDemo { scene }
}

const SCRIPT: &str = r#"// Toon Explosion — fires every particle layer at the blast point
// (`ref.emit`), then animates the starburst flash, the comic word and the
// screen shake. See `demo_boom.rs` for how each layer is tuned.
const ref = (id) => getCompByID(id)
const dt = Math.min(ctx.deltaSeconds || 1 / 60, 1 / 20)
const g = ctx.globals.boom || (ctx.globals.boom = {
  auto: 0.6, clicked: false, wasDown: false, shake: 0, nextFlash: 0, nextWord: 0,
  flashes: Array.from({ length: FLASHES }, () => ({ t: -1 })),
  words: Array.from({ length: WORDS }, () => ({ t: -1, x: 0, y: 0, tilt: 0 })),
})
const WORDS_LIST = ['BOOM!', 'KA-BOOM!', 'POW!', 'BLAM!', 'KRAKA-BOOM!', 'WHAM!']
// [layer, particles per blast]
const LAYERS = [['smokeRim', 11], ['smoke', 11], ['debris', 9], ['fireRim', 16], ['fire', 16], ['core', 7], ['sparks', 22], ['ring', 46]]

function explode(x, y) {
  for (const [id, n] of LAYERS) {
    const r = ref(id)
    r.x = x; r.y = y
    r.emit(n)
  }
  const f = g.flashes[g.nextFlash]
  Object.assign(f, { t: 0, x, y, spin: Math.random() * 360 })
  g.nextFlash = (g.nextFlash + 1) % FLASHES
  const w = g.words[g.nextWord]
  Object.assign(w, { t: 0, x: x + (Math.random() - 0.5) * 60, y: y + 95, tilt: (Math.random() - 0.5) * 24 })
  const text = WORDS_LIST[Math.floor(Math.random() * WORDS_LIST.length)]
  ref('word' + g.nextWord).textContent = text
  ref('wordShadow' + g.nextWord).textContent = text
  g.nextWord = (g.nextWord + 1) % WORDS
  g.shake = Math.min(1.4, g.shake + 1)
  // A thump and a crackle.
  ctx.playTone({ freq: 120, to: 35, duration: 0.45, type: 'noise', volume: 0.35 })
  ctx.playTone({ freq: 70, to: 30, duration: 0.35, type: 'sine', volume: 0.4 })
}

// Click to blow up at the cursor; until then, it demos itself.
const down = ctx.keysDown.has('mouse')
if (down && !g.wasDown) { g.clicked = true; explode(ctx.mouseX, ctx.mouseY) }
g.wasDown = down
if (!g.clicked) {
  g.auto -= dt
  if (g.auto <= 0) {
    g.auto = 1.1 + Math.random() * 0.6
    explode((Math.random() - 0.5) * SCREEN_W * 0.7, -40 + (Math.random() - 0.5) * 160)
  }
}

// Starburst flash: snaps open, spins a touch, collapses — 0.22 s.
for (let i = 0; i < FLASHES; i++) {
  const f = g.flashes[i]
  const r = ref('flash' + i)
  if (f.t < 0) { if (r.visible) r.visible = false; continue }
  f.t += dt
  const k = f.t / 0.22
  if (k >= 1) { f.t = -1; r.visible = false; continue }
  const s = k < 0.3 ? 0.4 + (k / 0.3) * 0.9 : 1.3 * (1 - (k - 0.3) / 0.7)
  r.visible = true
  r.x = f.x; r.y = f.y
  r.scaleX = s; r.scaleY = s
  r.rotation = f.spin + k * 25
}

// Comic word: pops in with overshoot, wobbles, floats up, pops out — 1 s.
for (let i = 0; i < WORDS; i++) {
  const w = g.words[i]
  const word = ref('word' + i), shadow = ref('wordShadow' + i)
  if (w.t < 0) { if (word.visible) { word.visible = false; shadow.visible = false } continue }
  w.t += dt
  const t = w.t
  if (t >= 1) { w.t = -1; word.visible = false; shadow.visible = false; continue }
  let s
  if (t < 0.12) s = (t / 0.12) * 1.45
  else if (t < 0.24) s = 1.45 - ((t - 0.12) / 0.12) * 0.45
  else if (t < 0.85) s = 1 + Math.sin((t - 0.24) * 14) * 0.04
  else s = 1 - (t - 0.85) / 0.15
  const y = w.y + t * 28
  const rot = w.tilt + Math.sin(t * 18) * 4 * (1 - t)
  for (const [r, ox, oy] of [[shadow, 5, -5], [word, 0, 0]]) {
    r.visible = true
    r.x = w.x + ox * s; r.y = y + oy * s
    r.scaleX = s; r.scaleY = s
    r.rotation = rot
  }
}

// Screen shake (decays fast).
g.shake = Math.max(0, g.shake - dt * 3.2)
const cam = ref('camera')
const amp = g.shake * g.shake * 14
cam.x = (Math.random() - 0.5) * amp
cam.y = (Math.random() - 0.5) * amp
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_every_particle_layer_the_script_fires() {
        let mut document = Document::new();
        let demo = build_demo_boom(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let layers: Vec<&str> = artboard.node_ids().filter_map(|id| artboard.node(id)).filter(|n| n.particle_system().is_some()).map(|n| n.id.as_str()).collect();
        for id in ["smokeRim", "smoke", "debris", "fireRim", "fire", "core", "sparks", "ring"] {
            assert!(layers.contains(&id), "missing layer {id}");
            assert!(SCRIPT.contains(&format!("['{id}',")), "script doesn't fire {id}");
        }
    }
}
