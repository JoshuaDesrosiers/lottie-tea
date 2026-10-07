//! "Stress Test" — a scene that scales up on demand, for profiling (press
//! F3 for the frame breakdown).
//!
//! Pools built once, hidden; the script reveals tiers live:
//! - Up / Down: moving sprites, 0..=4000 in steps of 500 (each one a script
//!   ref writing its position every frame — measures script + draw cost).
//! - Right / Left: dynamic physics crates, 0..=300 in steps of 50.
//! - L: toggles 16 shadow-casting lights; P: toggles 8 particle fountains.

use an1mbyt3_core::{
    Animation, ArtboardId, BoxCollider2D, CameraNode, Command, Document, EmitterShape, FontAsset, GradientStop, ImageId, Light2DNode, LoopMode, Node,
    NodeBehavior, NodeBehaviorEntry, NodeKind, ParticleRange, ParticleSystemNode, Path, ProgramDef, Rigidbody2D, ShapeNode, SimulationSpace, StateDef,
    StateMachineDef,
};
use glam::Vec2;

use crate::demo_dungeon::glow;
use crate::demo_ninja::{add_grid_frames, add_node, image_node, text_node, with_id};

const UNITS_BYTES: &[u8] = include_bytes!("../assets/dungeon/units.png");
const PIXEL_FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");

pub(crate) const SPRITES: usize = 4000;
pub(crate) const CRATES: usize = 300;
const LIGHTS: usize = 16;
const FOUNTAINS: usize = 8;
const W: f32 = 960.0;
const H: f32 = 540.0;

pub(crate) struct StressDemo {
    pub scene: ArtboardId,
    pub images: Vec<ImageId>,
}

fn rect(name: &str, w: f32, h: f32, color: [f32; 4], at: Vec2) -> Node {
    let mut node = Node::new(name, None, NodeKind::Shape(ShapeNode { path: Path::rect(w, h), fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None }));
    node.transform.translation = at;
    node
}

fn hidden(mut node: Node) -> Node {
    node.visible = false;
    node
}

pub(crate) fn build_demo_stress(document: &mut Document) -> StressDemo {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: PIXEL_FONT_BYTES.to_vec() });
    let units = add_grid_frames(document, "stress_unit", UNITS_BYTES, 16, 16, 150);
    let images = units.clone();

    let inverse = document.apply(Command::AddArtboard { name: "Stress Test".into(), width: W, height: H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    let camera = with_id(
        Node::with_components("Main Camera", None, vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.08, 0.08, 0.1, 1.0], ..CameraNode::default() })]),
        "camera",
    );
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));

    // Floor + walls for the crates.
    let solid = |w: f32, h: f32| vec![NodeBehaviorEntry { behavior: NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::new(w, h), ..Default::default() }), enabled: true }];
    add_node(document, &mut inverses, scene, Node { behaviors: solid(W, 20.0), ..rect("Floor", W, 20.0, [0.25, 0.25, 0.3, 1.0], Vec2::new(0.0, -H / 2.0 + 10.0)) });
    for x in [-W / 2.0 + 5.0, W / 2.0 - 5.0] {
        add_node(document, &mut inverses, scene, Node { behaviors: solid(10.0, H), ..rect("Wall", 10.0, H, [0.25, 0.25, 0.3, 1.0], Vec2::new(x, 0.0)) });
    }

    // Sprite pool: unit frames cycling across rows, scattered.
    for i in 0..SPRITES {
        let frame = units[(i * 7) % units.len()];
        let mut sprite = with_id(image_node("Sprite", None, frame, 16.0, 16.0), &format!("s{i}"));
        let h = (i as u32).wrapping_mul(2654435761);
        sprite.transform.translation = Vec2::new(((h % 900) as f32) - 450.0, (((h >> 10) % 460) as f32) - 210.0);
        add_node(document, &mut inverses, scene, hidden(sprite));
    }

    // Crate pool: dynamic bodies stacked in columns.
    for i in 0..CRATES {
        let mut c = rect("Crate", 14.0, 14.0, [0.7, 0.5, 0.3, 1.0], Vec2::new(-420.0 + (i % 50) as f32 * 17.0, -H / 2.0 + 30.0 + (i / 50) as f32 * 16.0));
        c.behaviors = vec![
            NodeBehaviorEntry { behavior: NodeBehavior::Rigidbody2D(Rigidbody2D::default()), enabled: true },
            NodeBehaviorEntry { behavior: NodeBehavior::BoxCollider2D(BoxCollider2D { size: Vec2::new(14.0, 14.0), ..Default::default() }), enabled: true },
        ];
        add_node(document, &mut inverses, scene, hidden(with_id(c, &format!("c{i}"))));
    }

    // Lights (global ambient always on; locals toggled).
    let ambient = Light2DNode { color: [0.6, 0.6, 0.7, 1.0], intensity: 0.5, ..Light2DNode::global() };
    add_node(document, &mut inverses, scene, Node::with_components("Global Light 2D", None, vec![NodeBehavior::Light2D(ambient)]));
    for i in 0..LIGHTS {
        let mut light = glow([0.6 + 0.4 * (i % 3) as f32 / 2.0, 0.6, 1.0 - 0.4 * (i % 2) as f32], 1.2, 110.0);
        light.shadows_enabled = true;
        let mut node = with_id(Node::with_components("Light", None, vec![NodeBehavior::Light2D(light)]), &format!("l{i}"));
        node.transform.translation = Vec2::new(-420.0 + (i % 8) as f32 * 120.0, if i < 8 { 140.0 } else { -60.0 });
        add_node(document, &mut inverses, scene, hidden(node));
    }

    // Particle fountains.
    for i in 0..FOUNTAINS {
        let mut config = ParticleSystemNode::default();
        config.main.looping = true;
        config.main.start_lifetime = ParticleRange { min: 1.2, max: 2.0 };
        config.main.start_speed = ParticleRange { min: 80.0, max: 160.0 };
        config.main.start_size = ParticleRange { min: 2.0, max: 4.0 };
        config.main.start_color = [0.5 + 0.06 * i as f32, 0.8, 1.0, 1.0];
        config.main.gravity_modifier = 1.0;
        config.main.simulation_space = SimulationSpace::World;
        config.main.max_particles = 400;
        config.emission.rate_over_time = 200.0;
        config.emission.bursts.clear();
        config.shape = EmitterShape::Cone { angle: 20.0, radius: 3.0 };
        config.color_over_lifetime = vec![
            GradientStop { offset: 0.0, color: [1.0; 4], midpoint: 0.5 },
            GradientStop { offset: 1.0, color: [1.0, 1.0, 1.0, 0.0], midpoint: 0.5 },
        ];
        let mut node = with_id(Node::with_components("Fountain", None, vec![NodeBehavior::ParticleSystem(config)]), &format!("f{i}"));
        node.transform.translation = Vec2::new(-400.0 + i as f32 * 115.0, -H / 2.0 + 25.0);
        node.unlit = true;
        add_node(document, &mut inverses, scene, hidden(node));
    }

    let mut hud = text_node(Some(camera_id), "hud", "", font, 8.0, [1.0, 0.95, 0.7, 1.0], Vec2::new(0.0, H / 2.0 - 14.0));
    hud.unlit = true;
    add_node(document, &mut inverses, scene, hud);
    let mut help = text_node(Some(camera_id), "help", "UP/DOWN SPRITES  LEFT/RIGHT CRATES  L LIGHTS  P PARTICLES  F3 STATS", font, 8.0, [0.75, 0.78, 0.9, 1.0], Vec2::new(0.0, H / 2.0 - 30.0));
    help.unlit = true;
    add_node(document, &mut inverses, scene, help);

    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Stress (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Stress".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Run".into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(format!("const SPRITES = {SPRITES}, CRATES = {CRATES}, LIGHTS = {LIGHTS}, FOUNTAINS = {FOUNTAINS}\n{STRESS_SCRIPT}"))),
            }],
            initial_state: "Run".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));

    inverses.reverse();
    document.commit_batch(inverses);
    StressDemo { scene, images }
}

