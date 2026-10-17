//! **Layer effects** — a node's [`an1mbyt3_core::ImageEffects`] stack, run
//! on its flattened layer (see `layers.rs`) between drawing the layer and
//! compositing it.
//!
//! Each effect becomes one or more full-screen passes over layer-sized
//! scratch targets (a small pool per nesting level, like the layers' own):
//! most effects are a single pass; Blur is two (horizontal + vertical, with a
//! per-pixel radius for progressive / depth blur); Glow and Drop Shadow are
//! three (prepare + blur H, blur V, composite with the untouched image).
//! The shader is `layer_fx.wgsl` (one mode switch). The **Code** effect gets
//! its own pipeline per distinct source, compiled from the user's function
//! (see [`code_shader_source`] for the accepted dialect) and cached; a source
//! that doesn't compile is skipped and its error kept for the editor.
//!
//! Everything is self-contained — its own uniform buffers and bind groups per
//! pass — so it never contends with the scene's post chain for uniform slots
//! or ping-pong targets.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;

use an1mbyt3_core::image_effects::{AsciiCharset, BlurMode, HalftoneShape, PatternForm, RibShape, StrokePosition};
use an1mbyt3_core::ImageEffect;
use glam::Vec2;

use super::{build_mvp, linear_rgba, Camera, GpuImageTexture, Renderer};

const SHADER: &str = include_str!("layer_fx.wgsl");
/// The part of [`SHADER`] every effect shares (bindings, helpers) — Code
/// effects are compiled as this + the user's function.
fn shader_header() -> &'static str {
    SHADER.split("// ---- Blur").next().unwrap_or(SHADER)
}

/// What the editor attaches to a `SceneOp::Layer` to run effects on it.
pub struct LayerFx<'a> {
    /// The enabled effects, in order.
    pub effects: Vec<&'a ImageEffect>,
    /// The node's (subtree's) world-space bounding box — effect positions
    /// and sizes are relative to it / in its units.
    pub world_min: Vec2,
    pub world_max: Vec2,
    /// A depth image for Blur (Depth) / Depth Map, if the node has one.
    pub depth: Option<&'a GpuImageTexture>,
    /// Background behind the content: colour, image (+ its opacity),
    /// corner radius and padding (node units). See [`LayerFx::with_background`].
    background: [f32; 4],
    background_image: Option<&'a GpuImageTexture>,
    background_image_opacity: f32,
    background_radius: f32,
    background_padding: f32,
    /// Mask the result to the content's own alpha.
    clip: bool,
    /// The box in fractions of the target (x, y, w, h; y down) — filled in
    /// when the frame's uniforms are written (that's where the camera is).
    rect: Cell<[f32; 4]>,
    /// The pixels the effects can touch (box + their spread), in target
    /// pixels `x, y, w, h` — every pass and the composite are scissored to it.
    scissor: Cell<Option<[u32; 4]>>,
}

impl<'a> LayerFx<'a> {
    pub fn new(effects: Vec<&'a ImageEffect>, world_min: Vec2, world_max: Vec2, depth: Option<&'a GpuImageTexture>) -> Self {
        Self {
            effects,
            world_min,
            world_max,
            depth,
            background: [0.0; 4],
            background_image: None,
            background_image_opacity: 0.0,
            background_radius: 0.0,
            background_padding: 0.0,
            clip: false,
            rect: Cell::new([0.0, 0.0, 1.0, 1.0]),
            scissor: Cell::new(None),
        }
    }

    /// Adds a background (colour, image + opacity, corner radius and padding
    /// in node units) drawn behind the content before the effects.
    pub fn with_background(mut self, color: [f32; 4], image: Option<&'a GpuImageTexture>, image_opacity: f32, radius: f32, padding: f32) -> Self {
        self.background = color;
        self.background_image = image;
        self.background_image_opacity = image_opacity;
        self.background_radius = radius;
        self.background_padding = padding;
        self
    }

    /// Masks the final image to the content's own alpha.
    pub fn with_clip(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }

    fn has_background(&self) -> bool {
        self.background[3] > 0.0 || (self.background_image.is_some() && self.background_image_opacity > 0.0)
    }

    /// The scissor box from the last run (`None` = whole target).
    pub(super) fn scissor(&self) -> Option<[u32; 4]> {
        self.scissor.get()
    }

