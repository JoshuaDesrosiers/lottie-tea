//! Unity URP-style 2D lighting, as one extra stage of
//! [`Renderer::render_with_post_effects`] (see `light.wgsl` for the shader
//! side). Only runs when a caller passes a [`Lighting`] — a scene with no
//! lights renders exactly as it always has.
//!
//! Per frame, after the scene has been drawn into `post_scene_view`:
//!
//! 1. The three light maps (Multiply, Additive, Volumetric; `Rgba16Float`
//!    so intensity > 1 survives) are cleared to the Global lights' ambient.
//! 2. Each light, in caller order (already sorted by Light Order):
//!    - its shadow mask is cleared, and if it casts shadows, every caster
//!      edge is extruded away from the light into a quad that writes 1
//!      (the caster's own polygon is then written back to 0 unless it
//!      self-shadows, or written to 1 if it *only* self-shadows);
//!    - its quad is drawn into the Multiply or Additive map (Additive or
//!      AlphaBlend overlap), coverage cut by the blurred mask;
//!    - if volumetric, drawn again into the Volumetric map.
//! 3. `fs_composite` writes `scene * multiply + additive (+ volumetric)`
//!    into `post_lit_view`, and unlit draws (HUD) are drawn on top of that.
//!
//! The post-effect chain then reads `post_lit_view` instead of the raw
//! scene, so effects (and a camera vignette) apply to the lit image.

use glam::{Affine2, Vec2};
use wgpu::util::DeviceExt;

use super::{build_mvp, wgpu_blend_state, BlendMode, Camera, DrawCall, GpuImageTexture, Renderer};
use an1mbyt3_core::Transform2D;

/// Light map format: half-float so stacked/over-bright lights don't clip
/// before the composite multiplies them in.
pub(super) const LIGHT_MAP_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const SHADOW_MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
/// How far shadow volumes are extruded — "to infinity" for any sane scene.
const SHADOW_EXTRUDE: f32 = 50_000.0;
/// `light.wgsl`'s freeform point budget (16 vec4 x 2 points).
pub const MAX_FREEFORM_POINTS: usize = 32;

/// The shape-specific half of a light.
pub enum LightShape<'a> {
    /// A radial light with optional cone, aimed along the light's local +Y.
    /// Angles are *full* cone widths in degrees (360 = point light).
    Spot { inner_radius: f32, outer_radius: f32, inner_angle_degrees: f32, outer_angle_degrees: f32 },
    /// A polygon (light-local), full inside, fading out over `falloff_size`.
    Freeform { points: &'a [Vec2], falloff_size: f32 },
    /// A texture used as the light's shape and tint, `size` light-local.
    Sprite { texture: &'a GpuImageTexture, size: Vec2 },
}

/// One non-Global light, ready to draw.
pub struct LightDraw<'a> {
    pub shape: LightShape<'a>,
    /// The light node's world transform.
    pub transform: Transform2D,
    /// Linear RGB, already multiplied by intensity.
    pub color: [f32; 3],
    /// `true` = Additive blend style (adds light), `false` = Multiply.
    pub additive_blend_style: bool,
    /// `true` = AlphaBlend overlap operation, `false` = Additive overlap.
    pub alpha_blend_overlap: bool,
    /// `true` = Max overlap: per channel, the brighter of this light and
    /// what's already there (overlapping lights merge, never stack).
    /// Takes precedence over `alpha_blend_overlap`.
    pub max_overlap: bool,
    /// Unity's Falloff Strength, 0..=1.
    pub falloff_strength: f32,
    /// `Some` = this light casts shadows from every [`ShadowCasterShape`].
    pub shadows: Option<LightShadows>,
    /// `Some` = also glow "in the air" into the volumetric map.
    pub volumetric: Option<LightVolumetric>,
    /// Crisp brightness steps in the falloff (concentric rings); `0`/`1` =
    /// one solid pool.
    pub falloff_steps: u32,
    /// Blur between the steps: `0` = crisp rings, `1` = smooth falloff.
    pub edge_blur: f32,
    /// Evaluate the light per `pixel_size`-unit block (`0` = per pixel).
    pub pixel_size: f32,
}

