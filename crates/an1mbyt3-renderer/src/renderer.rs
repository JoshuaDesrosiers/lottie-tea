//! The `Renderer`: owns the wgpu device/queue/surface/pipeline and draws
//! uploaded meshes to that surface.
//!
//! Deliberately windowing-library-agnostic: `Renderer::new` takes an
//! already-created `wgpu::Surface` rather than a `winit::window::Window` (or
//! any other window type), because `wgpu::Surface` itself is the
//! windowing-agnostic abstraction — it can come from winit (this crate's
//! `render_shape` example), from Tauri's window (Phase 4), or from an HTML
//! `<canvas>` (Phase 8's WASM runtime), and this crate shouldn't need to know
//! or care which. See AN1MBYT3_BUILD_PLAN.md §1's process/compilation
//! topology table.

use glam::{Affine2, Mat4, Vec2};
use wgpu::util::DeviceExt;

pub use an1mbyt3_core::BlendMode;
pub use an1mbyt3_core::{ImageFilterMode, ImageProcessing};
pub use an1mbyt3_core::PostEffect;
use an1mbyt3_core::Transform2D;
use an1mbyt3_geometry::Mesh;

mod lighting;
mod post;
mod three_d;
pub use three_d::{Blend3D, FluidLayer3D, GpuMaterial3D, GpuMesh3D, MeshDraw3D, ParticleQuad3D, PointLight3D, Scene3D, Shadows3D, MATERIAL_PARAMS_3D, MAX_POINT_LIGHTS, TOON_ARCANE_WGSL, TOON_MTOON_WGSL};
pub use lighting::{LightDraw, LightShadows, LightShape, LightVolumetric, Lighting, ShadowCasterShape, MAX_FREEFORM_POINTS};

/// How many gradient stops `shader.wgsl` has room for — matches that
/// shader's own `MAX_GRADIENT_STOPS` constant exactly; a `GradientDef` with
/// more stops than this silently drops the extras (`DrawUniforms::from_draw`
/// truncates), same "authoring UI's job to stay within budget" convention
/// this workspace already applies elsewhere rather than growing every
/// uniform buffer to accommodate an unbounded case that never comes up in
/// practice.
const MAX_GRADIENT_STOPS: usize = 8;

/// Per-draw uniform data uploaded to the GPU: a fully composed
/// model-view-projection matrix, a flat fill color, and an optional
/// gradient (kind/start/end/stops) — see `shader.wgsl`'s own doc comment
/// for the packed-stop layout and why `stop_count == 0` is the "no
/// gradient, just use `color`" fallback every non-gradient `DrawCall`
/// already produces via `DrawUniforms::from_draw`'s `Default`-derived zero
/// fill.
///
/// `#[repr(C)]` + `Pod`/`Zeroable` (via `bytemuck`) make this safely castable
/// to a raw byte slice for `Queue::write_buffer` — see `Renderer::render`.
#[repr(C)]
#[derive(Copy, Clone, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawUniforms {
    mvp: [[f32; 4]; 4],
    color: [f32; 4],
    gradient_start_end: [f32; 4],
    gradient_meta: [f32; 4],
    stop_offsets: [[f32; 4]; 2],
    stop_colors: [[f32; 4]; MAX_GRADIENT_STOPS],
    stop_midpoints: [[f32; 4]; 2],
}

impl DrawUniforms {
    /// Builds this draw's whole uniform block from its already-composed
    /// `mvp` and its `DrawCall`'s `fill_color`/`gradient` — the one place
    /// every `Renderer::render*`/`render_to_texture` call site goes through,
    /// so the packed-stop layout only has to be gotten right once.
    fn from_draw(mvp: Mat4, draw: &DrawCall<'_>) -> Self {
        // A textured draw (`fs_textured`) reuses this same packed slot for
        // its mesh's own local bounds instead of a gradient axis — see
        // `GpuMesh::local_min`/`local_max`'s own doc comment. Checked first
        // and unconditionally: `texture`/`gradient` are mutually exclusive
        // in practice (`set_draw_pipeline` picks one whole pipeline or the
        // other per draw), so there's nothing to lose by preferring texture
        // bounds over a `gradient` that would just be ignored anyway. No
        // `blend_alpha_correction` needed either — a textured draw is always
        // Normal-blend (`DrawCall::texture`'s own doc comment), which needs
        // no shader-side correction in the first place.
        if draw.texture.is_some() || draw.material.is_some() {
            // `gradient_meta.w`: `1.0` tells `compute_textured_color` to
            // sample at this mesh's own authored per-vertex UV instead of
            // deriving one from local position/bounds — see that function's
            // own doc comment. Keyed off `GpuMesh::has_vertex_uv`, not
            // anything on `DrawCall` itself, so no textured-draw call site
            // anywhere in `an1mbyt3-wasm` needed to change to opt in/out.
            let use_vertex_uv = if draw.mesh.has_vertex_uv { 1.0 } else { 0.0 };
            return Self {
                mvp: mvp.to_cols_array_2d(),
                color: linear_rgba(draw.fill_color),
                gradient_start_end: [draw.mesh.local_min[0], draw.mesh.local_min[1], draw.mesh.local_max[0], draw.mesh.local_max[1]],
                gradient_meta: [0.0, 0.0, 0.0, use_vertex_uv],
                ..Default::default()
            };
        }

        let blend_correction = blend_alpha_correction(draw.blend_mode);

        let Some(gradient) = draw.gradient else {
            return Self { mvp: mvp.to_cols_array_2d(), color: linear_rgba(draw.fill_color), gradient_meta: [0.0, 0.0, blend_correction, 0.0], ..Default::default() };
        };

        let kind = match gradient.kind {
            an1mbyt3_core::GradientKind::Linear => 1.0,
            an1mbyt3_core::GradientKind::Radial => 2.0,
            an1mbyt3_core::GradientKind::Conic => 3.0,
            an1mbyt3_core::GradientKind::Bilinear => 4.0,
        };
        let stop_count = gradient.stops.len().min(MAX_GRADIENT_STOPS);
        let mut stop_offsets = [[0.0f32; 4]; 2];
        let mut stop_colors = [[0.0f32; 4]; MAX_GRADIENT_STOPS];
        let mut stop_midpoints = [[0.0f32; 4]; 2];
        for (i, stop) in gradient.stops.iter().take(stop_count).enumerate() {
            stop_offsets[i / 4][i % 4] = stop.offset;
            stop_colors[i] = linear_rgba(stop.color);
            stop_midpoints[i / 4][i % 4] = stop.midpoint;
        }

        Self {
            mvp: mvp.to_cols_array_2d(),
            color: linear_rgba(draw.fill_color),
            gradient_start_end: [gradient.start.x, gradient.start.y, gradient.end.x, gradient.end.y],
            gradient_meta: [kind, stop_count as f32, blend_correction, 0.0],
            stop_offsets,
            stop_colors,
            stop_midpoints,
        }
    }
}

/// `Multiply`/`Screen`/`Add`'s `wgpu_blend_state`/`BlendState::ADDITIVE`
/// factors are plain fixed-function color math (`Dst*src`/`One*src +
/// OneMinusSrc*dst`/`One*src + One*dst`) with *no* alpha weighting at all —
/// unlike `Normal`'s `BlendState::ALPHA_BLENDING`, the one real
/// alpha-composited preset among the four (an earlier version of this
/// function assumed `BlendState::ADDITIVE` was *also* alpha-weighted like
/// `ALPHA_BLENDING`; it isn't — its own `src_factor`/`dst_factor` are both
/// plain `One`, same alpha-blind shape as `Multiply`/`Screen`). A gradient
/// that fades via alpha (the Fill tool's own default "solid -> transparent"
/// gradient) has no visible fade at all under any of the three as a result —
/// every pixel gets the *same* full-strength blend regardless of the
/// gradient's alpha at that point, reading as a flat "solid" fill instead of
/// a fade. Fixed-function GPU blending can't weight by source alpha *and*
/// read the destination color in one step (no compound "`Dst * SrcAlpha`"
/// factor exists), so the fix has to happen in the shader instead:
/// `fs_main`'s final return applies the standard "premultiply toward this
/// mode's identity color" trick (white for `Multiply`, black for `Screen`
/// *and* `Add` — multiplying/screening/adding the identity color is a no-op
/// for each) so the *shader's own output* already encodes the alpha-weighted
/// fade, and the (alpha-blind) fixed-function blend on top of that now
/// composites correctly. `Screen` and `Add` share the exact same correction
/// (`2.0`, "mix toward black") since both have a black identity element —
/// only their GPU-side blend *equation* differs, per each one's own
/// `wgpu_blend_state`/`ADDITIVE` arm, not the shader-side correction needed.
/// Returns which correction `fs_main` should apply, packed into
/// `gradient_meta.z` (`0.0` none, `1.0` multiply, `2.0` screen/add) — see
/// that shader's own doc comment for the two branches.
fn blend_alpha_correction(mode: BlendMode) -> f32 {
    match mode {
        BlendMode::Multiply => 1.0,
        BlendMode::Screen | BlendMode::Add => 2.0,
        BlendMode::Normal => 0.0,
    }
}

/// Matches `post_process.wgsl`'s `PostParams` struct exactly — see that
/// shader's doc comment for why the fields are packed into two `vec4`s
/// instead of individual scalars (sidesteps WGSL uniform alignment rules by
/// construction rather than computing padding by hand).
///
/// Both `vec4`s' last components are repurposed per-`PostEffect` rather than
/// growing this struct for each new effect — the shader dispatches on
/// `texel_size_radius.w` as a mode discriminant (`0.0` blur/present, `1.0`
/// pixelate, `2.0` color grading) and reinterprets the rest of the fields
/// accordingly, the same "one flexible shader, not one per effect" approach
/// `post_process.wgsl`'s own module doc already established for blur vs.
/// glow's bright-pass. See `Renderer::render_with_post_effect` for exactly
/// which fields each `PostEffect` variant writes.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct PostUniforms {
    /// (texel_width, texel_height, blur_radius | pixelate_block_size | posterize_levels, mode)
    texel_size_radius: [f32; 4],
    /// Blur/present/pixelate/posterize modes: (unused, unused, unused, unused).
    /// Glow mode: (brightness_threshold, output_intensity, apply_threshold_flag, unused).
    /// Color-grading mode: (brightness, contrast, saturation, unused).
    /// Gradient-map mode: `shadow` color (rgba).
    threshold_intensity: [f32; 4],
    /// Gradient-map mode only: `highlight` color (rgba). Unused (zeroed) by
    /// every other mode — a third uniform slot rather than packing this
    /// into the two above, since `GradientMap` is the first effect needing
    /// more than 8 floats total.
    gradient_map_highlight: [f32; 4],
}

/// One post-process draw's own uniform buffer + bind group (texture +
/// sampler + [`PostUniforms`]) — `Renderer::render_with_post_effects` builds
/// up to two of these *per active effect, per frame* (via
/// `create_post_binding`) rather than keeping one persistent pair, since
/// which texture is "the source" changes at every step of a multi-effect
/// chain. Two are needed for [`PostEffect::Glow`] specifically, for the same
/// reason [`DrawBinding`] exists at all: it needs two post-process draws in
/// the *same* submitted frame with *different* uniform values (the
/// plain-present base layer, then the thresholded/blurred glow layer), and
/// sharing one uniform buffer between two draws in one submission means the
/// second `queue.write_buffer` call clobbers the first before the GPU reads
/// either — the exact bug `DrawBinding`'s own doc comment describes, here
/// reappearing between two *post-process* draws instead of two shape draws.
struct PostBinding {
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

/// A [`Mesh`] uploaded to GPU buffers, ready to draw.
///
/// Uploading is a separate, explicit step from drawing (rather than
/// `Renderer::render` re-uploading every frame) precisely to embody
/// AN1MBYT3_BUILD_PLAN.md §6: "tessellate them into GPU-friendly geometry,
/// cache static geometry, and update only affected geometry during
/// animation." A `GpuMesh`'s vertex/index buffers never change after upload.
///
/// Holds only geometry, deliberately — no uniform buffer/bind group live
/// here. Those are per-*instance* (see [`DrawBinding`]), while a `GpuMesh` is
/// meant to be shared across every instance of the same shape (Phase 7
/// components: one `GpuMesh`, many instances with different transforms/fill
/// colors). A uniform buffer baked into `GpuMesh` would mean every instance
/// sharing one mesh also shares one uniform slot — the last `DrawCall`'s
/// `queue.write_buffer` would clobber every earlier one before the GPU ever
/// reads it, so all instances would render with only the last instance's
/// transform/color. That exact bug shipped in the first version of the
/// Phase 7 `components` example before this split.
pub struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    /// Per-vertex UV — always present (every pipeline's vertex state binds
    /// it, `shader.wgsl`'s `vs_main` always forwards it), but only
    /// *meaningful* when `has_vertex_uv` is true. Zero-filled otherwise
    /// (`Renderer::upload_mesh`), harmless: the textured fragment path only
    /// reads it when `has_vertex_uv` told it to (`DrawUniforms::from_draw`'s
    /// `gradient_meta.w` flag) — every other draw keeps deriving UV from
    /// local position/bounds exactly as before `uvs` existed at all.
    uv_buffer: wgpu::Buffer,
    /// Whether `uv_buffer` holds real, authored per-vertex UVs (a `Tilemap`'s
    /// many-sub-rects-of-one-tileset mesh) rather than zero-padding — see
    /// `uv_buffer`'s own doc comment for how this gates the fragment shader.
    has_vertex_uv: bool,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    /// This mesh's own local (pre-transform) axis-aligned bounds — computed
    /// once at upload from `Mesh::positions`. Exists purely so a *textured*
    /// draw (`DrawCall::texture`, e.g. `NodeKind::Shape::image`) can map its
    /// own local vertex position into `0..1` UV space regardless of the
    /// shape's actual size/path (`shader.wgsl`'s `fs_textured`) — the
    /// `unit_quad_mesh` an `Image` node always draws through happens to
    /// already span exactly `-0.5..0.5`, so this comes out to that same
    /// range for it, but any other tessellated path's own bounds work the
    /// same way with no special-casing. Never updated after upload (matching
    /// this whole struct's "vertex/index buffers never change" doc comment
    /// above) — a skinned mesh's `update_mesh_positions` can move these
    /// bounds out of date, but nothing skinned is textured today, so that's
    /// harmless, not a silent-corruption case actually reachable yet.
    local_min: [f32; 2],
    local_max: [f32; 2],
}

/// Per-instance GPU resources for one [`DrawCall`]: a uniform buffer holding
/// that instance's transform/color, and the bind group pointing at it.
///
/// Separate from [`GpuMesh`] so that N instances of the same shared mesh get
/// N independent `DrawBinding`s (see `GpuMesh`'s doc comment for why sharing
/// a single uniform buffer across instances is wrong). Create one per
/// `DrawCall` that will exist concurrently — e.g. one per component instance,
/// not one per `GpuMesh`.
pub struct DrawBinding {
    /// Where this binding's uniforms live in the shared pool.
    pool: std::rc::Rc<std::cell::RefCell<UniformPool>>,
    page: usize,
    slot: usize,
    bind_group: wgpu::BindGroup,
}

impl DrawBinding {
    /// Stages this draw's uniforms. Nothing reaches the GPU until the
    /// renderer flushes the pool right before it submits the frame — one
    /// upload per pool page instead of one per draw.
    fn write(&self, bytes: &[u8]) {
        self.pool.borrow_mut().write(self.page, self.slot, bytes);
    }
}

impl Drop for DrawBinding {
    fn drop(&mut self) {
        if let Ok(mut pool) = self.pool.try_borrow_mut() {
            pool.free.push((self.page, self.slot));
        }
    }
}