    /// Projects the world box through the camera (called with the frame's
    /// uniforms).
    pub(super) fn update_rect(&self, viewport_size: (f32, f32), camera: Camera) {
        let mvp = build_mvp(glam::Affine2::IDENTITY, viewport_size, camera);
        let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
        for corner in [self.world_min, Vec2::new(self.world_max.x, self.world_min.y), self.world_max, Vec2::new(self.world_min.x, self.world_max.y)] {
            let ndc = mvp * glam::Vec4::new(corner.x, corner.y, 0.0, 1.0);
            let f = Vec2::new((ndc.x + 1.0) * 0.5, (1.0 - ndc.y) * 0.5);
            lo = lo.min(f);
            hi = hi.max(f);
        }
        self.rect.set([lo.x, lo.y, (hi.x - lo.x).max(1e-6), (hi.y - lo.y).max(1e-6)]);
    }
}

/// Per-pass uniforms — `Fx` in `layer_fx.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FxUniform {
    a: [f32; 4],
    rect: [f32; 4],
    k: [f32; 4],
    p: [[f32; 4]; 6],
    c: [[f32; 4]; 8],
}

const MODE_COPY: f32 = 0.0;
const MODE_BLUR_H: f32 = 1.0;
const MODE_BLUR_V: f32 = 2.0;
const MODE_GLOW_COMPOSITE: f32 = 3.0;
const MODE_SHADOW_COMPOSITE: f32 = 4.0;
const MODE_STROKE: f32 = 5.0;
const MODE_PATTERN: f32 = 6.0;
const MODE_HALFTONE: f32 = 7.0;
const MODE_ASCII: f32 = 8.0;
const MODE_GRAIN: f32 = 9.0;
const MODE_GRADIENT_MAP: f32 = 10.0;
const MODE_DEPTH_MAP: f32 = 11.0;
const MODE_RIBBED: f32 = 12.0;
const MODE_SCANNER: f32 = 13.0;
const MODE_BACKGROUND: f32 = 14.0;
const MODE_CLIP: f32 = 15.0;

/// A pass's input: the layer itself or scratch target `n`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Src {
    Layer,
    T(usize),
}

struct Pass {
    u: FxUniform,
    a: Src,
    b: Src,
    dst: usize,
    /// `Some(source)` = a Code effect's own pipeline.
    code: Option<String>,
    /// Bind the background image (not the depth image) as the aux texture.
    aux_background: bool,
}

pub(super) struct FxResources {
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    white: GpuImageTexture,
}

#[derive(Default)]
pub(super) struct LayerFxState {
    resources: OnceCell<FxResources>,
    /// Scratch targets per nesting level: 4 views at one size.
    targets: RefCell<Vec<(Vec<wgpu::TextureView>, (u32, u32))>>,
    /// Code effects by source: the pipeline, or why it didn't compile.
    code: RefCell<HashMap<String, Result<wgpu::RenderPipeline, String>>>,
    /// Uniform buffers reused frame to frame (one per pass in a frame).
    buffers: RefCell<Vec<wgpu::Buffer>>,
    next_buffer: Cell<usize>,
}

const SCRATCH: usize = 4;

impl Renderer {
    /// Start of a frame: the pooled uniform buffers are free again.
    pub(super) fn layer_fx_new_frame(&self) {
        self.layer_fx.next_buffer.set(0);
    }

