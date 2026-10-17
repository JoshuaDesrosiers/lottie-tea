//! **Flattened layers** — a run of draws rendered together into an
//! offscreen texture first, then pasted back as one image.
//!
//! Drawing a clipped subtree straight onto the scene treats every draw on
//! its own: a fade (e.g. a clip's "hidden part opacity") fades each
//! particle separately, so overlapping particles stack into dark blotches.
//! A layer is drawn once at full strength — overlaps resolve inside it the
//! way they do anyway — and then composited with per-stencil-level
//! opacities: the whole layer at 100 % inside a clip, and the same image at
//! the fade's opacity in the part the clip cuts away. That's group opacity
//! as in Photoshop / Affinity.
//!
//! The layer texture holds premultiplied colour (ordinary alpha blending
//! onto transparent black produces exactly that), so the composite is a
//! premultiplied "over" scaled by the opacity. Layers can nest: each level
//! gets its own colour + stencil texture from a small pool.

use std::cell::{Cell, OnceCell, RefCell};

use super::{create_stencil_view, LayerFx, no_depth_write_stencil_state, DrawCall, Renderer, SceneOp, STENCIL_FORMAT, TRANSPARENT};

/// One pooled layer target (a nesting level).
pub(super) struct LayerTarget {
    view: wgpu::TextureView,
    stencil: wgpu::TextureView,
    size: (u32, u32),
}

/// Layer pipeline + pool, created on first use.
pub(super) struct LayerResources {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}

#[derive(Default)]
pub(super) struct Layers {
    resources: OnceCell<LayerResources>,
    targets: RefCell<Vec<LayerTarget>>,
    level: Cell<usize>,
}

const SHADER: &str = r#"
struct Composite { opacity: vec4<f32> };
@group(0) @binding(0) var layer_tex: texture_2d<f32>;
@group(0) @binding(1) var<uniform> u: Composite;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let c = textureLoad(layer_tex, vec2<i32>(pos.xy), 0);
    return c * u.opacity.x;
}
"#;

impl Renderer {
    fn layer_resources(&self) -> &LayerResources {
        self.layers.resources.get_or_init(|| {
            let device = &self.device;
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("an1mbyt3 layer composite"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                        count: None,
                    },
                ],
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("an1mbyt3 layer composite"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("an1mbyt3 layer composite"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
            let face = wgpu::StencilFaceState { compare: wgpu::CompareFunction::Equal, fail_op: wgpu::StencilOperation::Keep, depth_fail_op: wgpu::StencilOperation::Keep, pass_op: wgpu::StencilOperation::Keep };
            let premultiplied = wgpu::BlendState {
                color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
                alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
            };
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("an1mbyt3 layer composite"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &[] },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(no_depth_write_stencil_state(wgpu::StencilState { front: face, back: face, read_mask: 0xff, write_mask: 0x00 })),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: self.render_format, blend: Some(premultiplied), write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            });
            LayerResources { layout, pipeline }
        })
    }

    /// The pooled target for nesting `level`, at `size`.
    fn layer_target(&self, level: usize, size: (u32, u32)) {
        let mut targets = self.layers.targets.borrow_mut();
        while targets.len() <= level {
            targets.push(LayerTarget { view: self.post_scene_view.clone(), stencil: create_stencil_view(&self.device, 1, 1), size: (0, 0) });
        }
        if targets[level].size != size {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("an1mbyt3 layer"),
                size: wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.render_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            targets[level] = LayerTarget { view: texture.create_view(&wgpu::TextureViewDescriptor::default()), stencil: create_stencil_view(&self.device, size.0, size.1), size };
        }
    }

    /// Draws a layer's content into its offscreen texture, then pastes it
    /// onto `target_view` once per composite `(stencil level, opacity)`,
    /// tested against `stencil_view` (the island's stencil, loaded).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_layer(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        stencil_view: &wgpu::TextureView,
        size: (u32, u32),
        unmasked: &[DrawCall<'_>],
        masked: &[SceneOp<'_>],
        composites: &[(u32, f32)],
        fx: Option<&LayerFx<'_>>,
    ) {
        let level = self.layers.level.get();
        self.layer_target(level, size);
        // (Views are cheap handles; clone out of the pool so the borrow
        // ends before the nested draw, which may grow the pool.)
        let (view, stencil) = {
            let targets = self.layers.targets.borrow();
            (targets[level].view.clone(), targets[level].stencil.clone())
        };
        self.layers.level.set(level + 1);
        self.draw_masked_scene_sized(encoder, &view, &stencil, wgpu::LoadOp::Clear(TRANSPARENT), unmasked, masked, None, size);
        // The node's image effects run on the drawn layer; the composite
        // below then pastes their result instead.
        let view = match fx {
            Some(fx) => self.run_layer_fx(encoder, level, size, &view, fx),
            None => view,
        };
        self.layers.level.set(level);

        let res = self.layer_resources();
        for (depth, opacity) in composites {
            if *opacity <= 0.0 {
                continue;
            }
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("an1mbyt3 layer opacity"), size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            self.queue.write_buffer(&buffer, 0, bytemuck::cast_slice(&[opacity.clamp(0.0, 1.0), 0.0f32, 0.0, 0.0]));
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("an1mbyt3 layer composite"),
                layout: &res.layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: buffer.as_entire_binding() }],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("an1mbyt3 layer composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target_view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }, depth_slice: None })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: stencil_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                    stencil_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // Effects say how far they reach; nothing outside that is drawn.
            if let Some([x, y, w, h]) = fx.and_then(|f| f.scissor()) {
                if w == 0 || h == 0 {
                    continue;
                }
                pass.set_scissor_rect(x, y, w, h);
            }
            pass.set_pipeline(&res.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.set_stencil_reference(*depth);
            pass.draw(0..3, 0..1);
        }
    }
}

// Keep the stencil format in view for readers of this module.
const _: wgpu::TextureFormat = STENCIL_FORMAT;