/// Every `DrawBinding`'s uniforms, packed into a few large GPU buffers
/// ("pages") with a CPU mirror each. Draw calls write into the mirror; the
/// dirty range of each page is uploaded once per submit. On the web every
/// `queue.writeBuffer` is a call into WebGPU, so per-draw uploads dominated
/// big scenes' frame time.
struct UniformPool {
    /// Bytes per slot: `DrawUniforms` rounded up to the device's uniform
    /// offset alignment (bind groups point at fixed offsets).
    stride: usize,
    pages: Vec<UniformPage>,
    free: Vec<(usize, usize)>,
}

struct UniformPage {
    buffer: wgpu::Buffer,
    mirror: Vec<u8>,
    used: usize,
    dirty: Option<(usize, usize)>,
}

const SLOTS_PER_PAGE: usize = 512;

impl UniformPool {
    fn new(device: &wgpu::Device) -> Self {
        let align = device.limits().min_uniform_buffer_offset_alignment as usize;
        let size = std::mem::size_of::<DrawUniforms>();
        Self { stride: size.div_ceil(align) * align, pages: Vec::new(), free: Vec::new() }
    }

    /// A free slot (reusing dropped bindings' slots first).
    fn allocate(&mut self, device: &wgpu::Device) -> (usize, usize) {
        if let Some(slot) = self.free.pop() {
            return slot;
        }
        if self.pages.last().is_none_or(|page| page.used == SLOTS_PER_PAGE) {
            let bytes = self.stride * SLOTS_PER_PAGE;
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("an1mbyt3 draw uniform pool page"),
                size: bytes as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.pages.push(UniformPage { buffer, mirror: vec![0; bytes], used: 0, dirty: None });
        }
        let page_index = self.pages.len() - 1;
        let page = &mut self.pages[page_index];
        page.used += 1;
        (page_index, page.used - 1)
    }

    fn write(&mut self, page: usize, slot: usize, bytes: &[u8]) {
        let start = slot * self.stride;
        let page = &mut self.pages[page];
        page.mirror[start..start + bytes.len()].copy_from_slice(bytes);
        let end = start + bytes.len();
        page.dirty = Some(match page.dirty {
            Some((lo, hi)) => (lo.min(start), hi.max(end)),
            None => (start, end),
        });
    }

    /// Uploads every page's dirty range (call before `queue.submit`).
    fn flush(&mut self, queue: &wgpu::Queue) {
        for page in &mut self.pages {
            if let Some((lo, hi)) = page.dirty.take() {
                // `write_buffer` needs a 4-byte-aligned size.
                let hi = hi.div_ceil(4) * 4;
                queue.write_buffer(&page.buffer, lo as wgpu::BufferAddress, &page.mirror[lo..hi.min(page.mirror.len())]);
            }
        }
    }
}

/// One raster image uploaded to the GPU, ready to sample — the texture
/// counterpart to [`GpuMesh`]. Built by [`Renderer::upload_texture`], either
/// straight from a decoded source (a caller that wants no baked processing)
/// or from [`Renderer::bake_image`]'s output (resize/filter/effect already
/// applied once, not per frame). Holds only a pre-built bind group against
/// `Renderer`'s `image_bind_group_layout` (texture + sampler, no uniform
/// buffer — an `Image` `DrawCall`'s transform/tint still comes through the
/// ordinary per-draw uniform at group 0, same as every other draw); the
/// underlying texture/view/sampler don't need their own handles kept
/// alongside it — wgpu keeps a bind group's referenced resources alive
/// internally for as long as the bind group itself lives.
#[derive(Clone)]
pub struct GpuImageTexture {
    bind_group: wgpu::BindGroup,
}

/// A small, **linear** (not sRGB) RGBA8 texture that's rewritten every
/// frame — data for a shader rather than a picture (e.g. a goo liquid's
/// distance field, 16-bit values split across two channels so linear
/// filtering stays exact). Its storage only ever grows; a smaller write
/// fills the top-left corner, so shaders should map UVs with
/// `textureDimensions` (the capacity), not the written size.
pub struct DataTexture {
    texture: wgpu::Texture,
    pub image: GpuImageTexture,
    width: u32,
    height: u32,
}

impl DataTexture {
    /// The texture's full (capacity) size in texels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// `an1mbyt3_core::BlendMode` (see that type's own doc comment for why it
/// lives there, not here) — how a draw's color combines with whatever's
/// already in the framebuffer, AN1MBYT3_BUILD_PLAN.md §6's "compositing,
/// masks, blend modes... dedicated shader passes" (Phase 10). Each variant
/// is backed by its own `wgpu::RenderPipeline` (see
/// `Renderer::PIPELINE_BLEND_MODES`) since a pipeline's blend state is fixed
/// at creation, not settable per draw call — `render()` picks the matching
/// pipeline for each `DrawCall`.
///
/// `Multiply`/`Screen` use the standard fixed-function *approximations* of
/// those Photoshop-style blend modes (the same ones most real-time engines
/// use), not a true dual-source/shader-computed blend — accurate for how
/// they read against an opaque or simple background, revisit only if a
/// specific overlapping-transparency case turns out to look wrong.
/// Every variant, in the fixed order `Renderer` builds one pipeline per
/// mode — iterated at construction time, not meant for per-frame use.
const ALL_BLEND_MODES: [BlendMode; 4] = [BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen, BlendMode::Add];

/// Maps a [`BlendMode`] to the `wgpu::BlendState` `Renderer` builds a
/// pipeline from. A free function, not an inherent `impl BlendMode` (like
/// this used to be before the enum moved to `an1mbyt3-core` — see that
/// type's doc comment) — Rust's orphan rule forbids an inherent `impl` on a
/// type defined in a different crate; only trait impls get that flexibility.
fn wgpu_blend_state(mode: BlendMode) -> wgpu::BlendState {
    match mode {
        BlendMode::Normal => wgpu::BlendState::ALPHA_BLENDING,
        BlendMode::Add => wgpu::BlendState::ADDITIVE,
        BlendMode::Multiply => wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Dst,
                dst_factor: wgpu::BlendFactor::Zero,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::Zero,
                operation: wgpu::BlendOperation::Add,
            },
        },
        BlendMode::Screen => wgpu::BlendState {
            // `result = src + dst*(1-src)` — the standard fixed-function
            // "screen" blend (complement of `Multiply`).
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrc,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        },
    }
}

/// Maps `buffer` for reading and strips wgpu's row-alignment padding into a
/// tightly-packed `Vec<u8>` — shared by `Renderer::render_to_texture`/
/// `Renderer::bake_image`'s near-identical readback.
///
/// Uses a `futures_channel::oneshot` (a real `Future`), not
/// `std::sync::mpsc` — this matters specifically for wasm32: `map_async`'s
/// callback only ever fires once control returns to the browser's own event
/// loop (there is no second OS thread to run it on), which a genuine
/// `.await` yields to correctly but a blocking `Receiver::recv()` never can.
/// Calling this with a blocking channel is exactly what deadlocked the
/// whole page — a caller awaiting an async fn that internally blocks
/// forever waiting for a callback that can only run after that same call
/// returns is a real, first-hand-hit bug, not a hypothetical one.
/// `device.poll(Wait)` still runs first: on native backends it drives the
/// callback to completion synchronously (so the following `.await` resolves
/// immediately), while on wasm it's a harmless no-op and the `.await` is
/// what does the actual waiting.
async fn read_mapped_buffer(device: &wgpu::Device, buffer: &wgpu::Buffer, unpadded_bytes_per_row: u32, padded_bytes_per_row: u32, rows: u32) -> Vec<u8> {
    let (sender, receiver) = futures_channel::oneshot::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    let _ = device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
    receiver
        .await
        .expect("map_async's callback channel closed without sending")
        .expect("failed to map a GPU readback buffer");

    let mapped = buffer.slice(..).get_mapped_range().expect("readback buffer was just confirmed mapped above");
    let mut pixels = Vec::with_capacity((unpadded_bytes_per_row * rows) as usize);
    for row in 0..rows {
        let start = (row * padded_bytes_per_row) as usize;
        let end = start + unpadded_bytes_per_row as usize;
        pixels.extend_from_slice(&mapped[start..end]);
    }
    drop(mapped);
    buffer.unmap();
    pixels
}


/// What a render pass currently has bound, so consecutive draws that share
/// a pipeline / texture / mesh (e.g. thousands of sprites on one quad and
/// one sprite sheet) don't re-issue those calls — on the web each one is a
/// separate call into WebGPU, and they dominated big scenes' frame time.
struct BoundState {
    pipeline: *const wgpu::RenderPipeline,
    texture: *const wgpu::BindGroup,
    mesh: *const GpuMesh,
}

impl Default for BoundState {
    fn default() -> Self {
        Self { pipeline: std::ptr::null(), texture: std::ptr::null(), mesh: std::ptr::null() }
    }
}

/// Draws one unmasked `DrawCall`, binding only what changed since the
/// previous draw in this pass (`bound`).
fn draw_flat<'a>(pass: &mut wgpu::RenderPass<'a>, renderer: &'a Renderer, draw: &DrawCall<'a>, bound: &mut BoundState) {
    // A material (shader graph) draw: its own pipeline, the shared material
    // globals at group 2, and always a texture at group 1 (white if the
    // node has no image) — see `GpuMaterial`.
    let materials = draw.material.map(|_| renderer.material_resources());
    let pipeline: &'a wgpu::RenderPipeline = match (draw.material, draw.texture) {
        (Some(material), _) => &material.pipeline,
        (None, Some(_)) => &renderer.textured_pipeline,
        (None, None) => renderer.pipelines.get(&draw.blend_mode).expect("Renderer always builds a pipeline for every BlendMode variant"),
    };
    if !std::ptr::eq(bound.pipeline, pipeline) {
        pass.set_pipeline(pipeline);
        bound.pipeline = pipeline;
        // Re-bind the texture after a pipeline switch, to be safe.
        bound.texture = std::ptr::null();
        if let Some(resources) = materials {
            pass.set_bind_group(2, &resources.globals_bind_group, &[]);
        }
    }
    pass.set_bind_group(0, &draw.binding.bind_group, &[]);
    let texture = draw.texture.or(materials.map(|r| &r.white));
    if let Some(texture) = texture {
        if !std::ptr::eq(bound.texture, &texture.bind_group) {
            pass.set_bind_group(1, &texture.bind_group, &[]);
            bound.texture = &texture.bind_group;
        }
    }
    if !std::ptr::eq(bound.mesh, draw.mesh) {
        pass.set_vertex_buffer(0, draw.mesh.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, draw.mesh.uv_buffer.slice(..));
        pass.set_index_buffer(draw.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        bound.mesh = draw.mesh;
    }
    pass.draw_indexed(0..draw.mesh.index_count, 0, 0..1);
}

/// One shape to draw this frame: a [`GpuMesh`] (shared geometry) plus a
/// [`DrawBinding`] (this instance's own transform/color uniform slot) plus
/// the transform/color values to write into that binding this frame.
/// Multiple `DrawCall`s can share the same `GpuMesh` as long as each has its
/// own `DrawBinding` (Phase 7: a component instantiated more than once).
pub struct DrawCall<'a> {
    pub mesh: &'a GpuMesh,
    pub binding: &'a DrawBinding,
    pub transform: Transform2D,
    pub fill_color: [f32; 4],
    pub blend_mode: BlendMode,
    /// `Some` overrides `fill_color` with a gradient fill — see
    /// `an1mbyt3_core::GradientDef`'s own doc comment. Borrowed rather
    /// than owned since a `DrawCall` is already a per-frame, read-only
    /// reference bundle (matching `mesh`/`binding`'s own `&'a` fields).
    pub gradient: Option<&'a an1mbyt3_core::GradientDef>,
    /// `Some` draws this call as a textured quad (sampling `uniforms.color`
    /// as a tint multiplier) via `Renderer`'s dedicated textured pipeline,
    /// instead of the flat-fill-or-gradient pipeline every other draw uses —
    /// see `shader.wgsl`'s `fs_textured`. Normal-blend only for this pass
    /// (`blend_mode` is ignored when this is `Some`), matching masks' own
    /// "Normal-only for now" scope-narrowing.
    pub texture: Option<&'a GpuImageTexture>,
    /// `Some` shades this draw with a compiled shader-graph material
    /// ([`Renderer::compile_material`]) instead of the built-in fill —
    /// unmasked draws only for now (inside a mask it falls back to the
    /// plain fill / texture). Normal blend.
    pub material: Option<&'a GpuMaterial>,
}

/// A compiled shader-graph material: one render pipeline whose fragment
/// stage is the graph's generated `material_fragment` (see
/// `an1mbyt3_core::shader_graph`). Cheap to keep around; recompile only
/// when the graph changes.
pub struct GpuMaterial {
    pipeline: wgpu::RenderPipeline,
    /// The same shader, stencil-tested (`Equal`, no writes) for content
    /// inside a clip mask — `draw_masked_scene`'s masked pass has a
    /// depth/stencil attachment, so it can't use `pipeline` (no
    /// depth_stencil state). Without this, a material draw under a mask
    /// fell back to the plain textured pipeline and showed its raw input
    /// texture (e.g. a liquid's packed field as green/magenta blur).
    masked_pipeline: wgpu::RenderPipeline,
}

/// Shared state every material pipeline binds: the layout (draw uniforms,
/// texture, globals), the globals uniform (time), and a 1×1 white texture
/// for shapes that have no image. Created on first use.
struct MaterialResources {
    layout: wgpu::PipelineLayout,
    /// Group 2 (2D) / group 3 (3D) layout — the 3D pass's material
    /// pipelines bind the same globals.
    globals_layout: wgpu::BindGroupLayout,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    white: GpuImageTexture,
}

/// The glue between `shader.wgsl` and a generated `material_fragment` —
/// defines everything `an1mbyt3_core::shader_graph`'s module doc lists as
/// the renderer's side of the contract.
const MATERIAL_GLUE: &str = r#"
struct MaterialGlobals {
    // x = seconds since start; yzw reserved.
    time: vec4<f32>,
};
@group(2) @binding(0)
var<uniform> material_globals: MaterialGlobals;

// The sprite's UV, exactly as `compute_textured_color` derives it.
fn material_uv(in: VertexOutput) -> vec2<f32> {
    if uniforms.gradient_meta.w > 0.5 {
        return in.uv;
    }
    let bounds_min = uniforms.gradient_start_end.xy;
    let bounds_max = uniforms.gradient_start_end.zw;
    let size = max(bounds_max - bounds_min, vec2<f32>(0.0001, 0.0001));
    let normalized = (in.local_position - bounds_min) / size;
    return vec2<f32>(normalized.x, 1.0 - normalized.y);
}

// The Facing UV node on a flat sprite: just its UV (a sprite always faces
// the camera).
fn material_facing_uv(in: VertexOutput) -> vec2<f32> {
    return material_uv(in);
}

// The View Fresnel node on a flat sprite: there's no surface normal in 2D,
// so it fakes a sphere — 0 at the sprite's centre, 1 at its edge.
fn material_fresnel(in: VertexOutput, power: f32) -> f32 {
    let r = clamp(length(material_uv(in) * 2.0 - vec2<f32>(1.0, 1.0)), 0.0, 1.0);
    return pow(r, max(power, 0.0001));
}

@fragment
fn fs_material(in: VertexOutput) -> @location(0) vec4<f32> {
    return material_fragment(in);
}
"#;

