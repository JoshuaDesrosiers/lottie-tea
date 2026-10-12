//! `an1mbyt3-renderer`: wgpu, shaders, batching, masks, compositing, blend modes, GPU resources.
//!
//! See AN1MBYT3_BUILD_PLAN.md §6 ("Rendering Strategy") — SVG is never the
//! runtime renderer; this crate is the only thing that touches the GPU.
//!
//! Owns (so far): [`Renderer`], which uploads tessellated [`an1mbyt3_geometry::Mesh`]es
//! as [`GpuMesh`]es (uploaded once, cached — §6) and draws them each frame,
//! updating only each draw's transform/color uniform rather than
//! re-tessellating or re-uploading vertex data.
//!
//! Does NOT own (yet): batching/culling, masks, blend modes, or compositing
//! passes — Phase 1 draws one flat-fill shape at a time with alpha blending
//! and nothing else; those arrive as the renderer grows through later phases
//! (AN1MBYT3_BUILD_PLAN.md §10).
//!
//! Deliberately windowing-library-agnostic — see `renderer.rs`'s module doc
//! for why `Renderer::new` takes a `wgpu::Surface` rather than a `winit`
//! window type. `winit`/`pollster` appear only as dev-dependencies, used by
//! the `render_shape` example (the Phase 1 smoke test for
//! AN1MBYT3_BUILD_PLAN.md §11's vertical slice), not by the library itself.

mod renderer;

pub use renderer::{linear_rgba, srgb_to_linear, 
    BlendMode, Camera, DataTexture, DrawBinding, DrawCall, GpuImageTexture, GpuMaterial, GpuMesh, Blend3D, FluidLayer3D, GpuMaterial3D, GpuMesh3D, MeshDraw3D, ParticleQuad3D, PointLight3D, Scene3D, Shadows3D, MATERIAL_PARAMS_3D, MAX_POINT_LIGHTS, TOON_ARCANE_WGSL, TOON_MTOON_WGSL, ImageFilterMode, ImageProcessing, LightDraw, LightShadows, LightShape, LightVolumetric, Lighting, LitGroup, PostEffect,
    RenderError, Renderer, SceneOp, ShadowCasterShape, MAX_FREEFORM_POINTS,
};
