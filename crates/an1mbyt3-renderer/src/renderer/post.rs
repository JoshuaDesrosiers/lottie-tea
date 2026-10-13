//! The post-process chain behind [`Renderer::render_with_post_effects`].
//!
//! It runs the artboard's effect stack over the offscreen scene image and
//! presents the result. Two ideas keep it cheap at any resolution:
//!
//! - **Fusing.** Per-pixel effects (Color Grading, Posterize, Gradient Map,
//!   Vignette), Pixelate, a Glow's composite and the final present all run
//!   in **one** full-screen draw, the "uber" pass (`post_uber.wgsl`). A
//!   new pass only starts where an effect needs the *result* of the ones
//!   before it as a texture: a Glow (it blooms the image so far), a Blur,
//!   or a Pixelate after colour ops. A typical stack (Glow + grading +
//!   vignette) is one full-resolution pass plus the bloom chain.
//! - **Working small.** Glow is a bloom mip chain (`post_bloom.wgsl`):
//!   threshold while downsampling to ½, keep halving, add back up. It is
//!   cheap at 4K. A Blur with radius ≥ 2 runs at half resolution, where it
//!   looks the same and costs a quarter.
//!
//! Nothing is allocated per frame. Uniforms live in a pool of fixed-slot
//! buffers, and bind groups are cached by (which targets, which sampler,
//! which slot). The pass sequence is the same frame to frame for a given
//! stack, so after the first frame every lookup hits. The cache is
//! dropped whenever the targets are rebuilt (resize, resolution scale).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use super::{create_post_scene_view, linear_rgba, wgpu_blend_state, BlendMode, PostEffect, PostUniforms, Renderer, TRANSPARENT};

/// Bytes per pooled uniform slot (fits [`UberUniforms`] and [`PostUniforms`]).
const UNIFORM_SLOT: u64 = 1024;
/// Per-pixel colour ops one uber pass can apply (more starts a new pass).
const MAX_OPS: usize = 8;
/// Bloom mip levels at most (½ … 1/128).
const MAX_BLOOM_LEVELS: usize = 7;

/// Every texture a post pass reads or writes, by name — the bind-group
/// cache's key (texture views have no cheap identity of their own).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum PostTarget {
    Scene,
    /// 2D lighting's lit composite (`lighting_targets.lit_view`).
    Lit,
    Ping,
    Pong,
    BlurTmp,
    HalfA,
    HalfB,
    Bloom(u8),
    /// 1×1 transparent black: the uber pass's "no bloom".
    Black,
}

/// The uber pass's uniforms (`post_uber.wgsl`'s `Uber`).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct UberUniforms {
    texel: [f32; 4],
    info: [f32; 4],
    ops: [[f32; 4]; MAX_OPS * 3],
    /// The pass's sampling op (anaglyph, aberration, sharpen, edges,
    /// distortions): mode + params, 0 = a plain sample.
    sample: [[f32; 4]; 2],
    /// The pass's Palette colours (one Palette per pass), linear.
    palette: [[f32; 4]; an1mbyt3_core::PALETTE_MAX],
}

/// (pass kind, source, second texture, nearest sampler?, uniform slot).
type BindKey = (u8, PostTarget, PostTarget, bool, usize);

