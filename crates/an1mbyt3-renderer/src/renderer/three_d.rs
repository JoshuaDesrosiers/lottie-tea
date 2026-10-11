//! The 3D pass: depth-tested triangle meshes drawn through a perspective
//! camera, before (underneath) the 2D layer.
//!
//! Usage, once per frame: build a [`Scene3D`] (camera matrices, lights and
//! the meshes to draw) and hand it to [`Renderer::set_scene_3d`]; the next
//! `render*` call draws it into the frame first, then the 2D scene on top.
//! `set_scene_3d(None)` (or never calling it) means no 3D pass at all, so
//! every 2D-only scene renders exactly as before.
//!
//! - Meshes are uploaded once ([`Renderer::upload_mesh_3d`]) and shared via
//!   `Rc`; per-draw data (model matrix, colour) goes into one
//!   dynamic-offset uniform buffer, grown as needed.
//! - A draw can wear a **shader-graph material** ([`Renderer::compile_material_3d`]):
//!   the same graphs as 2D, linked against this pass's shader, whose output
//!   becomes the surface colour that then gets lit.
//! - **Overlay** draws (editor gizmos, selection boxes) are drawn last with
//!   no depth test, so they're never hidden inside geometry.
//! - The depth buffer is created per target size on demand.

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;

use an1mbyt3_geometry::Mesh3DData;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use super::Renderer;

/// Point lights the shader handles at once (`three_d.wgsl`).
pub const MAX_POINT_LIGHTS: usize = 8;
/// Per-draw uniform stride (wgpu's minimum dynamic-offset alignment).
const DRAW_STRIDE: u64 = 256;
/// Joints one skeleton can have (the shader's `MAX_JOINTS`), and the bytes
/// of one skeleton's slot in the joint palette buffer (16 KiB: also the
/// smallest uniform-block limit WebGL2 guarantees).
pub const MAX_JOINTS: usize = 256;
const JOINT_SLOT: u64 = (MAX_JOINTS * 64) as u64;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SHADER_SOURCE: &str = include_str!("three_d.wgsl");
const FLUID_SHADER_SOURCE: &str = include_str!("fluid3d.wgsl");
/// The fluid density buffer: one float channel's worth, blendable.
const DENSITY_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Bytes between fluid layers' uniforms (dynamic-offset alignment).
const FLUID_STRIDE: u64 = 256;

/// What a material's generated code needs beyond `three_d.wgsl` itself —
/// the 3D side of the contract `an1mbyt3_core::shader_graph` documents.
/// (`uniforms.color` and `VertexOutput.local_position` already exist in
/// the 3D shader.)
const MATERIAL_GLUE_3D: &str = r#"
@group(2) @binding(0)
var image_texture: texture_2d<f32>;
@group(2) @binding(1)
var image_sampler: sampler;
struct MaterialGlobals {
    time: vec4<f32>,
};
@group(3) @binding(0)
var<uniform> material_globals: MaterialGlobals;

fn material_uv(in: VertexOutput) -> vec2<f32> {
    return in.uv;
}

// The shader graph's Facing UV node: where the surface normal points, as
// seen from the camera — (0.5, 0.5) on the part facing you, the edge of the
// 0..1 square at the silhouette, +V downwards like texture UVs. On a sphere
// this is a disc that always faces the viewer: draw an iris / pupil with
// it and the eye stares at you from every angle.
fn material_facing_uv(in: VertexOutput) -> vec2<f32> {
    let n = normalize(in.normal);
    return vec2<f32>(dot(n, scene.camera_right.xyz) * 0.5 + 0.5, 0.5 - dot(n, scene.camera_up.xyz) * 0.5);
}

// The shader graph's View Fresnel node: 0 where the surface faces the
// camera, 1 at the silhouette, shaped by `power`.
fn material_fresnel(in: VertexOutput, power: f32) -> f32 {
    let n = normalize(in.normal);
    let to_eye = normalize(scene.camera_pos.xyz - in.world);
    return pow(clamp(1.0 - abs(dot(n, to_eye)), 0.0, 1.0), max(power, 0.0001));
}

@fragment
fn fs_material(in: VertexOutput) -> @location(0) vec4<f32> {
    return shade(in, material_fragment(in));
}
"#;

/// A 3D mesh on the GPU (interleaved position / normal / UV), plus — for a
/// skinned mesh — a second buffer of per-vertex joint indices and weights
/// (`upload_mesh_3d_skinned`).
pub struct GpuMesh3D {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    skin_buffer: Option<wgpu::Buffer>,
}

/// A shader-graph material compiled for the 3D pass — one pipeline per
/// way a mesh can be blended (`Blend3D`).
pub struct GpuMaterial3D {
    pipeline: wgpu::RenderPipeline,
    transparent: wgpu::RenderPipeline,
    additive: wgpu::RenderPipeline,
    double_sided: wgpu::RenderPipeline,
    /// The same, skinned on the GPU (characters).
    skinned: wgpu::RenderPipeline,
    skinned_double_sided: wgpu::RenderPipeline,
    skinned_transparent: wgpu::RenderPipeline,
}

impl GpuMaterial3D {
    fn for_blend(&self, blend: Blend3D, double_sided: bool, skinned: bool) -> &wgpu::RenderPipeline {
        match blend {
            Blend3D::Opaque if skinned && double_sided => &self.skinned_double_sided,
            Blend3D::Opaque if skinned => &self.skinned,
            Blend3D::Transparent | Blend3D::Additive if skinned => &self.skinned_transparent,
            Blend3D::Opaque if double_sided => &self.double_sided,
            Blend3D::Opaque => &self.pipeline,
            Blend3D::Transparent => &self.transparent,
            Blend3D::Additive => &self.additive,
        }
    }
}

/// How a mesh mixes with what's behind it.
///
/// - `Opaque`: depth-tested and depth-writing, drawn first, in any order.
/// - `Transparent`: alpha-blended ("see-through glass"), drawn after every
///   opaque mesh, farthest first, without writing depth — so things behind
///   it (other glass, particles) still show through.
/// - `Additive`: like `Transparent`, but its colour *adds* light (glows,
///   energy, holograms): black is invisible, overlaps get brighter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Blend3D {
    #[default]
    Opaque,
    Transparent,
    Additive,
}

