//! "Shatterline" — a Smash Hit-style rail shooter built on breakable glass
//! (`Primitive3D::Glass`).
//!
//! You glide down an endless architectural corridor that never stops. Tap
//! (click) to throw chrome balls; every throw costs one. Hitting crystals
//! earns balls back (+3), and ten crystals in a row without missing one
//! steps up **multiball** (two, three … five balls per throw). Any glass
//! still standing where the camera passes is a crash: −10 balls. Run out
//! and the run is over. You start with 40.
//!
//! - **Glass** is real fracture, not a swap to a "broken" model: each pane
//!   is pre-cut into Voronoi pieces; a ball knocks out the pieces round its
//!   impact (the heart splinters into slivers), cracks whiten the pieces
//!   next to the hole, and anything cut off from the pane's held edges
//!   falls. Bars held top and bottom lose a bite, not the whole bar; a
//!   pane hanging from the ceiling drops everything below a break; fan
//!   blades held at their hub lose their tips. Shards tumble, bounce on
//!   the floor and fade (all in `an1mbyt3_geometry::glass`).
//! - **Sixteen zones** (+ endless), each its own palette, speed, setting
//!   and mix of obstacles. Settings: *interior* halls, *open* sky
//!   platforms (a sky dome, floating pylons, balls roll off the edge) and
//!   *canyons* (towering walls, open sky). Obstacles: framed walls, double
//!   walls, glass cages, 2×2 grids, sliders, swinging hangers, rotating
//!   fans, spinners, a rotating **DNA helix** of glass rungs, swinging
//!   **hammers** (metal: dodge them), **thick glass** (cracks, takes 2–3
//!   hits) and doors you open by shattering their diamond target. A
//!   checkpoint gate opens each zone; reached zones unlock on the start
//!   menu (saved in `ctx.storage`).
//! - **Rapid fire**: an orange crystal; for 8 s, hold to fire free balls.
//! - **Crashing** throws the 10 lost balls out in front of you (they
//!   tumble and bounce, never break anything) and shakes the camera.
//! - **Look**: shader-graph materials — chrome balls (a procedural matcap
//!   reflecting a sky, a dark horizon line and the zone's floor, with hot
//!   highlights), glass (thin tinted faces that thicken towards grazing
//!   angles, bright thick edges, whitened cracks), glowing crystals and
//!   world-locked floor/wall tiles that stream past (no particles: the
//!   glass itself is the effect) — fog, emissive arch
//!   strips and lamps, then Glow (bloom), colour grading and a vignette.
//! - **Sound**: every effect is synthesized here into WAV assets (shatters
//!   from dozens of tiny resonant tinks over a noise burst, a bell for
//!   crystals, a thud for crashes, a pad drone).
//!
//! All gameplay lives in `demo_shatter.js` (prefix constants from here).

use an1mbyt3_core::{
    Animation, ArtboardId, AudioAsset, Camera3D, CameraNode, Command, Document, Light3D, Light3DKind, LoopMode, Material, MaterialId, MaterialKind, Mesh3D, Node, Node3DContent,
    NodeBehavior, NodeId, NodeKind, PostEffect, PostEffectLayer, Primitive3D, ProgramDef, ShaderDomain, ShaderEdgeDef, ShaderGraphDef, ShaderNodeDef,
    ShaderNodeKind, StateDef, StateMachineDef, Transform3D,
};
use an1mbyt3_geometry::glass::{SIDE_ALL, SIDE_BOTTOM, SIDE_LEFT, SIDE_RIGHT, SIDE_TOP};
use glam::{Vec2, Vec3};

use crate::demo_ninja::{add_node, with_id};

const SCREEN_W: f32 = 960.0;
const SCREEN_H: f32 = 540.0;
/// The corridor: half its width, and its height (metres).
const HALF_WIDTH: f32 = 4.0;
const HEIGHT: f32 = 6.0;
/// The camera's eye height and field of view.
const EYE: f32 = 1.7;
const FOV: f32 = 68.0;
/// Pools (the script recycles them as the corridor streams past).
const ARCHES: usize = 14;
const ARCH_GAP: f32 = 12.0;
const BALLS: usize = 40;
const CRYSTALS: usize = 12;
const BIG_PANES: usize = 10;
const SMALL_PANES: usize = 10;
const BARS: usize = 21;
const FANS: usize = 2;
const BLADES: usize = 4;
const HANGERS: usize = 4;
const SPINNERS: usize = 3;
const DOORS: usize = 2;
const DECOS: usize = 16;
const LAMPS: usize = 4;
/// Open-sky pylons, the 10 balls a crash throws away (+2 spare), DNA
/// helixes × their glass rungs, hammers, thick panes, power-up crystals.
const PYLONS: usize = 16;
const LOST: usize = 12;
const HELIXES: usize = 2;
const RUNGS: usize = 18;
/// DNA strand: rung spacing along its axis, twist per rung (degrees), and
/// the backbones' distance from the axis.
const RUNG_GAP: f32 = 0.8;
const RUNG_TWIST: f32 = 28.0;
const STRAND_R: f32 = 0.72;
const HAMMERS: usize = 3;
const THICK: usize = 4;
const POWERS: usize = 3;
/// Pane sizes (local units, before the script's per-obstacle scale).
const BIG: (f32, f32) = (3.2, 2.6);
const SMALL: (f32, f32) = (1.5, 1.5);
const BAR: (f32, f32) = (0.2, HEIGHT);
const BLADE: (f32, f32) = (2.5, 0.55);
const HANG: (f32, f32) = (1.4, 3.6);
const SPIN: (f32, f32) = (2.2, 2.2);
const TARGET: f32 = 0.75;
const THICK_PANE: (f32, f32) = (2.8, 2.4);
/// A hammer's head: how far below its pivot, and its size.
const HAMMER_REACH: f32 = 4.1;

pub(crate) struct ShatterDemo {
    pub scene: ArtboardId,
}

// ------------------------------------------------------------ nodes --

fn node3d(name: &str, parent: Option<NodeId>, transform: Transform3D, content: Node3DContent) -> Node {
    with_id(Node::new(name, parent, NodeKind::Node3D(an1mbyt3_core::Node3D::new(transform, content))), name)
}

fn mesh(primitive: Primitive3D, color: [f32; 4], shininess: f32, unlit: bool, material: Option<MaterialId>) -> Node3DContent {
    Node3DContent::Mesh(Mesh3D { primitive, color, shininess, unlit, material, animation: None, animation_loop: true, animation_overlay: None })
}

fn at(position: Vec3) -> Transform3D {
    Transform3D::at(position)
}

fn scaled(position: Vec3, scale: Vec3) -> Transform3D {
    Transform3D { position, rotation_degrees: Vec3::ZERO, scale }
}

/// Parked out of sight until the script places it.
const PARKED: Vec3 = Vec3::new(0.0, -50.0, 0.0);

fn hidden(mut node: Node) -> Node {
    node.visible = false;
    node
}

fn html_node(name: &str, parent: Option<NodeId>, html: &str, css: &str, interactive: bool) -> Node {
    let mut node = Node::with_components(
        name,
        parent,
        vec![NodeBehavior::UiHtml(an1mbyt3_core::UiHtml { html: html.into(), css: css.into(), width: 1600.0, height: 900.0, interactive })],
    );
    node.transform.scale = Vec2::splat(SCREEN_W / 1600.0);
    with_id(node, name)
}

// --------------------------------------------------------- materials --

/// A node input: wired from another node's output, or a constant.
#[derive(Clone, Copy)]
enum In {
    /// Output 0 of a node.
    N(u32),
    /// Output `port` of a node.
    P(u32, u8),
    F(f32),
    V(f32, f32),
    C([f32; 4]),
}

/// A tiny builder so graphs read as expressions instead of edge lists.
struct Graph {
    nodes: Vec<ShaderNodeDef>,
    edges: Vec<ShaderEdgeDef>,
}

impl Graph {
    fn new() -> Self {
        Self { nodes: Vec::new(), edges: Vec::new() }
    }

    fn next_id(&self) -> u32 {
        self.nodes.len() as u32 + 1
    }

    /// Lays nodes out on a grid (only matters when opened in the Shader tab).
    fn position(id: u32) -> Vec2 {
        Vec2::new((id % 10) as f32 * 220.0, (id / 10) as f32 * 160.0)
    }

