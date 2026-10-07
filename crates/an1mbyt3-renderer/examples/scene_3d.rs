//! The 3D pass, end to end on a real GPU (headless): a lit cube, a sphere
//! in front of it, a floor and a point light, rendered offscreen through a
//! perspective camera; depth, lighting and the 2D-on-top order are checked
//! and the frame saved as `scene_3d_output.png`.

use std::rc::Rc;

use an1mbyt3_core::{Path, Primitive3D, Transform2D, Transform3D};
use an1mbyt3_geometry::{primitive_mesh, tessellate_path};
use an1mbyt3_renderer::{BlendMode, Camera, DrawCall, MeshDraw3D, PointLight3D, Renderer, Scene3D};
use glam::{Mat4, Vec2, Vec3};

const WIDTH: u32 = 480;
const HEIGHT: u32 = 320;

fn main() {
    pollster::block_on(run());
}

async fn run() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let renderer = Renderer::new_headless(&instance, WIDTH, HEIGHT).await;
    let cube = Rc::new(renderer.upload_mesh_3d(&primitive_mesh(&Primitive3D::cube())));
    let sphere = Rc::new(renderer.upload_mesh_3d(&primitive_mesh(&Primitive3D::sphere())));
    let plane = Rc::new(renderer.upload_mesh_3d(&primitive_mesh(&Primitive3D::plane())));

    let eye = Vec3::new(0.0, 1.6, 4.0);
    let view = glam::camera::rh::view::look_at_mat4(eye, Vec3::new(0.0, 0.4, 0.0), Vec3::Y);
    let projection = glam::camera::rh::proj::directx::perspective(60f32.to_radians(), WIDTH as f32 / HEIGHT as f32, 0.05, 100.0);
    let spun = Transform3D { position: Vec3::new(0.0, 0.5, 0.0), rotation_degrees: Vec3::new(0.0, 35.0, 0.0), ..Default::default() };
    let draw = |mesh: &Rc<_>, model: Mat4, color: [f32; 4]| MeshDraw3D { shininess: 0.4, ..MeshDraw3D::new(Rc::clone(mesh), model, color) };
    renderer.set_scene_3d(Some(Scene3D {
        view,
        projection,
        camera_position: eye,
        clear_color: Some([0.45, 0.62, 0.85, 1.0]),
        sun_direction: Vec3::new(-0.4, -1.0, -0.3),
        sun_color: Vec3::splat(0.9),
        ambient: Vec3::new(0.25, 0.28, 0.35),
        fog_color: Vec3::new(0.45, 0.62, 0.85),
        fog_density: 0.03,
        point_lights: vec![PointLight3D { position: Vec3::new(1.5, 1.0, 1.5), color: Vec3::new(1.0, 0.5, 0.2) * 2.0, range: 4.0 }],
        draws: vec![
            draw(&plane, Mat4::IDENTITY, [0.35, 0.55, 0.3, 1.0]),
            draw(&cube, spun.matrix(), [0.85, 0.25, 0.25, 1.0]),
            // In front of the cube (closer to the camera) — must occlude it.
            draw(&sphere, Mat4::from_translation(Vec3::new(0.0, 0.5, 1.4)) * Mat4::from_scale(Vec3::splat(0.6)), [0.3, 0.5, 0.95, 1.0]),
        ],
    }));

    // A 2D square on top (the HUD layer).
    let square = renderer.upload_mesh(&tessellate_path(&Path::rect(40.0, 40.0)));
    let binding = renderer.create_draw_binding();
    let hud = [DrawCall {
        mesh: &square,
        binding: &binding,
        transform: Transform2D { translation: Vec2::new(-200.0, 120.0), ..Transform2D::default() },
        fill_color: [1.0, 1.0, 0.0, 1.0],
        blend_mode: BlendMode::Normal,
        gradient: None,
        texture: None,
        material: None,
    }];
    let pixels = renderer.render_to_texture((WIDTH as f32, HEIGHT as f32), Camera::default(), wgpu::Color::BLACK, &hud).await;
    image::RgbaImage::from_raw(WIDTH, HEIGHT, pixels.clone()).unwrap().save("scene_3d_output.png").unwrap();
    let px = |x: u32, y: u32| {
        let i = ((y * WIDTH + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2]]
    };
    let (sky, centre, hud_px) = (px(240, 10), px(240, 175), px(40, 40));
    println!("sky {sky:?} centre(sphere) {centre:?} hud {hud_px:?} floor {:?}", px(240, 300));
    assert!(sky[2] > sky[0], "sky is blue: {sky:?}");
    assert!(centre[2] > centre[0], "the blue sphere is in front of the red cube: {centre:?}");
    assert!(hud_px[0] > 200 && hud_px[1] > 200 && hud_px[2] < 60, "2D draws on top of 3D: {hud_px:?}");
    println!("scene_3d: OK (wrote scene_3d_output.png)");
}