/// Why a frame could not be rendered.
///
/// wgpu 30 replaced the older `Result<SurfaceTexture, SurfaceError>` return
/// from `get_current_texture` with a `CurrentSurfaceTexture` enum that folds
/// success and failure into one type (`Success`/`Suboptimal` carry the
/// texture; everything else is a reason). This mirrors that enum's failure
/// variants under `an1mbyt3-renderer`'s own name so callers don't depend on
/// a wgpu type that may keep changing shape across versions.
#[derive(Debug)]
pub enum RenderError {
    /// Acquiring the next frame timed out — skip this frame and try again.
    Timeout,
    /// The window/canvas is occluded (minimized, hidden) — skip this frame.
    Occluded,
    /// The surface configuration is stale — call [`Renderer::resize`] and retry.
    Outdated,
    /// The surface was lost and needs to be recreated from scratch.
    Lost,
    /// wgpu reported a validation error while acquiring the frame.
    Validation,
}

pub struct Renderer {
    /// The format every frame renders at: the swapchain's sRGB view
    /// (`config.format` + sRGB suffix — see the surface setup in `new`).
    render_format: wgpu::TextureFormat,
    /// `None` for a [`Renderer::new_headless`] renderer — one with no
    /// window/canvas to present to at all, only ever drawn via
    /// [`Renderer::render_to_texture`]. `render`/`render_with_mask`/
    /// `render_with_post_effect` all panic if called on a headless renderer
    /// (a caller bug, not a recoverable condition — matches this crate's
    /// other panicking conventions).
    surface: Option<wgpu::Surface<'static>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    /// One pipeline per [`BlendMode`] — see that enum's doc comment for why
    /// a single pipeline can't serve every blend mode. Built once, up front,
    /// covering `BlendMode::ALL`, rather than lazily per first use of a new
    /// mode; there are only 4 of these and never more (a fixed enum), so
    /// there's no unbounded-growth risk to guard against.
    pipelines: std::collections::HashMap<BlendMode, wgpu::RenderPipeline>,
    /// Writes a mask shape's coverage into the stencil buffer (ref value 1,
    /// no color output) — the first of the two passes [`Renderer::render_with_mask`]
    /// runs. See that method's doc comment for the overall masking approach.
    mask_write_pipeline: wgpu::RenderPipeline,
    /// Draws masked content, stencil-tested against the mask pass's ref
    /// value 1 — the second of the two `render_with_mask` passes. One
    /// pipeline per [`BlendMode`], same reasoning as the unmasked `pipelines`
    /// map above — *not* a single Normal-only pipeline forcing every masked
    /// draw through `ALPHA_BLENDING` regardless of its own `blend_mode` (an
    /// earlier, deliberately-deferred version of this). That forcing was a
    /// real, reproduced bug once `blend_alpha_correction` started shader-side
    /// pre-blending non-Normal draws toward their mode's identity color: the
    /// shader's own correction *assumes* it's paired with that mode's real
    /// (alpha-blind) GPU blend equation to compensate for — paired with
    /// `ALPHA_BLENDING` instead, the correction's forced `alpha = 1.0`
    /// combined with `ALPHA_BLENDING`'s `src*srcAlpha + dst*(1-srcAlpha)`
    /// collapses to `src` alone, discarding whatever the masked content was
    /// layered over entirely — a clipped, blend-moded gradient rendering as
    /// a flat opaque color "on black" instead of blending with what's behind
    /// it. Keyed the same way `pipelines` is, so `SceneOp::Draw`'s dispatch
    /// mirrors `set_draw_pipeline`'s unmasked lookup exactly.
    masked_content_pipelines: std::collections::HashMap<BlendMode, wgpu::RenderPipeline>,
    /// `mask_write_pipeline`'s textured counterpart (`fs_textured_mask_write`) —
    /// a mask (or masked content) that's an `Image` node, or a `Shape` with
    /// its own `image` set, needs its actual sampled texture (and that
    /// texture's real per-pixel alpha) to matter, not the flat/gradient
    /// shader's `uniforms.color`. A separate pipeline (not a texture-aware
    /// branch inside `mask_write_pipeline` itself) for the same reason
    /// `textured_pipeline` is separate from `pipelines` — it binds a second
    /// bind group (`image_bind_group_layout`) no flat-fill/gradient draw
    /// needs. Without this (and `masked_content_textured_pipeline` below),
    /// any textured draw caught up in masking silently fell back to
    /// `mask_write_pipeline`/`masked_content_pipelines`' own `fs_main`/no
    /// texture sampling at all — reading `uniforms.color` alone, which for
    /// an `Image` node is just its flat tint, not its actual pixels: an
    /// image being masked (or used as a mask) rendered as a flat, usually
    /// near-white, opaque rectangle instead of its real content, and a mask
    /// built from a partially-transparent image clipped to its whole
    /// tessellated quad instead of its actual (per-pixel-alpha) silhouette.
    mask_write_textured_pipeline: wgpu::RenderPipeline,
    /// `masked_content_pipelines`' textured counterpart (`fs_textured`,
    /// unchanged — no discard needed here, alpha blending already composites
    /// a sampled texture's transparency correctly; only the *mask* pass
    /// needs to discard). See `mask_write_textured_pipeline`'s own doc
    /// comment for why a separate pipeline exists at all.
    masked_content_textured_pipeline: wgpu::RenderPipeline,
    /// Stencil buffer backing `render_with_mask`, sized to match the current
    /// swapchain — recreated in [`Renderer::resize`] alongside the surface
    /// reconfiguration, same as the surface itself. Unused (never written or
    /// read) by the plain [`Renderer::render`] path, which has no
    /// `depth_stencil_attachment` at all.
    stencil_view: wgpu::TextureView,
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    /// Shared, batched storage for every `DrawBinding`'s uniforms.
    uniform_pool: std::rc::Rc<std::cell::RefCell<UniformPool>>,
    /// Whether the surface actually ended up configured for alpha
    /// transparency — see [`Renderer::new_transparent`]. Callers should check
    /// this (via [`Renderer::supports_transparency`]) before clearing with
    /// `alpha: 0.0`; on a surface that came back `false`, an alpha-0 clear
    /// would just render as fully opaque black instead of see-through.
    transparent: bool,
    /// Presents a post-process bind group's texture as-is (`blend: None`) —
    /// used both for `PostEffect::Blur`'s single draw and `PostEffect::Glow`'s
    /// plain-present base layer. See [`Renderer::render_with_post_effect`].
    post_replace_pipeline: wgpu::RenderPipeline,
    post_bind_group_layout: wgpu::BindGroupLayout,
    post_sampler: wgpu::Sampler,
    /// The offscreen render target every `render_with_post_effect` call
    /// draws the real scene into (instead of the swapchain directly) —
    /// recreated in [`Renderer::resize`] alongside the surface and stencil
    /// buffer, since it must match the swapchain's own dimensions. Unused by
    /// the plain [`Renderer::render`]/[`Renderer::render_with_mask`] paths,
    /// which draw straight to the swapchain. Also what [`Renderer::render_to_texture`]
    /// reads back — the same offscreen target serves both features rather
    /// than each keeping its own.
    post_scene_view: wgpu::TextureView,
    /// The `wgpu::Texture` backing `post_scene_view` — kept alongside the
    /// view because [`Renderer::render_to_texture`]'s `copy_texture_to_buffer`
    /// call needs the `Texture` itself (a copy source is a texture, not a
    /// view), while every other post-processing use only ever needed the view.
    post_scene_texture: wgpu::Texture,
    /// Ping-pong offscreen targets for [`Renderer::render_with_post_effects`]'s
    /// effect *chain* — effect N in a multi-effect stack reads whichever of
    /// these currently holds the previous effect's output and writes into
    /// the other one, alternating each step (the last effect in the chain
    /// writes straight to the swapchain instead, skipping this pair
    /// entirely). Two are needed, not one, for the same reason a single
    /// texture can't be both a render pass's source and its own destination
    /// in the same pass. Unlike `post_scene_view`, these have no persistent
    /// bind group of their own (`post_binding_a`/`post_binding_b` used to
    /// serve that role for the old single-effect path) — a chain's bind
    /// groups are built fresh per effect per frame instead, since which
    /// texture is "the source" changes every step; see
    /// `render_with_post_effects` for why that's an acceptable cost here
    /// (an editor's post-fx stack is a handful of effects at most, not a
    /// hot per-triangle path). Recreated in [`Renderer::resize`] alongside
    /// `post_scene_view`.
    post_ping_view: wgpu::TextureView,
    post_pong_view: wgpu::TextureView,
    /// Scratch target for a `Blur`/`Glow` sub-pass's horizontal-blur output,
    /// before the vertical pass finishes it — deliberately a *third*,
    /// dedicated texture rather than reusing `post_ping_view`/`post_pong_view`:
    /// whichever of those is this effect's `source_view` in the outer chain
    /// loop can't also be written to mid-effect (a texture can't be both a
    /// pass's sampled input and its own render target), and since either one
    /// *can* be the source depending on the effect's position in the chain,
    /// only a texture that's never assigned either chain role is safe to use
    /// unconditionally here. See `render_with_post_effects` for the two-pass
    /// call sequence this backs.
    post_blur_intermediate_view: wgpu::TextureView,
    /// A stencil buffer sized to match the post-effect targets above
    /// (`post_scene_view` etc.), not the swapchain — see
    /// [`Renderer::set_resolution_scale`]'s own doc comment for why those
    /// can be a *different* (smaller) size than `stencil_view`/the surface
    /// at `resolution_scale > 1.0`, and [`PostTargets`] for why this is
    /// bundled alongside them.
    post_stencil_view: wgpu::TextureView,
    /// The pixel block size `post_scene_view` and its sibling targets are
    /// currently divided down by — `1.0` until
    /// [`Renderer::set_resolution_scale`] is called with something else.
    /// Tracked so [`Renderer::resize`] knows to keep rebuilding those
    /// targets at *this* scale (not silently snapping back to native)
    /// whenever the window/canvas itself resizes.
    current_resolution_scale: f32,
    /// Adaptive resolution: the fraction (0.4 … 1) of native size the
    /// scene and post targets render at, on top of
    /// `current_resolution_scale` — lowered by the host when frames run
    /// long (see [`Renderer::set_dynamic_scale`]) and upscaled smoothly.
    dynamic_scale: f32,
    /// The fused post-process chain's pipelines, mip targets and caches
    /// (`renderer/post.rs`).
    post: post::PostChain,
    /// Point/nearest-filtered counterpart to `post_sampler` (which is fixed
    /// Linear) — used only for `render_with_post_effects`'s very last pass
    /// (the one writing to the swapchain) when `current_resolution_scale >
    /// 1.0`, so upscaling a block-downsampled render reads as crisp hard
    /// blocks (the actual visual point of `Artboard::resolution_scale`)
    /// instead of a blurry linear-filtered stretch.
    post_sampler_nearest: wgpu::Sampler,
    /// Draws a raster `Image` node's textured quad — Normal-blend only (see
    /// [`DrawCall::texture`]'s doc comment). A separate pipeline layout from
    /// `pipeline_layout` (not a variant folded into the existing
    /// `BlendMode`-keyed `pipelines` map) because it binds a second bind
    /// group (`image_bind_group_layout`) no flat-fill/gradient draw needs at
    /// all.
    textured_pipeline: wgpu::RenderPipeline,
    /// Shader-graph material plumbing, built on first use
    /// (`Renderer::material_resources`).
    material_resources: std::cell::OnceCell<MaterialResources>,
    /// The 3D pass's GPU state and this frame's pending scene
    /// (`Renderer::set_scene_3d` — see `renderer/three_d.rs`).
    state_3d: three_d::State3D,
    /// Texture + sampler bind group layout for [`GpuImageTexture`] — no
    /// uniform buffer entry here (unlike `post_bind_group_layout`): an
    /// `Image` draw's transform/tint still comes through the ordinary
    /// per-draw uniform at group 0, this group only ever adds group 1.
    image_bind_group_layout: wgpu::BindGroupLayout,
    /// [`Renderer::bake_image`]'s dedicated pipeline — see where it's built
    /// for why this can't just be `post_replace_pipeline`.
    bake_pipeline: wgpu::RenderPipeline,
    /// 2D lighting's pipelines/defaults (`renderer/lighting.rs`).
    lighting: lighting::LightingResources,
    /// 2D lighting's light maps/shadow mask/lit output — sized with (and
    /// rebuilt alongside) the post targets above.
    lighting_targets: lighting::LightingTargets,
}

/// `Depth24PlusStencil8` rather than a stencil-only format like `Stencil8`:
/// it's the one combined depth/stencil format WebGPU (and every native
/// backend wgpu targets) guarantees support for without an extra feature
/// flag — this renderer never actually *uses* the depth aspect (see
/// `no_depth_write_stencil_state` below for the pipeline state that leaves
/// it inert), but picking the universally-supported format over a
/// stencil-only one avoids a portability question this crate doesn't need to
/// answer yet.
const STENCIL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

/// The real scene's own offscreen/scratch targets (`post_scene_view` and
/// the post-effect chain's intermediate ping/pong steps) clear to this,
/// not the editor's opaque background color — the grid/chrome now owns
/// that (drawn straight to the swapchain first, see `render_with_masks`'s
/// own doc comment), and the real content needs a transparent base so its
/// own empty regions correctly show the grid through once the final
/// post-effect pass alpha-composites back over it, instead of every
/// intermediate stage silently baking in an opaque background.
const TRANSPARENT: wgpu::Color = wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

/// One sRGB-encoded channel (what a color picker / hex code means) to linear
/// light. Every color the engine is handed — fills, gradient stops, text,
/// particles, lights, clear/background, vignette — is authored in sRGB, while
/// shading and blending happen in linear space and the swapchain is an sRGB
/// view that re-encodes on write. Values above 1 (HDR tints) keep growing.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// [`srgb_to_linear`] on RGB, alpha untouched.
pub fn linear_rgba(c: [f32; 4]) -> [f32; 4] {
    [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), c[3]]
}

/// A clear color as the sRGB render target expects it (linear).
fn linear_clear(c: wgpu::Color) -> wgpu::Color {
    let f = |v: f64| srgb_to_linear(v as f32) as f64;
    wgpu::Color { r: f(c.r), g: f(c.g), b: f(c.b), a: c.a }
}

/// Builds a stencil-only `DepthStencilState` against [`STENCIL_FORMAT`] —
/// not via `wgpu::DepthStencilState::stencil()`, whose doc comment says
/// `depth_write_enabled`/`depth_compare` can be left `None` for a
/// stencil-only *use*, but wgpu 30's actual validation rejects `None` for
/// `Depth24PlusStencil8` specifically (a combined format, unlike a true
/// stencil-only format) with "Depth write enabled must be specified for
/// depth format Depth24PlusStencil8" — discovered by that exact validation
/// panic the first time this ran. `Some(false)`/`Some(CompareFunction::Always)`
/// is the explicit "don't do anything with the depth aspect" spelling that
/// actually passes validation for this format.
fn no_depth_write_stencil_state(stencil: wgpu::StencilState) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: STENCIL_FORMAT,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Always),
        stencil,
        bias: wgpu::DepthBiasState::default(),
    }
}

fn create_stencil_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("an1mbyt3 mask stencil texture"),
        size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: STENCIL_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// The offscreen target `render_with_post_effect`/`render_to_texture` render
