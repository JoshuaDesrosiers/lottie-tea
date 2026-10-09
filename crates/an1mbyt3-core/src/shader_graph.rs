//! Shader Graph — a node-based material, compiled to WGSL.
//!
//! The Godot VisualShader / Unity Shader Graph counterpart: a
//! [`ShaderGraphDef`] is a small typed dataflow graph (UV → Panner →
//! Sample Texture → Multiply → Output, say) that [`compile_wgsl`] turns into
//! one WGSL function,
//!
//! ```wgsl
//! fn material_fragment(in: VertexOutput) -> vec4<f32> { ... }
//! ```
//!
//! which the renderer links against its own sprite shader (`an1mbyt3-renderer`
//! appends the result to `shader.wgsl` plus a small "material glue" block)
//! and runs as the fragment stage for every sprite / shape wearing the
//! material (a `SpriteRenderer` component whose `material` is a
//! [`crate::MaterialKind::ShaderGraph`]).
//!
//! **The contract with the renderer** — names the generated code uses and
//! the renderer's glue must define:
//! - `VertexOutput` with `local_position: vec2<f32>` (shape-local position).
//! - `material_uv(in)`: the sprite's 0..1 UV (top-left origin, like every
//!   image editor), the same UV the plain textured shader samples with.
//! - `material_globals.time.x`: seconds since the editor started.
//! - `uniforms.color`: the node's own tint / fill colour.
//! - `image_texture` / `image_sampler`: the node's image (a 1×1 white
//!   texture for a shape without one).
//!
//! **Types.** Every port is `Float`, `Vec2`, `Vec3` or `Vec4`; math nodes
//! are *dynamic* — they take the widest type among their wired inputs (so
//! `Multiply(texture colour, 0.5)` is a `Vec4`), and anything else is
//! converted the usual shader way: a float splats to every component (but
//! into a `Vec4` it becomes an opaque grey — alpha 1), a wider vector
//! truncates, a narrower one pads with 0 (and alpha 1).
//!
//! **3D.** [`ShaderDomain`] is the hook: a later `Spatial` domain adds
//! normal / roughness / emission outputs and 3D inputs, compiled by this
//! same code path against a 3D vertex stage.

use std::collections::HashMap;
use std::fmt::Write as _;

use glam::Vec2;
use serde::{Deserialize, Serialize};

/// What a shader graph shades. Only 2D canvas items so far (sprites and
/// shapes); appended-last-only, like every other `.teab`-encoded enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ShaderDomain {
    #[default]
    Canvas2D,
}

/// A value type flowing along a wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ShaderType {
    Float,
    Vec2,
    Vec3,
    Vec4,
}

impl ShaderType {
    pub fn wgsl(self) -> &'static str {
        match self {
            ShaderType::Float => "f32",
            ShaderType::Vec2 => "vec2<f32>",
            ShaderType::Vec3 => "vec3<f32>",
            ShaderType::Vec4 => "vec4<f32>",
        }
    }

    pub fn components(self) -> usize {
        match self {
            ShaderType::Float => 1,
            ShaderType::Vec2 => 2,
            ShaderType::Vec3 => 3,
            ShaderType::Vec4 => 4,
        }
    }
}

/// A port's declared type: fixed, or *dynamic* (resolved per node from
/// whatever is wired in — see the module doc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortType {
    Fixed(ShaderType),
    Dynamic,
}

/// What an unwired input uses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PortDefault {
    /// A constant (the node's per-input override, or this).
    Value([f32; 4]),
    /// The sprite's UV — so "Sample Texture" with nothing wired samples the
    /// sprite normally.
    Uv,
}

/// One input or output socket.
#[derive(Debug, Clone, Copy)]
pub struct PortSpec {
    pub name: &'static str,
    pub ty: PortType,
    pub default: PortDefault,
}

const fn port(name: &'static str, ty: ShaderType, value: [f32; 4]) -> PortSpec {
    PortSpec { name, ty: PortType::Fixed(ty), default: PortDefault::Value(value) }
}
const fn dynamic(name: &'static str, value: f32) -> PortSpec {
    PortSpec { name, ty: PortType::Dynamic, default: PortDefault::Value([value, value, value, value]) }
}
const fn uv_port(name: &'static str) -> PortSpec {
    PortSpec { name, ty: PortType::Fixed(ShaderType::Vec2), default: PortDefault::Uv }
}
const fn out(name: &'static str, ty: ShaderType) -> PortSpec {
    PortSpec { name, ty: PortType::Fixed(ty), default: PortDefault::Value([0.0; 4]) }
}
const fn out_dynamic(name: &'static str) -> PortSpec {
    PortSpec { name, ty: PortType::Dynamic, default: PortDefault::Value([0.0; 4]) }
}