#[derive(Clone, Copy)]
pub struct LightShadows {
    /// 0..=1 — how much of this light a caster blocks.
    pub strength: f32,
    /// 0..=1 — edge blur.
    pub softness: f32,
}

#[derive(Clone, Copy)]
pub struct LightVolumetric {
    pub intensity: f32,
    /// 0 = volumetric glow ignores shadows; 1 = fully blocked like the light.
    pub shadow_strength: f32,
}

/// A shadow caster, already in world space.
pub struct ShadowCasterShape {
    /// Closed outline (world space), any winding.
    pub outline: Vec<Vec2>,
    /// Casts a shadow onto what's behind it.
    pub casts: bool,
    /// Its own surface is darkened by its own shape.
    pub self_shadows: bool,
}

/// Everything the lighting stage needs for one frame.
pub struct Lighting<'a> {
    /// Sum of Global lights with Multiply blend style (color x intensity).
    pub ambient_multiply: [f32; 3],
    /// Sum of Global lights with Additive blend style.
    pub ambient_additive: [f32; 3],
    /// Non-Global lights, in draw (Light Order) order.
    pub lights: &'a [LightDraw<'a>],
    pub casters: &'a [ShadowCasterShape],
    /// Draws that skip lighting (`Node::unlit`) — drawn over the lit image.
    pub unlit: &'a [DrawCall<'a>],
}

/// `light.wgsl`'s `LightUniforms`, byte for byte.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct LightUniforms {
    mvp: [[f32; 4]; 4],
    color: [f32; 4],
    shape: [f32; 4],
    spot: [f32; 4],
    shadow: [f32; 4],
    sprite: [f32; 4],
    /// (edge blur, pixel size, falloff steps, unused)
    pixel: [f32; 4],
    points: [[f32; 4]; 16],
}

/// Pipelines/layouts/defaults the lighting stage owns, built once with the
/// renderer.
pub(super) struct LightingResources {
    light_additive_pipeline: wgpu::RenderPipeline,
    light_alpha_pipeline: wgpu::RenderPipeline,
    light_max_pipeline: wgpu::RenderPipeline,
    shadow_on_pipeline: wgpu::RenderPipeline,
    shadow_off_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    light_uniform_layout: wgpu::BindGroupLayout,
    mask_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    mask_sampler: wgpu::Sampler,
    /// Bound as the sprite texture for non-Sprite lights (the layout always
    /// has a group 2).
    white_texture: GpuImageTexture,
    /// GPU buffers reused frame to frame — see [`LightingFrameBuffers`].
    frame: std::cell::RefCell<LightingFrameBuffers>,
}

/// The lighting stage's per-frame GPU data, kept alive and rewritten with
/// `queue.write_buffer` each frame instead of re-created.
///
/// It used to allocate, per light per frame, a vertex buffer for its shadow
/// volumes, another for its quad, and a uniform buffer + bind group per
/// light pass (twice for volumetric lights). `createBuffer` /
/// `createBindGroup` are real JS -> GPU-process round trips on the web, and
/// a big lit scene (Sunset Tactics) spent ~3% of every frame in them.
///
/// Safe to rewrite each frame because `draw_lighting` runs once per command
/// encoder and every write lands in the queue before that encoder's submit;
/// each light pass reads its own uniform slot / vertex range, never one a
/// later light overwrote.
struct LightingFrameBuffers {
    /// World -> clip, shared by every shadow draw this frame.
    shadow_uniform: wgpu::Buffer,
    shadow_bind: wgpu::BindGroup,
    /// One `LightUniforms` buffer + bind group per light pass drawn this
    /// frame (grown on demand, never shrunk).
    light_slots: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    /// Every vertex the frame draws (caster fans, each light's shadow
    /// volumes and quad) in one buffer: `None` until the first lit frame,
    /// regrown to the next power of two when a frame needs more.
    vertices: Option<wgpu::Buffer>,
}

/// The size-dependent lighting targets, rebuilt with the other post targets.
pub(super) struct LightingTargets {
    pub multiply_view: wgpu::TextureView,
    pub additive_view: wgpu::TextureView,
    pub volumetric_view: wgpu::TextureView,
    pub shadow_mask_view: wgpu::TextureView,
    /// Composite output (`config.format`) — the effect chain's source when
    /// lighting is on.
    pub lit_view: wgpu::TextureView,
}