/// the real scene into — `TEXTURE_BINDING` (so the post-process pass can
/// sample it back) and `COPY_SRC` (so `render_to_texture` can read it back
/// to CPU memory) on top of the usual `RENDER_ATTACHMENT`. Returns the
/// `Texture` itself alongside its view since `render_to_texture`'s
/// `copy_texture_to_buffer` needs the texture, not just the view.
fn create_post_scene_view(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("an1mbyt3 post-process scene texture"),
        size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Every size-dependent offscreen resource `render_with_post_effects` needs,
/// bundled so building them (constructor) and rebuilding them (`resize`,
/// `set_resolution_scale`) share one function instead of four/five separate
/// calls kept manually in sync. Sized independently of the swapchain itself
/// — see [`Renderer::set_resolution_scale`]'s own doc comment for why: at
/// `resolution_scale > 1.0`, every texture here is *smaller* than the real
/// surface (so every post effect runs over fewer pixels, not needlessly at
/// full native size only to be thrown away by pixelation anyway), and only
/// the post-effect chain's very last pass (writing to the swapchain view,
/// which isn't part of this bundle) upscales back out.
struct PostTargets {
    scene_texture: wgpu::Texture,
    scene_view: wgpu::TextureView,
    ping_view: wgpu::TextureView,
    pong_view: wgpu::TextureView,
    blur_intermediate_view: wgpu::TextureView,
    /// A stencil buffer matching *this* bundle's own (possibly reduced)
    /// size — `render_with_masks`' `self.stencil_view` is swapchain-sized
    /// and can't be reused here (a render pass's depth/stencil attachment
    /// must match its color attachment's dimensions exactly).
    stencil_view: wgpu::TextureView,
}

fn build_post_targets(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> PostTargets {
    let (scene_texture, scene_view) = create_post_scene_view(device, format, width, height);
    let (_, ping_view) = create_post_scene_view(device, format, width, height);
    let (_, pong_view) = create_post_scene_view(device, format, width, height);
    let (_, blur_intermediate_view) = create_post_scene_view(device, format, width, height);
    let stencil_view = create_stencil_view(device, width, height);
    PostTargets { scene_texture, scene_view, ping_view, pong_view, blur_intermediate_view, stencil_view }
}

/// Builds one [`PostBinding`] against `scene_view` — called fresh per active
/// effect per frame by `Renderer::render_with_post_effects` (see that
/// method's doc comment for why no persistent binding is kept around
/// instead), since a bind group captures a specific `TextureView` and which
/// view is "the source" changes at every step of a multi-effect chain.
fn create_post_binding(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    scene_view: &wgpu::TextureView,
) -> PostBinding {
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("an1mbyt3 post-process uniform buffer"),
        size: std::mem::size_of::<PostUniforms>() as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("an1mbyt3 post-process bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(scene_view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            wgpu::BindGroupEntry { binding: 2, resource: uniform_buffer.as_entire_binding() },
        ],
    });
    PostBinding { uniform_buffer, bind_group }
}

/// One operation in a mask-aware scene, in the exact order
/// [`Renderer::render_with_masks`] must submit them — built by a caller
/// walking its own scene tree depth-first (`WasmEditor::render`), entering
/// a `WriteMask` right before that group's own masked content and
/// returning to the shallower `depth` on the way back out of it.
pub enum SceneOp<'a> {
    /// Writes `mask`'s coverage into the stencil buffer, promoting it from
    /// `depth` to `depth + 1` wherever it's covered *and* the buffer
    /// already held `depth` — see `mask_write_stencil_face`'s own doc
    /// comment for why this composes correctly for nested masks. Every
    /// `Draw`/nested `WriteMask` belonging to that group's own masked
    /// content must come after this in the list, at `depth + 1`. `island`
    /// identifies which independent, not-nested-under-anything-else-masked
    /// top-level masked subtree this belongs to — see its own doc comment
    /// on [`Draw`](SceneOp::Draw) for why this exists at all.
    WriteMask { mask: DrawCall<'a>, depth: u32, island: u32 },
    /// A plain draw, visible only where the stencil buffer currently equals
    /// `depth` — i.e., inside every mask this content is actually nested
    /// under, and none it isn't. `island` is a caller-assigned id, unique
    /// per independent top-level masked subtree (every root-level masked
    /// `Group`/`clip_children` node starts a new one; nested masks inside it
    /// inherit the same id) — `depth` alone isn't enough to keep two
    /// *sibling* masked subtrees from cross-contaminating each other's
    /// stencil state: both start at the same small `depth` (1, 2, ...), so
    /// without some other distinguishing key, whichever one's `WriteMask`
    /// ran first leaves the stencil buffer in a state the other's content
    /// test can't tell apart from its own mask — any screen-space overlap
    /// between the two (easy to hit right after duplicating a masked node,
    /// since the copy usually starts near/overlapping the original) leaks
    /// one's content into the other's mask region. `Renderer::draw_masked_scene`
    /// groups ops by this id and gives each its own stencil-cleared render
    /// pass specifically to prevent that.
    Draw { draw: DrawCall<'a>, depth: u32, island: u32 },
}

impl SceneOp<'_> {
    fn island(&self) -> u32 {
        match self {
            SceneOp::WriteMask { island, .. } | SceneOp::Draw { island, .. } => *island,
        }
    }
}

impl Renderer {
    /// Requests a GPU adapter/device compatible with `surface` and builds the
    /// (currently single, flat-fill) render pipeline, with an opaque
    /// background — the right choice for a native window surface, which
    /// nearly always only supports `CompositeAlphaMode::Opaque` anyway.
    ///
    /// Async because wgpu's adapter/device request is async on every target —
    /// native callers use `pollster::block_on` (see the `render_shape`
    /// example), the WASM runtime (Phase 8) simply `.await`s it directly.
    pub async fn new(instance: &wgpu::Instance, surface: wgpu::Surface<'static>, width: u32, height: u32) -> Self {
        Self::new_with_alpha_mode(instance, Some(surface), width, height, false).await
    }

    /// Same as [`Renderer::new`], but asks the surface for an alpha-transparent
    /// `CompositeAlphaMode` (`PreMultiplied`/`PostMultiplied`) instead of
    /// `Opaque` — so a clear color with `alpha: 0.0` actually shows whatever
    /// is behind the surface rather than opaque black. This is what lets a
    /// web `<canvas>` sit directly on a styled page (any background color,
    /// gradient, or image) with only the drawn shapes visible, instead of
    /// looking like a separate boxed widget — the whole point of a
    /// browser-embeddable runtime (AN1MBYT3_BUILD_PLAN.md §8's "consumers
    /// drop this into their own page" model).
    ///
    /// Falls back to an opaque surface (returning `false` from
    /// [`Renderer::supports_transparency`]) if the adapter's surface
    /// capabilities don't list a transparent alpha mode — native window
    /// surfaces typically don't, so this is really only expected to report
    /// `true` for a web canvas target, but callers get an honest answer
    /// instead of a silent wrong-looking render.
    pub async fn new_transparent(
        instance: &wgpu::Instance,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
    ) -> Self {
        Self::new_with_alpha_mode(instance, Some(surface), width, height, true).await
    }

    /// Builds a renderer with no window/canvas surface at all — every frame
    /// is drawn via [`Renderer::render_to_texture`] into an in-memory
    /// buffer, never presented to any OS surface. For embedding contexts
    /// where there's no `wgpu::Surface` to have in the first place: the
    /// Tauri desktop editor's canvas is an ordinary HTML `<canvas>` element
    /// inside a WebView (AN1MBYT3_BUILD_PLAN.md §15), not a native window
    /// surface this crate could create a `wgpu::Surface` against — a real
    /// native-compositing approach (a second child window carrying an
    /// actual swapchain, z-ordered under a transparent WebView) is possible
    /// future work, but is Windows-specific native-windowing work on its
    /// own; this readback-based approach works identically on every
    /// platform Tauri supports, at the cost of a per-frame GPU-to-CPU copy.
    ///
    /// `render`/`render_with_mask`/`render_with_post_effect` all panic if
    /// called on a headless renderer (see `self.surface`'s field doc comment).
    pub async fn new_headless(instance: &wgpu::Instance, width: u32, height: u32) -> Self {
        Self::new_with_alpha_mode(instance, None, width, height, false).await
    }

    /// Reports whether this renderer's surface is actually configured for
    /// alpha transparency — only ever `true` after [`Renderer::new_transparent`]
    /// *and* only if the adapter supported it. Callers should branch their
    /// clear color's alpha on this rather than assuming `new_transparent`
    /// always succeeds.
    pub fn supports_transparency(&self) -> bool {
        self.transparent
    }