use ShaderType::{Float as F, Vec2 as V2, Vec4 as V4};

/// Every node type. Serialized by name (string tags), so adding new ones
/// never disturbs saved graphs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShaderNodeKind {
    // ---- output ----
    Output,
    // ---- inputs ----
    Uv,
    LocalPosition,
    Time,
    Tint,
    SampleTexture,
    FloatConstant,
    Vec2Constant,
    ColorConstant,
    // ---- math (dynamic) ----
    Add,
    Subtract,
    Multiply,
    Divide,
    Min,
    Max,
    Power,
    Mix,
    Step,
    Smoothstep,
    OneMinus,
    Negate,
    Abs,
    Fract,
    Floor,
    Saturate,
    Sin,
    Cos,
    Remap,
    Posterize,
    // ---- vector ----
    Length,
    Distance,
    Dot,
    Split,
    Combine,
    MakeVec2,
    // ---- UV ----
    TilingOffset,
    Panner,
    RotateUv,
    Twirl,
    // ---- procedural / colour ----
    Noise,
    Checker,
    Circle,
    HueShift,
    Grayscale,
    Fresnel2D,
    /// Running-bond bricks: per-brick colour variation and mortar lines.
    Brick,
    /// How edge-on the surface is to the camera: 0 where it faces you, 1
    /// at the silhouette (`pow(1 - |n·v|, Power)`). Rim glows, glass, and
    /// anything that should "look at" the viewer. On a flat sprite it fakes
    /// a sphere (centre 0, edge 1).
    ViewFresnel,
    /// Where the surface faces, as seen from the camera, as a UV: (0.5,
    /// 0.5) on the part pointing at you, the square's edge at the
    /// silhouette. On a sphere it's a disc that always faces the viewer —
    /// irises, pupils, targets that stare back. On a sprite: its UV.
    FacingUv,
}