/// One camera-facing particle quad (world space), ready to draw — built by
/// whoever simulates the particles (the editor's `ParticleSystem3D`).
///
/// `corners` go round the quad; the shader draws a soft glow (or, with
/// `hardness`, a crisp shape) across it. Colours are **premultiplied**
/// linear RGBA: `rgb` is how much light it adds, `a` how much of what's
/// behind it it hides — an additive particle is just `a = 0`, a smoky or
/// inky one `a > 0`, so both blend modes share one pipeline and one
/// sorted pass.
#[derive(Debug, Clone, Copy)]
pub struct ParticleQuad3D {
    pub corners: [[f32; 3]; 4],
    /// Premultiplied colour per corner (ribbon segments fade along).
    pub colors: [[f32; 4]; 4],
    /// UV per corner. A sprite uses the full `0..1` square (a round blob);
    /// a ribbon segment pins `u` to 0.5 so only the distance *across* it
    /// shapes the edge (a band, not a chain of beads).
    pub uvs: [[f32; 2]; 4],
    /// 0 = soft glowing falloff, 1 = a crisp, solid edge (ink, droplets).
    pub hardness: f32,
    /// Distance from the camera (sorting: farthest drawn first).
    pub depth: f32,
}

/// One **fluid** layer: particles drawn as a single liquid surface instead
/// of individual sprites (screen-space metaballs). Each quad splats soft
/// density (its colours' alpha = how much); where the summed density
/// passes `threshold` the pixel is "inside" the liquid, shaded with `body`,
/// a glossy highlight from the surface's slope, and `edge` (its alpha =
/// strength) where the surface curves away. Touching particles merge,
/// separating ones stretch into drips — ink, slime, water, lava.
#[derive(Debug, Clone)]
pub struct FluidLayer3D {
    pub quads: Vec<ParticleQuad3D>,
    /// Linear RGB, alpha = opacity.
    pub body: [f32; 4],
    pub edge: [f32; 4],
    pub threshold: f32,
    /// Highlight strength (0 = matte).
    pub gloss: f32,
}

impl ParticleQuad3D {
    /// The standard sprite UVs.
    pub const SPRITE_UVS: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
}

/// One mesh to draw this frame.
#[derive(Clone)]
pub struct MeshDraw3D {
    pub mesh: Rc<GpuMesh3D>,
    pub model: Mat4,
    /// Straight linear RGBA (the material's Tint, when there is one).
    pub color: [f32; 4],
    pub shininess: f32,
    pub unlit: bool,
    /// Shades the surface with a compiled shader graph.
    pub material: Option<Rc<GpuMaterial3D>>,
    /// The texture a material's Sample Texture node reads (an imported
    /// model's base colour map); white when `None`. Only used with a
    /// `material` (a plain draw has no texture binding).
    pub texture: Option<super::GpuImageTexture>,
    /// Drawn last, ignoring depth — editor gizmos.
    pub overlay: bool,
    /// Opaque / see-through / glowing (see `Blend3D`).
    pub blend: Blend3D,
    /// Draw both faces (only with a `material`, opaque).
    pub double_sided: bool,
    /// Skinned on the GPU: this skeleton's joint matrices (model space,
    /// `world × inverse bind`; draws sharing a skeleton share the `Rc`).
    /// The mesh must have been uploaded with `upload_mesh_3d_skinned`.
    pub joints: Option<Rc<Vec<Mat4>>>,
    /// > 0: draw this skinned mesh as a toon **outline** instead — pushed
    /// out along its normals by this many model units, front faces culled,
    /// flat `color` (fogged).
    pub outline: f32,
    /// The material's parameters, `uniforms.material[0..6]` in its shader
    /// (e.g. a Toon material's colours and sliders) — set per draw, so
    /// editing them never recompiles anything.
    pub params: [[f32; 4]; MATERIAL_PARAMS_3D],
}

impl MeshDraw3D {
    pub fn new(mesh: Rc<GpuMesh3D>, model: Mat4, color: [f32; 4]) -> Self {
        Self { mesh, model, color, shininess: 0.3, unlit: false, material: None, texture: None, overlay: false, blend: Blend3D::Opaque, double_sided: false, joints: None, outline: 0.0, params: [[0.0; 4]; MATERIAL_PARAMS_3D] }
    }
}

/// A point light (world space).
#[derive(Debug, Clone, Copy)]
pub struct PointLight3D {
    pub position: Vec3,
    /// Colour × intensity.
    pub color: Vec3,
    pub range: f32,
}