    fn op(&mut self, kind: ShaderNodeKind, inputs: &[In]) -> u32 {
        let id = self.next_id();
        let mut defaults = Vec::with_capacity(inputs.len());
        for (port, input) in inputs.iter().enumerate() {
            let wire = |from: u32, from_port: u8| ShaderEdgeDef { from, from_port, to: id, to_port: port as u8 };
            match *input {
                In::N(n) => {
                    self.edges.push(wire(n, 0));
                    defaults.push(None);
                }
                In::P(n, p) => {
                    self.edges.push(wire(n, p));
                    defaults.push(None);
                }
                In::F(v) => defaults.push(Some([v; 4])),
                In::V(x, y) => defaults.push(Some([x, y, 0.0, 0.0])),
                In::C(c) => defaults.push(Some(c)),
            }
        }
        self.nodes.push(ShaderNodeDef { id, kind, position: Self::position(id), value: [0.0; 4], defaults });
        id
    }

    fn color(&mut self, c: [f32; 4]) -> u32 {
        let id = self.next_id();
        self.nodes.push(ShaderNodeDef { id, kind: ShaderNodeKind::ColorConstant, position: Self::position(id), value: c, defaults: Vec::new() });
        id
    }

    fn finish(mut self, out: u32) -> ShaderGraphDef {
        self.op(ShaderNodeKind::Output, &[In::N(out)]);
        ShaderGraphDef { domain: ShaderDomain::Canvas2D, nodes: self.nodes, edges: self.edges }
    }
}

/// Chrome (drawn unlit): a procedural matcap read through Facing UV — a
/// white sky above that fades into the zone colour (the ball's Tint) at
/// the horizon, a dark horizon line, the zone's floor below darkening to
/// the bottom, a darker rim for roundness, and two hot highlights up-left.
fn chrome_graph() -> ShaderGraphDef {
    use In::*;
    use ShaderNodeKind as K;
    let mut g = Graph::new();
    let uv = g.op(K::FacingUv, &[]);
    let s = g.op(K::Split, &[N(uv)]);
    let d = g.op(K::Distance, &[N(uv), V(0.5, 0.5)]);
    let tint = g.op(K::Tint, &[]);
    // Sky: white at the top, towards a pale zone colour near the horizon.
    let sky_low = g.op(K::Mix, &[C([0.86, 0.9, 1.0, 1.0]), N(tint), F(0.45)]);
    let sky_t = g.op(K::Smoothstep, &[F(0.04), F(0.48), P(s, 1)]);
    let sky = g.op(K::Mix, &[C([1.0, 1.0, 1.0, 1.0]), N(sky_low), N(sky_t)]);
    // Ground: the zone colour, darkening towards the bottom.
    let ground_hi = g.op(K::Multiply, &[N(tint), F(0.62)]);
    let ground_lo = g.op(K::Multiply, &[N(tint), F(0.1)]);
    let ground_t = g.op(K::Smoothstep, &[F(0.52), F(0.98), P(s, 1)]);
    let ground = g.op(K::Mix, &[N(ground_hi), N(ground_lo), N(ground_t)]);
    let below = g.op(K::Step, &[F(0.5), P(s, 1)]);
    let env = g.op(K::Mix, &[N(sky), N(ground), N(below)]);
    // The horizon: a thin dark band right at the middle.
    let b1 = g.op(K::Smoothstep, &[F(0.43), F(0.5), P(s, 1)]);
    let b2 = g.op(K::Smoothstep, &[F(0.5), F(0.58), P(s, 1)]);
    let band = g.op(K::Subtract, &[N(b1), N(b2)]);
    let band_k = g.op(K::Multiply, &[N(band), F(0.85)]);
    let banded = g.op(K::Mix, &[N(env), C([0.04, 0.04, 0.06, 1.0]), N(band_k)]);
    // Side reflections: soft vertical bands (the corridor walls).
    let wall = g.op(K::Smoothstep, &[F(0.18), F(0.02), P(s, 0)]);
    let wall2 = g.op(K::Smoothstep, &[F(0.82), F(0.98), P(s, 0)]);
    let walls = g.op(K::Add, &[N(wall), N(wall2)]);
    let walls_k = g.op(K::Multiply, &[N(walls), F(0.35)]);
    let walled = g.op(K::Mix, &[N(banded), N(ground_hi), N(walls_k)]);
    // A darker rim gives the sphere its weight.
    let rim = g.op(K::Smoothstep, &[F(0.34), F(0.5), N(d)]);
    let rim_k = g.op(K::Multiply, &[N(rim), F(0.62)]);
    let rimmed = g.op(K::Mix, &[N(walled), C([0.06, 0.06, 0.08, 1.0]), N(rim_k)]);
    // Highlights (alpha 0, so adding them keeps the ball opaque).
    let o1 = g.op(K::TilingOffset, &[N(uv), V(1.0, 1.0), V(0.15, 0.2)]);
    let h1 = g.op(K::Circle, &[N(o1), F(0.055), F(0.07)]);
    let o2 = g.op(K::TilingOffset, &[N(uv), V(1.0, 1.0), V(-0.13, 0.25)]);
    let h2 = g.op(K::Circle, &[N(o2), F(0.022), F(0.025)]);
    let h2k = g.op(K::Multiply, &[N(h2), F(0.8)]);
    let hl = g.op(K::Add, &[N(h1), N(h2k)]);
    let white = g.color([1.0, 1.0, 1.0, 0.0]);
    let glint = g.op(K::Multiply, &[N(white), N(hl)]);
    let lit = g.op(K::Add, &[N(rimmed), N(glint)]);
    let out = g.op(K::Saturate, &[N(lit)]);
    g.finish(out)
}

/// Glass (lit, see-through): murky tinted sheets you can always
/// read — faces are the pane's Tint darkened (deep teal for the default
/// cyan), about half opaque head on, brightening and thickening towards
/// grazing angles (View Fresnel); side faces (`uv.x` = 1: hole rims, the
/// pane's edge, every shard's edges) **glow** — near-white green-cyan
/// above the bloom threshold, so breaks light up; cracked pieces (`uv.y`)
/// frost over.
fn glass_graph() -> ShaderGraphDef {
    use In::*;
    use ShaderNodeKind as K;
    let mut g = Graph::new();
    let uv = g.op(K::Uv, &[]);
    let s = g.op(K::Split, &[N(uv)]);
    let tint = g.op(K::Tint, &[]);
    let t = g.op(K::Split, &[N(tint)]);
    let fresnel = g.op(K::ViewFresnel, &[F(2.0)]);
    let fresnel_k = g.op(K::Multiply, &[N(fresnel), F(0.42)]);
    let face_a = g.op(K::Add, &[F(0.46), N(fresnel_k)]);
    // Faces: dark tint, lifted towards the rim by the fresnel.
    let rim = g.op(K::Multiply, &[N(fresnel), F(0.35)]);
    let fr0 = g.op(K::Multiply, &[P(t, 0), F(0.2)]);
    let fg0 = g.op(K::Multiply, &[P(t, 1), F(0.38)]);
    let fb0 = g.op(K::Multiply, &[P(t, 2), F(0.45)]);
    let fr = g.op(K::Add, &[N(fr0), N(rim)]);
    let fg = g.op(K::Add, &[N(fg0), N(rim)]);
    let fb = g.op(K::Add, &[N(fb0), N(rim)]);
    let face = g.op(K::Combine, &[N(fr), N(fg), N(fb), N(face_a)]);
    // Edges: a glowing, nearly white green-cyan.
    // (Above 1.0 on purpose: still past the bloom threshold once lit.)
    let er0 = g.op(K::Multiply, &[P(t, 0), F(1.1)]);
    let er = g.op(K::Add, &[N(er0), F(0.3)]);
    let eg0 = g.op(K::Multiply, &[P(t, 1), F(1.5)]);
    let eg = g.op(K::Add, &[N(eg0), F(0.25)]);
    let eb0 = g.op(K::Multiply, &[P(t, 2), F(1.4)]);
    let eb = g.op(K::Add, &[N(eb0), F(0.15)]);
    let edge = g.op(K::Combine, &[N(er), N(eg), N(eb), F(1.0)]);
    let crack_k = g.op(K::Multiply, &[P(s, 1), F(0.55)]);
    let cracked = g.op(K::Mix, &[N(face), C([0.75, 0.9, 0.92, 0.78]), N(crack_k)]);
    let out = g.op(K::Mix, &[N(cracked), N(edge), P(s, 0)]);
    g.finish(out)
}