impl ShaderNodeKind {
    /// Every kind, in palette order.
    pub const ALL: &'static [ShaderNodeKind] = &[
        Self::Output,
        Self::Uv,
        Self::LocalPosition,
        Self::Time,
        Self::Tint,
        Self::SampleTexture,
        Self::FloatConstant,
        Self::Vec2Constant,
        Self::ColorConstant,
        Self::Add,
        Self::Subtract,
        Self::Multiply,
        Self::Divide,
        Self::Min,
        Self::Max,
        Self::Power,
        Self::Mix,
        Self::Step,
        Self::Smoothstep,
        Self::OneMinus,
        Self::Negate,
        Self::Abs,
        Self::Fract,
        Self::Floor,
        Self::Saturate,
        Self::Sin,
        Self::Cos,
        Self::Remap,
        Self::Posterize,
        Self::Length,
        Self::Distance,
        Self::Dot,
        Self::Split,
        Self::Combine,
        Self::MakeVec2,
        Self::TilingOffset,
        Self::Panner,
        Self::RotateUv,
        Self::Twirl,
        Self::Noise,
        Self::Checker,
        Self::Circle,
        Self::HueShift,
        Self::Grayscale,
        Self::Fresnel2D,
        Self::ViewFresnel,
        Self::FacingUv,
        Self::Brick,
    ];

    /// Palette category (the editor groups by this).
    pub fn category(self) -> &'static str {
        use ShaderNodeKind::*;
        match self {
            Output => "Output",
            Uv | LocalPosition | Time | Tint | SampleTexture | FloatConstant | Vec2Constant | ColorConstant | FacingUv => "Input",
            Add | Subtract | Multiply | Divide | Min | Max | Power | Mix | Step | Smoothstep | OneMinus | Negate | Abs | Fract | Floor | Saturate | Sin | Cos | Remap
            | Posterize => "Math",
            Length | Distance | Dot | Split | Combine | MakeVec2 => "Vector",
            TilingOffset | Panner | RotateUv | Twirl => "UV",
            Noise | Checker | Circle | HueShift | Grayscale | Fresnel2D | ViewFresnel | Brick => "Effect",
        }
    }

    pub fn inputs(self) -> &'static [PortSpec] {
        use ShaderNodeKind::*;
        const ONE: [f32; 4] = [1.0; 4];
        const ZERO: [f32; 4] = [0.0; 4];
        match self {
            Output => const { &[port("Color", V4, ONE)] },
            Uv | LocalPosition | Time | Tint | FloatConstant | Vec2Constant | ColorConstant | FacingUv => &[],
            SampleTexture => const { &[uv_port("UV")] },
            Add | Subtract => const { &[dynamic("A", 0.0), dynamic("B", 0.0)] },
            Multiply | Divide | Power => const { &[dynamic("A", 1.0), dynamic("B", 1.0)] },
            Min | Max => const { &[dynamic("A", 0.0), dynamic("B", 1.0)] },
            Mix => const { &[dynamic("A", 0.0), dynamic("B", 1.0), port("T", F, [0.5; 4])] },
            Step => const { &[dynamic("Edge", 0.5), dynamic("X", 0.0)] },
            Smoothstep => const { &[dynamic("Edge 0", 0.0), dynamic("Edge 1", 1.0), dynamic("X", 0.5)] },
            OneMinus | Negate | Abs | Fract | Floor | Saturate | Sin | Cos => const { &[dynamic("In", 0.0)] },
            Remap => const { &[dynamic("In", 0.0), port("In Min", F, [-1.0; 4]), port("In Max", F, ONE), port("Out Min", F, ZERO), port("Out Max", F, ONE)] },
            Posterize => const { &[dynamic("In", 0.0), port("Steps", F, [4.0; 4])] },
            Length => const { &[dynamic("In", 0.0)] },
            Distance | Dot => const { &[dynamic("A", 0.0), dynamic("B", 0.0)] },
            Split => const { &[port("In", V4, ZERO)] },
            Combine => const { &[port("X", F, ZERO), port("Y", F, ZERO), port("Z", F, ZERO), port("W", F, ONE)] },
            MakeVec2 => const { &[port("X", F, ZERO), port("Y", F, ZERO)] },
            TilingOffset => const { &[uv_port("UV"), port("Tiling", V2, ONE), port("Offset", V2, ZERO)] },
            Panner => const { &[uv_port("UV"), port("Speed", V2, [0.1, 0.0, 0.0, 0.0])] },
            RotateUv => const { &[uv_port("UV"), port("Angle", F, ZERO), port("Center", V2, [0.5; 4])] },
            Twirl => const { &[uv_port("UV"), port("Strength", F, [4.0; 4]), port("Center", V2, [0.5; 4])] },
            Noise => const { &[uv_port("UV"), port("Scale", F, [8.0; 4])] },
            Checker => const { &[uv_port("UV"), port("Count", V2, [4.0; 4]), port("Color A", V4, [0.15, 0.15, 0.18, 1.0]), port("Color B", V4, ONE)] },
            Circle => const { &[uv_port("UV"), port("Radius", F, [0.4; 4]), port("Softness", F, [0.02; 4])] },
            HueShift => const { &[port("Color", V4, ONE), port("Shift", F, ZERO)] },
            Grayscale => const { &[port("Color", V4, ONE)] },
            Fresnel2D => const { &[uv_port("UV"), port("Power", F, [2.0; 4])] },
            ViewFresnel => const { &[port("Power", F, [1.0; 4])] },
            Brick => const {
                &[
                    uv_port("UV"),
                    port("Count", V2, [4.0, 8.0, 0.0, 0.0]),
                    port("Mortar", F, [0.06; 4]),
                    port("Brick", V4, [0.62, 0.3, 0.2, 1.0]),
                    port("Mortar Color", V4, [0.8, 0.76, 0.7, 1.0]),
                ]
            },
        }
    }

    pub fn outputs(self) -> &'static [PortSpec] {
        use ShaderNodeKind::*;
        match self {
            Output => &[],
            Uv | LocalPosition | Vec2Constant | MakeVec2 | TilingOffset | Panner | RotateUv | Twirl | FacingUv => const { &[out("UV", V2)] },
            Time => const { &[out("Time", F)] },
            Tint | ColorConstant => const { &[out("Color", V4)] },
            SampleTexture => const { &[out("RGBA", V4), out("A", F)] },
            FloatConstant => const { &[out("Value", F)] },
            Add | Subtract | Multiply | Divide | Min | Max | Power | Mix | Step | Smoothstep | OneMinus | Negate | Abs | Fract | Floor | Saturate | Sin | Cos | Remap
            | Posterize => const { &[out_dynamic("Out")] },
            Length | Distance | Dot => const { &[out("Out", F)] },
            Split => const { &[out("X", F), out("Y", F), out("Z", F), out("W", F)] },
            Combine => const { &[out("Out", V4)] },
            Noise | Circle | Fresnel2D | ViewFresnel => const { &[out("Out", F)] },
            Checker | HueShift | Grayscale => const { &[out("Color", V4)] },
            Brick => const { &[out("Color", V4), out("Mortar", F)] },
        }
    }

    /// Constant nodes keep their value in [`ShaderNodeDef::value`].
    pub fn has_value(self) -> bool {
        matches!(self, Self::FloatConstant | Self::Vec2Constant | Self::ColorConstant)
    }
}