/// A texture view and its size.
struct Sized {
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

/// The chain's GPU state (owned by the [`Renderer`]).
pub(super) struct PostChain {
    uber_layout: wgpu::BindGroupLayout,
    /// Uber into an intermediate target (replace) / onto the swapchain
    /// (alpha-blended over the chrome).
    uber_replace: wgpu::RenderPipeline,
    uber_blend: wgpu::RenderPipeline,
    bloom_down: wgpu::RenderPipeline,
    bloom_up: wgpu::RenderPipeline,
    uniforms: RefCell<Vec<wgpu::Buffer>>,
    next_uniform: Cell<usize>,
    binds: RefCell<HashMap<BindKey, wgpu::BindGroup>>,
    bloom: Vec<Sized>,
    half_a: Sized,
    half_b: Sized,
    black: wgpu::TextureView,
}

fn sized(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> Sized {
    let (width, height) = (width.max(1), height.max(1));
    let (_, view) = create_post_scene_view(device, format, width, height);
    Sized { view, width, height }
}

impl PostChain {
    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat, post_layout: &wgpu::BindGroupLayout, width: u32, height: u32) -> Self {
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let sampler_entry = |binding| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None };
        let uber_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("an1mbyt3 post uber layout"),
            entries: &[
                texture_entry(0),
                sampler_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                texture_entry(3),
                sampler_entry(4),
            ],
        });
        let pipeline = |label: &str, layout: &wgpu::BindGroupLayout, module: &wgpu::ShaderModule, entry: &str, blend: Option<wgpu::BlendState>| {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &[Some(layout)], immediate_size: 0 });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState { module, entry_point: Some("vs_main"), compilation_options: wgpu::PipelineCompilationOptions::default(), buffers: &[] },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let uber = device.create_shader_module(wgpu::include_wgsl!("post_uber.wgsl"));
        let bloom = device.create_shader_module(wgpu::include_wgsl!("post_bloom.wgsl"));
        let black = {
            let (texture, view) = create_post_scene_view(device, format, 1, 1);
            drop(texture);
            view
        };
        let mut chain = Self {
            uber_replace: pipeline("an1mbyt3 post uber (replace)", &uber_layout, &uber, "fs_main", None),
            uber_blend: pipeline("an1mbyt3 post uber (blend)", &uber_layout, &uber, "fs_main", Some(wgpu_blend_state(BlendMode::Normal))),
            bloom_down: pipeline("an1mbyt3 bloom down", post_layout, &bloom, "fs_down", None),
            bloom_up: pipeline("an1mbyt3 bloom up", post_layout, &bloom, "fs_up", Some(wgpu_blend_state(BlendMode::Add))),
            uber_layout,
            uniforms: RefCell::new(Vec::new()),
            next_uniform: Cell::new(0),
            binds: RefCell::new(HashMap::new()),
            bloom: Vec::new(),
            half_a: sized(device, format, 1, 1),
            half_b: sized(device, format, 1, 1),
            black,
        };
        chain.resize(device, format, width, height);
        chain
    }

    /// Rebuilds the size-dependent targets for post targets of `width ×
    /// height` (and forgets every cached bind group — they point at the
    /// old textures).
    pub(super) fn resize(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) {
        self.bloom = (0..MAX_BLOOM_LEVELS).map(|k| sized(device, format, width >> (k + 1), height >> (k + 1))).collect();
        self.half_a = sized(device, format, width.div_ceil(2), height.div_ceil(2));
        self.half_b = sized(device, format, width.div_ceil(2), height.div_ceil(2));
        self.binds.borrow_mut().clear();
    }
}

/// How a run of effects is waiting to be drawn: the image it starts from,
/// and what the next uber pass will do to it.
struct Pending {
    source: PostTarget,
    pixelate: f32,
    /// A bloom to add (always `Bloom(0)`) and its intensity.
    bloom: Option<f32>,
    ops: Vec<[[f32; 4]; 3]>,
    /// How the source is sampled (see `sample_op`).
    sample: Option<[[f32; 4]; 2]>,
    /// A Palette op's colours (linear); one per pass.
    palette: Option<[[f32; 4]; an1mbyt3_core::PALETTE_MAX]>,
}

impl Pending {
    fn from(source: PostTarget) -> Self {
        Self { source, pixelate: 0.0, bloom: None, ops: Vec::new(), sample: None, palette: None }
    }
    fn has_work(&self) -> bool {
        self.pixelate > 0.0 || self.bloom.is_some() || !self.ops.is_empty() || self.sample.is_some()
    }
}

