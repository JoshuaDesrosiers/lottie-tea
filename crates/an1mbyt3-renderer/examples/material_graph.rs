//! Shader-graph materials, end to end on a real GPU (headless): three
//! squares wearing three compiled graphs — the UV as a colour, a checker,
//! and `Time` (set to 0.5) — rendered offscreen; pixels are checked and the
//! frame saved as `material_graph_output.png`.

use an1mbyt3_core::{compile_wgsl, Path, ShaderDomain, ShaderEdgeDef, ShaderGraphDef, ShaderNodeDef, ShaderNodeKind, Transform2D};
use an1mbyt3_geometry::tessellate_path;
use an1mbyt3_renderer::{BlendMode, Camera, DrawCall, Renderer};
use glam::Vec2;

const WIDTH: u32 = 400;
const HEIGHT: u32 = 200;

fn node(id: u32, kind: ShaderNodeKind) -> ShaderNodeDef {
    ShaderNodeDef { id, kind, position: Vec2::ZERO, value: [0.0; 4], defaults: Vec::new() }
}

/// `kind`'s first output straight into the Output.
fn single(kind: ShaderNodeKind) -> ShaderGraphDef {
    ShaderGraphDef { domain: ShaderDomain::Canvas2D, nodes: vec![node(1, kind), node(2, ShaderNodeKind::Output)], edges: vec![ShaderEdgeDef { from: 1, from_port: 0, to: 2, to_port: 0 }] }
}

fn main() {
    pollster::block_on(run());
}

async fn run() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let renderer = Renderer::new_headless(&instance, WIDTH, HEIGHT).await;
    renderer.set_shader_time(0.5);

    let uv = renderer.compile_material(&compile_wgsl(&single(ShaderNodeKind::Uv)).unwrap()).expect("uv material");
    let checker = renderer.compile_material(&compile_wgsl(&single(ShaderNodeKind::Checker)).unwrap()).expect("checker material");
    let time = renderer.compile_material(&compile_wgsl(&single(ShaderNodeKind::Time)).unwrap()).expect("time material");

    let mesh = renderer.upload_mesh(&tessellate_path(&Path::rect(100.0, 100.0)));
    let bindings = [renderer.create_draw_binding(), renderer.create_draw_binding(), renderer.create_draw_binding()];
    let draws: Vec<DrawCall> = [(&uv, -130.0), (&checker, 0.0), (&time, 130.0)]
        .into_iter()
        .zip(&bindings)
        .map(|((material, x), binding)| DrawCall {
            mesh: &mesh,
            binding,
            transform: Transform2D { translation: Vec2::new(x, 0.0), ..Transform2D::default() },
            fill_color: [1.0; 4],
            blend_mode: BlendMode::Normal,
            gradient: None,
            texture: None,
            material: Some(material),
        })
        .collect();
    let pixels = renderer.render_to_texture((WIDTH as f32, HEIGHT as f32), Camera::default(), wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }, &draws).await;
    let px = |x: u32, y: u32| {
        let i = ((y * WIDTH + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    // Screen (0,0) is top-left; the UV square spans x 20..120, y 50..150.
    let uv_top_left = px(25, 55);
    let uv_top_right = px(115, 55);
    let uv_bottom_left = px(25, 145);
    let time_center = px(330, 100);
    println!("uv TL {uv_top_left:?} TR {uv_top_right:?} BL {uv_bottom_left:?} | checker {:?} {:?} | time {time_center:?}", px(160, 60), px(190, 60));
    image::RgbaImage::from_raw(WIDTH, HEIGHT, pixels.clone()).unwrap().save("material_graph_output.png").unwrap();
    // UV: u grows to the right (red), v grows downward (green).
    assert!(uv_top_left[0] < 80 && uv_top_left[1] < 80, "top-left ~ uv(0,0)");
    assert!(uv_top_right[0] > 220 && uv_top_right[1] < 80, "top-right ~ uv(1,0)");
    assert!(uv_bottom_left[0] < 80 && uv_bottom_left[1] > 220, "bottom-left ~ uv(0,1)");
    // Time = 0.5 (linear) -> ~188 in sRGB.
    assert!((170..=205).contains(&time_center[0]), "time drives the material: {time_center:?}");
    // Checker: neighbouring cells differ.
    assert_ne!(px(160, 60), px(190, 60), "checker alternates");
    println!("material_graph: OK (wrote material_graph_output.png)");
}