    /// Every windowed draw path (`render`/`render_with_mask`/
    /// `render_with_post_effect`) goes through this instead of touching
    /// `self.surface` directly — calling any of them on a
    /// [`Renderer::new_headless`] renderer is a caller bug (there is no
    /// surface to draw to at all; use [`Renderer::render_to_texture`]
    /// instead), so it panics with a clear message rather than an opaque
    /// `Option::unwrap` failure.
    fn surface(&self) -> &wgpu::Surface<'static> {
        self.surface
            .as_ref()
            .expect("this Renderer has no window surface (built via Renderer::new_headless) — use render_to_texture instead")
    }

    async fn new_with_alpha_mode(
        instance: &wgpu::Instance,
        surface: Option<wgpu::Surface<'static>>,
        width: u32,
        height: u32,
        want_transparent: bool,
    ) -> Self {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: surface.as_ref(),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .expect("an1mbyt3-renderer: no suitable GPU adapter found for this surface");

        // WebGL2 (the fallback where a browser has no WebGPU) can't meet
        // WebGPU's default limits (e.g. 16 KB uniform bindings vs 64 KB,
        // fewer bind groups) — ask for exactly what the adapter offers
        // there instead of failing device creation.
        let is_gl = adapter.get_info().backend == wgpu::Backend::Gl;
        let required_limits = if is_gl { adapter.limits() } else { wgpu::Limits::default() };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("an1mbyt3-renderer device"),
                required_features: wgpu::Features::empty(),
                required_limits,
                ..Default::default()
            })
            .await
            .expect("an1mbyt3-renderer: failed to create GPU device");

        let (config, transparent) = match &surface {
            Some(surface) => {
                let mut config = surface
                    .get_default_config(&adapter, width, height)
                    .expect("an1mbyt3-renderer: surface is not supported by the chosen adapter");

                let transparent = if want_transparent {
                    // On wasm32, `surface.get_capabilities(&adapter).alpha_modes`
                    // cannot be trusted: wgpu 30's own web backends hard-code it to
                    // `[Opaque]` regardless of what the browser actually supports —
                    // the WebGPU backend (`wgpu/src/backend/webgpu.rs`) reports only
                    // `Opaque` from `get_capabilities` even though its own
                    // `configure()` correctly maps `PreMultiplied` to the browser's
                    // `GpuCanvasAlphaMode::Premultiplied`; the WebGL2 backend
                    // (`wgpu-hal/src/gles/adapter.rs`) reports the same `[Opaque]`
                    // next to a literal `//TODO` comment, while a WebGL2 canvas is
                    // alpha-capable by default per spec regardless. Gating on the
                    // (wrong) reported capabilities here is exactly what first
                    // produced an opaque black box instead of a transparent canvas —
                    // so on wasm32, request `PreMultiplied` unconditionally instead
                    // of trusting `get_capabilities()`. Native surfaces (window
                    // compositors) genuinely do only support `Opaque` in practice, so
                    // the capability check stays real there.
                    #[cfg(target_arch = "wasm32")]
                    {
                        config.alpha_mode = wgpu::CompositeAlphaMode::PreMultiplied;
                        true
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let caps = surface.get_capabilities(&adapter);
                        let transparent_mode = caps.alpha_modes.iter().copied().find(|mode| {
                            matches!(mode, wgpu::CompositeAlphaMode::PreMultiplied | wgpu::CompositeAlphaMode::PostMultiplied)
                        });
                        match transparent_mode {
                            Some(mode) => {
                                config.alpha_mode = mode;
                                true
                            }
                            None => false,
                        }
                    }
                } else {
                    false
                };

                // Render through an sRGB *view* of the swapchain (WebGPU's
                // preferred canvas format, `bgra8unorm`, has no sRGB
                // encoding): shaders output linear light and the view
                // encodes it on write, so images show their true colors.
                // WebGL2 can't reinterpret a surface through a different
                // view format; it offers an sRGB surface format directly,
                // so use that (same result: linear in, sRGB on screen).
                if is_gl {
                    if let Some(format) = surface.get_capabilities(&adapter).formats.iter().copied().find(|f| f.is_srgb()) {
                        config.format = format;
                    }
                } else {
                    let srgb = config.format.add_srgb_suffix();
                    if srgb != config.format && !config.view_formats.contains(&srgb) {
                        config.view_formats.push(srgb);
                    }
                }
                surface.configure(&device, &config);
                (config, transparent)
            }
            None => {
                // Headless: there's no real surface to negotiate a
                // format/alpha-mode against, so the config is just built by
                // hand. `Rgba8UnormSrgb` because it's a universally
                // supported render-attachment + texture-binding + copy-src
                // format — exactly what `render_to_texture`'s readback needs.
                let config = wgpu::SurfaceConfiguration {
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    color_space: wgpu::SurfaceColorSpace::Srgb,
                    width: width.max(1),
                    height: height.max(1),
                    present_mode: wgpu::PresentMode::Fifo,
                    desired_maximum_frame_latency: 2,
                    alpha_mode: wgpu::CompositeAlphaMode::Opaque,
                    view_formats: Vec::new(),
                };
                (config, false)
            }
        };

        let render_format = config.format.add_srgb_suffix();
        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("an1mbyt3 per-draw uniform layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("an1mbyt3 pipeline layout"),
            bind_group_layouts: &[Some(&uniform_bind_group_layout)],
            immediate_size: 0,
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        // One pipeline per `BlendMode` — everything about them is identical
        // (same shader, same vertex layout, same target format) except the
        // one `ColorTargetState::blend` value, which is why this loop exists
        // instead of four hand-duplicated `create_render_pipeline` calls.
        let pipelines = ALL_BLEND_MODES
            .into_iter()
            .map(|blend_mode| {
                let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("an1mbyt3 flat-fill pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &[
                            Some(wgpu::VertexBufferLayout {
                                array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                                step_mode: wgpu::VertexStepMode::Vertex,
                                attributes: &[wgpu::VertexAttribute {
                                    format: wgpu::VertexFormat::Float32x2,
                                    offset: 0,
                                    shader_location: 0,
                                }],
                            }),
                            // Every pipeline using `vs_main` binds this slot
                            // unconditionally (`GpuMesh::uv_buffer`'s own doc
                            // comment) — `fs_main` itself never reads `in.uv`,
                            // but `vs_main`'s one shared `VertexOutput` always
                            // forwards it, so the vertex stage needs it bound
                            // regardless of which fragment entry point follows.
                            Some(wgpu::VertexBufferLayout {
                                array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                                step_mode: wgpu::VertexStepMode::Vertex,
                                attributes: &[wgpu::VertexAttribute {
                                    format: wgpu::VertexFormat::Float32x2,
                                    offset: 0,
                                    shader_location: 1,
                                }],
                            }),
                        ],
                    },
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: render_format,
                            blend: Some(wgpu_blend_state(blend_mode)),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                });
                (blend_mode, pipeline)
            })
            .collect();

        let vertex_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 0 }],
        };
        // See the flat-fill loop's own matching buffer literal above for why
        // every pipeline binds this slot unconditionally.
        let uv_buffer_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 1 }],
        };

        // Raster `Image` nodes: a second bind group layout (texture +
        // sampler, no uniform buffer — group 0's existing per-draw uniform
        // already carries this draw's transform/tint) and a dedicated
        // pipeline sharing `shader.wgsl`'s `vs_main` but sampling through
        // its `fs_textured` entry point instead of `fs_main`. Normal-blend
        // only for this pass (see `DrawCall::texture`'s doc comment).
        let image_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("an1mbyt3 image texture bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let textured_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("an1mbyt3 textured pipeline layout"),
            bind_group_layouts: &[Some(&uniform_bind_group_layout), Some(&image_bind_group_layout)],
            immediate_size: 0,
        });
        let textured_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("an1mbyt3 textured pipeline"),
            layout: Some(&textured_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(vertex_buffer_layout.clone()), Some(uv_buffer_layout.clone())],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_textured"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: render_format,
                    blend: Some(wgpu_blend_state(BlendMode::Normal)),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        // Stencil state shared by both masking pipelines' `front`/`back`
        // faces — this renderer draws 2D shapes with no concept of
        // back-facing triangles to treat differently, so both faces always
        // behave identically.
        // `compare: Equal` (against whatever `set_stencil_reference` was
        // last set to, per draw) + `pass_op: IncrementClamp` — not
        // `Always`/`Replace` — is what lets masks *nest*: writing a mask at
        // depth `D+1` only succeeds where the stencil buffer already holds
        // `D` (i.e., inside every ancestor mask's own region already), so a
        // nested mask's effective clip region is automatically intersected
        // with its ancestors' rather than overwriting them. A single,
        // non-nested mask (the only case this pipeline handled before
        // nesting existed) is unaffected: written against the buffer's
        // cleared `0`, `compare: Equal(0)` is just as true as `Always` was,
        // and `IncrementClamp` from `0` lands on the exact same `1`
        // `Replace` used to write directly — see `Renderer::render_with_masks`'s
        // own doc comment for how the reference actually gets threaded
        // through a whole (possibly nested) scene.
        let mask_write_stencil_face = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Equal,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::IncrementClamp,
        };
        let mask_write_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("an1mbyt3 mask-write pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(vertex_buffer_layout.clone()), Some(uv_buffer_layout.clone())],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(no_depth_write_stencil_state(wgpu::StencilState {
                front: mask_write_stencil_face,
                back: mask_write_stencil_face,
                read_mask: 0xff,
                write_mask: 0xff,
            })),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_mask_write"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                // Only the stencil buffer is written by this pass — the
                // mask shape itself is never meant to be visible, only its
                // coverage.
                targets: &[Some(wgpu::ColorTargetState {
                    format: render_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::empty(),
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        // `mask_write_textured_pipeline`'s own doc comment (on the struct
        // field) has the full reasoning — same shape as `mask_write_pipeline`
        // above, just `textured_pipeline_layout`/`fs_textured_mask_write`
        // instead of the plain flat/gradient layout and entry point.
        let mask_write_textured_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("an1mbyt3 mask-write textured pipeline"),
            layout: Some(&textured_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(vertex_buffer_layout.clone()), Some(uv_buffer_layout.clone())],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(no_depth_write_stencil_state(wgpu::StencilState {
                front: mask_write_stencil_face,
                back: mask_write_stencil_face,
                read_mask: 0xff,
                write_mask: 0xff,
            })),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_textured_mask_write"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: render_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::empty(),
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let masked_content_stencil_face = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Equal,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Keep,
        };
        // One pipeline per `BlendMode`, same reasoning as the unmasked
        // `pipelines` map (`masked_content_pipelines`'s own doc comment on
        // the struct field has the fuller story on why a single Normal-only
        // pipeline here was a real bug, not just an intentional narrowing).
        let masked_content_pipelines: std::collections::HashMap<BlendMode, wgpu::RenderPipeline> = ALL_BLEND_MODES
            .into_iter()
            .map(|blend_mode| {
                let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("an1mbyt3 masked-content pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &[Some(vertex_buffer_layout.clone()), Some(uv_buffer_layout.clone())],
                    },
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: Some(no_depth_write_stencil_state(wgpu::StencilState {
                        front: masked_content_stencil_face,
                        back: masked_content_stencil_face,
                        read_mask: 0xff,
                        write_mask: 0x00,
                    })),
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: render_format,
                            blend: Some(wgpu_blend_state(blend_mode)),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                });
                (blend_mode, pipeline)
            })
            .collect();

        // `masked_content_textured_pipeline`'s own doc comment (on the
        // struct field) has the full reasoning — Normal-blend only, unlike
        // `masked_content_pipelines` above, matching `textured_pipeline`'s
        // own existing Normal-only scope for every textured draw (masked or
        // not) — `DrawUniforms::from_draw` never computes a blend
        // correction for a textured draw either, so there's no mismatched
        // shader-assumes-a-different-pipeline bug to fix here the way there
        // was for flat/gradient masked content. Same
        // `textured_pipeline_layout`/`fs_textured` shape otherwise.
        let masked_content_textured_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("an1mbyt3 masked-content textured pipeline"),
            layout: Some(&textured_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(vertex_buffer_layout), Some(uv_buffer_layout)],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(no_depth_write_stencil_state(wgpu::StencilState {
                front: masked_content_stencil_face,
                back: masked_content_stencil_face,
                read_mask: 0xff,
                write_mask: 0x00,
            })),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_textured"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: render_format,
                    blend: Some(wgpu_blend_state(BlendMode::Normal)),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let stencil_view = create_stencil_view(&device, config.width, config.height);

        // Post-processing (Phase 11): a separate bind group layout/pipeline
        // layout/shader from the shape-drawing setup above, since a
        // full-screen post-process draw samples a texture and needs no
        // per-vertex position input at all (see `post_process.wgsl`'s
        // full-screen-triangle trick), unlike every shape `DrawCall`.
        let post_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("an1mbyt3 post-process bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let post_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("an1mbyt3 post-process pipeline layout"),
            bind_group_layouts: &[Some(&post_bind_group_layout)],
            immediate_size: 0,
        });
        let post_shader = device.create_shader_module(wgpu::include_wgsl!("post_process.wgsl"));

        let build_post_pipeline = |label: &str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&post_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &post_shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &post_shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: render_format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let post_replace_pipeline = build_post_pipeline("an1mbyt3 post-process replace pipeline", None);
        // A separate pipeline (not `post_replace_pipeline`) for
        // `Renderer::bake_image`, fixed to `Rgba8UnormSrgb` rather than
        // `config.format`: bake_image's readback interprets the result as
        // tightly-packed RGBA bytes (the same layout `upload_texture`
        // expects on the way back in), which only holds if the target
        // texture's channel order is actually RGBA — `config.format` can be
        // BGRA-ordered on some native backends, which would silently swap
        // red/blue in the baked result. Baking always writes to its own
        // internally-owned offscreen texture (never presented), so it's free
        // to pick this fixed format regardless of what the live swapchain uses.
        let bake_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("an1mbyt3 bake_image pipeline"),
            layout: Some(&post_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &post_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &post_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8UnormSrgb, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let post_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("an1mbyt3 post-process sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let post_sampler_nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("an1mbyt3 post-process nearest sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let post_targets = build_post_targets(&device, render_format, config.width, config.height);
        let lighting = lighting::build_lighting_resources(&device, &queue, render_format, &image_bind_group_layout);
        let lighting_targets = lighting::build_lighting_targets(&device, render_format, config.width, config.height);
        let post = post::PostChain::new(&device, render_format, &post_bind_group_layout, config.width, config.height);

        Self {
            uniform_pool: std::rc::Rc::new(std::cell::RefCell::new(UniformPool::new(&device))),
            surface,
            device,
            queue,
            config,
            render_format,
            pipelines,
            mask_write_pipeline,
            mask_write_textured_pipeline,
            masked_content_pipelines,
            masked_content_textured_pipeline,
            stencil_view,
            uniform_bind_group_layout,
            transparent,
            post_replace_pipeline,
            post_bind_group_layout,
            post_sampler,
            post_sampler_nearest,
            post_scene_view: post_targets.scene_view,
            post_scene_texture: post_targets.scene_texture,
            post_ping_view: post_targets.ping_view,
            post_pong_view: post_targets.pong_view,
            post_blur_intermediate_view: post_targets.blur_intermediate_view,
            post_stencil_view: post_targets.stencil_view,
            current_resolution_scale: 1.0,
            dynamic_scale: 1.0,
            post,
            textured_pipeline,
            material_resources: std::cell::OnceCell::new(),
            state_3d: Default::default(),
            image_bind_group_layout,
            bake_pipeline,
            lighting,
            lighting_targets,
        }
    }

    /// Reconfigures the surface after the window/canvas resizes. A no-op for
    /// zero sizes (e.g. a minimized window) rather than a panic, since
    /// `SurfaceConfiguration` requires nonzero dimensions.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        // A headless renderer (`Renderer::new_headless`) has no surface to
        // reconfigure — resizing it just means resizing the offscreen
        // target below, which every case needs regardless.
        if let Some(surface) = &self.surface {
            surface.configure(&self.device, &self.config);
        }
        // The stencil buffer backing `render_with_mask` must match the
        // swapchain's own dimensions (a `RenderPassDepthStencilAttachment`'s
        // view has to match the color attachment's size) — recreated here
        // for the same reason `surface.configure` is, just for a texture
        // this renderer owns directly instead of one the surface owns.
        self.stencil_view = create_stencil_view(&self.device, width, height);

        // Same reasoning as the stencil buffer above, for the post-effect
        // offscreen targets `render_with_post_effects`/`render_to_texture`
        // use — rebuilt at `current_resolution_scale` (not necessarily
        // native `width`/`height`), so a window resize doesn't silently
        // reset an artboard's `resolution_scale` back to native.
        self.rebuild_post_targets();
    }

    /// Rebuilds `post_scene_view` and its sibling offscreen targets (see
    /// [`PostTargets`]) at `self.current_resolution_scale` of the surface's
    /// current `width`/`height` — the shared implementation behind
    /// [`Renderer::resize`] (surface size changed, same scale) and
    /// [`Renderer::set_resolution_scale`] (scale changed, same surface
    /// size).
    fn rebuild_post_targets(&mut self) {
        let (scaled_width, scaled_height) = self.post_target_dims();
        let targets = build_post_targets(&self.device, self.render_format, scaled_width, scaled_height);
        self.post_scene_texture = targets.scene_texture;
        self.post_scene_view = targets.scene_view;
        self.post_ping_view = targets.ping_view;
        self.post_pong_view = targets.pong_view;
        self.post_blur_intermediate_view = targets.blur_intermediate_view;
        self.post_stencil_view = targets.stencil_view;
        self.lighting_targets = lighting::build_lighting_targets(&self.device, self.render_format, scaled_width, scaled_height);
        self.post.resize(&self.device, self.render_format, scaled_width, scaled_height);
    }

    /// Sets `Artboard::resolution_scale`'s current value — a pixel block
    /// size, in absolute pixels, not a fraction of surface size: `1.0` (the
    /// default, and every artboard's value before this field existed) means
    /// native/no pixelation; larger values (e.g. `8.0`) divide every
    /// offscreen post-effect target down by that factor, so a block of that
    /// many *native* pixels ends up sampled from one texel once the final
    /// pass below nearest-upscales back out — independent of the
    /// canvas/window's own size, since the divide is always against the
    /// surface's current native size, not some other fixed reference. Scene
    /// *and* every post effect render into this smaller buffer — effects run
    /// over fewer pixels rather than needlessly at full native resolution
    /// only to be discarded by pixelation anyway. A no-op if `scale` already
    /// matches the current value (avoids reallocating four GPU textures on
    /// every frame a caller re-asserts the same value — see this method's
    /// caller in `an1mbyt3-wasm` for why that matters: it's called once per
    /// frame, unconditionally, with whatever the active artboard's current
    /// `resolution_scale` happens to be).
    /// The pixel block size the scene renders at (1 = native).
    pub fn resolution_scale(&self) -> f32 {
        self.current_resolution_scale
    }

    pub fn set_resolution_scale(&mut self, scale: f32) {
        let clamped = scale.clamp(1.0, 64.0);
        if clamped == self.current_resolution_scale {
            return;
        }
        self.current_resolution_scale = clamped;
        self.rebuild_post_targets();
    }

    /// `post_scene_view`'s (and its siblings') actual current size — the
    /// swapchain's own `config.width`/`height` divided by
    /// `current_resolution_scale`, floored to at least `1`. Shared by
    /// `rebuild_post_targets` (what size to build them at) and
    /// `render_with_post_effects` (what `texel_size` to hand the shader —
    /// every post-process pass samples *these* targets, never the
    /// swapchain directly, so it's this size that matters for correct
    /// per-texel math, not the native surface size).
    fn post_target_dims(&self) -> (u32, u32) {
        let scale = self.dynamic_scale / self.current_resolution_scale;
        let width = ((self.config.width as f32) * scale).max(1.0) as u32;
        let height = ((self.config.height as f32) * scale).max(1.0) as u32;
        (width, height)
    }

    /// Native pixels per post-target pixel (resolution block size ÷
    /// adaptive scale) — for effects sized in screen pixels (2D light
    /// softness) so they look the same whatever the internal resolution.
    pub(crate) fn post_pixel_scale(&self) -> f32 {
        self.current_resolution_scale / self.dynamic_scale
    }

    /// **Adaptive resolution**: render the scene and post effects at
    /// `scale` (0.4 … 1, snapped to 1/20 steps so small adjustments don't
    /// reallocate targets every frame) of native size, upscaled smoothly to
    /// the window. The host lowers it when frames run long and raises it
    /// back when there's headroom, so a big window or a heavy post stack
    /// costs resolution, not frame rate. `1.0` = native (and the fast path
    /// straight to the swapchain when nothing else needs the offscreen
    /// targets).
    pub fn set_dynamic_scale(&mut self, scale: f32) {
        let snapped = ((scale.clamp(0.4, 1.0) * 20.0).round() / 20.0).clamp(0.4, 1.0);
        if snapped == self.dynamic_scale {
            return;
        }
        self.dynamic_scale = snapped;
        self.rebuild_post_targets();
    }

    /// The current adaptive resolution scale (see [`Renderer::set_dynamic_scale`]).
    pub fn dynamic_scale(&self) -> f32 {
        self.dynamic_scale
    }

    /// Uploads `mesh`'s vertex/index data to the GPU once. See [`GpuMesh`]'s
    /// doc comment for why this is separate from drawing.
    pub fn upload_mesh(&self, mesh: &Mesh) -> GpuMesh {
        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("an1mbyt3 mesh vertex buffer"),
            contents: bytemuck::cast_slice(&mesh.positions),
            // COPY_DST even though most meshes never get rewritten after
            // upload — skinned meshes (`an1mbyt3_geometry::skin_mesh`,
            // Phase 5) need `update_mesh_positions` to rewrite this buffer
            // every frame, and there's no benefit to a separate "dynamic"
            // upload path just to omit one usage flag on the common case.
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("an1mbyt3 mesh index buffer"),
            contents: bytemuck::cast_slice(&mesh.indices),
            // COPY_DST: `rewrite_mesh` refills it in place.
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        });

        // Real per-vertex UVs only when `mesh.uvs` actually has one entry per
        // vertex — otherwise a same-length zero-filled buffer, so every
        // pipeline's vertex state can unconditionally bind this slot (`GpuMesh::uv_buffer`'s
        // own doc comment) regardless of which meshes happen to carry real UVs.
        let has_vertex_uv = !mesh.uvs.is_empty() && mesh.uvs.len() == mesh.positions.len();
        let zero_uvs;
        let uv_source: &[[f32; 2]] = if has_vertex_uv {
            &mesh.uvs
        } else {
            zero_uvs = vec![[0.0f32, 0.0]; mesh.positions.len()];
            &zero_uvs
        };
        let uv_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("an1mbyt3 mesh uv buffer"),
            contents: bytemuck::cast_slice(uv_source),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let mut local_min = [f32::MAX, f32::MAX];
        let mut local_max = [f32::MIN, f32::MIN];
        for &[x, y] in &mesh.positions {
            local_min = [local_min[0].min(x), local_min[1].min(y)];
            local_max = [local_max[0].max(x), local_max[1].max(y)];
        }
        if mesh.positions.is_empty() {
            local_min = [-0.5, -0.5];
            local_max = [0.5, 0.5];
        }

        GpuMesh { vertex_buffer, uv_buffer, has_vertex_uv, index_buffer, index_count: mesh.indices.len() as u32, local_min, local_max }
    }

    /// Refills `gpu` with `mesh` in place (no new GPU buffers) when it fits
    /// in `gpu`'s existing buffers; otherwise replaces `gpu` with a fresh
    /// upload that has 50 % headroom, so a mesh that changes every frame
    /// (a liquid's outline) settles into reusing its buffers. Never use on
    /// a `GpuMesh` other draws still read this frame. A mesh with UVs
    /// (one per vertex) keeps them; one without must stay without.
    pub fn rewrite_mesh(&self, gpu: &mut GpuMesh, mesh: &Mesh) {
        let vbytes = (mesh.positions.len() * 8) as u64;
        let ibytes = (mesh.indices.len() * 4) as u64;
        let with_uv = !mesh.uvs.is_empty() && mesh.uvs.len() == mesh.positions.len();
        let fits = gpu.has_vertex_uv == with_uv && vbytes <= gpu.vertex_buffer.size() && ibytes <= gpu.index_buffer.size() && vbytes > 0 && ibytes > 0;
        if !fits {
            *gpu = self.upload_mesh_with_headroom(mesh, 1.5);
            return;
        }
        if with_uv {
            self.queue.write_buffer(&gpu.uv_buffer, 0, bytemuck::cast_slice(&mesh.uvs));
        }
        self.queue.write_buffer(&gpu.vertex_buffer, 0, bytemuck::cast_slice(&mesh.positions));
        self.queue.write_buffer(&gpu.index_buffer, 0, bytemuck::cast_slice(&mesh.indices));
        gpu.index_count = mesh.indices.len() as u32;
        let mut local_min = [f32::MAX, f32::MAX];
        let mut local_max = [f32::MIN, f32::MIN];
        for &[x, y] in &mesh.positions {
            local_min = [local_min[0].min(x), local_min[1].min(y)];
            local_max = [local_max[0].max(x), local_max[1].max(y)];
        }
        gpu.local_min = local_min;
        gpu.local_max = local_max;
    }

    /// [`Renderer::upload_mesh`] with buffers `headroom`× larger than
    /// needed (zero-padded), for meshes [`Renderer::rewrite_mesh`] refills.
    pub fn upload_mesh_with_headroom(&self, mesh: &Mesh, headroom: f32) -> GpuMesh {
        let grow = |n: usize| ((n as f32 * headroom.max(1.0)).ceil() as usize).max(n).max(3);
        let mut positions = mesh.positions.clone();
        positions.resize(grow(mesh.positions.len()), [0.0, 0.0]);
        let mut indices = mesh.indices.clone();
        let real = indices.len();
        indices.resize(grow(real), 0);
        let uvs = if !mesh.uvs.is_empty() && mesh.uvs.len() == mesh.positions.len() {
            let mut uvs = mesh.uvs.clone();
            uvs.resize(positions.len(), [0.0, 0.0]);
            uvs
        } else {
            Vec::new()
        };
        let mut gpu = self.upload_mesh(&Mesh { positions, indices, uvs });
        gpu.index_count = real as u32;
        let mut local_min = [f32::MAX, f32::MAX];
        let mut local_max = [f32::MIN, f32::MIN];
        for &[x, y] in &mesh.positions {
            local_min = [local_min[0].min(x), local_min[1].min(y)];
            local_max = [local_max[0].max(x), local_max[1].max(y)];
        }
        if !mesh.positions.is_empty() {
            gpu.local_min = local_min;
            gpu.local_max = local_max;
        }
        gpu
    }

    /// Allocates one instance's uniform buffer + bind group — see
    /// [`DrawBinding`]'s doc comment for why this is separate from
    /// [`Renderer::upload_mesh`]. Call once per `DrawCall` that will exist
    /// concurrently (e.g. once per component instance), not once per mesh.
    pub fn create_draw_binding(&self) -> DrawBinding {
        let (page, slot) = self.uniform_pool.borrow_mut().allocate(&self.device);
        let pool = self.uniform_pool.borrow();
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("an1mbyt3 draw bind group"),
            layout: &self.uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &pool.pages[page].buffer,
                    offset: (slot * pool.stride) as wgpu::BufferAddress,
                    size: wgpu::BufferSize::new(std::mem::size_of::<DrawUniforms>() as u64),
                }),
            }],
        });
        drop(pool);
        DrawBinding { pool: self.uniform_pool.clone(), page, slot, bind_group }
    }

    /// Uploads `rgba` (tightly packed, `width * height * 4` bytes) as a GPU
    /// texture ready to draw through a textured [`DrawCall`], sampled with
    /// `filter`. Used both for the live draw-time texture (whatever
    /// `image_cache` in `an1mbyt3-wasm` last baked) and inside
    /// [`Renderer::bake_image`] itself (which uploads the *source* pixels
    /// before running its one-shot effect pass).
    fn material_resources(&self) -> &MaterialResources {
        self.material_resources.get_or_init(|| {
            let globals_layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("an1mbyt3 material globals layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                }],
            });
            let globals_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("an1mbyt3 material globals"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let globals_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("an1mbyt3 material globals bind group"),
                layout: &globals_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals_buffer.as_entire_binding() }],
            });
            let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("an1mbyt3 material pipeline layout"),
                bind_group_layouts: &[Some(&self.uniform_bind_group_layout), Some(&self.image_bind_group_layout), Some(&globals_layout)],
                immediate_size: 0,
            });
            let white = self.upload_texture(1, 1, &[255, 255, 255, 255], ImageFilterMode::Nearest);
            MaterialResources { layout, globals_layout, globals_buffer, globals_bind_group, white }
        })
    }

    /// Sets the `Time` every shader-graph material sees (seconds). Call
    /// once per frame before drawing.
    pub fn set_shader_time(&self, seconds: f32) {
        let resources = self.material_resources();
        self.queue.write_buffer(&resources.globals_buffer, 0, bytemuck::cast_slice(&[seconds, 0.0f32, 0.0, 0.0]));
    }

    /// The full WGSL a material is built from: the sprite shader, the
    /// material glue, then `fragment` (an `an1mbyt3_core::compile_wgsl`
    /// result).
    pub fn material_shader_source(fragment: &str) -> String {
        let mut source = String::from(include_str!("shader.wgsl"));
        source.push_str(MATERIAL_GLUE);
        source.push_str(fragment);
        source
    }

    /// Checks a material's WGSL with naga (the same front end wgpu uses),
    /// returning a readable error instead of a GPU validation panic.
    pub fn validate_material(fragment: &str) -> Result<(), String> {
        let source = Self::material_shader_source(fragment);
        let module = naga::front::wgsl::parse_str(&source).map_err(|e| e.emit_to_string(&source))?;
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
            .validate(&module)
            .map_err(|e| e.emit_to_string(&source))?;
        Ok(())
    }

    /// Compiles a shader-graph material (validated first — an invalid graph
    /// is an `Err`, never a GPU error).
    pub fn compile_material(&self, fragment: &str) -> Result<GpuMaterial, String> {
        Self::validate_material(fragment)?;
        let source = Self::material_shader_source(fragment);
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("an1mbyt3 material shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let layout2 = |location| wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: if location == 0 {
                &[wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 0 }]
            } else {
                &[wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 1 }]
            },
        };
        // Built twice: unmasked (no depth/stencil) and masked (stencil
        // `Equal` against the clip's depth, like `masked_content_pipelines`).
        let build = |label: &str, depth_stencil: Option<wgpu::DepthStencilState>| {
            self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&self.material_resources().layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(layout2(0)), Some(layout2(1))],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_material"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: self.render_format, blend: Some(wgpu_blend_state(BlendMode::Normal)), write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let face = wgpu::StencilFaceState {
            compare: wgpu::CompareFunction::Equal,
            fail_op: wgpu::StencilOperation::Keep,
            depth_fail_op: wgpu::StencilOperation::Keep,
            pass_op: wgpu::StencilOperation::Keep,
        };
        let pipeline = build("an1mbyt3 material pipeline", None);
        let masked_pipeline = build(
            "an1mbyt3 masked material pipeline",
            Some(no_depth_write_stencil_state(wgpu::StencilState { front: face, back: face, read_mask: 0xff, write_mask: 0x00 })),
        );
        Ok(GpuMaterial { pipeline, masked_pipeline })
    }

    pub fn upload_texture(&self, width: u32, height: u32, rgba: &[u8], filter: ImageFilterMode) -> GpuImageTexture {
        self.upload_texture_wrapped(width, height, rgba, filter, false)
    }

    /// [`Renderer::upload_texture`], optionally **repeating** outside 0..1
    /// (`repeat`) instead of clamping — imported 3D models (glTF's default
    /// sampler wraps, and atlas-textured models like Kenney's rely on it).
    pub fn upload_texture_wrapped(&self, width: u32, height: u32, rgba: &[u8], filter: ImageFilterMode, repeat: bool) -> GpuImageTexture {
        let (_texture, view) = self.create_texture_with_data(width, height, rgba);
        let address = if repeat { wgpu::AddressMode::Repeat } else { wgpu::AddressMode::ClampToEdge };

        let wgpu_filter = match filter {
            ImageFilterMode::Nearest => wgpu::FilterMode::Nearest,
            ImageFilterMode::Linear => wgpu::FilterMode::Linear,
        };
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("an1mbyt3 image sampler"),
            address_mode_u: address,
            address_mode_v: address,
            address_mode_w: address,
            mag_filter: wgpu_filter,
            min_filter: wgpu_filter,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("an1mbyt3 image bind group"),
            layout: &self.image_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        GpuImageTexture { bind_group }
    }

    /// Writes `rgba` (`width × height` RGBA8 texels) into `target`,
    /// (re)creating it — with 25 % headroom, linear-filtered, clamped —
    /// when it's missing or too small.
    pub fn write_data_texture(&self, target: &mut Option<DataTexture>, width: u32, height: u32, rgba: &[u8]) {
        let (width, height) = (width.max(1), height.max(1));
        if target.as_ref().is_none_or(|t| t.width < width || t.height < height) {
            let (cw, ch) = ((width as f32 * 1.25).ceil() as u32, (height as f32 * 1.25).ceil() as u32);
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("an1mbyt3 data texture"),
                size: wgpu::Extent3d { width: cw, height: ch, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("an1mbyt3 data sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("an1mbyt3 data texture bind group"),
                layout: &self.image_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                ],
            });
            *target = Some(DataTexture { texture, image: GpuImageTexture { bind_group }, width: cw, height: ch });
        }
        let t = target.as_ref().expect("created above");
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &t.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: Some(height) },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
    }

    /// Creates an `Rgba8UnormSrgb` texture and immediately writes `rgba`
    /// into it — the common first step of both [`Renderer::upload_texture`]
    /// (which wraps the result in a ready-to-draw [`GpuImageTexture`]) and
    /// [`Renderer::bake_image`] (which needs the raw view to build a
    /// post-process bind group against `post_bind_group_layout`, a
    /// different shape than `image_bind_group_layout`, so it can't go
    /// through `upload_texture` itself).
    fn create_texture_with_data(&self, width: u32, height: u32, rgba: &[u8]) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("an1mbyt3 image texture"),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width.max(1) * 4), rows_per_image: Some(height.max(1)) },
            wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Applies `processing` to `source_rgba` exactly once, returning the
    /// resulting RGBA8 pixels (and their dimensions) for a caller to cache
    /// and feed to [`Renderer::upload_texture`] — the whole point being that
    /// this runs on import / on a processing-param edit, never every frame,
    /// which is what actually makes "pre-processing" cheaper than a live
    /// per-frame effect rather than just a different way to spell one.
    ///
    /// Reuses `post_process.wgsl`'s existing Pixelate/ColorGrading fragment
    /// math (via `post_replace_pipeline`, the same pipeline
    /// `render_with_post_effects` uses to present a plain/effected texture)
    /// rather than a separate shader — `PostEffect::None` is a plain
    /// passthrough sample, so an asset with no baked effect still goes
    /// through this same one pass (resize/resample is real work even with
    /// no effect chosen). The output size is `processing.resize`, or the
    /// source's own size if `None`.
    pub async fn bake_image(&self, source_rgba: &[u8], src_width: u32, src_height: u32, processing: &ImageProcessing) -> (Vec<u8>, u32, u32) {
        let (_source_texture, source_view) = self.create_texture_with_data(src_width, src_height, source_rgba);
        let (out_width, out_height) = processing.resize.unwrap_or((src_width, src_height));

        // A dedicated sampler (not `self.post_sampler`, which is
        // fixed-Linear) so a resize honors `processing.filter` too — a
        // pixel-art asset wants `Nearest` for its resample, not just its
        // live draw-time sampling.
        let wgpu_filter = match processing.filter {
            ImageFilterMode::Nearest => wgpu::FilterMode::Nearest,
            ImageFilterMode::Linear => wgpu::FilterMode::Linear,
        };
        let bake_sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("an1mbyt3 bake_image sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu_filter,
            min_filter: wgpu_filter,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("an1mbyt3 bake_image target texture"),
            size: wgpu::Extent3d { width: out_width.max(1), height: out_height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());

        let texel_size = [1.0 / src_width.max(1) as f32, 1.0 / src_height.max(1) as f32];
        let uniforms = match processing.effect {
            PostEffect::None | PostEffect::Blur { .. } | PostEffect::Glow { .. } => PostUniforms {
                texel_size_radius: [texel_size[0], texel_size[1], 0.0, 0.0],
                threshold_intensity: [0.0, 1.0, 0.0, 0.0],
                gradient_map_highlight: [0.0; 4],
            },
            PostEffect::Pixelate { block_size } => PostUniforms {
                texel_size_radius: [texel_size[0], texel_size[1], block_size, 1.0],
                threshold_intensity: [0.0, 0.0, 0.0, 0.0],
                gradient_map_highlight: [0.0; 4],
            },
            PostEffect::ColorGrading { brightness, contrast, saturation } => PostUniforms {
                texel_size_radius: [texel_size[0], texel_size[1], 0.0, 2.0],
                threshold_intensity: [brightness, contrast, saturation, 0.0],
                gradient_map_highlight: [0.0; 4],
            },
            PostEffect::Posterize { levels } => PostUniforms {
                texel_size_radius: [texel_size[0], texel_size[1], levels, 3.0],
                threshold_intensity: [0.0, 0.0, 0.0, 0.0],
                gradient_map_highlight: [0.0; 4],
            },
            PostEffect::GradientMap { shadow, highlight } => PostUniforms {
                texel_size_radius: [texel_size[0], texel_size[1], 0.0, 4.0],
                threshold_intensity: linear_rgba(shadow),
                gradient_map_highlight: linear_rgba(highlight),
            },
            PostEffect::Vignette { color, intensity, radius, blur } => PostUniforms {
                texel_size_radius: [texel_size[0], texel_size[1], 0.0, 7.0],
                threshold_intensity: linear_rgba(color),
                gradient_map_highlight: [intensity, radius, blur, 0.0],
            },
        };
        let binding = create_post_binding(&self.device, &self.post_bind_group_layout, &bake_sampler, &source_view);
        self.queue.write_buffer(&binding.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("an1mbyt3 bake_image encoder") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("an1mbyt3 bake_image pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.bake_pipeline);
            pass.set_bind_group(0, &binding.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        let unpadded_bytes_per_row = out_width * 4;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("an1mbyt3 bake_image readback buffer"),
            size: (padded_bytes_per_row * out_height) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded_bytes_per_row), rows_per_image: Some(out_height) },
            },
            wgpu::Extent3d { width: out_width, height: out_height, depth_or_array_layers: 1 },
        );

        self.uniform_pool.borrow_mut().flush(&self.queue);
        self.queue.submit(Some(encoder.finish()));

        let pixels = read_mapped_buffer(&self.device, &output_buffer, unpadded_bytes_per_row, padded_bytes_per_row, out_height).await;

        (pixels, out_width, out_height)
    }

    /// Rewrites `mesh`'s vertex positions in place — for a skinned mesh
    /// (`an1mbyt3_geometry::skin_mesh`, Phase 5), whose vertex positions
    /// change every frame as bones move, unlike an ordinary rigid mesh whose
    /// vertex buffer never changes after `upload_mesh` (only its per-draw
    /// transform uniform does).
    ///
    /// `positions.len()` must equal the vertex count `mesh` was originally
    /// uploaded with — skinning only ever moves vertices, it never adds or
    /// removes them (see `skin_mesh`'s doc comment: topology is preserved),
    /// so this never needs to resize the buffer, only overwrite its
    /// contents. Panics (via the underlying `wgpu` validation) if the sizes
    /// don't match — a caller bug (a `SkinBinding` built for a different
    /// mesh), not a recoverable condition.
    pub fn update_mesh_positions(&self, mesh: &GpuMesh, positions: &[[f32; 2]]) {
        self.queue.write_buffer(&mesh.vertex_buffer, 0, bytemuck::cast_slice(positions));
    }

    /// Draws `draws` to the surface, clearing to `clear_color` first.
    ///
    /// `viewport_size` is the surface's actual pixel dimensions and `camera`
    /// is the world-space pan/zoom to view it through — together they build
    /// the orthographic world-to-NDC projection each draw's transform is
    /// composed with (see `build_mvp`'s doc comment), so shape geometry never
    /// has to know about window pixel size or the current view. Pass
    /// `Camera::default()` for the pre-camera behavior (viewport == artboard
    /// size, no pan/zoom) — every native example and the web runtime's
    /// `WasmPlayer`/`WasmBoneIk` do this; only `WasmEditor`'s desktop canvas
    /// uses a real camera.
    pub fn render(
        &self,
        viewport_size: (f32, f32),
        camera: Camera,
        clear_color: wgpu::Color,
        draws: &[DrawCall<'_>],
    ) -> Result<(), RenderError> {
        // Uniform writes are queued before the render pass is recorded below,
        // so they're guaranteed (per wgpu's `Queue::write_buffer` contract) to
        // land before this frame's command buffer executes on submit.
        for draw in draws {
            let mvp = build_mvp(draw.transform.to_affine2(), viewport_size, camera);
            let uniforms = DrawUniforms::from_draw(mvp, draw);
            draw.binding.write(bytemuck::bytes_of(&uniforms));
        }

        let current = match self.surface().get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout => return Err(RenderError::Timeout),
            wgpu::CurrentSurfaceTexture::Occluded => return Err(RenderError::Occluded),
            wgpu::CurrentSurfaceTexture::Outdated => return Err(RenderError::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(RenderError::Lost),
            wgpu::CurrentSurfaceTexture::Validation => return Err(RenderError::Validation),
        };
        let view = current.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(self.render_format), ..Default::default() });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("an1mbyt3 frame encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("an1mbyt3 render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(linear_clear(clear_color)),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // A pipeline change per draw (rather than sorting/batching draws
            // by blend mode first) is the simple, obviously-correct version
            // — worth revisiting only once profiling shows pipeline-switch
            // overhead actually matters for a real scene's draw count.
            let mut bound = BoundState::default();
            for draw in draws {
                draw_flat(&mut pass, self, draw, &mut bound);
            }
        }

        self.uniform_pool.borrow_mut().flush(&self.queue);
        self.queue.submit(Some(encoder.finish()));
        // `present` lives on `Queue` (not `SurfaceTexture`) in this wgpu version.
        self.queue.present(current);
        Ok(())
    }

    /// Draws `content` clipped to `mask`'s shape — AN1MBYT3_BUILD_PLAN.md
    /// §6's "masks... dedicated shader passes / stencil-buffer techniques"
    /// (Phase 10 exit criterion: "a mask layer clips a group's contents").
    ///
    /// Two passes within one render pass, both against the stencil buffer
    /// (`self.stencil_view`, cleared to 0 at the start of this call):
    /// 1. `mask`'s geometry is rasterized with `mask_write_pipeline` —
    ///    stencil compare `Always`, op `Replace` with reference 1, and a
    ///    color `write_mask` of *none* (the mask shape itself never becomes
    ///    visible, only its coverage is recorded).
    /// 2. Every draw in `content` runs through `masked_content_pipelines`
    ///    (keyed by that draw's own `blend_mode`) — stencil compare `Equal`
    ///    against reference 1, so a fragment only survives where the mask
    ///    pass actually wrote a 1, i.e. inside the mask's shape.
    ///
    /// Deliberately a separate method from [`Renderer::render`] rather than
    /// folding masking into one universal draw-list API: this is the
    /// smallest real vertical slice proving the stencil technique works (one
    /// masked group, filling the whole frame) — generalizing to N masked
    /// groups mixed with N unmasked draws in one frame is real future work
    /// (see [`Renderer::render_with_masks`], which already does exactly
    /// that), not something this first version needs to solve to prove the
    /// primitive.
    pub fn render_with_mask(
        &self,
        viewport_size: (f32, f32),
        camera: Camera,
        clear_color: wgpu::Color,
        mask: &DrawCall<'_>,
        content: &[DrawCall<'_>],
    ) -> Result<(), RenderError> {
        let write_draw_uniforms = |draw: &DrawCall<'_>| {
            let mvp = build_mvp(draw.transform.to_affine2(), viewport_size, camera);
            let uniforms = DrawUniforms::from_draw(mvp, draw);
            draw.binding.write(bytemuck::bytes_of(&uniforms));
        };
        write_draw_uniforms(mask);
        for draw in content {
            write_draw_uniforms(draw);
        }

        let current = match self.surface().get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout => return Err(RenderError::Timeout),
            wgpu::CurrentSurfaceTexture::Occluded => return Err(RenderError::Occluded),
            wgpu::CurrentSurfaceTexture::Outdated => return Err(RenderError::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(RenderError::Lost),
            wgpu::CurrentSurfaceTexture::Validation => return Err(RenderError::Validation),
        };
        let view = current.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(self.render_format), ..Default::default() });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("an1mbyt3 masked frame encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("an1mbyt3 masked render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(linear_clear(clear_color)),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.stencil_view,
                    // No depth aspect is ever meaningfully used (both
                    // masking pipelines are built via
                    // `no_depth_write_stencil_state`, which pins
                    // `depth_write_enabled`/`depth_compare` to "always pass,
                    // never write") — `Discard` after the pass since there's
                    // nothing meaningful to keep.
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: wgpu::StoreOp::Discard,
                    }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_stencil_reference(1);

            pass.set_pipeline(&self.mask_write_pipeline);
            pass.set_bind_group(0, &mask.binding.bind_group, &[]);
            pass.set_vertex_buffer(0, mask.mesh.vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, mask.mesh.uv_buffer.slice(..));
            pass.set_index_buffer(mask.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mask.mesh.index_count, 0, 0..1);

            for draw in content {
                pass.set_pipeline(
                    self.masked_content_pipelines
                        .get(&draw.blend_mode)
                        .expect("Renderer always builds a masked-content pipeline for every BlendMode variant"),
                );
                pass.set_bind_group(0, &draw.binding.bind_group, &[]);
                pass.set_vertex_buffer(0, draw.mesh.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, draw.mesh.uv_buffer.slice(..));
                pass.set_index_buffer(draw.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..draw.mesh.index_count, 0, 0..1);
            }
        }

        self.uniform_pool.borrow_mut().flush(&self.queue);
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(current);
        Ok(())
    }

    /// Like [`Renderer::render`], but `masked` additionally carries content
    /// that lives inside one or more nested `GroupNode::mask`s (see
    /// [`SceneOp`]) — `unmasked` is everything else (any node with no masked
    /// ancestor at all), drawn first, completely unaffected by any of this
    /// (same pipelines `render` itself uses, no stencil attachment involved).
    ///
    /// `chrome` (the artboard grid, motion-path markers — editor-only
    /// decoration, never part of the actual artwork) draws in its own pass
    /// *before* either of those, straight to the swapchain at whatever the
    /// real surface resolution is — this is what keeps it unaffected by
    /// `Artboard::resolution_scale`/post effects even when `unmasked`/`masked`
    /// end up rendered through a scaled/post-processed offscreen buffer
    /// instead (see [`Renderer::render_with_post_effects`]): the grid simply
    /// never enters that pipeline at all, it's composited directly onto the
    /// final frame both here and there.
    ///
    /// Three render passes into the same swapchain view/one encoder: `chrome`
    /// clears the frame (this pass owns that responsibility now, not
    /// `unmasked`); `unmasked` then *loads* that same color content (so it
    /// draws over the grid, not instead of it) with no stencil attachment at
    /// all (so it can keep using every existing per-`BlendMode` pipeline
    /// exactly as `render` does, unmodified); `masked` then also *loads* and
    /// adds a fresh stencil attachment (cleared to `0`) for
    /// [`Renderer::mask_write_pipeline`]/[`Renderer::masked_content_pipelines`]
    /// to do their nested clipping against, exactly like
    /// [`Renderer::render_with_mask`]'s own single-level version — just with
    /// `set_stencil_reference` driven by each [`SceneOp`]'s own `depth`
    /// instead of a single hardcoded `1`, which is the one thing that
    /// actually makes nesting work.
    pub fn render_with_masks(
        &self,
        viewport_size: (f32, f32),
        camera: Camera,
        clear_color: wgpu::Color,
        chrome: &[DrawCall<'_>],
        unmasked: &[DrawCall<'_>],
        masked: &[SceneOp<'_>],
    ) -> Result<(), RenderError> {
        for draw in chrome {
            let mvp = build_mvp(draw.transform.to_affine2(), viewport_size, camera);
            let uniforms = DrawUniforms::from_draw(mvp, draw);
            draw.binding.write(bytemuck::bytes_of(&uniforms));
        }
        self.write_masked_scene_uniforms(viewport_size, camera, unmasked, masked);

        let current = match self.surface().get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout => return Err(RenderError::Timeout),
            wgpu::CurrentSurfaceTexture::Occluded => return Err(RenderError::Occluded),
            wgpu::CurrentSurfaceTexture::Outdated => return Err(RenderError::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(RenderError::Lost),
            wgpu::CurrentSurfaceTexture::Validation => return Err(RenderError::Validation),
        };
        let view = current.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(self.render_format), ..Default::default() });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("an1mbyt3 masked-scene frame encoder") });

        self.draw_masked_scene(&mut encoder, &view, &self.stencil_view, wgpu::LoadOp::Clear(linear_clear(clear_color)), chrome, &[], None);
        self.draw_masked_scene(&mut encoder, &view, &self.stencil_view, wgpu::LoadOp::Load, unmasked, masked, Some((self.config.width, self.config.height)));

        self.uniform_pool.borrow_mut().flush(&self.queue);
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(current);
        Ok(())
    }

    /// Writes every draw's (mask or content) MVP/color uniforms for a
    /// `(unmasked, masked)` scene pair — the uniform-upload half of
    /// [`Renderer::draw_masked_scene`], split out since it doesn't need a
    /// command encoder and both of that method's callers already need to do
    /// this before opening their own encoder.
    fn write_masked_scene_uniforms(&self, viewport_size: (f32, f32), camera: Camera, unmasked: &[DrawCall<'_>], masked: &[SceneOp<'_>]) {
        for draw in unmasked {
            let mvp = build_mvp(draw.transform.to_affine2(), viewport_size, camera);
            let uniforms = DrawUniforms::from_draw(mvp, draw);
            draw.binding.write(bytemuck::bytes_of(&uniforms));
        }
        for op in masked {
            let draw = match op {
                SceneOp::WriteMask { mask, .. } => mask,
                SceneOp::Draw { draw, .. } => draw,
            };
            let mvp = build_mvp(draw.transform.to_affine2(), viewport_size, camera);
            let uniforms = DrawUniforms::from_draw(mvp, draw);
            draw.binding.write(bytemuck::bytes_of(&uniforms));
        }
    }

    /// Draws `unmasked` (flat, no stencil) then `masked` (stencil-clipped,
    /// see [`Renderer::render_with_masks`]'s own doc comment for the
    /// two-pass write-then-test technique) into `target_view`, within
    /// `encoder` — the shared drawing core behind both
    /// [`Renderer::render_with_masks`] (target = the swapchain view) and
    /// [`Renderer::render_with_post_effects`] (target = `post_scene_view`).
    /// Extracting this is what makes clipping and post-effects compose:
    /// before this, a post-effect chain always drew a *flat*, unmasked
    /// version of the scene into its offscreen texture (masked geometry was
    /// unwrapped back into plain `DrawCall`s one level up, in
    /// `an1mbyt3-wasm`'s per-frame dispatch), so any clip silently vanished
    /// the instant a post effect was active — the two features were built
    /// independently and never actually drew into the same place.
    /// `stencil_view` is passed in (rather than always `self.stencil_view`)
    /// so a future differently-sized offscreen target (e.g. a reduced-
    /// resolution render for `Artboard::resolution_scale`) can supply its
    /// own matching stencil texture instead of assuming the swapchain's.
    ///
    /// `load` (not a bare `clear_color`) since a caller drawing straight to
    /// the swapchain has already put the artboard grid/chrome there first
    /// (see `render_with_masks`'s own doc comment) and needs this pass to
    /// `Load` and alpha-composite over it, while a caller drawing into a
    /// scratch offscreen texture (`render_with_post_effects`'s `post_scene_view`)
    /// has nothing there yet and needs a real `Clear`.
    fn draw_masked_scene(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        stencil_view: &wgpu::TextureView,
        load: wgpu::LoadOp<wgpu::Color>,
        unmasked: &[DrawCall<'_>],
        masked: &[SceneOp<'_>],
        // `Some(target size)` on the *scene* call (not the chrome call):
        // draw the pending 3D scene first, so the 2D layer lands on top.
        with_3d: Option<(u32, u32)>,
    ) {
        let mut load = load;
        if let Some((width, height)) = with_3d {
            if self.draw_3d_pass(encoder, target_view, width, height, load) {
                load = wgpu::LoadOp::Load;
            }
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("an1mbyt3 unmasked pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut bound = BoundState::default();
            for draw in unmasked {
                draw_flat(&mut pass, self, draw, &mut bound);
            }
        }

        // Grouped into one render pass *per island* (see `SceneOp::Draw`'s own
        // doc comment for why): each island gets a fresh `stencil_ops: Clear(0)`,
        // so two independent masked subtrees — even ones using the same small
        // `depth` values, which every top-level masked subtree does — never
        // see each other's leftover stencil state, regardless of whether
        // their on-screen regions overlap. Color still `Load`s across every
        // sub-pass (compositing onto what the unmasked pass and any earlier
        // island already drew), only the stencil/depth attachment resets.
        let mut start = 0;
        while start < masked.len() {
            let island = masked[start].island();
            let mut end = start + 1;
            while end < masked.len() && masked[end].island() == island {
                end += 1;
            }

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("an1mbyt3 masked pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    // `Load`, not `Clear` — this pass adds onto whatever the
                    // unmasked pass (or an earlier island's own pass) above
                    // already drew, into the same view.
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: stencil_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0), store: wgpu::StoreOp::Discard }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            for op in &masked[start..end] {
                match op {
                    SceneOp::WriteMask { mask, depth, .. } => {
                        pass.set_stencil_reference(*depth);
                        // Textured mask (an `Image` node, or a `Shape` with
                        // its own `image`, used as a mask) needs its actual
                        // sampled alpha to matter — see
                        // `mask_write_textured_pipeline`'s own doc comment
                        // for why a flat/gradient-only mask write silently
                        // ignored this and clipped to the mask's whole
                        // tessellated geometry instead.
                        match mask.texture {
                            Some(texture) => {
                                pass.set_pipeline(&self.mask_write_textured_pipeline);
                                pass.set_bind_group(0, &mask.binding.bind_group, &[]);
                                pass.set_bind_group(1, &texture.bind_group, &[]);
                            }
                            None => {
                                pass.set_pipeline(&self.mask_write_pipeline);
                                pass.set_bind_group(0, &mask.binding.bind_group, &[]);
                            }
                        }
                        pass.set_vertex_buffer(0, mask.mesh.vertex_buffer.slice(..));
                        pass.set_vertex_buffer(1, mask.mesh.uv_buffer.slice(..));
                        pass.set_index_buffer(mask.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..mask.mesh.index_count, 0, 0..1);
                    }
                    SceneOp::Draw { draw, depth, .. } => {
                        pass.set_stencil_reference(*depth);
                        // Same reasoning as the `WriteMask` arm above, for
                        // content being clipped rather than doing the
                        // clipping — an `Image`/textured `Shape` drawn
                        // through the flat/gradient-only pipeline read
                        // `uniforms.color` alone (just its flat tint) and
                        // never sampled its texture at all, rendering as a
                        // flat, usually near-white, opaque rectangle instead
                        // of its actual pixels. Materials (shader graphs,
                        // GPU liquids) use their own stencil-tested twin.
                        match (draw.material, draw.texture) {
                            (Some(material), texture) => {
                                let resources = self.material_resources();
                                pass.set_pipeline(&material.masked_pipeline);
                                pass.set_bind_group(0, &draw.binding.bind_group, &[]);
                                pass.set_bind_group(1, &texture.unwrap_or(&resources.white).bind_group, &[]);
                                pass.set_bind_group(2, &resources.globals_bind_group, &[]);
                            }
                            (None, Some(texture)) => {
                                pass.set_pipeline(&self.masked_content_textured_pipeline);
                                pass.set_bind_group(0, &draw.binding.bind_group, &[]);
                                pass.set_bind_group(1, &texture.bind_group, &[]);
                            }
                            (None, None) => {
                                pass.set_pipeline(
                                    self.masked_content_pipelines
                                        .get(&draw.blend_mode)
                                        .expect("Renderer always builds a masked-content pipeline for every BlendMode variant"),
                                );
                                pass.set_bind_group(0, &draw.binding.bind_group, &[]);
                            }
                        }
                        pass.set_vertex_buffer(0, draw.mesh.vertex_buffer.slice(..));
                        pass.set_vertex_buffer(1, draw.mesh.uv_buffer.slice(..));
                        pass.set_index_buffer(draw.mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..draw.mesh.index_count, 0, 0..1);
                    }
                }
            }

            drop(pass);
            start = end;
        }
    }

    /// Renders `draws` exactly as [`Renderer::render`] would, then applies
    /// `effect` as a single post pass — a one-effect convenience wrapper
    /// around [`Renderer::render_with_post_effects`] (`&[effect]`), kept for
    /// every caller that only ever wants exactly one effect (the renderer
    /// examples, `an1mbyt3-cli`) so they don't need to build a one-element
    /// slice themselves. See that method's own doc comment for the actual
    /// pass structure — identical to what this used to do inline before the
    /// layered post-fx panel (AN1MBYT3_BUILD_PLAN.md §11) needed a real
    /// multi-effect *chain*, not just one effect at a time.
    ///
    /// Every shape in `draws` still renders through the exact same
    /// `BlendMode`-selected pipeline [`Renderer::render`] uses — nothing
    /// about how an individual shape draws changes; this only affects what
    /// happens to the combined image afterward, matching §10's exit
    /// criterion ("without changing how its own shapes are drawn").
    pub fn render_with_post_effect(
        &self,
        viewport_size: (f32, f32),
        camera: Camera,
        clear_color: wgpu::Color,
        draws: &[DrawCall<'_>],
        effect: PostEffect,
    ) -> Result<(), RenderError> {
        self.render_with_post_effects(viewport_size, camera, clear_color, &[], draws, &[], &[effect], None)
    }

    /// Like [`Renderer::render_with_post_effect`], but chains an ordered
    /// *stack* of effects — AN1MBYT3_BUILD_PLAN.md §11's "layered approach,
    /// allow for duplicate effects" post-processing panel. Each effect's
    /// output feeds the next one's input (a Blur followed by a ColorGrading
    /// followed by another Blur really does blur, then grade, then blur
    /// again — not three independent passes over the original scene), via
    /// `post_ping_view`/`post_pong_view` ping-ponging: effect N reads
    /// whichever of the two currently holds the previous effect's result and
    /// writes into the other one, except the *last* active effect, which
    /// writes straight to the swapchain (skipping one otherwise-pointless
    /// extra copy). Every pass — scene draw included — happens at
    /// `current_resolution_scale`'s (possibly reduced) size, not full native
    /// resolution: see `Renderer::set_resolution_scale`'s own doc comment
    /// for why running effects over fewer pixels is deliberate rather than a
    /// shortcut. `PostEffect::None` entries are filtered out first — they
    /// exist as a value (the "disabled" state of a stack entry a caller
    /// hasn't deleted) but contribute nothing to the chain; an all-`None`
    /// (or empty) `effects` degrades to a plain [`Renderer::render`], same
    /// as `PostEffect::None` did for the old single-effect method.
    ///
    /// Builds each effect's post-process bind group(s) fresh, per effect,
    /// per frame (via the same `create_post_binding` the old single-effect
    /// path used once) rather than keeping a persistent pair the way
    /// `post_binding_a`/`post_binding_b` used to — which *source* texture
    /// each effect reads from changes every step of the chain, so a
    /// persistent binding would need rebuilding just as often anyway. An
    /// editor's post-fx stack tops out at a handful of layers, not
    /// thousands of draws, so the extra per-effect buffer/bind-group
    /// allocation this costs is not a real concern the way it would be on a
    /// true per-triangle hot path.
    pub fn render_with_post_effects(
        &self,
        viewport_size: (f32, f32),
        camera: Camera,
        clear_color: wgpu::Color,
        chrome: &[DrawCall<'_>],
        unmasked: &[DrawCall<'_>],
        masked: &[SceneOp<'_>],
        effects: &[PostEffect],
        lighting: Option<&Lighting<'_>>,
    ) -> Result<(), RenderError> {
        let active: Vec<PostEffect> = effects.iter().copied().filter(|effect| *effect != PostEffect::None).collect();
        if active.is_empty() && self.current_resolution_scale == 1.0 && self.dynamic_scale == 1.0 && lighting.is_none() {
            return self.render_with_masks(viewport_size, camera, clear_color, chrome, unmasked, masked);
        }
        if let Some(lighting) = lighting {
            for draw in lighting.unlit {
                let mvp = build_mvp(draw.transform.to_affine2(), viewport_size, camera);
                let uniforms = DrawUniforms::from_draw(mvp, draw);
                draw.binding.write(bytemuck::bytes_of(&uniforms));
            }
        }

        for draw in chrome {
            let mvp = build_mvp(draw.transform.to_affine2(), viewport_size, camera);
            let uniforms = DrawUniforms::from_draw(mvp, draw);
            draw.binding.write(bytemuck::bytes_of(&uniforms));
        }
        self.write_masked_scene_uniforms(viewport_size, camera, unmasked, masked);

        // Every post-process pass below samples `post_scene_view`/its
        // siblings, never the swapchain directly — so `texel_size` has to
        // match *their* actual size, which is only the native surface size
        // when `resolution_scale == 1.0` (see `post_target_dims`'s own doc
        // comment).

        let current = match self.surface().get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout => return Err(RenderError::Timeout),
            wgpu::CurrentSurfaceTexture::Occluded => return Err(RenderError::Occluded),
            wgpu::CurrentSurfaceTexture::Outdated => return Err(RenderError::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(RenderError::Lost),
            wgpu::CurrentSurfaceTexture::Validation => return Err(RenderError::Validation),
        };
        let swapchain_view = current.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(self.render_format), ..Default::default() });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("an1mbyt3 post-process frame encoder") });

        // Chrome (the artboard grid, motion-path markers) draws straight to
        // the swapchain first, at the real native surface size, completely
        // outside the scaled/post-processed pipeline below — see this
        // method's own doc comment for why. Every later pass that writes to
        // `swapchain_view` uses `LoadOp::Load` (never `Clear`) precisely so
        // it never gets erased.
        self.draw_masked_scene(&mut encoder, &swapchain_view, &self.stencil_view, wgpu::LoadOp::Clear(linear_clear(clear_color)), chrome, &[], None);

        // No real effects active, but rendering at a reduced resolution
        // (`Artboard::resolution_scale`) — still has to go through the
        // scaled offscreen target + upscale, just with one plain present
        // pass instead of the full effect-chain loop below.
        // The scene always draws into `post_scene_view` first; with 2D
        // lighting on, the lit composite (`lighting_targets.lit_view`) is
        // what every later stage reads instead — see `renderer/lighting.rs`.
        self.draw_masked_scene(&mut encoder, &self.post_scene_view, &self.post_stencil_view, wgpu::LoadOp::Clear(TRANSPARENT), unmasked, masked, Some(self.post_target_dims()));
        let source = match lighting {
            Some(lighting) => {
                self.draw_lighting(&mut encoder, viewport_size, camera, lighting);
                post::PostTarget::Lit
            }
            None => post::PostTarget::Scene,
        };

        // The effect stack (fused where it can be — see `renderer/post.rs`),
        // ending in the pass that presents onto the swapchain over the
        // chrome. With no effects that's one plain upscale/present pass.
        self.run_post_chain(&mut encoder, source, &active, &swapchain_view);

        self.uniform_pool.borrow_mut().flush(&self.queue);
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(current);
        Ok(())
    }

    /// Renders `draws` exactly like [`Renderer::render`], but into an
    /// in-memory buffer instead of any window/canvas surface — for embedding
    /// contexts with no OS window surface to present to at all, the reason
    /// [`Renderer::new_headless`] exists (see that constructor's doc
    /// comment: the Tauri desktop editor's canvas, an ordinary HTML
    /// `<canvas>` inside a WebView, not a native `wgpu::Surface`).
    ///
    /// Returns tightly-packed RGBA8 bytes, `width * height * 4` of them,
    /// row-major with no padding — this strips wgpu's own row-alignment
    /// padding (`COPY_BYTES_PER_ROW_ALIGNMENT`) internally so callers never
    /// have to think about it. `width`/`height` are always this renderer's
    /// current `config.width`/`config.height` (whatever the last
    /// [`Renderer::new_headless`]/[`Renderer::resize`] call set).
    ///
    /// Works on a windowed renderer too (nothing here touches `self.surface`
    /// — it renders into the same `post_scene_view` offscreen target
    /// `render_with_post_effect` uses), but is really only meaningful paired
    /// with `new_headless`.
    pub async fn render_to_texture(&self, viewport_size: (f32, f32), camera: Camera, clear_color: wgpu::Color, draws: &[DrawCall<'_>]) -> Vec<u8> {
        for draw in draws {
            let mvp = build_mvp(draw.transform.to_affine2(), viewport_size, camera);
            let uniforms = DrawUniforms::from_draw(mvp, draw);
            draw.binding.write(bytemuck::bytes_of(&uniforms));
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("an1mbyt3 render_to_texture encoder") });

        let (scene_width, scene_height) = self.post_target_dims();
        let scene_load = if self.draw_3d_pass(&mut encoder, &self.post_scene_view, scene_width, scene_height, wgpu::LoadOp::Clear(linear_clear(clear_color))) {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(linear_clear(clear_color))
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("an1mbyt3 render_to_texture scene pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.post_scene_view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: scene_load, store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            let mut bound = BoundState::default();
            for draw in draws {
                draw_flat(&mut pass, self, draw, &mut bound);
            }
        }

        // wgpu requires each copied row's byte offset to be a multiple of
        // `COPY_BYTES_PER_ROW_ALIGNMENT` (256) for texture<->buffer copies —
        // `width * 4` (RGBA8) usually isn't, so the readback buffer is
        // allocated with the *padded* row size, and trimmed back down to
        // tightly-packed rows below, after mapping.
        let width = self.config.width;
        let height = self.config.height;
        let unpadded_bytes_per_row = width * 4;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("an1mbyt3 render_to_texture readback buffer"),
            size: (padded_bytes_per_row * height) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.post_scene_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded_bytes_per_row), rows_per_image: Some(height) },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );

        self.uniform_pool.borrow_mut().flush(&self.queue);
        self.queue.submit(Some(encoder.finish()));

        let pixels = read_mapped_buffer(&self.device, &output_buffer, unpadded_bytes_per_row, padded_bytes_per_row, height).await;

        pixels
    }
}