/// An effect that reads *neighbouring* pixels (so it runs where the uber
/// pass samples its source, once per pass): mode + params.
fn sample_op(effect: &PostEffect) -> Option<[[f32; 4]; 2]> {
    Some(match *effect {
        PostEffect::Anaglyph { offset, mono } => [[1.0, offset, mono, 0.0], [0.0; 4]],
        PostEffect::ChromaticAberration { strength } => [[2.0, strength, 0.0, 0.0], [0.0; 4]],
        PostEffect::Sharpen { amount } => [[3.0, amount, 0.0, 0.0], [0.0; 4]],
        PostEffect::EdgeDetect { thickness, threshold, color, only } => [[4.0, thickness, threshold, only], linear_rgba(color)],
        PostEffect::WaveDistortion { amplitude, frequency, speed } => [[5.0, amplitude, frequency, speed], [0.0; 4]],
        PostEffect::LensDistortion { strength, zoom } => [[6.0, strength, zoom, 0.0], [0.0; 4]],
        PostEffect::PixelScatter { amount, size, speed } => [[7.0, amount, size, speed], [0.0; 4]],
        _ => return None,
    })
}

/// The per-pixel colour op an effect is, if it is one.
fn colour_op(effect: &PostEffect) -> Option<[[f32; 4]; 3]> {
    Some(match *effect {
        PostEffect::ColorGrading { brightness, contrast, saturation } => [[2.0, brightness, contrast, saturation], [0.0; 4], [0.0; 4]],
        PostEffect::Posterize { levels } => [[3.0, levels, 0.0, 0.0], [0.0; 4], [0.0; 4]],
        PostEffect::GradientMap { shadow, highlight } => [[4.0, 0.0, 0.0, 0.0], linear_rgba(shadow), linear_rgba(highlight)],
        PostEffect::Vignette { color, intensity, radius, blur } => [[7.0, intensity, radius, blur], linear_rgba(color), [0.0; 4]],
        PostEffect::Dither { levels, scale, strength, mono, dark, light } => {
            let (d, l) = (linear_rgba(dark), linear_rgba(light));
            [[8.0, levels, scale, strength], [d[0], d[1], d[2], mono], l]
        }
        PostEffect::Scanlines { intensity, spacing, mask } => [[9.0, intensity, spacing, mask], [0.0; 4], [0.0; 4]],
        PostEffect::FilmGrain { amount, size } => [[10.0, amount, size, 0.0], [0.0; 4], [0.0; 4]],
        PostEffect::Halftone { size, angle, ink, paper, mix } => {
            let (i, p) = (linear_rgba(ink), linear_rgba(paper));
            [[11.0, size, angle, mix], i, p]
        }
        _ => return None,
    })
}

/// Bloom levels for a Glow `radius`. The old full-resolution Gaussian had
/// σ ≈ 3·radius pixels; each level doubles the reach.
fn bloom_levels(radius: f32, width: u32, height: u32) -> usize {
    let wanted = ((3.0 * radius.max(0.1)).log2().round() as i32 + 1).clamp(1, MAX_BLOOM_LEVELS as i32) as usize;
    // Never a level smaller than 2 px.
    let fits = (0..MAX_BLOOM_LEVELS).take_while(|k| (width >> (k + 1)) >= 2 && (height >> (k + 1)) >= 2).count().max(1);
    wanted.min(fits)
}

impl Renderer {
    fn post_view(&self, target: PostTarget) -> &wgpu::TextureView {
        match target {
            PostTarget::Scene => &self.post_scene_view,
            PostTarget::Lit => &self.lighting_targets.lit_view,
            PostTarget::Ping => &self.post_ping_view,
            PostTarget::Pong => &self.post_pong_view,
            PostTarget::BlurTmp => &self.post_blur_intermediate_view,
            PostTarget::HalfA => &self.post.half_a.view,
            PostTarget::HalfB => &self.post.half_b.view,
            PostTarget::Bloom(k) => &self.post.bloom[k as usize].view,
            PostTarget::Black => &self.post.black,
        }
    }

    /// A target's size (the full-size ones are the post target size).
    fn post_size(&self, target: PostTarget) -> (u32, u32) {
        match target {
            PostTarget::HalfA => (self.post.half_a.width, self.post.half_a.height),
            PostTarget::HalfB => (self.post.half_b.width, self.post.half_b.height),
            PostTarget::Bloom(k) => (self.post.bloom[k as usize].width, self.post.bloom[k as usize].height),
            PostTarget::Black => (1, 1),
            _ => self.post_target_dims(),
        }
    }