/// The sky dome (unlit, an inverted sphere round the camera): its Tint at
/// the horizon — the zone's fog colour, so distant geometry melts into it
/// — deepening and blueing towards the zenith, a little darker below.
/// Read through the sphere's own UV (v = 0 at the top, 0.5 the horizon).
fn sky_graph() -> ShaderGraphDef {
    use In::*;
    use ShaderNodeKind as K;
    let mut g = Graph::new();
    let uv = g.op(K::Uv, &[]);
    let s = g.op(K::Split, &[N(uv)]);
    let tint = g.op(K::Tint, &[]);
    let zenith = g.op(K::Multiply, &[N(tint), C([0.5, 0.68, 1.0, 1.0])]);
    let rise = g.op(K::Smoothstep, &[F(0.06), F(0.5), P(s, 1)]);
    let up = g.op(K::OneMinus, &[N(rise)]);
    let upper = g.op(K::Mix, &[N(tint), N(zenith), N(up)]);
    let below = g.op(K::Multiply, &[N(tint), F(0.8)]);
    let ground = g.op(K::Step, &[F(0.5), P(s, 1)]);
    let out = g.op(K::Mix, &[N(upper), N(below), N(ground)]);
    g.finish(out)
}

/// Crystals (unlit): the Tint deepened in the middle, blazing to near
/// white at the silhouette, with a slow shimmer.
fn crystal_graph() -> ShaderGraphDef {
    use In::*;
    use ShaderNodeKind as K;
    let mut g = Graph::new();
    let tint = g.op(K::Tint, &[]);
    let fresnel = g.op(K::ViewFresnel, &[F(1.4)]);
    let deep = g.op(K::Multiply, &[N(tint), F(0.55)]);
    let hot = g.op(K::Add, &[N(tint), C([0.55, 0.55, 0.55, 0.0])]);
    let base = g.op(K::Mix, &[N(deep), N(hot), N(fresnel)]);
    let uv = g.op(K::FacingUv, &[]);
    let pan = g.op(K::Panner, &[N(uv), V(0.05, 0.35)]);
    let noise = g.op(K::Noise, &[N(pan), F(7.0)]);
    let shimmer = g.op(K::Multiply, &[N(noise), F(0.3)]);
    let sheen = g.op(K::Multiply, &[C([0.6, 0.9, 1.0, 0.0]), N(shimmer)]);
    let lit = g.op(K::Add, &[N(base), N(sheen)]);
    let out = g.op(K::Saturate, &[N(lit)]);
    g.finish(out)
}

/// Floors and walls (lit): world-locked tiles — big 4 m slabs with crisp
/// grout lines and fainter 1 m sub-tiles, in the surface's Tint — so the
/// corridor visibly streams past.
fn tile_graph() -> ShaderGraphDef {
    use In::*;
    use ShaderNodeKind as K;
    let mut g = Graph::new();
    let lp = g.op(K::LocalPosition, &[]);
    let tint = g.op(K::Tint, &[]);
    let grout = |g: &mut Graph, tiling: f32, width: f32| {
        let t = g.op(K::TilingOffset, &[N(lp), V(tiling, tiling), V(0.0, 0.0)]);
        let f = g.op(K::Fract, &[N(t)]);
        let s = g.op(K::Split, &[N(f)]);
        let ix = g.op(K::OneMinus, &[P(s, 0)]);
        let iy = g.op(K::OneMinus, &[P(s, 1)]);
        let mx = g.op(K::Min, &[P(s, 0), N(ix)]);
        let my = g.op(K::Min, &[P(s, 1), N(iy)]);
        let m = g.op(K::Min, &[N(mx), N(my)]);
        g.op(K::Smoothstep, &[F(width), F(0.0), N(m)])
    };
    let big = grout(&mut g, 0.25, 0.012);
    let small = grout(&mut g, 1.0, 0.02);
    let small_k = g.op(K::Multiply, &[N(small), F(0.35)]);
    let lines = g.op(K::Max, &[N(big), N(small_k)]);
    let dark = g.op(K::Multiply, &[N(tint), F(0.72)]);
    let out = g.op(K::Mix, &[N(tint), N(dark), N(lines)]);
    g.finish(out)
}

fn add_material(document: &mut Document, inverses: &mut Vec<Command>, name: &str, graph: ShaderGraphDef) -> MaterialId {
    let inverse = document.apply(Command::AddMaterial { material: Material { name: name.into(), kind: MaterialKind::ShaderGraph(graph), deleted: false } });
    let Command::SetMaterialDeleted { material, .. } = inverse else { unreachable!("AddMaterial's inverse is always a SetMaterialDeleted") };
    inverses.push(inverse);
    material
}

// ------------------------------------------------------------ sounds --

const RATE: u32 = 22_050;

/// 16-bit mono PCM WAV.
fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    // Normalise to a safe peak.
    let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-6);
    let gain = 0.89 / peak;
    for s in samples {
        out.extend_from_slice(&(((s * gain).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

/// Deterministic noise for the synths.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
    fn unit(&mut self) -> f32 {
        self.next() * 0.5 + 0.5
    }
}

/// Breaking glass: a bright noise crack (high-passed, fast decay), then
/// `tinks` tiny resonant pings scattered over the tail — the pieces hitting
/// each other and the floor — plus, for big breaks, a low thump.
fn shatter_sound(seconds: f32, tinks: usize, thump: f32, seed: u32) -> Vec<f32> {
    let n = (seconds * RATE as f32) as usize;
    let mut out = vec![0.0f32; n];
    let mut rng = Noise(seed);
    let mut prev = 0.0;
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        let raw = rng.next();
        let hp = raw - prev;
        prev = raw;
        *s += hp * (-t * 26.0).exp() * 0.9 + raw * (-t * 9.0).exp() * 0.12;
        if thump > 0.0 {
            *s += (t * 72.0 * std::f32::consts::TAU).sin() * (-t * 14.0).exp() * thump;
        }
    }
    for k in 0..tinks {
        // Most pieces land early; a few trickle on.
        let start = rng.unit().powf(2.2) * seconds * 0.82;
        let freq = 2200.0 + rng.unit() * 7200.0;
        let decay = 30.0 + rng.unit() * 70.0;
        let amp = (0.25 + rng.unit() * 0.75) * (1.0 - k as f32 / tinks as f32 * 0.5);
        let begin = (start * RATE as f32) as usize;
        let len = ((5.0 / decay) * RATE as f32) as usize;
        for j in 0..len.min(n.saturating_sub(begin)) {
            let t = j as f32 / RATE as f32;
            out[begin + j] += (t * freq * std::f32::consts::TAU).sin() * (-t * decay).exp() * amp * 0.4;
        }
    }
    out
}

/// A crystal: a glassy bell (inharmonic partials) with a rising sparkle.
fn crystal_sound() -> Vec<f32> {
    let n = (1.1 * RATE as f32) as usize;
    let base = 1318.5;
    let partials = [(1.0, 1.0, 5.0), (2.0, 0.5, 7.0), (2.76, 0.4, 9.0), (5.4, 0.25, 14.0), (8.93, 0.12, 20.0)];
    let mut rng = Noise(99);
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let bell: f32 = partials.iter().map(|(m, a, d)| (t * base * m * std::f32::consts::TAU).sin() * a * (-t * d).exp()).sum();
            let sparkle = rng.next() * (-t * 30.0).exp() * 0.05;
            bell * (1.0 - (-t * 400.0).exp()) + sparkle
        })
        .collect()
}

/// A throw: a short airy whoosh (band-limited noise, swelling and gone).
fn throw_sound() -> Vec<f32> {
    let n = (0.22 * RATE as f32) as usize;
    let mut rng = Noise(7);
    let (mut lp, mut lp2) = (0.0f32, 0.0f32);
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let env = (t / 0.05).min(1.0) * (-((t - 0.05).max(0.0)) * 22.0).exp();
            let raw = rng.next();
            lp += (raw - lp) * 0.35;
            lp2 += (lp - lp2) * 0.35;
            (lp - lp2) * env + (t * (300.0 - t * 600.0) * std::f32::consts::TAU).sin() * env * 0.15
        })
        .collect()
}

