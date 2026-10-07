//! Editor canvas-bridge smoke test — proves `Renderer::new_headless`/
//! `render_to_texture` work in isolation, before wiring them into the Tauri
//! desktop editor's canvas (AN1MBYT3_BUILD_PLAN.md §15). No window at all:
//! renders a small scene purely off-screen and writes the result to a real
//! PNG file so it can be visually confirmed like every other example — just
//! via a saved image instead of a live window, since there's no window here
//! for a smoke test to show.
//!
//! Run with `cargo run -p an1mbyt3-renderer --example headless`, then open
//! `headless_output.png` (written to the current working directory).

use glam::Vec2;
use an1mbyt3_core::{Path, Transform2D};
use an1mbyt3_geometry::tessellate_path;
use an1mbyt3_renderer::{BlendMode, Camera, DrawCall, Renderer};

const WIDTH: u32 = 400;
const HEIGHT: u32 = 400;

fn main() {
    pollster::block_on(run());
}

async fn run() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let renderer = Renderer::new_headless(&instance, WIDTH, HEIGHT).await;

    let square_mesh = renderer.upload_mesh(&tessellate_path(&Path::rect(160.0, 160.0)));
    let square_binding = renderer.create_draw_binding();
    let accent_mesh = renderer.upload_mesh(&tessellate_path(&Path::rect(60.0, 220.0)));
    let accent_binding = renderer.create_draw_binding();

    let draws = [
        DrawCall {
            mesh: &square_mesh,
            binding: &square_binding,
            transform: Transform2D { translation: Vec2::new(-60.0, 0.0), ..Transform2D::default() },
            fill_color: [0.85, 0.2, 0.35, 1.0],
            blend_mode: BlendMode::Normal, gradient: None, texture: None,
            material: None,
        },
        DrawCall {
            mesh: &accent_mesh,
            binding: &accent_binding,
            transform: Transform2D { translation: Vec2::new(90.0, 0.0), ..Transform2D::default() },
            fill_color: [0.3, 0.55, 0.95, 1.0],
            blend_mode: BlendMode::Normal, gradient: None, texture: None,
            material: None,
        },
    ];

    let pixels = renderer
        .render_to_texture((WIDTH as f32, HEIGHT as f32), Camera::default(), wgpu::Color { r: 0.08, g: 0.08, b: 0.1, a: 1.0 }, &draws)
        .await;

    let image = image::RgbaImage::from_raw(WIDTH, HEIGHT, pixels).expect("pixel buffer size must match width*height*4");
    image.save("headless_output.png").expect("failed to write headless_output.png");
    println!("wrote headless_output.png ({WIDTH}x{HEIGHT})");
}