/// One node in a graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderNodeDef {
    /// Stable within its graph (edges refer to it).
    pub id: u32,
    pub kind: ShaderNodeKind,
    /// Where it sits in the graph editor.
    pub position: Vec2,
    /// A constant node's value (Float uses `.x`, Vec2 `.xy`).
    #[serde(default)]
    pub value: [f32; 4],
    /// Per-input constants for unwired inputs, by input index; a missing
    /// or `None` entry uses the port's own default.
    #[serde(default)]
    pub defaults: Vec<Option<[f32; 4]>>,
}

/// A wire: output `from_port` of node `from` → input `to_port` of `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderEdgeDef {
    pub from: u32,
    pub from_port: u8,
    pub to: u32,
    pub to_port: u8,
}

/// A whole shader graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderGraphDef {
    #[serde(default)]
    pub domain: ShaderDomain,
    pub nodes: Vec<ShaderNodeDef>,
    pub edges: Vec<ShaderEdgeDef>,
}

impl ShaderGraphDef {
    /// The starter graph a new material gets: the sprite drawn as-is,
    /// tinted by its colour (Sample Texture × Tint → Output).
    pub fn starter() -> Self {
        let node = |id, kind, x: f32, y: f32| ShaderNodeDef { id, kind, position: Vec2::new(x, y), value: [0.0; 4], defaults: Vec::new() };
        Self {
            domain: ShaderDomain::Canvas2D,
            nodes: vec![node(1, ShaderNodeKind::SampleTexture, 40.0, 40.0), node(2, ShaderNodeKind::Tint, 40.0, 200.0), node(3, ShaderNodeKind::Multiply, 300.0, 100.0), node(4, ShaderNodeKind::Output, 540.0, 100.0)],
            edges: vec![
                ShaderEdgeDef { from: 1, from_port: 0, to: 3, to_port: 0 },
                ShaderEdgeDef { from: 2, from_port: 0, to: 3, to_port: 1 },
                ShaderEdgeDef { from: 3, from_port: 0, to: 4, to_port: 0 },
            ],
        }
    }
}

/// A compile failure, for the graph editor to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderGraphError {
    /// The node at fault, if one is.
    pub node: Option<u32>,
    pub message: String,
}

impl std::fmt::Display for ShaderGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.node {
            Some(id) => write!(f, "node {id}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

/// WGSL helpers the generated code may call (always emitted — tiny).
pub const SHADER_GRAPH_LIBRARY: &str = r#"
fn sg_hash2(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(123.34, 456.21));
    let r = q + dot(q, q + 45.32);
    return fract(r.x * r.y);
}
fn sg_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = sg_hash2(i);
    let b = sg_hash2(i + vec2<f32>(1.0, 0.0));
    let c = sg_hash2(i + vec2<f32>(0.0, 1.0));
    let d = sg_hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}