    /// A uniform buffer for one pass this frame (pooled, never the same one
    /// twice in a frame — writes land before the frame's submit).
    fn fx_buffer(&self) -> wgpu::Buffer {
        let i = self.layer_fx.next_buffer.get();
        self.layer_fx.next_buffer.set(i + 1);
        let mut pool = self.layer_fx.buffers.borrow_mut();
        while pool.len() <= i {
            pool.push(self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("an1mbyt3 layer fx"), size: std::mem::size_of::<FxUniform>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }));
        }
        pool[i].clone()
    }

    fn fx_resources(&self) -> &FxResources {
        self.layer_fx.resources.get_or_init(|| {
            let device = &self.device;
            let tex = |binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                count: None,
            };
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("an1mbyt3 layer fx"),
                entries: &[
                    tex(0),
                    tex(1),
                    wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                        count: None,
                    },
                ],
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("an1mbyt3 layer fx"), bind_group_layouts: &[Some(&layout), Some(&self.image_bind_group_layout)], immediate_size: 0 });
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("an1mbyt3 layer fx"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
            let pipeline = self.fx_pipeline(&pipeline_layout, &module, "fs_main");
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("an1mbyt3 layer fx"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let white = self.upload_texture(1, 1, &[255, 255, 255, 255], an1mbyt3_core::ImageFilterMode::Linear);
            FxResources { layout, pipeline_layout, pipeline, sampler, white }
        })
    }

    fn fx_pipeline(&self, layout: &wgpu::PipelineLayout, module: &wgpu::ShaderModule, entry: &str) -> wgpu::RenderPipeline {
        self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("an1mbyt3 layer fx"),
            layout: Some(layout),
            vertex: wgpu::VertexState { module, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &[] },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: self.render_format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        })
    }

    /// The scratch views for nesting `level`, at `size`.
    fn fx_targets(&self, level: usize, size: (u32, u32)) -> Vec<wgpu::TextureView> {
        let mut targets = self.layer_fx.targets.borrow_mut();
        while targets.len() <= level {
            targets.push((Vec::new(), (0, 0)));
        }
        if targets[level].1 != size || targets[level].0.len() != SCRATCH {
            let views = (0..SCRATCH)
                .map(|_| {
                    self.device
                        .create_texture(&wgpu::TextureDescriptor {
                            label: Some("an1mbyt3 layer fx scratch"),
                            size: wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: self.render_format,
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                            view_formats: &[],
                        })
                        .create_view(&wgpu::TextureViewDescriptor::default())
                })
                .collect();
            targets[level] = (views, size);
        }
        targets[level].0.clone()
    }

    /// The Code effect `code`'s pipeline (compiling and caching it), or why
    /// it doesn't compile.
    fn fx_code_pipeline(&self, code: &str) -> Result<wgpu::RenderPipeline, String> {
        if let Some(entry) = self.layer_fx.code.borrow().get(code) {
            return entry.clone();
        }
        let res = self.fx_resources();
        let result = (|| {
            let source = code_shader_source(code)?;
            let module = naga::front::wgsl::parse_str(&source).map_err(|e| e.emit_to_string(&source))?;
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
                .validate(&module)
                .map_err(|e| e.emit_to_string(&source))?;
            let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("an1mbyt3 code effect"), source: wgpu::ShaderSource::Wgsl(source.into()) });
            Ok(self.fx_pipeline(&res.pipeline_layout, &module, "fs_code"))
        })();
        self.layer_fx.code.borrow_mut().insert(code.to_string(), result.clone());
        result
    }

    /// Why a Code effect's source doesn't compile (`None` = it does).
    pub fn code_effect_error(&self, code: &str) -> Option<String> {
        self.fx_code_pipeline(code).err()
    }

    /// Runs `fx` on `layer` (the layer just drawn, `size`, nesting `level`)
    /// and returns the view holding the result.
    pub(super) fn run_layer_fx(&self, encoder: &mut wgpu::CommandEncoder, level: usize, size: (u32, u32), layer: &wgpu::TextureView, fx: &LayerFx<'_>) -> wgpu::TextureView {
        let (passes, scissor) = plan_passes(fx, size, self.shader_time.get(), &|code| self.fx_code_pipeline(code).is_ok());
        fx.scissor.set(scissor);
        if passes.is_empty() {
            return layer.clone();
        }
        let res = self.fx_resources();
        let targets = self.fx_targets(level, size);
        let depth_aux = fx.depth.map(|t| &t.bind_group).unwrap_or(&res.white.bind_group);
        let background_aux = fx.background_image.map(|t| &t.bind_group).unwrap_or(&res.white.bind_group);
        let view = |s: Src| match s {
            Src::Layer => layer,
            Src::T(i) => &targets[i],
        };
        let mut last = Src::Layer;
        for pass in &passes {
            let pipeline = match &pass.code {
                Some(code) => match self.fx_code_pipeline(code) {
                    Ok(p) => p,
                    Err(_) => continue,
                },
                None => res.pipeline.clone(),
            };
            let a = pass.a;
            let buffer = self.fx_buffer();
            self.queue.write_buffer(&buffer, 0, bytemuck::bytes_of(&pass.u));
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("an1mbyt3 layer fx"),
                layout: &res.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view(a)) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view(pass.b)) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&res.sampler) },
                    wgpu::BindGroupEntry { binding: 3, resource: buffer.as_entire_binding() },
                ],
            });
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("an1mbyt3 layer fx"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &targets[pass.dst], resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(super::TRANSPARENT), store: wgpu::StoreOp::Store }, depth_slice: None })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some([x, y, w, h]) = scissor {
                rp.set_scissor_rect(x, y, w, h);
            }
            rp.set_pipeline(&pipeline);
            rp.set_bind_group(0, &bind, &[]);
            rp.set_bind_group(1, if pass.aux_background { background_aux } else { depth_aux }, &[]);
            rp.draw(0..3, 0..1);
            drop(rp);
            last = Src::T(pass.dst);
        }
        view(last).clone()
    }
}

