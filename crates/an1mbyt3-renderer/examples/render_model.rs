//! A Kenney character (walk + holding-right, mid-stride) and a rifle,
//! rendered headless through the 3D pass with their textures — checks glTF
//! import, node animation, and texture orientation. Writes
//! `render_model_output.png`.

use std::rc::Rc;

use an1mbyt3_core::{compile_wgsl, ShaderGraphDef};
use an1mbyt3_renderer::{Camera, ImageFilterMode, MeshDraw3D, Renderer, Scene3D};
use glam::{Mat4, Quat, Vec3};

const W: u32 = 480;
const H: u32 = 360;
const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../an1mbyt3-wasm/assets/kenney");

fn main() {
    pollster::block_on(run());
}

fn load(renderer: &Renderer, path: &str, dir: &str) -> (an1mbyt3_geometry::ModelData, Vec<Option<an1mbyt3_renderer::GpuImageTexture>>, Vec<Rc<an1mbyt3_renderer::GpuMesh3D>>) {
    let bytes = std::fs::read(path).unwrap();
    let model = an1mbyt3_geometry::parse_gltf(&bytes, &|uri| std::fs::read(format!("{dir}/{uri}")).ok()).unwrap();
    let textures = model
        .images
        .iter()
        .map(|png| {
            let img = image::load_from_memory(png).ok()?.to_rgba8();
            Some(renderer.upload_texture_wrapped(img.width(), img.height(), &img, ImageFilterMode::Nearest, true))
        })
        .collect();
    let meshes = model.primitives.iter().map(|p| Rc::new(renderer.upload_mesh_3d(&p.mesh))).collect();
    (model, textures, meshes)
}

async fn run() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let renderer = Renderer::new_headless(&instance, W, H).await;
    let textured = Rc::new(renderer.compile_material_3d(&compile_wgsl(&ShaderGraphDef::starter()).unwrap()).unwrap());
    let chars = format!("{ASSETS}/characters");
    let guns = format!("{ASSETS}/blasters");
    let (character, char_tex, char_meshes) = load(&renderer, &format!("{chars}/character-a.glb"), &chars);
    let (rifle, rifle_tex, rifle_meshes) = load(&renderer, &format!("{guns}/blaster-f.glb"), &guns);

    let mut draws = Vec::new();
    let mut add = |model: &an1mbyt3_geometry::ModelData, tex: &[Option<an1mbyt3_renderer::GpuImageTexture>], meshes: &[Rc<an1mbyt3_renderer::GpuMesh3D>], world: Mat4, pose: Vec<Mat4>| {
        for (p, mesh) in model.primitives.iter().zip(meshes) {
            let texture = p.image.and_then(|i| tex.get(i).cloned().flatten());
            draws.push(MeshDraw3D { material: texture.as_ref().map(|_| Rc::clone(&textured)), texture, ..MeshDraw3D::new(Rc::clone(mesh), world * pose[p.node], p.base_color) });
        }
    };
    // The character, as an agent: scaled 0.65, turned to face -Z (us at +Z looking back... we look at its front).
    let walk = character.clip("walk");
    let hold = character.clip("holding-right");
    let agent = Mat4::from_scale_rotation_translation(Vec3::splat(0.65), Quat::from_rotation_y(std::f32::consts::PI), Vec3::ZERO);
    add(&character, &char_tex, &char_meshes, agent, character.pose_layers(&[(walk, 0.17), (hold, 0.0)]));
    // The rifle where the agent holds it (pivot space -> same as agent's).
    let gun = Mat4::from_scale_rotation_translation(Vec3::splat(0.55), Quat::from_rotation_y(std::f32::consts::PI), Vec3::new(0.27, 1.12, -0.42));
    add(&rifle, &rifle_tex, &rifle_meshes, gun, rifle.pose(None, 0.0));

    // Camera in front of the agent (agent faces -Z, so we stand at -Z).
    let eye = Vec3::new(2.2, 1.6, -2.4);
    renderer.set_scene_3d(Some(Scene3D {
        view: glam::camera::rh::view::look_at_mat4(eye, Vec3::new(0.0, 0.95, 0.0), Vec3::Y),
        projection: glam::camera::rh::proj::directx::perspective(50f32.to_radians(), W as f32 / H as f32, 0.05, 100.0),
        camera_position: eye,
        clear_color: Some([0.93, 0.62, 0.47, 1.0]),
        sun_direction: Vec3::new(-0.5, -1.0, 0.6),
        sun_color: Vec3::splat(0.9),
        ambient: Vec3::new(0.3, 0.33, 0.4),
        fog_color: Vec3::ZERO,
        fog_density: 0.0,
        point_lights: Vec::new(),
        draws,
    }));
    let pixels = renderer.render_to_texture((W as f32, H as f32), Camera::default(), wgpu::Color::BLACK, &[]).await;
    image::RgbaImage::from_raw(W, H, pixels).unwrap().save("render_model_output.png").unwrap();
    println!("render_model: wrote render_model_output.png ({} char prims, {} rifle prims)", character.primitives.len(), rifle.primitives.len());
}
