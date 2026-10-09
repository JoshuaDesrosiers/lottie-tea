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
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SHADER_SOURCE: &str = include_str!("three_d.wgsl");

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

/// A 3D mesh on the GPU (interleaved position / normal / UV).
pub struct GpuMesh3D {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

/// A shader-graph material compiled for the 3D pass — one pipeline per
/// way a mesh can be blended (`Blend3D`).
pub struct GpuMaterial3D {
    pipeline: wgpu::RenderPipeline,
    transparent: wgpu::RenderPipeline,
    additive: wgpu::RenderPipeline,
}

impl GpuMaterial3D {
    fn for_blend(&self, blend: Blend3D) -> &wgpu::RenderPipeline {
        match blend {
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
/// `corners` go round the quad (UV `(0,0) (1,0) (1,1) (0,1)`); the shader
/// draws a soft round glow across it. `color` is **premultiplied** linear
/// RGBA: `rgb` is how much light it adds, `a` how much of what's behind it
/// it hides — an additive particle is just `a = 0`, a smoky one `a ≈ rgb`
/// weight, so both blend modes share one pipeline and one sorted pass.
#[derive(Debug, Clone, Copy)]
pub struct ParticleQuad3D {
    pub corners: [[f32; 3]; 4],
    pub color: [f32; 4],
    /// Distance from the camera (sorting: farthest drawn first).
    pub depth: f32,
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
}

impl MeshDraw3D {
    pub fn new(mesh: Rc<GpuMesh3D>, model: Mat4, color: [f32; 4]) -> Self {
        Self { mesh, model, color, shininess: 0.3, unlit: false, material: None, texture: None, overlay: false, blend: Blend3D::Opaque }
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
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DrawUniforms3D {
    model: [[f32; 4]; 4],
    normal_matrix: [[f32; 4]; 4],
    color: [f32; 4],
    params: [f32; 4],
}

/// GPU state for the 3D pass, built on first use.
pub(crate) struct Resources3D {
    pipeline: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    transparent_pipeline: wgpu::RenderPipeline,
    additive_pipeline: wgpu::RenderPipeline,
    particle_pipeline: wgpu::RenderPipeline,
    /// (capacity in vertices, buffer) for this frame's particle quads.
    particle_vertices: RefCell<(u64, wgpu::Buffer)>,
    material_layout: wgpu::PipelineLayout,
    scene_buffer: wgpu::Buffer,
    scene_bind_group: wgpu::BindGroup,
    draw_layout: wgpu::BindGroupLayout,
    /// (capacity in draws, buffer, bind group) — regrown when a frame has more.
    draws: RefCell<(u64, wgpu::Buffer, wgpu::BindGroup)>,
    /// (width, height, view) of the current depth buffer.
    depth: RefCell<Option<(u32, u32, wgpu::TextureView)>>,
}

/// The renderer's 3D state (a field on `Renderer`).
#[derive(Default)]
pub(crate) struct State3D {
    resources: OnceCell<Resources3D>,
    scene: RefCell<Option<Scene3D>>,
}

fn draw_buffer(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, capacity: u64) -> (u64, wgpu::Buffer, wgpu::BindGroup) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("an1mbyt3 3D draw uniforms"),
        size: capacity * DRAW_STRIDE,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("an1mbyt3 3D draw bind group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &buffer, offset: 0, size: wgpu::BufferSize::new(std::mem::size_of::<DrawUniforms3D>() as u64) }),
        }],
    });
    (capacity, buffer, bind_group)
}

/// Which kind of mesh pipeline to build.
#[derive(Clone, Copy, PartialEq)]
enum MeshPass {
    Opaque,
    /// Editor gizmos: always on top.
    Overlay,
    Transparent,
    Additive,
}

/// `(src, dst)` additive light: `rgb * a` added, alpha untouched.
const ADDITIVE_BLEND: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
    alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
};

/// A mesh pipeline: `entry` fragment stage, for one `MeshPass`.
fn mesh_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat, layout: &wgpu::PipelineLayout, module: &wgpu::ShaderModule, entry: &str, pass: MeshPass) -> wgpu::RenderPipeline {
    let overlay = pass == MeshPass::Overlay;
    // See-through and glowing surfaces test depth (opaque things hide
    // them) but don't write it (they never hide each other or particles).
    let writes_depth = pass == MeshPass::Opaque;
    let blend = if pass == MeshPass::Additive { ADDITIVE_BLEND } else { wgpu::BlendState::ALPHA_BLENDING };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(match pass {
            MeshPass::Opaque => "an1mbyt3 3D mesh pipeline",
            MeshPass::Overlay => "an1mbyt3 3D overlay pipeline",
            MeshPass::Transparent => "an1mbyt3 3D transparent pipeline",
            MeshPass::Additive => "an1mbyt3 3D additive pipeline",
        }),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: 32,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
            })],
        },
        primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
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