/// Everything the 3D pass needs for one frame.
#[derive(Clone)]
pub struct Scene3D {
    pub view: Mat4,
    pub projection: Mat4,
    pub camera_position: Vec3,
    /// Clears the frame to this first; `None` draws over what's there.
    pub clear_color: Option<[f32; 4]>,
    /// The direction sunlight travels (needn't be normalized).
    pub sun_direction: Vec3,
    /// Sun colour × intensity (zero = no sun).
    pub sun_color: Vec3,
    /// Sky ambient light.
    pub ambient: Vec3,
    /// Fog colour and density (density 0 = no fog).
    pub fog_color: Vec3,
    pub fog_density: f32,
    /// At most [`MAX_POINT_LIGHTS`] are used.
    pub point_lights: Vec<PointLight3D>,
    pub draws: Vec<MeshDraw3D>,
    /// Particle quads (any order — the pass sorts them, and interleaves
    /// them with see-through meshes by distance).
    pub particles: Vec<ParticleQuad3D>,
    /// Fluid layers, composited over the scene after it (in order).
    pub fluids: Vec<FluidLayer3D>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SceneUniforms {
    view_proj: [[f32; 4]; 4],
    camera_pos: [f32; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    ambient: [f32; 4],
    fog: [f32; 4],
    counts: [f32; 4],
    point_pos: [[f32; 4]; MAX_POINT_LIGHTS],
    point_color: [[f32; 4]; MAX_POINT_LIGHTS],
    /// The camera's world right / up axes (materials' Facing UV).
    camera_right: [f32; 4],
    camera_up: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DrawUniforms3D {
    model: [[f32; 4]; 4],
    normal_matrix: [[f32; 4]; 4],
    color: [f32; 4],
    params: [f32; 4],
    /// The material's own parameters (toon shade colour, rim, brush …) —
    /// fills the draw's 256-byte slot exactly.
    material: [[f32; 4]; MATERIAL_PARAMS_3D],
}

/// The Toon material's fragment, MToon style — compile with
/// [`Renderer::compile_material_3d`]; its parameters come per draw
/// ([`MeshDraw3D::params`], from `an1mbyt3_core::ToonMaterial::gpu_params`).
pub const TOON_MTOON_WGSL: &str = concat!(include_str!("toon/common.wgsl"), include_str!("toon/mtoon.wgsl"));

/// The Toon material's fragment, painterly Arcane style (see
/// [`TOON_MTOON_WGSL`]).
pub const TOON_ARCANE_WGSL: &str = concat!(include_str!("toon/common.wgsl"), include_str!("toon/arcane.wgsl"));

/// How many `vec4` material parameters a 3D draw carries
/// ([`MeshDraw3D::params`], `uniforms.material[i]` in WGSL).
pub const MATERIAL_PARAMS_3D: usize = 6;

/// GPU state for the 3D pass, built on first use.
pub(crate) struct Resources3D {
    pipeline: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    transparent_pipeline: wgpu::RenderPipeline,
    additive_pipeline: wgpu::RenderPipeline,
    particle_pipeline: wgpu::RenderPipeline,
    /// (capacity in vertices, buffer) for this frame's particle quads.
    particle_vertices: RefCell<(u64, wgpu::Buffer)>,
    /// Fluid: density splat + composite pipelines, their vertex buffer,
    /// per-layer uniforms and the density target (rebuilt on resize).
    density_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    composite_layout: wgpu::BindGroupLayout,
    fluid_vertices: RefCell<(u64, wgpu::Buffer)>,
    fluid_uniforms: RefCell<(u64, wgpu::Buffer)>,
    fluid_sampler: wgpu::Sampler,
    /// (width, height, density view, composite bind group, uniform capacity it binds).
    fluid_target: RefCell<Option<(u32, u32, wgpu::TextureView, wgpu::BindGroup, u64)>>,
    material_layout: wgpu::PipelineLayout,
    scene_buffer: wgpu::Buffer,
    scene_bind_group: wgpu::BindGroup,
    draw_layout: wgpu::BindGroupLayout,
    /// (capacity in draws, buffer, bind group) — regrown when a frame has more.
    draws: RefCell<(u64, wgpu::Buffer, wgpu::BindGroup)>,
    /// (capacity in skeleton slots, joint palette buffer) — group 1's
    /// binding 1; regrowing it rebuilds `draws`' bind group.
    joints: RefCell<(u64, wgpu::Buffer)>,
    /// Skinned draws without a material, and toon outlines.
    skinned_pipeline: wgpu::RenderPipeline,
    outline_pipeline: wgpu::RenderPipeline,
    /// Outlines for unskinned meshes (toon materials on props).
    outline_static_pipeline: wgpu::RenderPipeline,
    /// (width, height, view) of the current depth buffer.
    depth: RefCell<Option<(u32, u32, wgpu::TextureView)>>,
}

/// The renderer's 3D state (a field on `Renderer`).
#[derive(Default)]
pub(crate) struct State3D {
    resources: OnceCell<Resources3D>,
    scene: RefCell<Option<Scene3D>>,
}

fn draw_buffer(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, capacity: u64, joints: &wgpu::Buffer) -> (u64, wgpu::Buffer, wgpu::BindGroup) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("an1mbyt3 3D draw uniforms"),
        size: capacity * DRAW_STRIDE,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = draw_bind_group(device, layout, &buffer, joints);
    (capacity, buffer, bind_group)
}

fn draw_bind_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, draws: &wgpu::Buffer, joints: &wgpu::Buffer) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("an1mbyt3 3D draw bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: draws, offset: 0, size: wgpu::BufferSize::new(std::mem::size_of::<DrawUniforms3D>() as u64) }),
            },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: joints, offset: 0, size: wgpu::BufferSize::new(JOINT_SLOT) }) },
        ],
    })
}

fn joint_buffer(device: &wgpu::Device, slots: u64) -> (u64, wgpu::Buffer) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("an1mbyt3 3D joint palettes"),
        size: slots.max(1) * JOINT_SLOT,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (slots.max(1), buffer)
}

/// Which kind of mesh pipeline to build.
#[derive(Clone, Copy, PartialEq)]
enum MeshPass {
    Opaque,
    /// Opaque, both faces drawn (cloth, hair cards, leaves — glTF
    /// `doubleSided` materials).
    OpaqueDoubleSided,
    /// Editor gizmos: always on top.
    Overlay,
    /// Toon outlines: opaque, front faces culled (an inverted hull).
    Outline,
    Transparent,
    Additive,
}

/// `(src, dst)` additive light: `rgb * a` added, alpha untouched.
const ADDITIVE_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
    alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
};

/// A mesh pipeline: `entry` fragment stage, for one `MeshPass`; `skinned`
/// takes the joint/weight buffer too and skins in the vertex stage
/// (`vs_skinned`, or `vs_skinned_outline` for outlines).
fn mesh_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat, layout: &wgpu::PipelineLayout, module: &wgpu::ShaderModule, entry: &str, pass: MeshPass) -> wgpu::RenderPipeline {
    mesh_pipeline_with(device, format, layout, module, entry, pass, false)
}