fn sg_rotate(uv: vec2<f32>, center: vec2<f32>, angle: f32) -> vec2<f32> {
    let s = sin(angle);
    let c = cos(angle);
    let p = uv - center;
    return vec2<f32>(p.x * c - p.y * s, p.x * s + p.y * c) + center;
}
fn sg_rgb_to_hsv(c: vec3<f32>) -> vec3<f32> {
    let k = vec4<f32>(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = select(vec4<f32>(c.gb, k.xy), vec4<f32>(c.bg, k.wz), c.g < c.b);
    let q = select(vec4<f32>(p.xyw, c.r), vec4<f32>(c.r, p.yzx), p.x < c.r);
    let d = q.x - min(q.w, q.y);
    return vec3<f32>(abs(q.z + (q.w - q.y) / (6.0 * d + 1e-10)), d / (q.x + 1e-10), q.x);
}
fn sg_hsv_to_rgb(c: vec3<f32>) -> vec3<f32> {
    let p = abs(fract(c.xxx + vec3<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0);
    return c.z * mix(vec3<f32>(1.0), clamp(p - 1.0, vec3<f32>(0.0), vec3<f32>(1.0)), c.y);
}
"#;

/// Converts `expr` (of type `from`) to type `to`.
fn coerce(expr: &str, from: ShaderType, to: ShaderType) -> String {
    use ShaderType::*;
    match (from, to) {
        _ if from == to => expr.to_string(),
        // A float into a colour is a grey, *opaque* colour — splatting it
        // into alpha too would turn `Time -> Output` half-transparent.
        (Float, Vec4) => format!("vec4<f32>(vec3<f32>({expr}), 1.0)"),
        (Float, _) => format!("{}({expr})", to.wgsl()),
        (_, Float) => format!("({expr}).x"),
        (Vec2, Vec3) => format!("vec3<f32>({expr}, 0.0)"),
        (Vec2, Vec4) => format!("vec4<f32>({expr}, 0.0, 1.0)"),
        (Vec3, Vec4) => format!("vec4<f32>({expr}, 1.0)"),
        (Vec3 | Vec4, Vec2) => format!("({expr}).xy"),
        (Vec4, Vec3) => format!("({expr}).xyz"),
        _ => unreachable!("every pair is covered above"),
    }
}

fn float_lit(v: f32) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    // Suffixed `f`: a concrete f32, not an abstract float — naga rejects a
    // builtin call whose arguments are *all* abstract literals outside a
    // constant expression (`smoothstep(0.0, 1.0, 0.5)` with nothing wired).
    let s = format!("{v:?}");
    if s.contains('.') || s.contains('e') { format!("{s}f") } else { format!("{s}.0f") }
}

/// A constant of type `ty`, from a 4-float value.
fn literal(v: [f32; 4], ty: ShaderType) -> String {
    match ty {
        ShaderType::Float => float_lit(v[0]),
        _ => format!("{}({})", ty.wgsl(), v[..ty.components()].iter().map(|x| float_lit(*x)).collect::<Vec<_>>().join(", ")),
    }
}

/// Compiles `graph` to WGSL: [`SHADER_GRAPH_LIBRARY`] plus
/// `fn material_fragment(in: VertexOutput) -> vec4<f32>`. Only nodes the
/// Output depends on are emitted.
pub fn compile_wgsl(graph: &ShaderGraphDef) -> Result<String, ShaderGraphError> {
    let by_id: HashMap<u32, &ShaderNodeDef> = graph.nodes.iter().map(|n| (n.id, n)).collect();
    if by_id.len() != graph.nodes.len() {
        return Err(ShaderGraphError { node: None, message: "two nodes share an id".into() });
    }
    let outputs: Vec<&ShaderNodeDef> = graph.nodes.iter().filter(|n| n.kind == ShaderNodeKind::Output).collect();
    let output = match outputs.as_slice() {
        [one] => *one,
        [] => return Err(ShaderGraphError { node: None, message: "add an Output node".into() }),
        _ => return Err(ShaderGraphError { node: Some(outputs[1].id), message: "only one Output node is allowed".into() }),
    };
    // input (node, port) -> (from node, from port)
    let mut wire: HashMap<(u32, u8), (u32, u8)> = HashMap::new();
    for e in &graph.edges {
        let (Some(from), Some(to)) = (by_id.get(&e.from), by_id.get(&e.to)) else { continue };
        if e.from_port as usize >= from.kind.outputs().len() || e.to_port as usize >= to.kind.inputs().len() {
            continue;
        }
        wire.insert((e.to, e.to_port), (e.from, e.from_port));
    }

    // Depth-first from the Output: emit each node after its inputs.
    struct Ctx<'g> {
        by_id: &'g HashMap<u32, &'g ShaderNodeDef>,
        wire: &'g HashMap<(u32, u8), (u32, u8)>,
        /// Emitted nodes: their output types.
        done: HashMap<u32, Vec<ShaderType>>,
        visiting: Vec<u32>,
        body: String,
    }
    fn visit(c: &mut Ctx, id: u32) -> Result<(), ShaderGraphError> {
        if c.done.contains_key(&id) {
            return Ok(());
        }
        if c.visiting.contains(&id) {
            return Err(ShaderGraphError { node: Some(id), message: "wires form a loop".into() });
        }
        c.visiting.push(id);
        let node = c.by_id[&id];
        let inputs = node.kind.inputs();
        // Resolve each input: an expression and its type.
        let mut args: Vec<(String, ShaderType, bool)> = Vec::with_capacity(inputs.len());
        for (i, spec) in inputs.iter().enumerate() {
            if let Some(&(from, port)) = c.wire.get(&(id, i as u8)) {
                visit(c, from)?;
                let ty = c.done[&from][port as usize];
                args.push((format!("n{from}_{port}"), ty, true));
            } else {
                let ty = match spec.ty {
                    PortType::Fixed(t) => t,
                    PortType::Dynamic => ShaderType::Float,
                };
                let expr = match spec.default {
                    PortDefault::Uv => "material_uv(in)".to_string(),
                    PortDefault::Value(v) => literal(node.defaults.get(i).copied().flatten().unwrap_or(v), ty),
                };
                args.push((expr, ty, false));
            }
        }
        // Dynamic nodes take the widest wired dynamic input.
        let dyn_ty = inputs
            .iter()
            .zip(&args)
            .filter(|(spec, (_, _, wired))| spec.ty == PortType::Dynamic && *wired)
            .map(|(_, (_, ty, _))| *ty)
            .max()
            .unwrap_or(ShaderType::Float);
        let arg = |i: usize| -> String {
            let (expr, ty, _) = &args[i];
            let want = match inputs[i].ty {
                PortType::Fixed(t) => t,
                PortType::Dynamic => dyn_ty,
            };
            coerce(expr, *ty, want)
        };
        use ShaderNodeKind::*;
        let v = node.value;
        let d = dyn_ty.wgsl();
        let exprs: Vec<String> = match node.kind {
            Output => {
                let _ = writeln!(c.body, "    return {};", arg(0));
                vec![]
            }
            Uv => vec!["material_uv(in)".into()],
            LocalPosition => vec!["in.local_position".into()],
            Time => vec!["material_globals.time.x".into()],
            Tint => vec!["uniforms.color".into()],
            SampleTexture => {
                let _ = writeln!(c.body, "    let n{id}_tex = textureSample(image_texture, image_sampler, {});", arg(0));
                vec![format!("n{id}_tex"), format!("n{id}_tex.a")]
            }
            FloatConstant => vec![literal(v, F)],
            Vec2Constant => vec![literal(v, V2)],
            ColorConstant => vec![literal(v, V4)],
            Add => vec![format!("{} + {}", arg(0), arg(1))],
            Subtract => vec![format!("{} - {}", arg(0), arg(1))],
            Multiply => vec![format!("{} * {}", arg(0), arg(1))],
            // A divide by exactly zero yields 0 rather than inf/NaN.
            Divide => vec![format!("select({a} / {b}, {d}(0.0), {b} == {d}(0.0))", a = arg(0), b = arg(1))],
            Min => vec![format!("min({}, {})", arg(0), arg(1))],
            Max => vec![format!("max({}, {})", arg(0), arg(1))],
            Power => vec![format!("pow(abs({}), {})", arg(0), arg(1))],
            Mix => vec![format!("mix({}, {}, {d}({}))", arg(0), arg(1), arg(2))],
            Step => vec![format!("step({}, {})", arg(0), arg(1))],
            Smoothstep => vec![format!("smoothstep({}, {}, {})", arg(0), arg(1), arg(2))],
            OneMinus => vec![format!("{d}(1.0) - {}", arg(0))],
            Negate => vec![format!("-({})", arg(0))],
            Abs => vec![format!("abs({})", arg(0))],
            Fract => vec![format!("fract({})", arg(0))],
            Floor => vec![format!("floor({})", arg(0))],
            Saturate => vec![format!("clamp({}, {d}(0.0), {d}(1.0))", arg(0))],
            Sin => vec![format!("sin({})", arg(0))],
            Cos => vec![format!("cos({})", arg(0))],
            Remap => vec![format!(
                "{d}({om}) + ({x} - {d}({im})) * {d}(({oM}) - ({om})) / {d}(max(({iM}) - ({im}), 0.00001))",
                x = arg(0),
                im = arg(1),
                iM = arg(2),
                om = arg(3),
                oM = arg(4)
            )],
            Posterize => vec![format!("floor({x} * {d}(max({s}, 1.0))) / {d}(max({s}, 1.0))", x = arg(0), s = arg(1))],
            Length => vec![if dyn_ty == F { format!("abs({})", arg(0)) } else { format!("length({})", arg(0)) }],
            Distance => vec![if dyn_ty == F { format!("abs({} - {})", arg(0), arg(1)) } else { format!("distance({}, {})", arg(0), arg(1)) }],
            Dot => vec![if dyn_ty == F { format!("{} * {}", arg(0), arg(1)) } else { format!("dot({}, {})", arg(0), arg(1)) }],
            Split => {
                let _ = writeln!(c.body, "    let n{id}_v = {};", arg(0));
                vec![format!("n{id}_v.x"), format!("n{id}_v.y"), format!("n{id}_v.z"), format!("n{id}_v.w")]
            }
            Combine => vec![format!("vec4<f32>({}, {}, {}, {})", arg(0), arg(1), arg(2), arg(3))],
            MakeVec2 => vec![format!("vec2<f32>({}, {})", arg(0), arg(1))],
            TilingOffset => vec![format!("{} * {} + {}", arg(0), arg(1), arg(2))],
            Panner => vec![format!("{} + {} * material_globals.time.x", arg(0), arg(1))],
            RotateUv => vec![format!("sg_rotate({}, {}, {})", arg(0), arg(2), arg(1))],
            Twirl => {
                let _ = writeln!(c.body, "    let n{id}_d = {} - {};", arg(0), arg(2));
                vec![format!("sg_rotate({uv}, {cen}, {s} * length(n{id}_d))", uv = arg(0), cen = arg(2), s = arg(1))]
            }
            Noise => vec![format!("sg_noise({} * {})", arg(0), arg(1))],
            Checker => {
                let _ = writeln!(c.body, "    let n{id}_c = floor({} * {});", arg(0), arg(1));
                vec![format!("mix({}, {}, f32(i32(n{id}_c.x + n{id}_c.y) & 1))", arg(2), arg(3))]
            }
            Circle => vec![format!(
                "1.0 - smoothstep({r} - {s}, {r} + {s}, distance({uv}, vec2<f32>(0.5, 0.5)))",
                uv = arg(0),
                r = arg(1),
                s = arg(2)
            )],
            HueShift => {
                let _ = writeln!(c.body, "    let n{id}_c = {};", arg(0));
                let _ = writeln!(c.body, "    let n{id}_h = sg_rgb_to_hsv(n{id}_c.rgb);");
                vec![format!("vec4<f32>(sg_hsv_to_rgb(vec3<f32>(fract(n{id}_h.x + {}), n{id}_h.y, n{id}_h.z)), n{id}_c.a)", arg(1))]
            }
            Grayscale => {
                let _ = writeln!(c.body, "    let n{id}_c = {};", arg(0));
                vec![format!("vec4<f32>(vec3<f32>(dot(n{id}_c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722))), n{id}_c.a)")]
            }
            Fresnel2D => vec![format!("pow(clamp(distance({}, vec2<f32>(0.5, 0.5)) * 2.0, 0.0, 1.0), max({}, 0.0001))", arg(0), arg(1))],
            // Each pass's glue defines `material_fresnel` (3D: real normal and
            // camera; 2D: a radial stand-in).
            ViewFresnel => vec![format!("material_fresnel(in, {})", arg(0))],
            FacingUv => vec!["material_facing_uv(in)".to_string()],
            Brick => {
                // Every other row shifts half a brick; each brick gets its own
                // brightness from a hash of its cell.
                let _ = writeln!(c.body, "    var n{id}_p = {} * {};", arg(0), arg(1));
                let _ = writeln!(c.body, "    n{id}_p.x += 0.5 * (floor(n{id}_p.y) - 2.0 * floor(floor(n{id}_p.y) / 2.0));");
                let _ = writeln!(c.body, "    let n{id}_f = fract(n{id}_p);");
                let _ = writeln!(c.body, "    let n{id}_m = {};", arg(2));
                let _ = writeln!(c.body, "    let n{id}_mortar = max(step(n{id}_f.x, n{id}_m * 0.5) + step(1.0 - n{id}_m * 0.5, n{id}_f.x), step(n{id}_f.y, n{id}_m) + step(1.0 - n{id}_m, n{id}_f.y));");
                let _ = writeln!(c.body, "    let n{id}_tone = 0.82 + 0.36 * sg_hash2(floor(n{id}_p));");
                vec![format!("mix(vec4<f32>({b}.rgb * n{id}_tone, {b}.a), {m}, clamp(n{id}_mortar, 0.0, 1.0))", b = arg(3), m = arg(4)), format!("clamp(n{id}_mortar, 0.0, 1.0)")]
            }
        };
        let mut types = Vec::new();
        for (port, (spec, expr)) in node.kind.outputs().iter().zip(exprs).enumerate() {
            let ty = match spec.ty {
                PortType::Fixed(t) => t,
                PortType::Dynamic => dyn_ty,
            };
            let _ = writeln!(c.body, "    let n{id}_{port}: {} = {expr};", ty.wgsl());
            types.push(ty);
        }
        c.visiting.pop();
        c.done.insert(id, types);
        Ok(())
    }

    let mut c = Ctx { by_id: &by_id, wire: &wire, done: HashMap::new(), visiting: Vec::new(), body: String::new() };
    visit(&mut c, output.id)?;
    let mut src = String::from(SHADER_GRAPH_LIBRARY);
    src.push_str("\nfn material_fragment(in: VertexOutput) -> vec4<f32> {\n");
    src.push_str(&c.body);
    src.push_str("}\n");
    Ok(src)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u32, kind: ShaderNodeKind) -> ShaderNodeDef {
        ShaderNodeDef { id, kind, position: Vec2::ZERO, value: [0.0; 4], defaults: Vec::new() }
    }

    #[test]
    fn starter_graph_compiles_to_texture_times_tint() {
        let src = compile_wgsl(&ShaderGraphDef::starter()).unwrap();
        assert!(src.contains("textureSample(image_texture, image_sampler, material_uv(in))"), "{src}");
        assert!(src.contains("let n3_0: vec4<f32> = n1_0 * n2_0;"), "{src}");
        assert!(src.contains("return n3_0;"), "{src}");
    }

    #[test]
    fn dynamic_math_widens_and_converts() {
        // Time (float) * UV (vec2) -> vec2, then into Output's vec4.
        let graph = ShaderGraphDef {
            domain: ShaderDomain::Canvas2D,
            nodes: vec![node(1, ShaderNodeKind::Time), node(2, ShaderNodeKind::Uv), node(3, ShaderNodeKind::Multiply), node(4, ShaderNodeKind::Output)],
            edges: vec![
                ShaderEdgeDef { from: 1, from_port: 0, to: 3, to_port: 0 },
                ShaderEdgeDef { from: 2, from_port: 0, to: 3, to_port: 1 },
                ShaderEdgeDef { from: 3, from_port: 0, to: 4, to_port: 0 },
            ],
        };
        let src = compile_wgsl(&graph).unwrap();
        assert!(src.contains("let n3_0: vec2<f32> = vec2<f32>(n1_0) * n2_0;"), "{src}");
        assert!(src.contains("return vec4<f32>(n3_0, 0.0, 1.0);"), "{src}");
    }

    #[test]
    fn loops_and_missing_output_are_errors() {
        let mut g = ShaderGraphDef { domain: ShaderDomain::Canvas2D, nodes: vec![node(1, ShaderNodeKind::Add)], edges: vec![] };
        assert_eq!(compile_wgsl(&g).unwrap_err().message, "add an Output node");
        g.nodes.push(node(2, ShaderNodeKind::Output));
        g.edges = vec![ShaderEdgeDef { from: 1, from_port: 0, to: 1, to_port: 0 }, ShaderEdgeDef { from: 1, from_port: 0, to: 2, to_port: 0 }];
        assert_eq!(compile_wgsl(&g).unwrap_err().node, Some(1));
    }

    #[test]
    fn every_node_kind_compiles_wired_into_the_output() {
        // Each kind's first output feeds the Output (through a Combine for
        // non-vec4 outputs); compile errors here mean a bad template.
        for &kind in ShaderNodeKind::ALL {
            if kind == ShaderNodeKind::Output {
                continue;
            }
            let graph = ShaderGraphDef {
                domain: ShaderDomain::Canvas2D,
                nodes: vec![node(1, kind), node(2, ShaderNodeKind::Output)],
                edges: vec![ShaderEdgeDef { from: 1, from_port: 0, to: 2, to_port: 0 }],
            };
            compile_wgsl(&graph).unwrap_or_else(|e| panic!("{kind:?}: {e}"));
        }
    }
}