/// A 2D view camera: which world-space point sits at the center of the
/// viewport (`pan`), and how many viewport pixels correspond to one world
/// unit (`zoom`). World space is the same origin-centered, +y-up space
/// `an1mbyt3_core::Path`/`Transform2D` already use — a camera is a second,
/// independent transform layered *outside* every node's own model transform,
/// not a replacement for it.
///
/// `Default` (`pan: ZERO, zoom: 1.0`) reproduces the exact projection every
/// call site used before this type existed: a viewport exactly the
/// artboard's own size, centered on the origin, 1 world unit == 1 pixel —
/// see `build_mvp`'s doc comment for why that equivalence is what makes
/// `Camera::default()` a safe, behavior-preserving default for every
/// existing caller (native examples, the web runtime's `WasmPlayer`/
/// `WasmBoneIk`) that doesn't need pan/zoom at all. Only the desktop
/// editor's canvas (`WasmEditor`) uses a real, user-driven `Camera`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub pan: Vec2,
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self { pan: Vec2::ZERO, zoom: 1.0 }
    }
}

/// Composes a node's local transform with an orthographic world-to-NDC
/// projection (built from `viewport_size` and `camera`) into one
/// model-view-projection matrix.
///
/// World space is centered on the origin (matching `an1mbyt3_core::Path::rect`,
/// which builds shapes centered on `(0, 0)`) with +x right and +y up.
/// `viewport_size` is the actual render target's pixel dimensions (the
/// surface/canvas for `render`/`render_with_mask`/`render_with_post_effect`,
/// the target texture for `render_to_texture`) — *not* necessarily the
/// artboard's own width/height, now that a `Camera` can pan/zoom the view
/// independently of both. At `Camera::default()` with `viewport_size` equal
/// to the artboard's own size, this reduces to exactly the old
/// artboard-fills-the-viewport projection.
///
/// Built by hand (rather than via a `glam` conversion helper) because `glam`
/// has no direct `Affine2 -> Mat4` conversion: a 2D affine transform embeds
/// into 4x4 space as an identity Z row/column plus the 2D linear map and
/// translation in the X/Y rows — see the inline field mapping below.
fn build_mvp(transform: Affine2, viewport_size: (f32, f32), camera: Camera) -> Mat4 {
    let visible_hw = viewport_size.0 / 2.0 / camera.zoom;
    let visible_hh = viewport_size.1 / 2.0 / camera.zoom;
    let left = camera.pan.x - visible_hw;
    let right = camera.pan.x + visible_hw;
    let bottom = camera.pan.y - visible_hh;
    let top = camera.pan.y + visible_hh;
    // `glam` 0.33 deprecated this in favor of a new `glam::camera` module in
    // anticipation of a future API; that module isn't stabilized yet, so
    // sticking with the deprecated-but-still-correct function for now.
    #[allow(deprecated)]
    let projection = Mat4::orthographic_rh(left, right, bottom, top, -1.0, 1.0);

    let m = transform.matrix2;
    let t = transform.translation;
    #[rustfmt::skip]
    let model = Mat4::from_cols_array(&[
        m.x_axis.x, m.x_axis.y, 0.0, 0.0,
        m.y_axis.x, m.y_axis.y, 0.0, 0.0,
        0.0,        0.0,        1.0, 0.0,
        t.x,        t.y,        0.0, 1.0,
    ]);

    projection * model
}