fn create_target(device: &wgpu::Device, label: &str, format: wgpu::TextureFormat, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

pub(super) fn build_lighting_targets(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> LightingTargets {
    LightingTargets {
        multiply_view: create_target(device, "an1mbyt3 light map (multiply)", LIGHT_MAP_FORMAT, width, height),
        additive_view: create_target(device, "an1mbyt3 light map (additive)", LIGHT_MAP_FORMAT, width, height),
        volumetric_view: create_target(device, "an1mbyt3 light map (volumetric)", LIGHT_MAP_FORMAT, width, height),
        shadow_mask_view: create_target(device, "an1mbyt3 shadow mask", SHADOW_MASK_FORMAT, width, height),
        lit_view: create_target(device, "an1mbyt3 lit scene", format, width, height),
    }
}

/// Unity's Falloff Strength (0..=1) -> the shader's falloff exponent:
/// 0 gives a soft curve (exponent 0.25: light lingers), 0.5 a linear fade,
/// 1 a hard one (exponent 4: light drops off quickly near the edge).
fn falloff_exponent(strength: f32) -> f32 {
    (strength.clamp(0.0, 1.0) * 4.0 - 2.0).exp2()
}

fn single_entry_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        }],
    })
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None }
}

/// Builds every lighting pipeline. `image_layout` is the renderer's
/// texture+sampler layout (sprite lights bind a `GpuImageTexture` with it).
pub(super) fn build_lighting_resources(device: &wgpu::Device, queue: &wgpu::Queue, output_format: wgpu::TextureFormat, image_layout: &wgpu::BindGroupLayout) -> LightingResources {
    let shader = device.create_shader_module(wgpu::include_wgsl!("../light.wgsl"));
    let light_uniform_layout = single_entry_layout(device, "an1mbyt3 light uniform layout");
    let mask_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("an1mbyt3 shadow mask layout"),
        entries: &[texture_entry(0), sampler_entry(1)],
    });
    let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("an1mbyt3 light composite layout"),
        entries: &[texture_entry(0), texture_entry(1), texture_entry(2), texture_entry(3), sampler_entry(4)],
    });

    let position_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 0 }],
    };

    let light_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("an1mbyt3 light pipeline layout"),
        bind_group_layouts: &[Some(&light_uniform_layout), Some(&mask_layout), Some(image_layout)],
        immediate_size: 0,
    });
    let light_pipeline = |label: &str, blend: wgpu::BlendState| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&light_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_light"), compilation_options: Default::default(), buffers: &[Some(position_layout.clone())] },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_light"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: LIGHT_MAP_FORMAT, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let light_additive_pipeline = light_pipeline("an1mbyt3 light (additive overlap)", wgpu_blend_state(BlendMode::Add));
    let light_alpha_pipeline = light_pipeline("an1mbyt3 light (alpha-blend overlap)", wgpu::BlendState::ALPHA_BLENDING);
    let max_component = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Max };
    let light_max_pipeline = light_pipeline("an1mbyt3 light (max overlap)", wgpu::BlendState { color: max_component, alpha: max_component });

    let shadow_uniform_layout = single_entry_layout(device, "an1mbyt3 shadow uniform layout");
    let shadow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("an1mbyt3 shadow pipeline layout"),
        bind_group_layouts: &[Some(&shadow_uniform_layout)],
        immediate_size: 0,
    });
    let shadow_pipeline = |label: &str, entry: &str| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&shadow_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_shadow"), compilation_options: Default::default(), buffers: &[Some(position_layout.clone())] },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: SHADOW_MASK_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let shadow_on_pipeline = shadow_pipeline("an1mbyt3 shadow volume", "fs_shadow_on");
    let shadow_off_pipeline = shadow_pipeline("an1mbyt3 shadow caster clear", "fs_shadow_off");

    let composite_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("an1mbyt3 light composite pipeline layout"),
        bind_group_layouts: &[Some(&composite_layout)],
        immediate_size: 0,
    });
    let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("an1mbyt3 light composite"),
        layout: Some(&composite_pipeline_layout),
        vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_fullscreen"), compilation_options: Default::default(), buffers: &[] },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_composite"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format: output_format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    });

    let mask_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("an1mbyt3 shadow mask sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    // 1x1 opaque white, for non-Sprite lights' (unused) sprite slot.
    let white = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("an1mbyt3 white 1x1"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &[255, 255, 255, 255],
    );
    let white_view = white.create_view(&wgpu::TextureViewDescriptor::default());
    let white_texture = GpuImageTexture {
        bind_group: device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("an1mbyt3 white 1x1 bind group"),
            layout: image_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&white_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&mask_sampler) },
            ],
        }),
    };

    let shadow_uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("an1mbyt3 shadow mvp"),
        size: std::mem::size_of::<[[f32; 4]; 4]>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let shadow_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("an1mbyt3 shadow bind group"),
        layout: &shadow_uniform_layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: shadow_uniform.as_entire_binding() }],
    });
    let frame = std::cell::RefCell::new(LightingFrameBuffers { shadow_uniform, shadow_bind, light_slots: Vec::new(), vertices: None });

    LightingResources {
        frame,
        light_additive_pipeline,
        light_alpha_pipeline,
        light_max_pipeline,
        shadow_on_pipeline,
        shadow_off_pipeline,
        composite_pipeline,
        light_uniform_layout,
        mask_layout,
        composite_layout,
        mask_sampler,
        white_texture,
    }
}