fn mesh_pipeline_with(device: &wgpu::Device, format: wgpu::TextureFormat, layout: &wgpu::PipelineLayout, module: &wgpu::ShaderModule, entry: &str, pass: MeshPass, skinned: bool) -> wgpu::RenderPipeline {
    let overlay = pass == MeshPass::Overlay;
    // See-through and glowing surfaces test depth (opaque things hide
    // them) but don't write it (they never hide each other or particles).
    let writes_depth = matches!(pass, MeshPass::Opaque | MeshPass::OpaqueDoubleSided | MeshPass::Outline);
    let vertex_entry = match (skinned, pass) {
        (true, MeshPass::Outline) => "vs_skinned_outline",
        (true, _) => "vs_skinned",
        (false, MeshPass::Outline) => "vs_outline",
        _ => "vs_main",
    };
    let base_layout = wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2] };
    let skin_layout = wgpu::VertexBufferLayout { array_stride: 24, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![3 => Uint16x4, 4 => Float32x4] };
    let buffers_skinned = [Some(base_layout.clone()), Some(skin_layout)];
    let buffers_plain = [Some(base_layout)];
    let blend = if pass == MeshPass::Additive { ADDITIVE_BLEND } else { wgpu::BlendState::ALPHA_BLENDING };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(match pass {
            MeshPass::Opaque => "an1mbyt3 3D mesh pipeline",
            MeshPass::OpaqueDoubleSided => "an1mbyt3 3D double-sided mesh pipeline",
            MeshPass::Outline => "an1mbyt3 3D outline pipeline",
            MeshPass::Overlay => "an1mbyt3 3D overlay pipeline",
            MeshPass::Transparent => "an1mbyt3 3D transparent pipeline",
            MeshPass::Additive => "an1mbyt3 3D additive pipeline",
        }),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(vertex_entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: if skinned { &buffers_skinned } else { &buffers_plain },
        },
        primitive: wgpu::PrimitiveState {
            cull_mode: match pass {
                MeshPass::OpaqueDoubleSided => None,
                MeshPass::Outline => Some(wgpu::Face::Front),
                _ => Some(wgpu::Face::Back),
            },
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(writes_depth),
            depth_compare: Some(if overlay { wgpu::CompareFunction::Always } else { wgpu::CompareFunction::Less }),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Floats per particle vertex: position xyz, premultiplied colour rgba, uv,
/// hardness.
const PARTICLE_FLOATS: usize = 10;

/// The particle pipeline: camera-facing quads with a soft round glow,
/// premultiplied-alpha blending (so additive and smoky particles share it),
/// depth-tested against opaque meshes but never writing depth, no culling.
fn particle_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat, layout: &wgpu::PipelineLayout, module: &wgpu::ShaderModule) -> wgpu::RenderPipeline {
    let premultiplied = wgpu::BlendState {
        color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
        alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("an1mbyt3 3D particle pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_particle"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: (PARTICLE_FLOATS * 4) as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32x2, 3 => Float32],
            })],
        },
        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs_particle"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: Some(premultiplied), write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Splats fluid particles' density, additively, depth-tested against the
/// scene (so the liquid goes behind solid things) without writing depth.
fn density_pipeline(device: &wgpu::Device, layout: &wgpu::PipelineLayout, module: &wgpu::ShaderModule) -> wgpu::RenderPipeline {
    let add = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("an1mbyt3 3D fluid density pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_particle"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: (PARTICLE_FLOATS * 4) as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32x2, 3 => Float32],
            })],
        },
        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs_density"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState { format: DENSITY_FORMAT, blend: Some(wgpu::BlendState { color: add, alpha: add }), write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Composites one fluid layer over the target (premultiplied alpha).
fn composite_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat, layout: &wgpu::PipelineLayout, module: &wgpu::ShaderModule) -> wgpu::RenderPipeline {
    let premultiplied = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("an1mbyt3 3D fluid composite pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState { module, entry_point: Some("vs_fullscreen"), compilation_options: wgpu::PipelineCompilationOptions::default(), buffers: &[] },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs_composite"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState { color: premultiplied, alpha: premultiplied }), write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn fluid_uniform_buffer(device: &wgpu::Device, layers: u64) -> (u64, wgpu::Buffer) {
    let layers = layers.max(4);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("an1mbyt3 3D fluid uniforms"),
        size: layers * FLUID_STRIDE,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (layers, buffer)
}

/// Writes `quads` as particle-pipeline vertices.
fn quad_vertices(quads: &[&ParticleQuad3D], data: &mut Vec<f32>) {
    for q in quads {
        for k in [0usize, 1, 2, 0, 2, 3] {
            data.extend_from_slice(&q.corners[k]);
            data.extend_from_slice(&q.colors[k]);
            data.extend_from_slice(&q.uvs[k]);
            data.push(q.hardness);
        }
    }
}

fn particle_buffer(device: &wgpu::Device, vertices: u64) -> (u64, wgpu::Buffer) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("an1mbyt3 3D particle vertices"),
        size: vertices.max(6) * (PARTICLE_FLOATS * 4) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (vertices.max(6), buffer)
}