    /// Writes `bytes` into the next pooled uniform slot this frame.
    fn post_uniform(&self, bytes: &[u8]) -> usize {
        let slot = self.post.next_uniform.get();
        self.post.next_uniform.set(slot + 1);
        let mut pool = self.post.uniforms.borrow_mut();
        while pool.len() <= slot {
            pool.push(self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("an1mbyt3 post uniform slot"), size: UNIFORM_SLOT, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }));
        }
        self.queue.write_buffer(&pool[slot], 0, bytes);
        slot
    }

    /// The (cached) bind group for a pass. `kind` 0 = the shared post
    /// layout (texture, sampler, uniforms), 1 = the uber layout (+ bloom).
    fn post_bind(&self, kind: u8, source: PostTarget, second: PostTarget, nearest: bool, slot: usize) -> wgpu::BindGroup {
        let key = (kind, source, second, nearest, slot);
        if let Some(bind) = self.post.binds.borrow().get(&key) {
            return bind.clone();
        }
        let pool = self.post.uniforms.borrow();
        let sampler = if nearest { &self.post_sampler_nearest } else { &self.post_sampler };
        let bind = if kind == 0 {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("an1mbyt3 post pass"),
                layout: &self.post_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(self.post_view(source)) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: pool[slot].as_entire_binding() },
                ],
            })
        } else {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("an1mbyt3 post uber pass"),
                layout: &self.post.uber_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(self.post_view(source)) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: pool[slot].as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(self.post_view(second)) },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Sampler(&self.post_sampler) },
                ],
            })
        };
        self.post.binds.borrow_mut().insert(key, bind.clone());
        bind
    }

    /// One full-screen post draw into `dest`.
    fn post_draw(&self, encoder: &mut wgpu::CommandEncoder, pipeline: &wgpu::RenderPipeline, bind: &wgpu::BindGroup, dest: &wgpu::TextureView, load: wgpu::LoadOp<wgpu::Color>) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("an1mbyt3 post pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: dest, resolve_target: None, ops: wgpu::Operations { load, store: wgpu::StoreOp::Store }, depth_slice: None })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
    }

    /// A pass of the shared post shader (`post_process.wgsl`) from
    /// `source` into `dest` (both named targets).
    fn post_simple(&self, encoder: &mut wgpu::CommandEncoder, pipeline: &wgpu::RenderPipeline, source: PostTarget, dest: PostTarget, uniforms: PostUniforms, load: wgpu::LoadOp<wgpu::Color>) {
        let slot = self.post_uniform(bytemuck::bytes_of(&uniforms));
        let bind = self.post_bind(0, source, PostTarget::Black, false, slot);
        self.post_draw(encoder, pipeline, &bind, self.post_view(dest), load);
    }

    /// Runs `effects` over `source` (the rendered scene) and presents the
    /// result onto `swapchain` (alpha-blended over what's there).
    pub(super) fn run_post_chain(&self, encoder: &mut wgpu::CommandEncoder, source: PostTarget, effects: &[PostEffect], swapchain: &wgpu::TextureView) {
        self.post.next_uniform.set(0);
        let mut pending = Pending::from(source);
        for effect in effects {
            // Palette: a colour op whose colours ride in the pass's own
            // palette block (so a second Palette needs a new pass).
            // Gradient Map (stops): colours + offsets ride in the same block.
            if let PostEffect::GradientMapStops { colors, offsets, count, mix } = *effect {
                if pending.ops.len() == MAX_OPS || pending.palette.is_some() {
                    pending = Pending::from(self.flush_post(encoder, pending, None));
                }
                let n = an1mbyt3_core::GRADIENT_MAP_MAX;
                let mut block = [[0.0f32; 4]; an1mbyt3_core::PALETTE_MAX];
                for i in 0..n {
                    block[i] = linear_rgba(colors[i]);
                    block[n + i / 4][i % 4] = offsets[i];
                }
                pending.palette = Some(block);
                pending.ops.push([[13.0, count.min(n as u32) as f32, mix, 0.0], [0.0; 4], [0.0; 4]]);
                continue;
            }
            if let PostEffect::Palette { colors, count, dither, scale, mix } = *effect {
                if pending.ops.len() == MAX_OPS || pending.palette.is_some() {
                    pending = Pending::from(self.flush_post(encoder, pending, None));
                }
                pending.palette = Some(colors.map(linear_rgba));
                pending.ops.push([[12.0, count.min(an1mbyt3_core::PALETTE_MAX as u32) as f32, dither, scale], [mix, 0.0, 0.0, 0.0], [0.0; 4]]);
                continue;
            }
            if let Some(op) = colour_op(effect) {
                if pending.ops.len() == MAX_OPS {
                    pending = Pending::from(self.flush_post(encoder, pending, None));
                }
                pending.ops.push(op);
                continue;
            }
            if let Some(op) = sample_op(effect) {
                // Reads neighbours of the *image so far*: anything queued
                // becomes an image first (one sampling op per pass).
                if pending.has_work() {
                    pending = Pending::from(self.flush_post(encoder, pending, None));
                }
                pending.sample = Some(op);
                continue;
            }
            match *effect {
                PostEffect::Pixelate { block_size } => {
                    // Snaps the *sample position*: anything already queued
                    // has to be an image first.
                    if pending.has_work() {
                        pending = Pending::from(self.flush_post(encoder, pending, None));
                    }
                    pending.pixelate = block_size.max(1.0);
                }
                PostEffect::Glow { radius, threshold, intensity } => {
                    if pending.has_work() {
                        pending = Pending::from(self.flush_post(encoder, pending, None));
                    }
                    let levels = self.build_bloom(encoder, pending.source, radius, threshold);
                    // Each level holds the blurred bright pass at its own
                    // scale; their sum is ~`levels`× one blur's energy.
                    pending.bloom = Some(intensity / levels as f32 * 1.6);
                }
                PostEffect::Blur { radius } => {
                    if pending.has_work() {
                        pending = Pending::from(self.flush_post(encoder, pending, None));
                    }
                    pending = Pending::from(self.blur_post(encoder, pending.source, radius));
                }
                _ => {}
            }
        }
        self.flush_post(encoder, pending, Some(swapchain));
    }

    /// Draws `pending` as one uber pass: onto the swapchain when `final_view`
    /// is given (and returns `source`, unused), else into a free full-size
    /// target, which it returns.
    fn flush_post(&self, encoder: &mut wgpu::CommandEncoder, pending: Pending, final_view: Option<&wgpu::TextureView>) -> PostTarget {
        let (width, height) = self.post_target_dims();
        let mut ops = [[0.0f32; 4]; MAX_OPS * 3];
        for (i, op) in pending.ops.iter().enumerate() {
            ops[i * 3..i * 3 + 3].copy_from_slice(op);
        }
        let uniforms = UberUniforms {
            texel: [1.0 / width as f32, 1.0 / height as f32, pending.pixelate, pending.bloom.unwrap_or(0.0)],
            info: [pending.ops.len() as f32, self.shader_time.get(), 0.0, 0.0],
            ops,
            sample: pending.sample.unwrap_or([[0.0; 4]; 2]),
            palette: pending.palette.unwrap_or([[0.0; 4]; an1mbyt3_core::PALETTE_MAX]),
        };
        let slot = self.post_uniform(bytemuck::bytes_of(&uniforms));
        let bloom = if pending.bloom.is_some() { PostTarget::Bloom(0) } else { PostTarget::Black };
        match final_view {
            Some(view) => {
                // Pixel-art scenes (a resolution block size) upscale with
                // hard edges; everything else (incl. adaptive resolution)
                // smoothly.
                let nearest = self.current_resolution_scale > 1.0;
                let bind = self.post_bind(1, pending.source, bloom, nearest, slot);
                self.post_draw(encoder, &self.post.uber_blend, &bind, view, wgpu::LoadOp::Load);
                pending.source
            }
            None => {
                let dest = if pending.source == PostTarget::Ping { PostTarget::Pong } else { PostTarget::Ping };
                let bind = self.post_bind(1, pending.source, bloom, false, slot);
                self.post_draw(encoder, &self.post.uber_replace, &bind, self.post_view(dest), wgpu::LoadOp::Clear(TRANSPARENT));
                dest
            }
        }
    }

    /// Builds the bloom of `source` into `Bloom(0)` (half size). Returns
    /// how many levels it used.
    fn build_bloom(&self, encoder: &mut wgpu::CommandEncoder, source: PostTarget, radius: f32, threshold: f32) -> usize {
        let (width, height) = self.post_target_dims();
        let levels = bloom_levels(radius, width, height);
        let texel = |t: PostTarget| {
            let (w, h) = self.post_size(t);
            [1.0 / w as f32, 1.0 / h as f32]
        };
        let mut from = source;
        for k in 0..levels {
            let to = PostTarget::Bloom(k as u8);
            let t = texel(from);
            let first = if k == 0 { 1.0 } else { 0.0 };
            self.post_simple(encoder, &self.post.bloom_down, from, to, PostUniforms { texel_size_radius: [t[0], t[1], 1.0, 0.0], threshold_intensity: [threshold, 0.0, first, 0.0], gradient_map_highlight: [0.0; 4] }, wgpu::LoadOp::Clear(TRANSPARENT));
            from = to;
        }
        // Spread grows a little with the radius past what the levels give.
        let spread = (radius / 3.0).clamp(0.75, 2.0);
        for k in (1..levels).rev() {
            let t = texel(PostTarget::Bloom(k as u8));
            self.post_simple(encoder, &self.post.bloom_up, PostTarget::Bloom(k as u8), PostTarget::Bloom(k as u8 - 1), PostUniforms { texel_size_radius: [t[0], t[1], spread, 0.0], threshold_intensity: [0.0; 4], gradient_map_highlight: [0.0; 4] }, wgpu::LoadOp::Load);
        }
        levels
    }

    /// A separable Gaussian of `source`; returns the target holding it.
    /// Radius ≥ 2 runs at half resolution (a quarter of the pixels, and at
    /// that blur the difference doesn't show).
    fn blur_post(&self, encoder: &mut wgpu::CommandEncoder, source: PostTarget, radius: f32) -> PostTarget {
        let replace = &self.post_replace_pipeline;
        let clear = wgpu::LoadOp::Clear(TRANSPARENT);
        let uniforms = |texel: [f32; 2], radius: f32, mode: f32| PostUniforms { texel_size_radius: [texel[0], texel[1], radius, mode], threshold_intensity: [0.0, 1.0, 0.0, 0.0], gradient_map_highlight: [0.0; 4] };
        if radius >= 2.0 {
            let (w, h) = self.post_size(PostTarget::HalfA);
            let texel = [1.0 / w as f32, 1.0 / h as f32];
            if source != PostTarget::HalfA {
                self.post_simple(encoder, replace, source, PostTarget::HalfA, uniforms(texel, 0.0, 0.0), clear);
            }
            self.post_simple(encoder, replace, PostTarget::HalfA, PostTarget::HalfB, uniforms(texel, radius * 0.5, 5.0), clear);
            self.post_simple(encoder, replace, PostTarget::HalfB, PostTarget::HalfA, uniforms(texel, radius * 0.5, 6.0), clear);
            PostTarget::HalfA
        } else {
            let (w, h) = self.post_target_dims();
            let texel = [1.0 / w as f32, 1.0 / h as f32];
            let dest = if source == PostTarget::Ping { PostTarget::Pong } else { PostTarget::Ping };
            self.post_simple(encoder, replace, source, PostTarget::BlurTmp, uniforms(texel, radius, 5.0), clear);
            self.post_simple(encoder, replace, PostTarget::BlurTmp, dest, uniforms(texel, radius, 6.0), clear);
            dest
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bloom_levels_follow_the_radius_and_fit_the_target() {
        assert_eq!(bloom_levels(3.0, 1920, 1080), 4);
        assert!(bloom_levels(1.0, 1920, 1080) < bloom_levels(6.0, 1920, 1080));
        // A tiny target never gets a level under 2 px.
        assert_eq!(bloom_levels(8.0, 8, 8), 2);
        assert_eq!(bloom_levels(8.0, 2, 2), 1);
    }

    #[test]
    fn uber_uniforms_fit_a_slot() {
        assert!(std::mem::size_of::<UberUniforms>() as u64 <= UNIFORM_SLOT);
        assert!(std::mem::size_of::<PostUniforms>() as u64 <= UNIFORM_SLOT);
    }
}