/// Builds the pass list for `fx` at target `size` and time `time`. Code
/// effects whose source doesn't compile (`code_ok` false) are left out —
/// the editor shows why.
fn plan_passes(fx: &LayerFx<'_>, size: (u32, u32), time: f32, code_ok: &dyn Fn(&str) -> bool) -> (Vec<Pass>, Option<[u32; 4]>) {
    let [fx_x, fx_y, fx_w, fx_h] = fx.rect.get();
    let (w, h) = (size.0 as f32, size.1 as f32);
    let rect = [fx_x * w, fx_y * h, fx_w * w, fx_h * h];
    let world_w = (fx.world_max.x - fx.world_min.x).abs().max(1e-4);
    let ppu = rect[2] / world_w;
    let base = |mode: f32, amount: f32, variant: f32, aux: bool| FxUniform {
        a: [mode, time, w, h],
        rect,
        k: [ppu, amount, variant, if aux { 1.0 } else { 0.0 }],
        p: [[0.0; 4]; 6],
        c: [[0.0; 4]; 8],
    };
    let has_depth = fx.depth.is_some();
    // How far past the node's box each effect can reach (target px).
    let extent = rect[2].max(rect[3]);
    let margin: f32 = 3.0
        + if fx.has_background() { fx.background_padding.max(0.0) * ppu } else { 0.0 }
        + fx
            .effects
            .iter()
            .map(|e| match e {
                ImageEffect::DropShadow(e) => (e.offset[0].abs().max(e.offset[1].abs()) + e.blur + e.spread) * ppu * 1.3,
                ImageEffect::Stroke(e) => e.width * ppu + 2.0,
                ImageEffect::Glow(e) => e.radius * ppu * 1.3,
                ImageEffect::Blur(e) => e.radius * ppu * 1.3,
                ImageEffect::RibbedGlass(e) => e.size * ppu * 0.5,
                ImageEffect::ScannerDrag(e) => e.intensity.abs() * extent * 1.1 + 4.0,
                ImageEffect::Code(_) => extent * 0.15,
                _ => 0.0,
            })
            .sum::<f32>();
    let scissor = {
        let x0 = (rect[0] - margin).floor().clamp(0.0, w);
        let y0 = (rect[1] - margin).floor().clamp(0.0, h);
        let x1 = (rect[0] + rect[2] + margin).ceil().clamp(0.0, w);
        let y1 = (rect[1] + rect[3] + margin).ceil().clamp(0.0, h);
        if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
            return (Vec::new(), Some([0, 0, 0, 0]));
        }
        Some([x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32])
    };

    let mut passes = Vec::new();
    let mut cur = Src::Layer;
    // A scratch target that's none of `avoid`.
    let pick = |avoid: &[Src]| (0..SCRATCH).find(|i| !avoid.contains(&Src::T(*i))).unwrap_or(0);
    let single = |passes: &mut Vec<Pass>, cur: &mut Src, u: FxUniform, code: Option<String>| {
        let dst = pick(&[*cur]);
        passes.push(Pass { u, a: *cur, b: *cur, dst, code, aux_background: false });
        *cur = Src::T(dst);
    };

    // The background goes in first, behind the content.
    if fx.has_background() {
        let mut u = base(MODE_BACKGROUND, 1.0, 0.0, fx.background_image.is_some());
        u.p[0] = [fx.background_radius.max(0.0) * ppu, fx.background_padding * ppu, fx.background_image_opacity.clamp(0.0, 1.0), 0.0];
        u.c[0] = linear_rgba(fx.background);
        let dst = pick(&[cur]);
        passes.push(Pass { u, a: cur, b: cur, dst, code: None, aux_background: true });
        cur = Src::T(dst);
    }

    for effect in &fx.effects {
        match effect {
            ImageEffect::DropShadow(e) => {
                let mut u = base(MODE_BLUR_H, 1.0, 2.0, has_depth);
                u.p[0] = [e.blur * ppu * 0.5, 0.0, 0.0, 0.0];
                // Offset is in node units, y up in the world → down on screen.
                u.p[2] = [0.0, e.offset[0] * ppu, -e.offset[1] * ppu, e.spread * ppu];
                let t1 = pick(&[cur]);
                passes.push(Pass { u, a: cur, b: cur, dst: t1, code: None, aux_background: false });
                let mut v = base(MODE_BLUR_V, 1.0, 0.0, has_depth);
                v.p[0] = u.p[0];
                let t2 = pick(&[cur, Src::T(t1)]);
                passes.push(Pass { u: v, a: Src::T(t1), b: Src::T(t1), dst: t2, code: None, aux_background: false });
                let mut c = base(MODE_SHADOW_COMPOSITE, 1.0, 0.0, has_depth);
                c.c[0] = linear_rgba(e.color);
                let dst = pick(&[cur, Src::T(t2)]);
                passes.push(Pass { u: c, a: cur, b: Src::T(t2), dst, code: None, aux_background: false });
                cur = Src::T(dst);
            }
            ImageEffect::Stroke(e) => {
                let mut u = base(MODE_STROKE, 1.0, 0.0, has_depth);
                let pos = match e.position {
                    StrokePosition::Outside => 0.0,
                    StrokePosition::Center => 1.0,
                    StrokePosition::Inside => 2.0,
                };
                u.p[0] = [e.width * ppu, pos, 0.0, 0.0];
                u.c[0] = linear_rgba(e.color);
                single(&mut passes, &mut cur, u, None);
            }
            ImageEffect::Pattern(e) => {
                let mut u = base(MODE_PATTERN, e.amount, 0.0, has_depth);
                let form = match e.form {
                    PatternForm::Checker => 0.0,
                    PatternForm::Dots => 1.0,
                    PatternForm::Lines => 2.0,
                    PatternForm::Grid => 3.0,
                    PatternForm::Waves => 4.0,
                };
                u.p[0] = [e.scale * ppu, e.radius, e.smooth, e.angle];
                u.p[1] = [e.power, form, 0.0, 0.0];
                u.c[0] = linear_rgba(e.color);
                u.c[1] = linear_rgba(e.background);
                single(&mut passes, &mut cur, u, None);
            }
            ImageEffect::Halftone(e) => {
                let mut u = base(MODE_HALFTONE, e.amount, 0.0, has_depth);
                let shape = match e.shape {
                    HalftoneShape::Dots => 0.0,
                    HalftoneShape::Lines => 1.0,
                    HalftoneShape::Squares => 2.0,
                };
                u.p[0] = [e.size * ppu, e.angle, shape, if e.original_colors { 1.0 } else { 0.0 }];
                u.c[0] = linear_rgba(e.ink);
                u.c[1] = linear_rgba(e.paper);
                single(&mut passes, &mut cur, u, None);
            }
            ImageEffect::Ascii(e) => {
                let mut u = base(MODE_ASCII, e.amount, 0.0, has_depth);
                let set = match e.charset {
                    AsciiCharset::Standard => 0.0,
                    AsciiCharset::Simple => 1.0,
                    AsciiCharset::Binary => 2.0,
                    AsciiCharset::Blocks => 3.0,
                };
                u.p[0] = [e.size * ppu, e.density, e.contrast, set];
                u.p[1] = [if e.mono { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0];
                u.c[0] = linear_rgba(e.ink);
                u.c[1] = linear_rgba(e.background);
                single(&mut passes, &mut cur, u, None);
            }
            ImageEffect::Grain(e) => {
                let mut u = base(MODE_GRAIN, e.amount, 0.0, has_depth);
                u.p[0] = [(e.size * ppu).max(1.0), if e.animated { 1.0 } else { 0.0 }, if e.mono { 1.0 } else { 0.0 }, 0.0];
                single(&mut passes, &mut cur, u, None);
            }
            ImageEffect::Glow(e) => {
                let mut u = base(MODE_BLUR_H, 1.0, 1.0, has_depth);
                u.p[0] = [e.radius * ppu, 0.0, 0.0, 0.0];
                u.p[2] = [e.threshold, 0.0, 0.0, 0.0];
                let t1 = pick(&[cur]);
                passes.push(Pass { u, a: cur, b: cur, dst: t1, code: None, aux_background: false });
                let mut v = base(MODE_BLUR_V, 1.0, 0.0, has_depth);
                v.p[0] = u.p[0];
                let t2 = pick(&[cur, Src::T(t1)]);
                passes.push(Pass { u: v, a: Src::T(t1), b: Src::T(t1), dst: t2, code: None, aux_background: false });
                let mut c = base(MODE_GLOW_COMPOSITE, 1.0, 0.0, has_depth);
                c.p[0] = [e.intensity, 0.0, 0.0, 0.0];
                c.c[0] = linear_rgba(e.tint);
                let dst = pick(&[cur, Src::T(t2)]);
                passes.push(Pass { u: c, a: cur, b: Src::T(t2), dst, code: None, aux_background: false });
                cur = Src::T(dst);
            }
            ImageEffect::GradientMap(e) => {
                let mut u = base(MODE_GRADIENT_MAP, e.amount, 0.0, has_depth);
                let mut stops: Vec<_> = e.stops.iter().take(8).collect();
                stops.sort_by(|a, b| a.position.total_cmp(&b.position));
                u.p[0] = [stops.len() as f32, e.smooth, e.offset, 0.0];
                for (i, s) in stops.iter().enumerate() {
                    u.p[1 + i / 4][i % 4] = s.position;
                    u.c[i] = linear_rgba(s.color);
                }
                single(&mut passes, &mut cur, u, None);
            }
            ImageEffect::Blur(e) => {
                let mode = match e.mode {
                    BlurMode::Plain => 0.0,
                    BlurMode::Progressive => 1.0,
                    BlurMode::Depth => 2.0,
                };
                let mut u = base(MODE_BLUR_H, 1.0, 0.0, has_depth);
                u.p[0] = [e.radius * ppu, mode, e.focus, e.focus_range];
                u.p[1] = [e.start[0], e.start[1], e.end[0], e.end[1]];
                let t1 = pick(&[cur]);
                passes.push(Pass { u, a: cur, b: cur, dst: t1, code: None, aux_background: false });
                let mut v = u;
                v.a[0] = MODE_BLUR_V;
                let dst = pick(&[cur, Src::T(t1)]);
                passes.push(Pass { u: v, a: Src::T(t1), b: Src::T(t1), dst, code: None, aux_background: false });
                cur = Src::T(dst);
            }
            ImageEffect::DepthMap(e) => {
                let mut u = base(MODE_DEPTH_MAP, e.amount, 0.0, has_depth);
                u.p[0] = [e.contrast, if e.invert { 1.0 } else { 0.0 }, 0.0, 0.0];
                single(&mut passes, &mut cur, u, None);
            }
            ImageEffect::RibbedGlass(e) => {
                let mut u = base(MODE_RIBBED, e.distortion, 0.0, has_depth);
                let shape = match e.shape {
                    RibShape::Circles => 0.0,
                    RibShape::Lines => 1.0,
                    RibShape::Squares => 2.0,
                };
                u.p[0] = [e.size * ppu, e.curve, e.echo, e.widening];
                u.p[1] = [e.angle, e.dispersion, e.wave, e.wavelength * ppu];
                u.p[2] = [shape, e.center[0], e.center[1], 0.0];
                single(&mut passes, &mut cur, u, None);
            }
            ImageEffect::ScannerDrag(e) => {
                let mut u = base(MODE_SCANNER, 1.0, 0.0, has_depth);
                let points: Vec<_> = e.points.iter().take(8).collect();
                u.p[0] = [points.len() as f32, e.intensity, e.fringe, e.timing];
                u.p[1] = [e.angle, 0.0, 0.0, 0.0];
                for (i, p) in points.iter().enumerate() {
                    u.c[i] = [p[0], p[1], 0.0, 0.0];
                }
                if !points.is_empty() {
                    single(&mut passes, &mut cur, u, None);
                }
            }
            ImageEffect::Code(e) => {
                let mut u = base(MODE_COPY, e.amount, 0.0, has_depth);
                for (i, p) in e.params.iter().take(8).enumerate() {
                    u.p[i / 4][i % 4] = p.value;
                }
                for (i, c) in e.colors.iter().take(8).enumerate() {
                    u.c[i] = linear_rgba(c.value);
                }
                let names = (e.params.iter().take(8).map(|p| p.name.as_str()).collect::<Vec<_>>(), e.colors.iter().take(8).map(|c| c.name.as_str()).collect::<Vec<_>>());
                let code = code_with_bindings(&e.code, &names.0, &names.1);
                if code_ok(&code) {
                    single(&mut passes, &mut cur, u, Some(code));
                }
            }
        }
    }
    // Clip last: the result masked by the content's own (untouched) alpha.
    if fx.clip {
        let u = base(MODE_CLIP, 1.0, 0.0, false);
        let dst = pick(&[cur, Src::Layer]);
        passes.push(Pass { u, a: cur, b: Src::Layer, dst, code: None, aux_background: false });
    }
    (passes, scissor)
}