/// Floats per particle vertex: position xyz, premultiplied colour rgba, uv.
const PARTICLE_FLOATS: usize = 9;

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
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32x2],
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
            let draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("an1mbyt3 3D draw layout"), entries: &[uniform_entry(true)] });
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
            // Particles only read the scene uniforms (group 0).
            let particle_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("an1mbyt3 3D particle pipeline layout"),
                bind_group_layouts: &[Some(&scene_layout)],
                immediate_size: 0,
            });
            let particle_pipeline = particle_pipeline(device, self.render_format, &particle_layout, &shader);
            let particle_vertices = RefCell::new(particle_buffer(device, 6 * 256));
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
            let draws = RefCell::new(draw_buffer(device, &draw_layout, 64));
            Resources3D { pipeline, overlay_pipeline, transparent_pipeline, additive_pipeline, particle_pipeline, particle_vertices, material_layout, scene_buffer, scene_bind_group, draw_layout, draws, depth: RefCell::new(None) }
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
        Ok(GpuMaterial3D { pipeline: build(MeshPass::Opaque), transparent: build(MeshPass::Transparent), additive: build(MeshPass::Additive) })
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
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("an1mbyt3 3D indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        GpuMesh3D { vertex_buffer, index_buffer, index_count: if mesh.indices.is_empty() { 0 } else { indices.len() as u32 } }
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

        // Per-draw uniforms (grow the buffer if this frame has more).
        let count = scene.draws.len() as u64;
        {
            let mut draws = res.draws.borrow_mut();
            if count > draws.0 {
                *draws = draw_buffer(&self.device, &res.draw_layout, count.next_power_of_two());
            }
            let mut bytes = vec![0u8; (count * DRAW_STRIDE) as usize];
            for (i, d) in scene.draws.iter().enumerate() {
                let u = DrawUniforms3D {
                    model: d.model.to_cols_array_2d(),
                    normal_matrix: d.model.inverse().transpose().to_cols_array_2d(),
                    color: d.color,
                    params: [d.shininess, if d.unlit || d.overlay { 1.0 } else { 0.0 }, 0.0, 0.0],
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
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
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
            const UV: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
            for q in &quads {
                for k in [0usize, 1, 2, 0, 2, 3] {
                    data.extend_from_slice(&q.corners[k]);
                    data.extend_from_slice(&q.color);
                    data.extend_from_slice(&UV[k]);
                }
            }
            self.queue.write_buffer(&buf.1, 0, bytemuck::cast_slice(&data));
        }
        let particle_buf = res.particle_vertices.borrow();

        let mut bound: *const wgpu::RenderPipeline = std::ptr::null();
        let draw_mesh = |pass: &mut wgpu::RenderPass, i: usize, d: &MeshDraw3D, bound: &mut *const wgpu::RenderPipeline| {
            let pipeline: &wgpu::RenderPipeline = match (&d.material, d.overlay) {
                (_, true) => &res.overlay_pipeline,
                (Some(m), false) => m.for_blend(d.blend),
                (None, false) => match d.blend {
                    Blend3D::Opaque => &res.pipeline,
                    Blend3D::Transparent => &res.transparent_pipeline,
                    Blend3D::Additive => &res.additive_pipeline,
                },
            };
            if !std::ptr::eq(*bound, pipeline) {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &res.scene_bind_group, &[]);
                if d.material.is_some() && !d.overlay {
                    pass.set_bind_group(3, &materials.globals_bind_group, &[]);
                }
                *bound = pipeline;
            }
            if d.material.is_some() && !d.overlay {
                pass.set_bind_group(2, &d.texture.as_ref().unwrap_or(&materials.white).bind_group, &[]);
            }
            pass.set_bind_group(1, &draws.2, &[(i as u64 * DRAW_STRIDE) as u32]);
            pass.set_vertex_buffer(0, d.mesh.vertex_buffer.slice(..));
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
        true
    }
}