const STRESS_SCRIPT: &str = r#"
// Stress Test: reveal pooled objects in tiers; sprites bounce around.
const g = ctx.globals.stress || (ctx.globals.stress = {})
const keys = ctx.keysDown
const dt = Math.min(ctx.deltaSeconds, 1 / 20)
if (!g.init) {
  g.init = true
  g.sprites = []
  for (let i = 0; i < SPRITES; i++) {
    const r = getCompByID('s' + i)
    const a = (i * 2.399) % (Math.PI * 2)
    g.sprites.push({ r, x: r.x, y: r.y, vx: Math.cos(a) * (40 + (i % 60)), vy: Math.sin(a) * (40 + (i % 60)), on: false })
  }
  g.crates = []; for (let i = 0; i < CRATES; i++) g.crates.push(getCompByID('c' + i))
  g.lights = []; for (let i = 0; i < LIGHTS; i++) g.lights.push(getCompByID('l' + i))
  g.fountains = []; for (let i = 0; i < FOUNTAINS; i++) g.fountains.push(getCompByID('f' + i))
  g.spriteTier = 1; g.crateTier = 0; g.lightsOn = false; g.particlesOn = false
  g.was = {}
}
const press = (k) => { const d = keys.has(k); const p = d && !g.was[k]; g.was[k] = d; return p }
if (press('up')) g.spriteTier = Math.min(8, g.spriteTier + 1)
if (press('down')) g.spriteTier = Math.max(0, g.spriteTier - 1)
if (press('right')) g.crateTier = Math.min(6, g.crateTier + 1)
if (press('left')) g.crateTier = Math.max(0, g.crateTier - 1)
if (press('l')) g.lightsOn = !g.lightsOn
if (press('p')) g.particlesOn = !g.particlesOn

const nSprites = g.spriteTier * 500
for (let i = 0; i < SPRITES; i++) {
  const s = g.sprites[i]
  const want = i < nSprites
  if (want !== s.on) { s.on = want; s.r.visible = want }
  if (!want) continue
  s.x += s.vx * dt; s.y += s.vy * dt
  if (s.x < -460 || s.x > 460) s.vx = -s.vx
  if (s.y < -240 || s.y > 250) s.vy = -s.vy
  s.r.x = s.x; s.r.y = s.y
}
const nCrates = g.crateTier * 50
for (let i = 0; i < CRATES; i++) { const want = i < nCrates; if (g.crates[i].visible !== want) g.crates[i].visible = want }
for (const l of g.lights) if (l.visible !== g.lightsOn) l.visible = g.lightsOn
for (const f of g.fountains) if (f.visible !== g.particlesOn) f.visible = g.particlesOn
const hud = 'SPRITES ' + nSprites + '  CRATES ' + nCrates + '  LIGHTS ' + (g.lightsOn ? LIGHTS : 0) + '  PARTICLES ' + (g.particlesOn ? 'ON' : 'OFF')
const h = getCompByID('hud')
if (h.textContent !== hud) h.textContent = hud
"#;