/// The code effect's source with its param / colour names baked in (the
/// pipeline cache key — renaming a param recompiles).
fn code_with_bindings(code: &str, params: &[&str], colors: &[&str]) -> String {
    let mut out = String::new();
    for (i, name) in params.iter().enumerate() {
        out.push_str(&format!("//#param {i} {name}\n"));
    }
    for (i, name) in colors.iter().enumerate() {
        out.push_str(&format!("//#color {i} {name}\n"));
    }
    out.push_str(code);
    out
}

/// The full WGSL for a Code effect: the shared header, `time` / params /
/// colours as module-scope privates, `sample(uv)` (straight colour at a
/// node-local uv), the user's code (translated from the HLSL-ish dialect
/// when it isn't already WGSL), and the `fs_code` entry point.
///
/// Dialect accepted on top of WGSL: `float`/`float2..4`/`int` types,
/// `T name = …;` declarations, `T fn(T a, …) {` function headers, `for (int
/// i = 0; …)`, multi-component swizzle assignment (`c.rgb *= x;`), `lerp`,
/// `frac`, `saturate`.
pub fn code_shader_source(code: &str) -> Result<String, String> {
    let mut params: Vec<(usize, String)> = Vec::new();
    let mut colors: Vec<(usize, String)> = Vec::new();
    let mut body = String::new();
    for line in code.lines() {
        if let Some(rest) = line.strip_prefix("//#param ") {
            let mut it = rest.split_whitespace();
            if let (Some(i), Some(n)) = (it.next().and_then(|i| i.parse().ok()), it.next()) {
                params.push((i, n.to_string()));
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("//#color ") {
            let mut it = rest.split_whitespace();
            if let (Some(i), Some(n)) = (it.next().and_then(|i| i.parse().ok()), it.next()) {
                colors.push((i, n.to_string()));
            }
            continue;
        }
        body.push_str(line);
        body.push('\n');
    }
    if !body.contains("effect") {
        return Err("The code needs a function `effect(uv)` that returns a colour.".into());
    }
    let user = if body.contains("fn effect") { body } else { translate_hlsl(&body) };

    let mut src = String::new();
    src.push_str(shader_header());
    src.push_str(
        "\nalias float = f32;\nalias float2 = vec2<f32>;\nalias float3 = vec3<f32>;\nalias float4 = vec4<f32>;\nalias int = i32;\n\
         fn saturate(x: f32) -> f32 { return clamp(x, 0.0, 1.0); }\n\
         var<private> time: f32;\n",
    );
    for (_, name) in &params {
        src.push_str(&format!("var<private> {name}: f32;\n"));
    }
    for (_, name) in &colors {
        src.push_str(&format!("var<private> {name}: vec4<f32>;\n"));
    }
    src.push_str("fn sample(uv: vec2<f32>) -> vec4<f32> { return unpm(sa(local_px(uv))); }\n");
    src.push_str(&user);
    src.push_str("\n@fragment\nfn fs_code(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {\n    time = u.a.y;\n");
    for (i, name) in &params {
        let comp = ["x", "y", "z", "w"][i % 4];
        src.push_str(&format!("    {name} = u.p[{}].{comp};\n", i / 4));
    }
    for (i, name) in &colors {
        src.push_str(&format!("    {name} = u.c[{i}];\n"));
    }
    src.push_str(
        "    let src = unpm(sa(pos.xy));\n    let out = clamp(effect(local_uv(pos.xy)), vec4<f32>(0.0), vec4<f32>(1.0));\n    return pm(mix(src, out, u.k.y));\n}\n",
    );
    Ok(src)
}

const TYPES: [&str; 6] = ["float4", "float3", "float2", "float", "int", "bool"];

/// Line-by-line translation of the HLSL-ish dialect to WGSL (see
/// [`code_shader_source`]).
fn translate_hlsl(code: &str) -> String {
    let mut out = String::new();
    for line in code.lines() {
        let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        let t = line.trim();
        let t = t.replace("lerp(", "mix(").replace("frac(", "fract(");
        let translated = function_header(&t)
            .or_else(|| declaration(&t))
            .or_else(|| for_header(&t))
            .or_else(|| swizzle_assign(&t))
            .unwrap_or(t);
        out.push_str(&indent);
        out.push_str(&translated);
        out.push('\n');
    }
    out
}

/// `T` and the rest, if `s` starts with a known type followed by a space.
fn split_type(s: &str) -> Option<(&'static str, &str)> {
    TYPES.iter().find_map(|ty| s.strip_prefix(ty).filter(|r| r.starts_with(char::is_whitespace)).map(|r| (*ty, r.trim_start())))
}

fn ident_prefix(s: &str) -> (&str, &str) {
    let end = s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(s.len());
    (&s[..end], &s[end..])
}

/// `float4 name(float2 uv, float k) {` → `fn name(uv: float2, k: float) -> float4 {`
fn function_header(t: &str) -> Option<String> {
    let (ret, rest) = if let Some(r) = t.strip_prefix("void ") { ("", r.trim_start()) } else { split_type(t)? };
    let (name, after) = ident_prefix(rest);
    if name.is_empty() {
        return None;
    }
    let after = after.trim_start();
    let inner = after.strip_prefix('(')?;
    let close = inner.find(')')?;
    let tail = inner[close + 1..].trim();
    if !(tail.is_empty() || tail == "{") {
        return None;
    }
    let args: Vec<String> = inner[..close]
        .split(',')
        .filter(|a| !a.trim().is_empty())
        .map(|a| {
            let a = a.trim();
            match split_type(a) {
                Some((ty, n)) => format!("{}: {ty}", n.trim()),
                None => a.to_string(),
            }
        })
        .collect();
    let ret = if ret.is_empty() { String::new() } else { format!(" -> {ret}") };
    Some(format!("fn {name}({}){ret} {tail}", args.join(", ")).trim_end().to_string())
}

/// `float x = …;` → `var x: float = …;` (also `const float`).
fn declaration(t: &str) -> Option<String> {
    let t2 = t.strip_prefix("const ").unwrap_or(t);
    let (ty, rest) = split_type(t2)?;
    let (name, after) = ident_prefix(rest);
    if name.is_empty() {
        return None;
    }
    let after = after.trim_start();
    if after.starts_with('=') {
        Some(format!("var {name}: {ty} {after}"))
    } else if after.starts_with(';') {
        Some(format!("var {name}: {ty};"))
    } else {
        None
    }
}

/// `for (int i = 0; …)` → `for (var i: int = 0; …)`.
fn for_header(t: &str) -> Option<String> {
    let rest = t.strip_prefix("for")?.trim_start().strip_prefix('(')?;
    let (ty, after) = split_type(rest.trim_start())?;
    let (name, after) = ident_prefix(after);
    Some(format!("for (var {name}: {ty}{after}"))
}

/// `c.rgb op= expr;` → a whole-vector assignment (WGSL can't assign to a
/// multi-component swizzle). Assumes `c` is a 4-component vector.
fn swizzle_assign(t: &str) -> Option<String> {
    let (name, rest) = ident_prefix(t);
    if name.is_empty() {
        return None;
    }
    let rest = rest.strip_prefix('.')?;
    let (sw, rest) = ident_prefix(rest);
    if sw.len() < 2 || sw.len() > 4 || !sw.chars().all(|c| "rgbaxyzw".contains(c)) {
        return None;
    }
    let rest = rest.trim_start();
    let (op, expr) = ["+=", "-=", "*=", "/=", "="].iter().find_map(|op| rest.strip_prefix(op).map(|e| (*op, e)))?;
    let expr = expr.trim().strip_suffix(';')?.trim();
    let value = if op == "=" { format!("({expr})") } else { format!("({name}.{sw} {} ({expr}))", &op[..1]) };
    let slot = |c: char| match c {
        'r' | 'x' => 0,
        'g' | 'y' => 1,
        'b' | 'z' => 2,
        _ => 3,
    };
    let comps = ["x", "y", "z", "w"];
    let parts: Vec<String> = (0..4)
        .map(|i| match sw.chars().position(|c| slot(c) == i) {
            Some(k) => format!("_sw.{}", comps[k]),
            None => format!("{name}.{}", comps[i]),
        })
        .collect();
    Some(format!("{{ let _sw = {value}; {name} = vec4<f32>({}); }}", parts.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate(src: &str) {
        let module = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
    }

    #[test]
    fn builtin_shader_validates() {
        validate(SHADER);
    }

    #[test]
    fn template_code_effect_compiles() {
        let fx = an1mbyt3_core::image_effects::CodeEffect::default();
        let params: Vec<&str> = fx.params.iter().map(|p| p.name.as_str()).collect();
        let colors: Vec<&str> = fx.colors.iter().map(|c| c.name.as_str()).collect();
        let src = code_shader_source(&code_with_bindings(&fx.code, &params, &colors)).unwrap();
        validate(&src);
    }

    #[test]
    fn wgsl_code_effect_compiles() {
        let code = "fn effect(uv: vec2<f32>) -> vec4<f32> {\n    let c = sample(uv);\n    return vec4<f32>(1.0 - c.rgb, c.a);\n}\n";
        validate(&code_shader_source(code).unwrap());
    }

    #[test]
    fn hlsl_bits_translate() {
        assert_eq!(declaration("float t = time * speed;").unwrap(), "var t: float = time * speed;");
        assert_eq!(function_header("float4 effect(float2 uv) {").unwrap(), "fn effect(uv: float2) -> float4 {");
        assert!(swizzle_assign("c.rgb *= tint;").unwrap().contains("c = vec4<f32>(_sw.x, _sw.y, _sw.z, c.w)"));
        assert_eq!(for_header("for (int i = 0; i < 4; i++) {").unwrap(), "for (var i: int = 0; i < 4; i++) {");
    }
}
