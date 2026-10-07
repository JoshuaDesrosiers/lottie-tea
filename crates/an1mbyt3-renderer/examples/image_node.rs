//! Textured-quad smoke test — proves `Renderer::upload_texture` and the
//! dedicated textured pipeline (`shader.wgsl`'s `fs_textured`) draw a raster
//! image correctly, interleaved with an ordinary flat-fill `Shape` draw in
//! the same frame, without a live window (same headless-PNG-dump pattern
//! `headless.rs` already established for automated visual confirmation).
//!
//! No `an1mbyt3-image` dependency here on purpose: a hand-built checkerboard
//! (via `image`, already a dev-dependency for the PNG save step every
//! example here uses) is a real decoded-pixels source, and this example is
//! about proving the *renderer's* textured draw path, not the import crate.
//!
//! Run with `cargo run -p an1mbyt3-renderer --example image_node`, then open
//! `image_node_output.png` — a red square on the left, a checkerboard-
//! textured quad on the right.

use glam::Vec2;
use an1mbyt3_core::{ImageFilterMode, Path, Transform2D};
use an1mbyt3_geometry::{tessellate_path, unit_quad_mesh};
use an1mbyt3_renderer::{BlendMode, Camera, DrawCall, Renderer};

const WIDTH: u32 = 400;
const HEIGHT: u32 = 400;
const CHECKER_SIZE: u32 = 16;

fn main() {
    pollster::block_on(run());
}

/// A small red/white checkerboard — a pattern that makes UV orientation
/// mistakes (flipped/rotated sampling) visually obvious, unlike a flat or
/// gradient test texture.
fn checkerboard_rgba(size: u32, block: u32) -> image::RgbaImage {
    image::RgbaImage::from_fn(size, size, |x, y| {
        if (x / block + y / block) % 2 == 0 {
            image::Rgba([220, 50, 60, 255])
        } else {
            image::Rgba([250, 250, 250, 255])
        }
    })
}

async fn run() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let renderer = Renderer::new_headless(&instance, WIDTH, HEIGHT).await;

    let square_mesh = renderer.upload_mesh(&tessellate_path(&Path::rect(140.0, 140.0)));
    let square_binding = renderer.create_draw_binding();

    let checker = checkerboard_rgba(CHECKER_SIZE * 4, CHECKER_SIZE);
    let (checker_width, checker_height) = checker.dimensions();
    let texture = renderer.upload_texture(checker_width, checker_height, checker.as_raw(), ImageFilterMode::Nearest);
    let quad_mesh = renderer.upload_mesh(&unit_quad_mesh());
    let quad_binding = renderer.create_draw_binding();

    let draws = [
        DrawCall {
            mesh: &square_mesh,
            binding: &square_binding,
            transform: Transform2D { translation: Vec2::new(-90.0, 0.0), ..Transform2D::default() },
            fill_color: [0.85, 0.2, 0.35, 1.0],
            blend_mode: BlendMode::Normal,
            gradient: None,
            texture: None,
            material: None,
        },
        DrawCall {
            mesh: &quad_mesh,
            binding: &quad_binding,
            transform: Transform2D { translation: Vec2::new(90.0, 0.0), scale: Vec2::splat(150.0), ..Transform2D::default() },
            fill_color: [1.0, 1.0, 1.0, 1.0],
            blend_mode: BlendMode::Normal,
            gradient: None,
            texture: Some(&texture),
            material: None,
        },
    ];

    let pixels = renderer
        .render_to_texture((WIDTH as f32, HEIGHT as f32), Camera::default(), wgpu::Color { r: 0.08, g: 0.08, b: 0.1, a: 1.0 }, &draws)
        .await;

    let image = image::RgbaImage::from_raw(WIDTH, HEIGHT, pixels).expect("pixel buffer size must match width*height*4");
    image.save("image_node_output.png").expect("failed to write image_node_output.png");
    println!("wrote image_node_output.png ({WIDTH}x{HEIGHT})");
}