/// Fan-triangulates a closed outline (exact for convex outlines, which is
/// every bounds-rectangle caster; a reasonable approximation for the
/// occasional concave Shape outline).
fn fan_triangles(outline: &[Vec2], out: &mut Vec<[f32; 2]>) {
    if outline.len() < 3 {
        return;
    }
    for i in 1..outline.len() - 1 {
        out.push(outline[0].to_array());
        out.push(outline[i].to_array());
        out.push(outline[i + 1].to_array());
    }
}

/// Even-odd point-in-polygon test.
fn contains(outline: &[Vec2], p: Vec2) -> bool {
    let mut inside = false;
    let mut j = outline.len().wrapping_sub(1);
    for i in 0..outline.len() {
        let (a, b) = (outline[i], outline[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Every edge of every casting outline, extruded away from `light_pos`.
/// A caster the light sits *inside* is skipped (as in Unity): otherwise
/// every one of its edges would shadow outward and black out the whole
/// scene around it — e.g. a character's lantern overlapping a house while
/// standing in its doorway.
fn shadow_volume_triangles(casters: &[ShadowCasterShape], light_pos: Vec2) -> Vec<[f32; 2]> {
    let mut tris = Vec::new();
    for caster in casters.iter().filter(|c| c.casts) {
        let n = caster.outline.len();
        if n < 2 || contains(&caster.outline, light_pos) {
            continue;
        }
        for i in 0..n {
            let a = caster.outline[i];
            let b = caster.outline[(i + 1) % n];
            let far = |p: Vec2| {
                let dir = (p - light_pos).normalize_or_zero();
                p + dir * SHADOW_EXTRUDE
            };
            let (fa, fb) = (far(a), far(b));
            tris.extend_from_slice(&[a.to_array(), b.to_array(), fb.to_array(), a.to_array(), fb.to_array(), fa.to_array()]);
        }
    }
    tris
}

impl Renderer {
    /// Runs the lighting stage (see this module's doc comment), reading
    /// `post_scene_view` and writing the lit result into `post_lit_view`.
    pub(super) fn draw_lighting(&self, encoder: &mut wgpu::CommandEncoder, viewport_size: (f32, f32), camera: Camera, lighting: &Lighting<'_>) {
        let res = &self.lighting;
        let targets = &self.lighting_targets;
        let (target_w, target_h) = self.post_target_dims();
        // Shadow softness 1 = a 24px blur radius at native resolution.
        let softness_px = |softness: f32| softness.clamp(0.0, 1.0) * 24.0 / self.post_pixel_scale();

        let clear_pass = |encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, color: [f32; 3]| {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("an1mbyt3 light map clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: color[0] as f64, g: color[1] as f64, b: color[2] as f64, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        };
        clear_pass(encoder, &targets.multiply_view, lighting.ambient_multiply);
        clear_pass(encoder, &targets.additive_view, lighting.ambient_additive);
        clear_pass(encoder, &targets.volumetric_view, [0.0; 3]);

        let mut frame = res.frame.borrow_mut();
        let frame = &mut *frame;
        // World -> clip, shared by every shadow draw this frame.
        self.queue.write_buffer(&frame.shadow_uniform, 0, bytemuck::bytes_of(&build_mvp(Affine2::IDENTITY, viewport_size, camera).to_cols_array_2d()));
        let mask_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("an1mbyt3 shadow mask bind group"),
            layout: &res.mask_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&targets.shadow_mask_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&res.mask_sampler) },
            ],
        });

        // ---- Prepare: every vertex and uniform of the frame, up front ----
        // (so they upload as one vertex write + one write per uniform slot
        // before any pass is encoded).
        //
        // Caster polygons, triangulated once: those that keep their own
        // surface lit (cleared back to 0 after the volumes) and those that
        // shadow themselves (written to 1).
        let mut vertices: Vec<[f32; 2]> = Vec::new();
        let mut push = |data: &[[f32; 2]]| -> std::ops::Range<u32> {
            let start = vertices.len() as u32;
            vertices.extend_from_slice(data);
            start..vertices.len() as u32
        };
        let mut clear_own = Vec::new();
        let mut shade_own = Vec::new();
        for caster in lighting.casters {
            if caster.self_shadows {
                fan_triangles(&caster.outline, &mut shade_own);
            } else {
                fan_triangles(&caster.outline, &mut clear_own);
            }
        }
        let clear_own_range = push(&clear_own);
        let shade_own_range = push(&shade_own);

        /// One light, ready to encode.
        struct PreparedLight<'t> {
            shadows: bool,
            /// Vertex ranges in the frame's shared vertex buffer.
            volumes: std::ops::Range<u32>,
            quad: std::ops::Range<u32>,
            sprite_bind: &'t GpuImageTexture,
            /// (target map, uniforms, overlap: 0 = additive, 1 = alpha
            /// blend, 2 = max) — the light itself, plus its volumetric glow.
            passes: Vec<(&'t wgpu::TextureView, LightUniforms, u8)>,
        }
        let mut prepared: Vec<PreparedLight<'_>> = Vec::with_capacity(lighting.lights.len());
        for light in lighting.lights {
            let affine = light.transform.to_affine2();
            let light_pos = affine.translation;

            let (min, max, kind, sprite_bind) = match &light.shape {
                LightShape::Spot { outer_radius, .. } => {
                    let r = outer_radius.max(1.0);
                    (Vec2::splat(-r), Vec2::splat(r), 1.0, &res.white_texture)
                }
                LightShape::Freeform { points, falloff_size } => {
                    let mut min = Vec2::splat(f32::INFINITY);
                    let mut max = Vec2::splat(f32::NEG_INFINITY);
                    for p in points.iter() {
                        min = min.min(*p);
                        max = max.max(*p);
                    }
                    if !min.is_finite() {
                        continue;
                    }
                    let pad = Vec2::splat(falloff_size.max(0.0) + 1.0);
                    (min - pad, max + pad, 2.0, &res.white_texture)
                }
                LightShape::Sprite { texture, size } => (-*size * 0.5, *size * 0.5, 3.0, *texture),
            };
            let volumes = match light.shadows {
                Some(_) => push(&shadow_volume_triangles(lighting.casters, light_pos)),
                None => 0..0,
            };
            let quad = push(&[[min.x, min.y], [max.x, min.y], [max.x, max.y], [min.x, min.y], [max.x, max.y], [min.x, max.y]]);

            let mut uniforms = LightUniforms {
                mvp: build_mvp(affine, viewport_size, camera).to_cols_array_2d(),
                color: [light.color[0], light.color[1], light.color[2], if light.alpha_blend_overlap && !light.max_overlap { 1.0 } else { 0.0 }],
                shape: [kind, falloff_exponent(light.falloff_strength), 0.0, 0.0],
                spot: [0.0; 4],
                shadow: [
                    light.shadows.map(|s| s.strength.clamp(0.0, 1.0)).unwrap_or(0.0),
                    light.shadows.map(|s| softness_px(s.softness)).unwrap_or(0.0),
                    target_w as f32,
                    target_h as f32,
                ],
                sprite: [(max.x - min.x) * 0.5, (max.y - min.y) * 0.5, 0.0, 0.0],
                pixel: [light.edge_blur.clamp(0.0, 1.0), light.pixel_size.max(0.0), light.falloff_steps as f32, 0.0],
                points: [[0.0; 4]; 16],
            };
            match &light.shape {
                LightShape::Spot { inner_radius, outer_radius, inner_angle_degrees, outer_angle_degrees } => {
                    let half_cos = |deg: f32| (deg.clamp(0.0, 360.0).to_radians() * 0.5).cos();
                    // A full 360 cone has cos(180) = -1: the shader's
                    // "no cone" case.
                    uniforms.spot = [inner_radius.max(0.0), outer_radius.max(0.0), half_cos(*inner_angle_degrees), half_cos(*outer_angle_degrees)];
                }
                LightShape::Freeform { points, falloff_size } => {
                    let count = points.len().min(MAX_FREEFORM_POINTS);
                    for (i, p) in points.iter().take(count).enumerate() {
                        uniforms.points[i / 2][(i % 2) * 2] = p.x;
                        uniforms.points[i / 2][(i % 2) * 2 + 1] = p.y;
                    }
                    uniforms.shape[2] = count as f32;
                    uniforms.shape[3] = *falloff_size;
                }
                LightShape::Sprite { .. } => {}
            }

            let map = if light.additive_blend_style { &targets.additive_view } else { &targets.multiply_view };
            let overlap = if light.max_overlap {
                2
            } else if light.alpha_blend_overlap {
                1
            } else {
                0
            };
            let mut passes = vec![(map, uniforms, overlap)];
            if let Some(volumetric) = light.volumetric {
                let mut glow = uniforms;
                glow.color = [light.color[0] * volumetric.intensity, light.color[1] * volumetric.intensity, light.color[2] * volumetric.intensity, 0.0];
                glow.shadow[0] = if light.shadows.is_some() { volumetric.shadow_strength.clamp(0.0, 1.0) } else { 0.0 };
                passes.push((&targets.volumetric_view, glow, if light.max_overlap { 2 } else { 0 }));
            }
            prepared.push(PreparedLight { shadows: light.shadows.is_some(), volumes, quad, sprite_bind, passes });
        }

        // ---- Upload ----
        let vertex_bytes: &[u8] = bytemuck::cast_slice(&vertices);
        if !vertex_bytes.is_empty() {
            if frame.vertices.as_ref().is_none_or(|b| b.size() < vertex_bytes.len() as u64) {
                frame.vertices = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("an1mbyt3 lighting vertices"),
                    size: (vertex_bytes.len() as u64).next_power_of_two().max(4096),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
            }
            self.queue.write_buffer(frame.vertices.as_ref().expect("just ensured"), 0, vertex_bytes);
        }
        let pass_count: usize = prepared.iter().map(|p| p.passes.len()).sum();
        while frame.light_slots.len() < pass_count {
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("an1mbyt3 light uniforms"),
                size: std::mem::size_of::<LightUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("an1mbyt3 light bind group"),
                layout: &res.light_uniform_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }],
            });
            frame.light_slots.push((buffer, bind));
        }
        let mut slot = 0;
        for light in &prepared {
            for (_, uniforms, _) in &light.passes {
                self.queue.write_buffer(&frame.light_slots[slot].0, 0, bytemuck::bytes_of(uniforms));
                slot += 1;
            }
        }

        // ---- Encode ----
        // `range` (in vertices) as a slice of the shared vertex buffer.
        let vertex_slice = |range: &std::ops::Range<u32>| {
            let stride = std::mem::size_of::<[f32; 2]>() as u64;
            frame.vertices.as_ref().map(|b| b.slice(range.start as u64 * stride..range.end as u64 * stride))
        };
        let mut slot = 0;
        for light in &prepared {
            // ---- Shadow mask ----
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("an1mbyt3 shadow mask"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &targets.shadow_mask_view,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                if light.shadows {
                    pass.set_bind_group(0, &frame.shadow_bind, &[]);
                    for (range, pipeline) in [
                        (&light.volumes, &res.shadow_on_pipeline),
                        (&clear_own_range, &res.shadow_off_pipeline),
                        (&shade_own_range, &res.shadow_on_pipeline),
                    ] {
                        if range.is_empty() {
                            continue;
                        }
                        if let Some(slice) = vertex_slice(range) {
                            pass.set_pipeline(pipeline);
                            pass.set_vertex_buffer(0, slice);
                            pass.draw(0..range.len() as u32, 0..1);
                        }
                    }
                }
            }

            // ---- Light quad (and its volumetric glow) ----
            for (target, _, overlap) in &light.passes {
                let bind = &frame.light_slots[slot].1;
                slot += 1;
                let Some(quad) = vertex_slice(&light.quad) else { continue };
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("an1mbyt3 light"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(match overlap {
                    1 => &res.light_alpha_pipeline,
                    2 => &res.light_max_pipeline,
                    _ => &res.light_additive_pipeline,
                });
                pass.set_bind_group(0, bind, &[]);
                pass.set_bind_group(1, &mask_bind, &[]);
                pass.set_bind_group(2, &light.sprite_bind.bind_group, &[]);
                pass.set_vertex_buffer(0, quad);
                pass.draw(0..6, 0..1);
            }
        }

        // ---- Composite ----
        let composite_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("an1mbyt3 light composite bind group"),
            layout: &res.composite_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.post_scene_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&targets.multiply_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&targets.additive_view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&targets.volumetric_view) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&self.post_sampler) },
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("an1mbyt3 light composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.lit_view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&res.composite_pipeline);
            pass.set_bind_group(0, &composite_bind, &[]);
            pass.draw(0..3, 0..1);
        }

        // ---- Unlit overlay (HUD etc.) ----
        if !lighting.unlit.is_empty() {
            self.draw_masked_scene(encoder, &targets.lit_view, &self.post_stencil_view, wgpu::LoadOp::Load, lighting.unlit, &[], None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falloff_strength_maps_to_soft_linear_hard_exponents() {
        assert!((falloff_exponent(0.0) - 0.25).abs() < 1e-6);
        assert!((falloff_exponent(0.5) - 1.0).abs() < 1e-6);
        assert!((falloff_exponent(1.0) - 4.0).abs() < 1e-6);
    }

    #[test]
    fn each_casting_edge_extrudes_into_two_triangles_away_from_the_light() {
        let caster = ShadowCasterShape { outline: vec![Vec2::new(10.0, -1.0), Vec2::new(10.0, 1.0), Vec2::new(12.0, 1.0)], casts: true, self_shadows: false };
        let tris = shadow_volume_triangles(&[caster], Vec2::ZERO);
        assert_eq!(tris.len(), 3 * 6);
        // Far points sit far out along +X, away from a light at the origin.
        assert!(tris.iter().any(|p| p[0] > 1000.0));
        assert!(tris.iter().all(|p| p[0] >= 9.0));
    }

    #[test]
    fn a_light_inside_a_caster_is_not_shadowed_by_it() {
        let square = vec![Vec2::new(-10.0, -10.0), Vec2::new(10.0, -10.0), Vec2::new(10.0, 10.0), Vec2::new(-10.0, 10.0)];
        let caster = ShadowCasterShape { outline: square.clone(), casts: true, self_shadows: false };
        assert!(shadow_volume_triangles(std::slice::from_ref(&caster), Vec2::new(2.0, 3.0)).is_empty());
        assert!(!shadow_volume_triangles(&[caster], Vec2::new(30.0, 0.0)).is_empty());
    }

    #[test]
    fn non_casting_shapes_produce_no_volumes() {
        let caster = ShadowCasterShape { outline: vec![Vec2::ZERO, Vec2::X, Vec2::Y], casts: false, self_shadows: true };
        assert!(shadow_volume_triangles(&[caster], Vec2::new(5.0, 5.0)).is_empty());
    }
}