/// Doors sliding open: a filtered rumble with a rising tone.
fn door_sound() -> Vec<f32> {
    let n = (0.75 * RATE as f32) as usize;
    let mut rng = Noise(31);
    let mut lp = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let env = (t / 0.04).min(1.0) * (-t * 4.5).exp();
            lp += (rng.next() - lp) * 0.06;
            lp * env * 3.0 + (t * (110.0 + t * 160.0) * std::f32::consts::TAU).sin() * env * 0.35
        })
        .collect()
}

/// Passing a checkpoint: a soft major chord swelling in.
fn checkpoint_sound() -> Vec<f32> {
    let n = (2.2 * RATE as f32) as usize;
    let notes = [523.25, 659.25, 783.99, 1046.5];
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let env = (t / 0.25).min(1.0) * (-t * 1.6).exp();
            notes.iter().enumerate().map(|(k, f)| (t * f * std::f32::consts::TAU + k as f32).sin() * (1.0 - k as f32 * 0.18)).sum::<f32>() * env
        })
        .collect()
}

/// The ambient drone (loops): two detuned pad chords breathing slowly.
fn drone_sound() -> Vec<f32> {
    let seconds = 8.0;
    let n = (seconds * RATE as f32) as usize;
    // Frequencies chosen to complete whole cycles over the loop (no click).
    let notes = [110.0, 164.875, 220.0, 277.125, 330.0];
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let breathe = 0.65 + 0.35 * (t / seconds * std::f32::consts::TAU).sin();
            notes.iter().enumerate().map(|(k, f)| (t * f * std::f32::consts::TAU).sin() * (0.5 / (1.0 + k as f32 * 0.6)) + (t * (f + 0.125) * std::f32::consts::TAU).sin() * 0.18).sum::<f32>() * breathe
        })
        .collect()
}

// ------------------------------------------------------------- zones --

/// A zone's palette: fog (also the sky's horizon), floor, walls, arches,
/// glow strips/lamps, the glass tint and ambient light — and its setting
/// (`env`): `"interior"` (a hall: walls, ceiling, arches), `"open"` (a
/// platform in the sky: no walls, floating pylons, balls fall off the
/// edge) or `"canyon"` (towering walls, open sky).
struct Zone {
    name: &'static str,
    env: &'static str,
    fog: [f32; 3],
    floor: [f32; 3],
    wall: [f32; 3],
    arch: [f32; 3],
    glow: [f32; 3],
    glass: [f32; 3],
    ambient: [f32; 3],
}

macro_rules! zone {
    ($name:expr, $env:expr, $fog:expr, $floor:expr, $wall:expr, $arch:expr, $glow:expr, $glass:expr, $ambient:expr) => {
        Zone { name: $name, env: $env, fog: $fog, floor: $floor, wall: $wall, arch: $arch, glow: $glow, glass: $glass, ambient: $ambient }
    };
}

const ZONES: &[Zone] = &[
    zone!("Dawn", "interior", [0.8, 0.86, 0.92], [0.66, 0.71, 0.77], [0.78, 0.82, 0.88], [0.52, 0.58, 0.68], [1.0, 0.97, 0.9], [0.62, 0.95, 1.0], [0.5, 0.54, 0.6]),
    zone!("Lagoon", "open", [0.62, 0.86, 0.9], [0.5, 0.74, 0.76], [0.6, 0.86, 0.86], [0.32, 0.56, 0.6], [0.8, 1.0, 0.95], [0.7, 1.0, 0.95], [0.52, 0.62, 0.64]),
    zone!("Ember", "canyon", [0.98, 0.7, 0.5], [0.85, 0.55, 0.4], [0.9, 0.62, 0.46], [0.6, 0.32, 0.24], [1.0, 0.85, 0.55], [0.85, 1.0, 0.95], [0.66, 0.52, 0.46]),
    zone!("Orchard", "open", [0.76, 0.9, 0.66], [0.55, 0.72, 0.42], [0.74, 0.88, 0.62], [0.36, 0.5, 0.28], [1.0, 1.0, 0.8], [0.82, 1.0, 0.92], [0.54, 0.62, 0.48]),
    zone!("Violet Hall", "interior", [0.66, 0.56, 0.9], [0.5, 0.42, 0.74], [0.7, 0.62, 0.92], [0.32, 0.24, 0.55], [1.0, 0.86, 1.0], [0.8, 0.95, 1.0], [0.52, 0.48, 0.66]),
    zone!("Foundry", "interior", [0.2, 0.16, 0.15], [0.24, 0.21, 0.2], [0.3, 0.26, 0.24], [0.12, 0.1, 0.1], [1.0, 0.55, 0.15], [0.7, 1.0, 0.9], [0.3, 0.26, 0.24]),
    zone!("Skyway", "open", [0.64, 0.8, 0.98], [0.82, 0.85, 0.9], [0.8, 0.84, 0.9], [0.5, 0.56, 0.68], [1.0, 1.0, 0.95], [0.6, 0.92, 1.0], [0.58, 0.62, 0.7]),
    zone!("Glacier", "canyon", [0.76, 0.9, 0.98], [0.6, 0.76, 0.88], [0.7, 0.84, 0.94], [0.4, 0.58, 0.74], [0.75, 0.95, 1.0], [0.55, 0.9, 1.0], [0.52, 0.6, 0.68]),
    zone!("Rose", "interior", [0.98, 0.72, 0.8], [0.86, 0.55, 0.64], [0.97, 0.75, 0.82], [0.6, 0.3, 0.4], [1.0, 0.9, 0.92], [0.85, 1.0, 1.0], [0.66, 0.54, 0.58]),
    zone!("Dunes", "open", [0.98, 0.82, 0.6], [0.86, 0.68, 0.46], [0.92, 0.76, 0.56], [0.62, 0.42, 0.26], [1.0, 0.9, 0.7], [0.8, 1.0, 0.92], [0.66, 0.56, 0.46]),
    zone!("Night", "open", [0.06, 0.08, 0.18], [0.1, 0.12, 0.22], [0.12, 0.14, 0.26], [0.05, 0.06, 0.12], [0.3, 0.9, 1.0], [0.6, 0.9, 1.0], [0.22, 0.26, 0.38]),
    zone!("Abyss", "canyon", [0.1, 0.16, 0.2], [0.12, 0.18, 0.22], [0.16, 0.24, 0.28], [0.06, 0.1, 0.12], [0.4, 1.0, 0.8], [0.5, 1.0, 0.85], [0.22, 0.3, 0.32]),
    zone!("Helix Labs", "interior", [0.86, 0.92, 0.94], [0.76, 0.82, 0.84], [0.9, 0.94, 0.95], [0.28, 0.66, 0.6], [0.6, 1.0, 0.85], [0.6, 1.0, 0.9], [0.56, 0.62, 0.64]),
    zone!("Aurora", "open", [0.1, 0.2, 0.3], [0.14, 0.2, 0.3], [0.18, 0.26, 0.36], [0.08, 0.12, 0.18], [0.5, 1.0, 0.7], [0.55, 1.0, 0.85], [0.26, 0.32, 0.4]),
    zone!("Zenith", "open", [0.96, 0.88, 0.7], [0.78, 0.68, 0.48], [0.9, 0.82, 0.64], [0.6, 0.46, 0.24], [1.0, 0.95, 0.75], [0.7, 1.0, 0.92], [0.6, 0.56, 0.48]),
    zone!("Core", "interior", [0.2, 0.08, 0.12], [0.22, 0.1, 0.14], [0.28, 0.12, 0.18], [0.1, 0.04, 0.06], [1.0, 0.35, 0.5], [1.0, 0.82, 0.86], [0.32, 0.2, 0.24]),
];

fn zones_js() -> String {
    let c = |v: [f32; 3]| format!("[{:.3}, {:.3}, {:.3}]", v[0], v[1], v[2]);
    let items: Vec<String> = ZONES
        .iter()
        .map(|z| format!("{{ name: {:?}, env: {:?}, fog: {}, floor: {}, wall: {}, arch: {}, glow: {}, glass: {}, ambient: {} }}", z.name, z.env, c(z.fog), c(z.floor), c(z.wall), c(z.arch), c(z.glow), c(z.glass), c(z.ambient)))
        .collect();
    format!("[{}]", items.join(",\n  "))
}

// -------------------------------------------------------------- build --