impl Renderer {
    fn resources_3d(&self) -> &Resources3D {
        self.state_3d.resources.get_or_init(|| {
            let device = &self.device;
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("an1mbyt3 3D shader"), source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()) });
            let uniform_entry = |dynamic: bool| wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: dynamic, min_binding_size: None },
                count: None,
            };
            let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("an1mbyt3 3D scene layout"), entries: &[uniform_entry(false)] });
            let draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("an1mbyt3 3D draw layout"),
                entries: &[
                    uniform_entry(true),
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: wgpu::BufferSize::new(JOINT_SLOT) },
                        count: None,
                    },
                ],
            });
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("an1mbyt3 3D pipeline layout"),
                bind_group_layouts: &[Some(&scene_layout), Some(&draw_layout)],
                immediate_size: 0,
            });
            let materials = self.material_resources();
            let material_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("an1mbyt3 3D material pipeline layout"),
                bind_group_layouts: &[Some(&scene_layout), Some(&draw_layout), Some(&self.image_bind_group_layout), Some(&materials.globals_layout)],
                immediate_size: 0,
            });
            let pipeline = mesh_pipeline(device, self.render_format, &layout, &shader, "fs_main", MeshPass::Opaque);
            let overlay_pipeline = mesh_pipeline(device, self.render_format, &layout, &shader, "fs_main", MeshPass::Overlay);
            let transparent_pipeline = mesh_pipeline(device, self.render_format, &layout, &shader, "fs_main", MeshPass::Transparent);
            let additive_pipeline = mesh_pipeline(device, self.render_format, &layout, &shader, "fs_main", MeshPass::Additive);
            let skinned_pipeline = mesh_pipeline_with(device, self.render_format, &layout, &shader, "fs_main", MeshPass::Opaque, true);
            let outline_pipeline = mesh_pipeline_with(device, self.render_format, &layout, &shader, "fs_outline", MeshPass::Outline, true);
            let outline_static_pipeline = mesh_pipeline_with(device, self.render_format, &layout, &shader, "fs_outline", MeshPass::Outline, false);
            // Particles only read the scene uniforms (group 0).
            let particle_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("an1mbyt3 3D particle pipeline layout"),
                bind_group_layouts: &[Some(&scene_layout)],
                immediate_size: 0,
            });
            let particle_pipeline = particle_pipeline(device, self.render_format, &particle_layout, &shader);
            let particle_vertices = RefCell::new(particle_buffer(device, 6 * 256));
            let density_pipeline = density_pipeline(device, &particle_layout, &shader);
            let fluid_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("an1mbyt3 3D fluid shader"), source: wgpu::ShaderSource::Wgsl(FLUID_SHADER_SOURCE.into()) });
            let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("an1mbyt3 3D fluid composite layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: None },
                        count: None,
                    },
                ],
            });
            let composite_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("an1mbyt3 3D fluid composite pipeline layout"),
                bind_group_layouts: &[Some(&composite_layout)],
                immediate_size: 0,
            });
            let composite_pipeline = composite_pipeline(device, self.render_format, &composite_pipeline_layout, &fluid_shader);
            let fluid_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("an1mbyt3 3D fluid sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let fluid_vertices = RefCell::new(particle_buffer(device, 6 * 256));
            let fluid_uniforms = RefCell::new(fluid_uniform_buffer(device, 4));
            let scene_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("an1mbyt3 3D scene uniforms"),
                size: std::mem::size_of::<SceneUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("an1mbyt3 3D scene bind group"),
                layout: &scene_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: scene_buffer.as_entire_binding() }],
            });
            let joints = joint_buffer(device, 2);
            let draws = RefCell::new(draw_buffer(device, &draw_layout, 64, &joints.1));
            let joints = RefCell::new(joints);
            Resources3D {
                density_pipeline,
                composite_pipeline,
                composite_layout,
                fluid_vertices,
                fluid_uniforms,
                fluid_sampler,
                fluid_target: RefCell::new(None),
                pipeline, overlay_pipeline, transparent_pipeline, additive_pipeline, particle_pipeline, particle_vertices, material_layout, scene_buffer, scene_bind_group, draw_layout, draws, joints, skinned_pipeline, outline_pipeline, outline_static_pipeline, depth: RefCell::new(None) }
        })
    }

    /// The full WGSL a 3D material is built from.
    pub fn material_shader_source_3d(fragment: &str) -> String {
        let mut source = String::from(SHADER_SOURCE);
        source.push_str(MATERIAL_GLUE_3D);
        source.push_str(fragment);
        source
    }

    /// Validates a material's WGSL against the 3D pass (naga), like
    /// [`Renderer::validate_material`] does for 2D.
    pub fn validate_material_3d(fragment: &str) -> Result<(), String> {
        let source = Self::material_shader_source_3d(fragment);
        let module = naga::front::wgsl::parse_str(&source).map_err(|e| e.emit_to_string(&source))?;
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
            .validate(&module)
            .map_err(|e| e.emit_to_string(&source))?;
        Ok(())
    }

    /// Compiles a shader-graph material for 3D meshes (validated first).
    pub fn compile_material_3d(&self, fragment: &str) -> Result<GpuMaterial3D, String> {
        Self::validate_material_3d(fragment)?;
        let res = self.resources_3d();
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("an1mbyt3 3D material shader"),
            source: wgpu::ShaderSource::Wgsl(Self::material_shader_source_3d(fragment).into()),
        });
        let build = |pass| mesh_pipeline(&self.device, self.render_format, &res.material_layout, &module, "fs_material", pass);
        let skinned = |pass| mesh_pipeline_with(&self.device, self.render_format, &res.material_layout, &module, "fs_material", pass, true);
        Ok(GpuMaterial3D {
            pipeline: build(MeshPass::Opaque),
            transparent: build(MeshPass::Transparent),
            additive: build(MeshPass::Additive),
            double_sided: build(MeshPass::OpaqueDoubleSided),
            skinned: skinned(MeshPass::Opaque),
            skinned_double_sided: skinned(MeshPass::OpaqueDoubleSided),
            skinned_transparent: skinned(MeshPass::Transparent),
        })
    }

    /// Uploads a 3D mesh once; share it between draws via `Rc`.
    pub fn upload_mesh_3d(&self, mesh: &Mesh3DData) -> GpuMesh3D {
        let mut vertices: Vec<f32> = Vec::with_capacity(mesh.positions.len() * 8);
        for i in 0..mesh.positions.len() {
            vertices.extend_from_slice(&mesh.positions[i]);
            vertices.extend_from_slice(mesh.normals.get(i).unwrap_or(&[0.0, 1.0, 0.0]));
            vertices.extend_from_slice(mesh.uvs.get(i).unwrap_or(&[0.0, 0.0]));
        }
        // wgpu rejects empty buffers: keep one dummy vertex/index around.
        if vertices.is_empty() {
            vertices.resize(8, 0.0);
        }
        let indices: Vec<u32> = if mesh.indices.is_empty() { vec![0, 0, 0] } else { mesh.indices.clone() };
        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("an1mbyt3 3D vertices"),
            contents: bytemuck::cast_slice(&vertices),
            // COPY_DST: deforming meshes (skinned characters, morphs) are
            // rewritten in place every frame (`rewrite_mesh_3d`).
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("an1mbyt3 3D indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        GpuMesh3D { vertex_buffer, index_buffer, index_count: if mesh.indices.is_empty() { 0 } else { indices.len() as u32 }, skin_buffer: None }
    }

    /// Uploads a skinned mesh: its vertices plus, per vertex, up to four
    /// joint indices (into its skeleton's joint list) and weights — skinned
    /// on the GPU when drawn with `MeshDraw3D::joints`.
    pub fn upload_mesh_3d_skinned(&self, mesh: &Mesh3DData, joints: &[[u16; 4]], weights: &[[f32; 4]]) -> GpuMesh3D {
        let mut gpu = self.upload_mesh_3d(mesh);
        let count = mesh.positions.len().max(1);
        let mut bytes: Vec<u8> = Vec::with_capacity(count * 24);
        for v in 0..count {
            let j = joints.get(v).copied().unwrap_or([0; 4]);
            let w = weights.get(v).copied().unwrap_or([1.0, 0.0, 0.0, 0.0]);
            for k in j {
                bytes.extend_from_slice(&k.min((MAX_JOINTS - 1) as u16).to_le_bytes());
            }
            for k in w {
                bytes.extend_from_slice(&k.to_le_bytes());
            }
        }
        gpu.skin_buffer = Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("an1mbyt3 3D skin weights"), contents: &bytes, usage: wgpu::BufferUsages::VERTEX }));
        gpu
    }

    /// Rewrites a mesh's vertices in place (same vertex count and indices
    /// as when it was uploaded) — how a skinned / morphed mesh is updated
    /// every frame without allocating new GPU buffers. Returns `false` (and
    /// writes nothing) if the vertex count changed.
    pub fn rewrite_mesh_3d(&self, gpu: &GpuMesh3D, mesh: &Mesh3DData) -> bool {
        let count = mesh.positions.len();
        if count == 0 || gpu.vertex_buffer.size() != (count * 8 * 4) as u64 {
            return false;
        }
        let mut vertices: Vec<f32> = Vec::with_capacity(count * 8);
        for i in 0..count {
            vertices.extend_from_slice(&mesh.positions[i]);
            vertices.extend_from_slice(mesh.normals.get(i).unwrap_or(&[0.0, 1.0, 0.0]));
            vertices.extend_from_slice(mesh.uvs.get(i).unwrap_or(&[0.0, 0.0]));
        }
        self.queue.write_buffer(&gpu.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        true
    }

    /// [`Renderer::rewrite_mesh_3d`] from vertices already in the 3D
    /// layout (position, normal, UV: 8 floats each) — no repacking.
    pub fn rewrite_mesh_3d_interleaved(&self, gpu: &GpuMesh3D, vertices: &[f32]) -> bool {
        if vertices.is_empty() || gpu.vertex_buffer.size() != (vertices.len() * 4) as u64 {
            return false;
        }
        self.queue.write_buffer(&gpu.vertex_buffer, 0, bytemuck::cast_slice(vertices));
        true
    }

    /// Uploads a mesh from interleaved vertices (8 floats each) + indices.
    pub fn upload_mesh_3d_interleaved(&self, vertices: &[f32], indices: &[u32]) -> GpuMesh3D {
        let vertices: Vec<f32> = if vertices.is_empty() { vec![0.0; 8] } else { vertices.to_vec() };
        let index_data: Vec<u32> = if indices.is_empty() { vec![0, 0, 0] } else { indices.to_vec() };
        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("an1mbyt3 3D vertices (deforming)"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("an1mbyt3 3D indices"),
            contents: bytemuck::cast_slice(&index_data),
            usage: wgpu::BufferUsages::INDEX,
        });
        GpuMesh3D { vertex_buffer, index_buffer, index_count: if indices.is_empty() { 0 } else { index_data.len() as u32 }, skin_buffer: None }
    }

    /// The 3D scene to draw (under the 2D layer) on following frames;
    /// `None` = no 3D pass.
    pub fn set_scene_3d(&self, scene: Option<Scene3D>) {
        *self.state_3d.scene.borrow_mut() = scene;
    }

    /// Whether a 3D scene is set.
    pub fn has_scene_3d(&self) -> bool {
        self.state_3d.scene.borrow().is_some()
    }

    /// Draws the pending 3D scene (if any) into `target` of size
    /// `width × height`. `load` is what to do with the target's existing
    /// contents when the scene has no clear colour. Returns whether it drew.
    pub(crate) fn draw_3d_pass(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, width: u32, height: u32, load: wgpu::LoadOp<wgpu::Color>) -> bool {
        let scene_ref = self.state_3d.scene.borrow();
        let Some(scene) = scene_ref.as_ref() else { return false };
        let res = self.resources_3d();
        let materials = self.material_resources();

        // Scene uniforms.
        let mut uniforms = SceneUniforms::zeroed();
        uniforms.view_proj = (scene.projection * scene.view).to_cols_array_2d();
        uniforms.camera_pos = scene.camera_position.extend(1.0).into();
        let camera_world = scene.view.inverse();
        uniforms.camera_right = camera_world.x_axis.truncate().normalize_or(Vec3::X).extend(0.0).into();
        uniforms.camera_up = camera_world.y_axis.truncate().normalize_or(Vec3::Y).extend(0.0).into();
        uniforms.sun_dir = scene.sun_direction.normalize_or(Vec3::NEG_Y).extend(0.0).into();
        uniforms.sun_color = scene.sun_color.extend(0.0).into();
        uniforms.ambient = scene.ambient.extend(0.0).into();
        uniforms.fog = scene.fog_color.extend(scene.fog_density.max(0.0)).into();
        let lights = &scene.point_lights[..scene.point_lights.len().min(MAX_POINT_LIGHTS)];
        uniforms.counts = [lights.len() as f32, 0.0, 0.0, 0.0];
        for (i, l) in lights.iter().enumerate() {
            uniforms.point_pos[i] = l.position.extend(l.range).into();
            uniforms.point_color[i] = l.color.extend(0.0).into();
        }
        self.queue.write_buffer(&res.scene_buffer, 0, bytemuck::bytes_of(&uniforms));

        // Each draw's joint palette offset (0 = unskinned).
        let mut joint_slot: Vec<u32> = Vec::new();
        // Per-draw uniforms (grow the buffer if this frame has more).
        let count = scene.draws.len() as u64;
        {
            // Joint palettes: one slot per distinct skeleton this frame.
            let mut palettes: Vec<&Rc<Vec<Mat4>>> = Vec::new();
            joint_slot.clear();
            for d in &scene.draws {
                let slot = d.joints.as_ref().map_or(0, |j| match palettes.iter().position(|p| Rc::ptr_eq(p, j)) {
                    Some(i) => i,
                    None => {
                        palettes.push(j);
                        palettes.len() - 1
                    }
                });
                joint_slot.push((slot as u64 * JOINT_SLOT) as u32);
            }
            let mut joints = res.joints.borrow_mut();
            let mut draws = res.draws.borrow_mut();
            let mut rebind = false;
            if palettes.len() as u64 > joints.0 {
                *joints = joint_buffer(&self.device, (palettes.len() as u64).next_power_of_two());
                rebind = true;
            }
            if !palettes.is_empty() {
                let mut bytes = vec![0u8; palettes.len() * JOINT_SLOT as usize];
                for (i, p) in palettes.iter().enumerate() {
                    for (k, m) in p.iter().take(MAX_JOINTS).enumerate() {
                        let at = i * JOINT_SLOT as usize + k * 64;
                        bytes[at..at + 64].copy_from_slice(bytemuck::cast_slice(&m.to_cols_array()));
                    }
                }
                self.queue.write_buffer(&joints.1, 0, &bytes);
            }
            if count > draws.0 {
                *draws = draw_buffer(&self.device, &res.draw_layout, count.next_power_of_two(), &joints.1);
            } else if rebind {
                draws.2 = draw_bind_group(&self.device, &res.draw_layout, &draws.1, &joints.1);
            }
            let mut bytes = vec![0u8; (count * DRAW_STRIDE) as usize];
            for (i, d) in scene.draws.iter().enumerate() {
                let u = DrawUniforms3D {
                    model: d.model.to_cols_array_2d(),
                    normal_matrix: d.model.inverse().transpose().to_cols_array_2d(),
                    color: d.color,
                    params: [d.shininess, if d.unlit || d.overlay { 1.0 } else { 0.0 }, d.outline, 0.0],
                    material: d.params,
                };
                let at = i * DRAW_STRIDE as usize;
                bytes[at..at + std::mem::size_of::<DrawUniforms3D>()].copy_from_slice(bytemuck::bytes_of(&u));
            }
            if !bytes.is_empty() {
                self.queue.write_buffer(&draws.1, 0, &bytes);
            }
        }

        // Depth buffer at the target's size.
        {
            let mut depth = res.depth.borrow_mut();
            if depth.as_ref().is_none_or(|(w, h, _)| *w != width || *h != height) {
                let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("an1mbyt3 3D depth"),
                    size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: DEPTH_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                });
                *depth = Some((width, height, texture.create_view(&Default::default())));
            }
        }
        let depth = res.depth.borrow();
        let depth_view = &depth.as_ref().expect("created above").2;
        let draws = res.draws.borrow();

        let load = match scene.clear_color {
            Some(c) => wgpu::LoadOp::Clear(super::linear_clear(wgpu::Color { r: c[0] as f64, g: c[1] as f64, b: c[2] as f64, a: c[3] as f64 })),
            None => load,
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("an1mbyt3 3D pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target, resolve_target: None, ops: wgpu::Operations { load, store: wgpu::StoreOp::Store }, depth_slice: None })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                // Kept when fluid layers depth-test against it afterwards.
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: if scene.fluids.is_empty() { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store } }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        // Particle quads, farthest first, into this frame's vertex buffer.
        let mut quads: Vec<&ParticleQuad3D> = scene.particles.iter().collect();
        quads.sort_by(|a, b| b.depth.total_cmp(&a.depth));
        if !quads.is_empty() {
            let needed = quads.len() as u64 * 6;
            let mut buf = res.particle_vertices.borrow_mut();
            if needed > buf.0 {
                *buf = particle_buffer(&self.device, needed.next_power_of_two());
            }
            let mut data: Vec<f32> = Vec::with_capacity(quads.len() * 6 * PARTICLE_FLOATS);
            for q in &quads {
                for k in [0usize, 1, 2, 0, 2, 3] {
                    data.extend_from_slice(&q.corners[k]);
                    data.extend_from_slice(&q.colors[k]);
                    data.extend_from_slice(&q.uvs[k]);
                    data.push(q.hardness);
                }
            }
            self.queue.write_buffer(&buf.1, 0, bytemuck::cast_slice(&data));
        }
        let particle_buf = res.particle_vertices.borrow();

        let mut bound: *const wgpu::RenderPipeline = std::ptr::null();
        let draw_mesh = |pass: &mut wgpu::RenderPass, i: usize, d: &MeshDraw3D, bound: &mut *const wgpu::RenderPipeline| {
            // Skinned on the GPU only with both a skeleton and the mesh's
            // joint/weight buffer.
            let skinned = d.joints.is_some() && d.mesh.skin_buffer.is_some();
            let outline = d.outline > 0.0;
            let pipeline: &wgpu::RenderPipeline = match (&d.material, d.overlay) {
                _ if outline && skinned => &res.outline_pipeline,
                _ if outline => &res.outline_static_pipeline,
                (_, true) => &res.overlay_pipeline,
                (Some(m), false) => m.for_blend(d.blend, d.double_sided, skinned),
                (None, false) if skinned => &res.skinned_pipeline,
                (None, false) => match d.blend {
                    Blend3D::Opaque => &res.pipeline,
                    Blend3D::Transparent => &res.transparent_pipeline,
                    Blend3D::Additive => &res.additive_pipeline,
                },
            };
            if !std::ptr::eq(*bound, pipeline) {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &res.scene_bind_group, &[]);
                if d.material.is_some() && !d.overlay && !outline {
                    pass.set_bind_group(3, &materials.globals_bind_group, &[]);
                }
                *bound = pipeline;
            }
            if d.material.is_some() && !d.overlay && !outline {
                pass.set_bind_group(2, &d.texture.as_ref().unwrap_or(&materials.white).bind_group, &[]);
            }
            pass.set_bind_group(1, &draws.2, &[(i as u64 * DRAW_STRIDE) as u32, joint_slot.get(i).copied().unwrap_or(0)]);
            pass.set_vertex_buffer(0, d.mesh.vertex_buffer.slice(..));
            if skinned {
                if let Some(skin) = &d.mesh.skin_buffer {
                    pass.set_vertex_buffer(1, skin.slice(..));
                }
            }
            pass.set_index_buffer(d.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..d.mesh.index_count, 0, 0..1);
        };
        let draw_particles = |pass: &mut wgpu::RenderPass, range: std::ops::Range<usize>, bound: &mut *const wgpu::RenderPipeline| {
            if range.is_empty() {
                return;
            }
            pass.set_pipeline(&res.particle_pipeline);
            pass.set_bind_group(0, &res.scene_bind_group, &[]);
            *bound = &res.particle_pipeline;
            pass.set_vertex_buffer(0, particle_buf.1.slice(..));
            pass.draw((range.start * 6) as u32..(range.end * 6) as u32, 0..1);
        };

        // 1. Opaque meshes (depth-writing).
        for (i, d) in scene.draws.iter().enumerate() {
            if !d.overlay && d.blend == Blend3D::Opaque && d.mesh.index_count > 0 {
                draw_mesh(&mut pass, i, d, &mut bound);
            }
        }
        // 2. See-through meshes and particles together, farthest first:
        //    particles behind a glass sphere draw before it (tinted by it),
        //    ones in front draw after.
        let mut see_through: Vec<(usize, f32)> = scene
            .draws
            .iter()
            .enumerate()
            .filter(|(_, d)| !d.overlay && d.blend != Blend3D::Opaque && d.mesh.index_count > 0)
            .map(|(i, d)| (i, d.model.transform_point3(Vec3::ZERO).distance(scene.camera_position)))
            .collect();
        see_through.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut next_quad = 0usize;
        for (i, dist) in see_through {
            let start = next_quad;
            while next_quad < quads.len() && quads[next_quad].depth > dist {
                next_quad += 1;
            }
            draw_particles(&mut pass, start..next_quad, &mut bound);
            draw_mesh(&mut pass, i, &scene.draws[i], &mut bound);
        }
        draw_particles(&mut pass, next_quad..quads.len(), &mut bound);
        // 3. Overlays (gizmos), always on top.
        for (i, d) in scene.draws.iter().enumerate() {
            if d.overlay && d.mesh.index_count > 0 {
                draw_mesh(&mut pass, i, d, &mut bound);
            }
        }
        drop(pass);

        // 4. Fluid layers: splat density, then composite as one surface.
        let fluids: Vec<&FluidLayer3D> = scene.fluids.iter().filter(|l| !l.quads.is_empty()).collect();
        if !fluids.is_empty() {
            // Every layer's vertices in one buffer, and each layer's uniforms.
            let mut data: Vec<f32> = Vec::new();
            let mut ranges = Vec::with_capacity(fluids.len());
            for layer in &fluids {
                let start = data.len() / PARTICLE_FLOATS;
                let refs: Vec<&ParticleQuad3D> = layer.quads.iter().collect();
                quad_vertices(&refs, &mut data);
                ranges.push(start as u32..(data.len() / PARTICLE_FLOATS) as u32);
            }
            {
                let mut buf = res.fluid_vertices.borrow_mut();
                let needed = (data.len() / PARTICLE_FLOATS) as u64;
                if needed > buf.0 {
                    *buf = particle_buffer(&self.device, needed.next_power_of_two());
                }
                self.queue.write_buffer(&buf.1, 0, bytemuck::cast_slice(&data));
                let mut uniforms = res.fluid_uniforms.borrow_mut();
                if fluids.len() as u64 > uniforms.0 {
                    *uniforms = fluid_uniform_buffer(&self.device, (fluids.len() as u64).next_power_of_two());
                }
                let mut bytes = vec![0u8; (fluids.len() as u64 * FLUID_STRIDE) as usize];
                for (i, layer) in fluids.iter().enumerate() {
                    let values: [f32; 16] = [
                        layer.body[0], layer.body[1], layer.body[2], layer.body[3],
                        layer.edge[0], layer.edge[1], layer.edge[2], layer.edge[3],
                        layer.threshold, layer.threshold * 0.25, layer.gloss, 1.0 / width.max(1) as f32,
                        1.0 / height.max(1) as f32, 0.0, 0.0, 0.0,
                    ];
                    let at = i * FLUID_STRIDE as usize;
                    bytes[at..at + 64].copy_from_slice(bytemuck::cast_slice(&values));
                }
                self.queue.write_buffer(&uniforms.1, 0, &bytes);
            }
            // The density target, at the target's size (and its bind group).
            {
                let capacity = res.fluid_uniforms.borrow().0;
                let mut target_ref = res.fluid_target.borrow_mut();
                if target_ref.as_ref().is_none_or(|(w, h, _, _, cap)| *w != width || *h != height || *cap != capacity) {
                    let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("an1mbyt3 3D fluid density"),
                        size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: DENSITY_FORMAT,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    });
                    let view = texture.create_view(&Default::default());
                    let uniforms = res.fluid_uniforms.borrow();
                    let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("an1mbyt3 3D fluid composite bind group"),
                        layout: &res.composite_layout,
                        entries: &[
                            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&res.fluid_sampler) },
                            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &uniforms.1, offset: 0, size: wgpu::BufferSize::new(64) }) },
                        ],
                    });
                    *target_ref = Some((width, height, view, bind_group, capacity));
                }
            }
            let target_ref = res.fluid_target.borrow();
            let (_, _, density_view, composite_group, _) = target_ref.as_ref().expect("created above");
            let fluid_buf = res.fluid_vertices.borrow();
            for (i, range) in ranges.into_iter().enumerate() {
                {
                    let mut splat = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("an1mbyt3 3D fluid density"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: density_view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store }, depth_slice: None })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: depth_view,
                            depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    splat.set_pipeline(&res.density_pipeline);
                    splat.set_bind_group(0, &res.scene_bind_group, &[]);
                    splat.set_vertex_buffer(0, fluid_buf.1.slice(..));
                    splat.draw(range, 0..1);
                }
                let mut composite = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("an1mbyt3 3D fluid composite"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }, depth_slice: None })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                composite.set_pipeline(&res.composite_pipeline);
                composite.set_bind_group(0, composite_group, &[(i as u64 * FLUID_STRIDE) as u32]);
                composite.draw(0..3, 0..1);
            }
        }
        true
    }
}