pub(crate) fn build_demo_shatter(document: &mut Document) -> ShatterDemo {
    let mut inverses = Vec::new();
    let inverse = document.apply(Command::AddArtboard { name: "Shatterline".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    let chrome = add_material(document, &mut inverses, "Shatterline Chrome", chrome_graph());
    let glass = add_material(document, &mut inverses, "Shatterline Glass", glass_graph());
    let crystal = add_material(document, &mut inverses, "Shatterline Crystal", crystal_graph());
    let tiles = add_material(document, &mut inverses, "Shatterline Tiles", tile_graph());
    let sky_material = add_material(document, &mut inverses, "Shatterline Sky", sky_graph());

    for (name, samples) in [
        ("shatter", shatter_sound(0.9, 46, 0.0, 11)),
        ("shatterBig", shatter_sound(1.6, 90, 0.35, 23)),
        ("tink", shatter_sound(0.35, 10, 0.0, 5)),
        ("crash", shatter_sound(1.5, 70, 1.0, 41)),
        ("crystal", crystal_sound()),
        ("throw", throw_sound()),
        ("door", door_sound()),
        ("checkpoint", checkpoint_sound()),
        ("drone", drone_sound()),
    ] {
        document.add_audio(AudioAsset { name: name.into(), source: wav(&samples), file: None, deleted: false });
    }

    // The 2D camera carries the HTML HUD; the view is the Camera 3D.
    let hud_camera = with_id(
        Node::with_components("HUD Camera", None, vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.86, 0.92, 0.97, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })]),
        "hudCamera",
    );
    let hud_id = add_node(document, &mut inverses, scene, hud_camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(hud_id) }));
    add_node(document, &mut inverses, scene, html_node("hud", Some(hud_id), HUD_HTML, HUD_CSS, false));
    add_node(document, &mut inverses, scene, html_node("menu", Some(hud_id), MENU_HTML, MENU_CSS, true));

    let z0 = &ZONES[0];
    let rgba = |c: [f32; 3], a: f32| [c[0], c[1], c[2], a];
    let cam = Camera3D { fov_degrees: FOV, near: 0.05, far: 130.0, current: true, clear_color: Some(rgba(z0.fog, 1.0)) };
    add_node(document, &mut inverses, scene, node3d("cam", None, at(Vec3::new(0.0, EYE, 0.0)), Node3DContent::Camera(cam)));
    add_node(
        document,
        &mut inverses,
        scene,
        node3d(
            "sun",
            None,
            Transform3D { position: Vec3::new(0.0, 20.0, 0.0), rotation_degrees: Vec3::new(-58.0, 20.0, 0.0), scale: Vec3::ONE },
            Node3DContent::Light(Light3D { kind: Light3DKind::Directional, color: [1.0, 0.98, 0.94], intensity: 0.42, range: 10.0 }),
        ),
    );
    for i in 0..LAMPS {
        add_node(document, &mut inverses, scene, node3d(&format!("lamp{i}"), None, at(PARKED), Node3DContent::Light(Light3D { kind: Light3DKind::Point, color: z0.glow, intensity: 0.45, range: 12.0 })));
    }

    // The corridor shell (follows the camera; its tiles are world-locked).
    let shell_len = 240.0;
    for (name, position, size, color) in [
        ("floor", Vec3::new(0.0, -0.1, 0.0), Vec3::new(HALF_WIDTH * 2.0 + 0.2, 0.2, shell_len), z0.floor),
        ("ceil", Vec3::new(0.0, HEIGHT + 0.1, 0.0), Vec3::new(HALF_WIDTH * 2.0 + 0.2, 0.2, shell_len), z0.wall),
        ("wallL", Vec3::new(-HALF_WIDTH - 0.1, HEIGHT * 0.5, 0.0), Vec3::new(0.2, HEIGHT + 0.4, shell_len), z0.wall),
        ("wallR", Vec3::new(HALF_WIDTH + 0.1, HEIGHT * 0.5, 0.0), Vec3::new(0.2, HEIGHT + 0.4, shell_len), z0.wall),
    ] {
        add_node(document, &mut inverses, scene, node3d(name, None, scaled(position, Vec3::ONE), mesh(Primitive3D::Cube { size }, rgba(color, 1.0), if name == "floor" { 0.3 } else { 0.1 }, false, Some(tiles))));
    }

    // Arches: two pillars, a lintel, and a glowing strip under it.
    for i in 0..ARCHES {
        let root = add_node(document, &mut inverses, scene, node3d(&format!("arch{i}"), None, at(Vec3::new(0.0, 0.0, -(i as f32) * ARCH_GAP)), Node3DContent::Empty));
        for (part, position, size) in [
            ("L", Vec3::new(-HALF_WIDTH + 0.45, HEIGHT * 0.5, 0.0), Vec3::new(0.9, HEIGHT, 0.9)),
            ("R", Vec3::new(HALF_WIDTH - 0.45, HEIGHT * 0.5, 0.0), Vec3::new(0.9, HEIGHT, 0.9)),
            ("T", Vec3::new(0.0, HEIGHT - 0.35, 0.0), Vec3::new(HALF_WIDTH * 2.0, 0.7, 0.9)),
        ] {
            add_node(document, &mut inverses, scene, node3d(&format!("arch{i}{part}"), Some(root), at(position), mesh(Primitive3D::Cube { size }, rgba(z0.arch, 1.0), 0.3, false, None)));
        }
        add_node(document, &mut inverses, scene, node3d(&format!("arch{i}G"), Some(root), at(Vec3::new(0.0, HEIGHT - 0.72, 0.0)), mesh(Primitive3D::Cube { size: Vec3::new(HALF_WIDTH * 2.0 - 1.8, 0.06, 0.5) }, rgba(z0.glow, 1.0), 0.0, true, None)));
        for (side, x) in [("a", -HALF_WIDTH + 0.92), ("b", HALF_WIDTH - 0.92)] {
            add_node(document, &mut inverses, scene, node3d(&format!("arch{i}{side}"), Some(root), at(Vec3::new(x, HEIGHT * 0.45, 0.0)), mesh(Primitive3D::Cube { size: Vec3::new(0.06, HEIGHT * 0.8, 0.4) }, rgba(z0.glow, 1.0), 0.0, true, None)));
        }
    }

    // Wall decoration blocks.
    for i in 0..DECOS {
        add_node(document, &mut inverses, scene, node3d(&format!("deco{i}"), None, at(PARKED), mesh(Primitive3D::cube(), rgba(z0.arch, 1.0), 0.25, false, None)));
    }

    let glass_color = rgba(z0.glass, 0.5);
    let glass_mesh = |w: f32, h: f32, thickness: f32, shard: f32, seed: u32, anchors: u32| mesh(Primitive3D::Glass { width: w, height: h, thickness, shard, seed, anchors }, glass_color, 0.9, false, Some(glass));
    let frame = [0.16, 0.17, 0.2, 1.0];
    // A framed pane: an empty root (the script places / scales it), the
    // glass, and four thin metal frame bars round it.
    let framed = |document: &mut Document, inverses: &mut Vec<Command>, name: String, size: (f32, f32), shard: f32, seed: u32| {
        let root = add_node(document, inverses, scene, hidden(node3d(&format!("{name}r"), None, at(PARKED), Node3DContent::Empty)));
        add_node(document, inverses, scene, node3d(&name, Some(root), Transform3D::default(), glass_mesh(size.0, size.1, 0.035, shard, seed, SIDE_ALL)));
        let (hw, hh, t) = (size.0 * 0.5, size.1 * 0.5, 0.09);
        for (k, (position, bar)) in [
            (Vec3::new(0.0, hh + t * 0.5, 0.0), Vec3::new(size.0 + t * 2.0, t, t * 1.4)),
            (Vec3::new(0.0, -hh - t * 0.5, 0.0), Vec3::new(size.0 + t * 2.0, t, t * 1.4)),
            (Vec3::new(-hw - t * 0.5, 0.0, 0.0), Vec3::new(t, size.1, t * 1.4)),
            (Vec3::new(hw + t * 0.5, 0.0, 0.0), Vec3::new(t, size.1, t * 1.4)),
        ]
        .into_iter()
        .enumerate()
        {
            add_node(document, inverses, scene, node3d(&format!("{name}f{k}"), Some(root), at(position), mesh(Primitive3D::Cube { size: bar }, frame, 0.85, false, None)));
        }
    };
    for i in 0..BIG_PANES {
        framed(document, &mut inverses, format!("pb{i}"), BIG, 0.3, 100 + i as u32);
    }
    for i in 0..SMALL_PANES {
        framed(document, &mut inverses, format!("ps{i}"), SMALL, 0.22, 200 + i as u32);
    }
    for i in 0..SPINNERS {
        framed(document, &mut inverses, format!("sp{i}"), SPIN, 0.26, 300 + i as u32);
    }
    // Bars: floor-to-ceiling glass rods, held top and bottom.
    for i in 0..BARS {
        add_node(document, &mut inverses, scene, hidden(node3d(&format!("bar{i}"), None, at(PARKED), glass_mesh(BAR.0, BAR.1, 0.2, 0.42, 400 + i as u32, SIDE_TOP | SIDE_BOTTOM))));
    }
    // Fans: a hub turning BLADES glass blades, each held at its hub end.
    for f in 0..FANS {
        let pivot = add_node(document, &mut inverses, scene, hidden(node3d(&format!("fan{f}"), None, at(PARKED), Node3DContent::Empty)));
        add_node(document, &mut inverses, scene, node3d(&format!("fan{f}hub"), Some(pivot), Transform3D { position: Vec3::ZERO, rotation_degrees: Vec3::new(90.0, 0.0, 0.0), scale: Vec3::ONE }, mesh(Primitive3D::Cylinder { radius: 0.38, height: 0.3, segments: 24 }, frame, 0.9, false, None)));
        for b in 0..BLADES {
            let arm = add_node(
                document,
                &mut inverses,
                scene,
                node3d(&format!("fan{f}a{b}"), Some(pivot), Transform3D { position: Vec3::ZERO, rotation_degrees: Vec3::new(0.0, 0.0, b as f32 * 360.0 / BLADES as f32), scale: Vec3::ONE }, Node3DContent::Empty),
            );
            add_node(document, &mut inverses, scene, node3d(&format!("fan{f}b{b}"), Some(arm), at(Vec3::new(0.3 + BLADE.0 * 0.5, 0.0, 0.0)), glass_mesh(BLADE.0, BLADE.1, 0.05, 0.26, 500 + (f * BLADES + b) as u32, SIDE_LEFT)));
        }
    }
    // Hangers: a pane hanging from a ceiling pivot on a rod (swings).
    for i in 0..HANGERS {
        let pivot = add_node(document, &mut inverses, scene, hidden(node3d(&format!("hang{i}"), None, at(PARKED), Node3DContent::Empty)));
        add_node(document, &mut inverses, scene, node3d(&format!("hang{i}rod"), Some(pivot), at(Vec3::new(0.0, -0.35, 0.0)), mesh(Primitive3D::Cube { size: Vec3::new(0.08, 0.7, 0.08) }, frame, 0.9, false, None)));
        add_node(document, &mut inverses, scene, node3d(&format!("hang{i}g"), Some(pivot), at(Vec3::new(0.0, -0.7 - HANG.1 * 0.5, 0.0)), glass_mesh(HANG.0, HANG.1, 0.04, 0.3, 600 + i as u32, SIDE_TOP)));
    }
    // Doors: two metal leaves meeting in the middle, a glowing seam, and a
    // diamond glass target that opens them.
    for i in 0..DOORS {
        let root = add_node(document, &mut inverses, scene, hidden(node3d(&format!("door{i}"), None, at(PARKED), Node3DContent::Empty)));
        for (side, x) in [("L", -HALF_WIDTH * 0.5), ("R", HALF_WIDTH * 0.5)] {
            let leaf = add_node(document, &mut inverses, scene, node3d(&format!("door{i}{side}"), Some(root), at(Vec3::new(x, HEIGHT * 0.5, 0.0)), mesh(Primitive3D::Cube { size: Vec3::new(HALF_WIDTH, HEIGHT, 0.3) }, [0.22, 0.24, 0.28, 1.0], 0.75, false, None)));
            add_node(document, &mut inverses, scene, node3d(&format!("door{i}{side}s"), Some(leaf), at(Vec3::new(-x.signum() * (HALF_WIDTH * 0.5 - 0.05), 0.0, 0.17)), mesh(Primitive3D::Cube { size: Vec3::new(0.06, HEIGHT, 0.04) }, rgba(z0.glow, 1.0), 0.0, true, None)));
        }
        add_node(
            document,
            &mut inverses,
            scene,
            node3d(&format!("door{i}t"), Some(root), Transform3D { position: Vec3::new(0.0, 3.2, 0.32), rotation_degrees: Vec3::new(0.0, 0.0, 45.0), scale: Vec3::ONE }, glass_mesh(TARGET, TARGET, 0.06, 0.16, 700 + i as u32, 0)),
        );
    }

    // Crystals: blue octahedra (a 4 × 2 sphere) worth +3 balls.
    for i in 0..CRYSTALS {
        add_node(document, &mut inverses, scene, hidden(node3d(&format!("cry{i}"), None, scaled(PARKED, Vec3::new(0.42, 0.62, 0.42)), mesh(Primitive3D::Sphere { radius: 0.5, segments: 4, rings: 2 }, [0.15, 0.55, 1.0, 1.0], 0.0, true, Some(crystal)))));
    }
    // Balls: chrome spheres, 0.13 m.
    for i in 0..BALLS {
        add_node(document, &mut inverses, scene, hidden(node3d(&format!("ball{i}"), None, scaled(PARKED, Vec3::splat(0.26)), mesh(Primitive3D::Sphere { radius: 0.5, segments: 32, rings: 16 }, rgba(z0.floor, 1.0), 0.0, true, Some(chrome)))));
    }

    // The sky dome: a sphere turned inside out (negative X scale flips its
    // winding, so its inside faces are its front faces), riding on the
    // camera; shown in open and canyon zones.
    add_node(document, &mut inverses, scene, hidden(node3d("sky", None, scaled(PARKED, Vec3::new(-150.0, 150.0, 150.0)), mesh(Primitive3D::Sphere { radius: 0.5, segments: 32, rings: 16 }, rgba(z0.fog, 1.0), 0.0, true, Some(sky_material)))));
    // Pylons: tall slabs rising from the void beside open platforms.
    for i in 0..PYLONS {
        add_node(document, &mut inverses, scene, hidden(node3d(&format!("pyl{i}"), None, at(PARKED), mesh(Primitive3D::cube(), rgba(z0.arch, 1.0), 0.3, false, Some(tiles)))));
    }
    // The balls a crash costs, thrown out in front of you.
    for i in 0..LOST {
        add_node(document, &mut inverses, scene, hidden(node3d(&format!("lb{i}"), None, scaled(PARKED, Vec3::splat(0.26)), mesh(Primitive3D::Sphere { radius: 0.5, segments: 24, rings: 12 }, rgba(z0.floor, 1.0), 0.0, true, Some(chrome)))));
    }
    // DNA strands: long double helixes slung across the space like
    // bridges (the script tilts each one and turns it slowly round its own
    // axis). Along local X: two dark metal backbones spiralling round the
    // axis, joined every RUNG_GAP by a glass rung (held at both backbones —
    // smash its middle and the stubs stay). Where a strand crosses your
    // line, its rungs are in the way.
    let backbone = [0.11, 0.12, 0.14, 1.0];
    let span = (RUNGS - 1) as f32 * RUNG_GAP;
    let strand_point = |k: f32, side: f32| {
        let a = (k * RUNG_TWIST).to_radians();
        Vec3::new(-span * 0.5 + k * RUNG_GAP, side * STRAND_R * a.cos(), side * STRAND_R * a.sin())
    };
    for h in 0..HELIXES {
        let root = add_node(document, &mut inverses, scene, hidden(node3d(&format!("hx{h}"), None, at(PARKED), Node3DContent::Empty)));
        let spin = add_node(document, &mut inverses, scene, node3d(&format!("hx{h}s"), Some(root), Transform3D::default(), Node3DContent::Empty));
        for k in 0..RUNGS {
            let arm = add_node(
                document,
                &mut inverses,
                scene,
                node3d(&format!("hx{h}a{k}"), Some(spin), Transform3D { position: Vec3::new(-span * 0.5 + k as f32 * RUNG_GAP, 0.0, 0.0), rotation_degrees: Vec3::new(k as f32 * RUNG_TWIST, 0.0, 0.0), scale: Vec3::ONE }, Node3DContent::Empty),
            );
            add_node(document, &mut inverses, scene, node3d(&format!("hx{h}r{k}"), Some(arm), Transform3D::default(), glass_mesh(0.24, STRAND_R * 2.0 - 0.16, 0.14, 0.28, 800 + (h * RUNGS + k) as u32, SIDE_TOP | SIDE_BOTTOM)));
            // Backbone segments to the next rung (straight tubes between
            // the two strands' points — computed here once; the whole
            // strand then turns as one).
            if k + 1 < RUNGS {
                for (side_name, side) in [("t", 1.0f32), ("b", -1.0)] {
                    let from = strand_point(k as f32, side);
                    let to = strand_point(k as f32 + 1.0, side);
                    let dir = to - from;
                    let mut t = Transform3D { position: (from + to) * 0.5, rotation_degrees: Vec3::ZERO, scale: Vec3::new(1.0, dir.length() + 0.06, 1.0) };
                    t.set_rotation(glam::Quat::from_rotation_arc(Vec3::Y, dir.normalize()));
                    add_node(document, &mut inverses, scene, node3d(&format!("hx{h}b{k}{side_name}"), Some(spin), t, mesh(Primitive3D::Cylinder { radius: 0.12, height: 1.0, segments: 12 }, backbone, 0.85, false, None)));
                }
            }
        }
    }
    // Hammers: a beam across the track, a pivot swinging a rod and a heavy
    // metal head through the path. Metal: balls bounce off, you dodge.
    for i in 0..HAMMERS {
        let root = add_node(document, &mut inverses, scene, hidden(node3d(&format!("hm{i}"), None, at(PARKED), Node3DContent::Empty)));
        add_node(document, &mut inverses, scene, node3d(&format!("hm{i}b"), Some(root), at(Vec3::ZERO), mesh(Primitive3D::Cube { size: Vec3::new(HALF_WIDTH * 2.0 + 0.6, 0.4, 0.4) }, frame, 0.85, false, None)));
        let swing = add_node(document, &mut inverses, scene, node3d(&format!("hm{i}s"), Some(root), Transform3D::default(), Node3DContent::Empty));
        add_node(document, &mut inverses, scene, node3d(&format!("hm{i}r"), Some(swing), at(Vec3::new(0.0, -HAMMER_REACH * 0.5, 0.0)), mesh(Primitive3D::Cube { size: Vec3::new(0.14, HAMMER_REACH, 0.14) }, frame, 0.85, false, None)));
        add_node(document, &mut inverses, scene, node3d(&format!("hm{i}h"), Some(swing), at(Vec3::new(0.0, -HAMMER_REACH, 0.0)), mesh(Primitive3D::Cube { size: Vec3::new(1.6, 0.9, 0.9) }, [0.3, 0.32, 0.36, 1.0], 0.9, false, None)));
        add_node(document, &mut inverses, scene, node3d(&format!("hm{i}g"), Some(swing), at(Vec3::new(0.0, -HAMMER_REACH, 0.47)), mesh(Primitive3D::Cube { size: Vec3::new(1.4, 0.12, 0.04) }, rgba(z0.glow, 1.0), 0.0, true, None)));
    }
    // Thick glass: framed, heavier frame, deeper tint — it cracks on the
    // first hits and only breaks on the last.
    for i in 0..THICK {
        let root = add_node(document, &mut inverses, scene, hidden(node3d(&format!("pt{i}r"), None, at(PARKED), Node3DContent::Empty)));
        add_node(document, &mut inverses, scene, node3d(&format!("pt{i}"), Some(root), Transform3D::default(), mesh(Primitive3D::Glass { width: THICK_PANE.0, height: THICK_PANE.1, thickness: 0.16, shard: 0.36, seed: 900 + i as u32, anchors: SIDE_ALL }, [0.45, 0.8, 0.85, 0.72], 0.95, false, Some(glass))));
        let (hw, hh, t) = (THICK_PANE.0 * 0.5, THICK_PANE.1 * 0.5, 0.16);
        for (k, (position, bar)) in [
            (Vec3::new(0.0, hh + t * 0.5, 0.0), Vec3::new(THICK_PANE.0 + t * 2.0, t, t * 1.6)),
            (Vec3::new(0.0, -hh - t * 0.5, 0.0), Vec3::new(THICK_PANE.0 + t * 2.0, t, t * 1.6)),
            (Vec3::new(-hw - t * 0.5, 0.0, 0.0), Vec3::new(t, THICK_PANE.1, t * 1.6)),
            (Vec3::new(hw + t * 0.5, 0.0, 0.0), Vec3::new(t, THICK_PANE.1, t * 1.6)),
        ]
        .into_iter()
        .enumerate()
        {
            add_node(document, &mut inverses, scene, node3d(&format!("pt{i}f{k}"), Some(root), at(position), mesh(Primitive3D::Cube { size: bar }, [0.1, 0.11, 0.13, 1.0], 0.9, false, None)));
        }
    }
    // Power-ups: big orange crystals — rapid fire.
    for i in 0..POWERS {
        add_node(document, &mut inverses, scene, hidden(node3d(&format!("pw{i}"), None, scaled(PARKED, Vec3::new(0.6, 0.9, 0.6)), mesh(Primitive3D::Sphere { radius: 0.5, segments: 4, rings: 2 }, [1.0, 0.5, 0.08, 1.0], 0.0, true, Some(crystal)))));
    }

    // Post: bloom on the glowing strips / crystals / glints, a little
    // extra contrast and colour, and a soft vignette.
    inverses.push(document.apply(Command::SetPostEffects {
        artboard: scene,
        value: vec![
            PostEffectLayer { effect: PostEffect::Glow { radius: 3.0, threshold: 0.9, intensity: 0.38 }, enabled: true },
            PostEffectLayer { effect: PostEffect::ColorGrading { brightness: 0.0, contrast: 1.08, saturation: 1.12 }, enabled: true },
            PostEffectLayer { effect: PostEffect::Vignette { color: [0.0, 0.02, 0.06, 1.0], intensity: 0.32, radius: 0.78, blur: 0.5 }, enabled: true },
        ],
    }));

    let prefix = format!(
        "const W = {SCREEN_W}, H = {SCREEN_H}, HALF_WIDTH = {HALF_WIDTH}, HEIGHT = {HEIGHT}, EYE = {EYE}, FOV = {FOV}\n\
         const ARCHES = {ARCHES}, ARCH_GAP = {ARCH_GAP}, BALLS = {BALLS}, CRYSTALS = {CRYSTALS}, BIG_PANES = {BIG_PANES}, SMALL_PANES = {SMALL_PANES}, BARS = {BARS}\n\
         const FANS = {FANS}, BLADES = {BLADES}, HANGERS = {HANGERS}, SPINNERS = {SPINNERS}, DOORS = {DOORS}, DECOS = {DECOS}, LAMPS = {LAMPS}\n\
         const BIG = [{}, {}], SMALL = [{}, {}], BLADE = [{}, {}], HANG = [{}, {}], SPIN = [{}, {}]\n\
         const SHELL_LEN = {shell_len}\n\
         const RUNG_GAP = {RUNG_GAP}\n\
         const PYLONS = {PYLONS}, LOST = {LOST}, HELIXES = {HELIXES}, RUNGS = {RUNGS}, HAMMERS = {HAMMERS}, THICK = {THICK}, POWERS = {POWERS}, HAMMER_REACH = {HAMMER_REACH}\n\
         const ZONES = {}",
        BIG.0,
        BIG.1,
        SMALL.0,
        SMALL.1,
        BLADE.0,
        BLADE.1,
        HANG.0,
        HANG.1,
        SPIN.0,
        SPIN.1,
        zones_js()
    );
    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Shatterline (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Shatterline".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Play".into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(format!("{prefix}\n{SCRIPT}"))),
            }],
            initial_state: "Play".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));
    inverses.reverse();
    document.commit_batch(inverses);
    ShatterDemo { scene }
}

const SCRIPT: &str = include_str!("demo_shatter.js");

/// The in-game HUD (1600×900 CSS px, scaled to the screen).
const HUD_HTML: &str = r#"<div class="hud" id="root">
  <div class="flash" id="flash"></div>
  <div class="top">
    <div class="count"><i class="ball"></i><span id="balls">40</span></div>
    <div class="streak" id="streak"></div>
    <div class="multi" id="multi"></div>
  </div>
  <div class="zone"><div id="zoneName">DAWN</div><div class="dist" id="dist">0 M</div></div>
  <div class="pops" id="pops"></div>
  <div class="banner" id="banner"><div class="small" id="bannerSmall">CHECKPOINT</div><div class="big" id="bannerBig">LAGOON</div></div>
  <div class="hint" id="hint"></div>
  <div class="power" id="power"><span>RAPID FIRE</span><i id="powerBar"></i></div>
</div>"#;

const HUD_CSS: &str = r#"
.hud { position: absolute; inset: 0; font-family: 'Trebuchet MS', 'Segoe UI', sans-serif; color: #fff; pointer-events: none; overflow: hidden; }
.top { position: absolute; left: 50%; top: 96px; transform: translateX(-50%); display: flex; flex-direction: column; align-items: center; gap: 8px; }
.count { display: flex; align-items: center; gap: 14px; font-size: 64px; font-weight: 700; letter-spacing: 1px; text-shadow: 0 3px 14px rgb(0 20 40 / 0.35); font-variant-numeric: tabular-nums; }
.count.low { color: #ffd7d0; animation: pulse 0.6s infinite; }
@keyframes pulse { 50% { transform: scale(1.08); } }
.count.bump { animation: bump 0.25s; }
@keyframes bump { 40% { transform: scale(1.25); } }
.ball { width: 44px; height: 44px; border-radius: 50%; background: radial-gradient(circle at 34% 30%, #fff 0 9%, #dfe8f2 16%, #8e9aa8 48%, #39414c 70%, #c9d3dd 92%); box-shadow: 0 2px 10px rgb(0 0 0 / 0.35); }
.streak { display: flex; gap: 6px; }
.streak i { width: 16px; height: 16px; border-radius: 3px; transform: rotate(45deg); background: rgb(255 255 255 / 0.28); }
.streak i.on { background: #6fd3ff; box-shadow: 0 0 10px #6fd3ff; }
.multi { font-size: 30px; font-weight: 700; color: #bff0ff; text-shadow: 0 0 12px #3fb8ff; min-height: 36px; }
.zone { position: absolute; left: 40px; top: 96px; font-size: 26px; letter-spacing: 4px; font-weight: 700; text-shadow: 0 2px 10px rgb(0 0 0 / 0.3); }
.dist { font-size: 20px; opacity: 0.8; letter-spacing: 2px; margin-top: 4px; font-variant-numeric: tabular-nums; }
.flash { position: absolute; inset: 0; background: radial-gradient(transparent 30%, rgb(255 40 40 / 0.6)); opacity: 0; transition: opacity 0.5s; }
.flash.on { opacity: 1; transition: none; }
.pops { position: absolute; inset: 0; }
.pop { position: absolute; font-size: 40px; font-weight: 800; transform: translate(-50%, -50%); animation: rise 0.9s ease-out forwards; text-shadow: 0 0 12px rgb(80 190 255 / 0.9); }
.pop.bad { color: #ff5a4e; font-size: 88px; text-shadow: 0 0 18px rgb(255 0 0 / 0.6); }
.pop.multi { color: #bff0ff; font-size: 56px; }
@keyframes rise { from { opacity: 1; margin-top: 0; } to { opacity: 0; margin-top: -90px; } }
.banner { position: absolute; left: 0; right: 0; top: 34%; text-align: center; opacity: 0; transition: opacity 0.6s; text-shadow: 0 3px 18px rgb(0 0 0 / 0.35); }
.banner.on { opacity: 1; }
.banner .small { font-size: 26px; letter-spacing: 10px; }
.banner .big { font-size: 96px; font-weight: 800; letter-spacing: 14px; }
.power { position: absolute; left: 50%; top: 250px; transform: translateX(-50%); display: none; flex-direction: column; align-items: center; gap: 6px; font-size: 22px; font-weight: 800; letter-spacing: 6px; color: #ffcf8a; text-shadow: 0 0 14px rgb(255 140 30 / 0.9); }
.power.on { display: flex; }
.power i { display: block; height: 6px; width: 260px; border-radius: 3px; background: linear-gradient(90deg, #ff8a1e, #ffd27a); box-shadow: 0 0 12px rgb(255 140 30 / 0.8); transform-origin: left; }
.hint { position: absolute; left: 50%; bottom: 60px; transform: translateX(-50%); font-size: 22px; letter-spacing: 3px; opacity: 0.85; text-shadow: 0 2px 8px rgb(0 0 0 / 0.4); }
"#;

/// The start / game-over menu (interactive: zone buttons).
const MENU_HTML: &str = r#"<div class="menu" id="root">
  <div class="card">
    <div class="title">SHATTERLINE</div>
    <div class="sub" id="sub">Throw chrome. Break glass. Don't hit what's still standing.</div>
    <div class="stats" id="stats"></div>
    <div class="zones" id="zones"></div>
    <div class="help">CLICK / TAP to throw · each throw costs a ball · hit crystals for +3 · 10 crystals in a row = multiball · glass you crash into costs 10</div>
  </div>
</div>"#;

const MENU_CSS: &str = r#"
.menu { position: absolute; inset: 0; display: grid; place-items: center; font-family: 'Trebuchet MS', 'Segoe UI', sans-serif; color: #fff; background: radial-gradient(rgb(10 30 50 / 0.25), rgb(5 15 30 / 0.6)); }
.card { width: 1180px; padding: 46px 56px; border-radius: 6px; background: rgb(255 255 255 / 0.1); backdrop-filter: blur(10px); border: 1px solid rgb(255 255 255 / 0.35); box-shadow: 0 30px 80px rgb(0 20 40 / 0.35); text-align: center; }
.title { font-size: 104px; font-weight: 800; letter-spacing: 18px; margin-left: 18px; background: linear-gradient(180deg, #fff 0 45%, #a9c4d8 52%, #ffffff 100%); -webkit-background-clip: text; background-clip: text; color: transparent; filter: drop-shadow(0 4px 12px rgb(0 30 60 / 0.4)); }
.sub { font-size: 24px; opacity: 0.9; margin-top: 4px; letter-spacing: 1px; }
.stats { font-size: 22px; margin-top: 18px; min-height: 30px; letter-spacing: 2px; color: #d9f2ff; }
.zones { margin-top: 26px; display: grid; grid-template-columns: repeat(8, 1fr); gap: 10px; }
.z { pointer-events: auto; cursor: pointer; padding: 12px 4px 10px; border-radius: 4px; border: 2px solid rgb(255 255 255 / 0.5); background: rgb(255 255 255 / 0.08); color: #fff; font: inherit; font-size: 16px; font-weight: 700; letter-spacing: 1px; transition: transform 0.12s, background 0.12s; }
.z small { display: block; font-size: 14px; opacity: 0.75; font-weight: 400; margin-top: 4px; letter-spacing: 1px; }
.z:hover { transform: translateY(-3px); background: rgb(255 255 255 / 0.22); }
.z:focus-visible { outline: 3px solid #6fd3ff; }
.z.locked { opacity: 0.35; cursor: default; pointer-events: none; }
.z i { display: block; height: 6px; border-radius: 3px; margin-bottom: 10px; }
.help { margin-top: 28px; font-size: 17px; opacity: 0.75; line-height: 1.6; }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_pools_materials_and_sounds() {
        let mut document = Document::new();
        let demo = build_demo_shatter(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let glass_count = artboard
            .node_ids()
            .filter_map(|id| artboard.node(id))
            .filter(|n| matches!(&n.kind, NodeKind::Node3D(an1mbyt3_core::Node3D { content: Node3DContent::Mesh(Mesh3D { primitive: Primitive3D::Glass { .. }, .. }), .. })))
            .count();
        assert_eq!(glass_count, BIG_PANES + SMALL_PANES + SPINNERS + BARS + FANS * BLADES + HANGERS + DOORS + HELIXES * RUNGS + THICK);
        assert_eq!(ZONES.len(), 16);
        assert!(document.audio_assets().count() >= 9);
    }

    #[test]
    fn wav_header_is_valid() {
        let bytes = wav(&[0.0, 0.5, -0.5]);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(bytes.len(), 44 + 6);
    }
}
