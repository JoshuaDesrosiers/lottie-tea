//! Sunset Tactics in real 3D — the 3D layer of the "Tactics Match" scene.
//!
//! The match simulation is unchanged (it's 2D: positions on the Sunset
//! grid, in pixels). This module adds the scene's **3D twin**, which the
//! match script keeps in sync every frame while the 3D view is on:
//!
//! - The map extruded from the same grid: every wall run (merged into big
//!   boxes) stands 4.2 m tall, crates 1.3 m, floors per zone; 1 grid cell
//!   (8 px) = 1 m, and 2D `(x, y)` ↦ 3D `(x, ?, −y) / 8`.
//! - Procedural **shader-graph materials** (the Shader tab's graphs, linked
//!   against the 3D pass): sun-baked plaster walls, paving-stone floors,
//!   wooden crates and tiled plant sites. In 3D a graph's Local Position is
//!   world meters on the surface, so the patterns tile at real scale.
//! - A Kenney character per agent (`ag3d{i}`: a pivot with the animated
//!   model `ag3d{i}Model`, a rifle `ag3d{i}Gun` and a team ring), ability pools mirroring the 2D ones (`smoke3d{i}`,
//!   `wall3d{i}`, `slow3d{i}`, `swarm3d{i}`, `proj3d{i}`, `device3d{i}`,
//!   `tracer3d{i}`, `spike3d`), the first-person camera (`cam3d`) and a
//!   low, warm sunset sun.

use an1mbyt3_core::{
    AnimationName, ArtboardId, Camera3D, ModelAsset, ModelFile, ModelId, Command, Document, Light3D, Light3DKind, Material, MaterialId, MaterialKind, Mesh3D, Node, Node3D, Node3DContent, NodeId, NodeKind, Primitive3D,
    ShaderDomain, ShaderEdgeDef, ShaderGraphDef, ShaderNodeDef, ShaderNodeKind, Transform3D,
};
use glam::{Vec2, Vec3};

use crate::demo_ninja::{add_node, with_id};
use crate::demo_tactics::{borders_floor, merge_rects, to_world, Zone, CELL, FLOORS, GW};

/// Wall / crate heights (meters).
const WALL_H: f32 = 4.2;
const CRATE_H: f32 = 1.3;

/// A tiny shader-graph builder: nodes by kind with per-input defaults.
struct Graph {
    nodes: Vec<ShaderNodeDef>,
    edges: Vec<ShaderEdgeDef>,
}

impl Graph {
    fn new() -> Self {
        Self { nodes: Vec::new(), edges: Vec::new() }
    }
    /// Adds a node; `defaults` are `(input index, value)` overrides.
    fn node(&mut self, kind: ShaderNodeKind, defaults: &[(usize, [f32; 4])]) -> u32 {
        let id = self.nodes.len() as u32 + 1;
        let mut d = vec![None; kind.inputs().len()];
        for (i, v) in defaults {
            d[*i] = Some(*v);
        }
        self.nodes.push(ShaderNodeDef { id, kind, position: Vec2::new(40.0 + (id % 4) as f32 * 230.0, 40.0 + (id / 4) as f32 * 170.0), value: [0.0; 4], defaults: d });
        id
    }
    fn wire(&mut self, from: u32, from_port: u8, to: u32, to_port: u8) {
        self.edges.push(ShaderEdgeDef { from, from_port, to, to_port });
    }
    fn finish(self) -> ShaderGraphDef {
        ShaderGraphDef { domain: ShaderDomain::Canvas2D, nodes: self.nodes, edges: self.edges }
    }
}

/// `brightness = remap(noise(lp * scale), 0..1 -> lo..hi)`; returns the node.
fn noise_tone(g: &mut Graph, lp: u32, scale: f32, lo: f32, hi: f32) -> u32 {
    let noise = g.node(ShaderNodeKind::Noise, &[(1, [scale; 4])]);
    g.wire(lp, 0, noise, 0);
    let remap = g.node(ShaderNodeKind::Remap, &[(1, [0.0; 4]), (2, [1.0; 4]), (3, [lo; 4]), (4, [hi; 4])]);
    g.wire(noise, 0, remap, 0);
    remap
}

/// Sun-baked plaster: the tint, mottled at two scales.
fn plaster() -> ShaderGraphDef {
    let mut g = Graph::new();
    let lp = g.node(ShaderNodeKind::LocalPosition, &[]);
    let fine = noise_tone(&mut g, lp, 1.6, 0.86, 1.06);
    let broad = noise_tone(&mut g, lp, 0.18, 0.84, 1.08);
    let tint = g.node(ShaderNodeKind::Tint, &[]);
    let a = g.node(ShaderNodeKind::Multiply, &[]);
    g.wire(tint, 0, a, 0);
    g.wire(fine, 0, a, 1);
    let b = g.node(ShaderNodeKind::Multiply, &[]);
    g.wire(a, 0, b, 0);
    g.wire(broad, 0, b, 1);
    let out = g.node(ShaderNodeKind::Output, &[]);
    g.wire(b, 0, out, 0);
    g.finish()
}

/// Bricks / planks / paving: a Brick pattern in the tint, with grain.
fn bricks(count: [f32; 2], mortar: f32, mortar_color: [f32; 4], grain: f32) -> ShaderGraphDef {
    let mut g = Graph::new();
    let lp = g.node(ShaderNodeKind::LocalPosition, &[]);
    let tint = g.node(ShaderNodeKind::Tint, &[]);
    let brick = g.node(ShaderNodeKind::Brick, &[(1, [count[0], count[1], 0.0, 0.0]), (2, [mortar; 4]), (4, mortar_color)]);
    g.wire(lp, 0, brick, 0);
    g.wire(tint, 0, brick, 3);
    let tone = noise_tone(&mut g, lp, grain, 0.88, 1.06);
    let shaded = g.node(ShaderNodeKind::Multiply, &[]);
    g.wire(brick, 0, shaded, 0);
    g.wire(tone, 0, shaded, 1);
    let out = g.node(ShaderNodeKind::Output, &[]);
    g.wire(shaded, 0, out, 0);
    g.finish()
}

/// Two-tone square tiles (plant sites).
fn tiles() -> ShaderGraphDef {
    let mut g = Graph::new();
    let lp = g.node(ShaderNodeKind::LocalPosition, &[]);
    let tint = g.node(ShaderNodeKind::Tint, &[]);
    let darker = g.node(ShaderNodeKind::Multiply, &[(1, [0.78; 4])]);
    g.wire(tint, 0, darker, 0);
    let checker = g.node(ShaderNodeKind::Checker, &[(1, [0.8, 0.8, 0.0, 0.0])]);
    g.wire(lp, 0, checker, 0);
    g.wire(tint, 0, checker, 2);
    g.wire(darker, 0, checker, 3);
    let tone = noise_tone(&mut g, lp, 2.2, 0.9, 1.05);
    let shaded = g.node(ShaderNodeKind::Multiply, &[]);
    g.wire(checker, 0, shaded, 0);
    g.wire(tone, 0, shaded, 1);
    let out = g.node(ShaderNodeKind::Output, &[]);
    g.wire(shaded, 0, out, 0);
    g.finish()
}

fn add_material(document: &mut Document, inverses: &mut Vec<Command>, name: &str, graph: ShaderGraphDef) -> MaterialId {
    let inverse = document.apply(Command::AddMaterial { material: Material { name: name.into(), kind: MaterialKind::ShaderGraph(graph), deleted: false } });
    let Command::SetMaterialDeleted { material, .. } = inverse else { unreachable!("AddMaterial's inverse is always SetMaterialDeleted") };
    inverses.push(inverse);
    material
}

fn node3d(name: &str, parent: Option<NodeId>, transform: Transform3D, content: Node3DContent) -> Node {
    Node::new(name, parent, NodeKind::Node3D(Node3D::new(transform, content)))
}

fn mesh(primitive: Primitive3D, color: [f32; 4], material: Option<MaterialId>) -> Node3DContent {
    Node3DContent::Mesh(Mesh3D { primitive, color, shininess: 0.12, unlit: false, material, animation: None, animation_loop: true, animation_overlay: None })
}

fn hidden(mut node: Node) -> Node {
    node.visible = false;
    node
}

/// 2D world (pixels) -> 3D ground position (meters).
fn ground(p: Vec2, y: f32) -> Vec3 {
    Vec3::new(p.x / CELL, y, -p.y / CELL)
}

/// Adds the 3D twin of the Tactics Match scene (see the module doc).
pub(crate) fn add_3d_world(document: &mut Document, inverses: &mut Vec<Command>, scene: ArtboardId, grid: &[u8]) {
    let mat_plaster = add_material(document, inverses, "Sunset Plaster", plaster());
    let mat_paving = add_material(document, inverses, "Paving Stones", bricks([1.0, 2.0], 0.05, [0.28, 0.24, 0.22, 1.0], 3.0));
    let mat_wood = add_material(document, inverses, "Crate Wood", bricks([0.5, 3.0], 0.07, [0.18, 0.11, 0.06, 1.0], 6.0));
    let mat_tiles = add_material(document, inverses, "Site Tiles", tiles());

    // Camera (moved by the script to whoever you're viewing) and the sun.
    let cam = Camera3D { fov_degrees: 78.0, near: 0.05, far: 260.0, current: true, clear_color: Some([0.93, 0.62, 0.47, 1.0]) };
    add_node(document, inverses, scene, with_id(node3d("Camera 3D", None, Transform3D::at(Vec3::new(0.0, 30.0, 40.0)), Node3DContent::Camera(cam)), "cam3d"));
    let sun = Light3D { kind: Light3DKind::Directional, color: [1.0, 0.84, 0.66], intensity: 1.05, range: 10.0 };
    add_node(document, inverses, scene, node3d("Sunset Sun", None, Transform3D { position: Vec3::new(0.0, 40.0, 0.0), rotation_degrees: Vec3::new(-32.0, -58.0, 0.0), scale: Vec3::ONE }, Node3DContent::Light(sun)));

    // ---- The map ----
    let map = add_node(document, inverses, scene, node3d("Map 3D", None, Transform3D::default(), Node3DContent::Empty));
    let half = GW as f32 * CELL / 2.0;
    add_node(document, inverses, scene, node3d("Ground", Some(map), Transform3D::at(Vec3::new(0.0, -0.12, 0.0)), mesh(Primitive3D::Plane { size: 160.0, subdivisions: 1 }, [0.36, 0.3, 0.27, 1.0], Some(mat_paving))));
    for f in FLOORS {
        let (a, b) = (to_world(f.0, f.1), to_world(f.2, f.3));
        let (tint, material) = match f.4 {
            Zone::Defender => ([0.74, 0.58, 0.5, 1.0], mat_paving),
            Zone::Mid => ([0.66, 0.64, 0.6, 1.0], mat_paving),
            Zone::Attacker => ([0.55, 0.62, 0.55, 1.0], mat_paving),
            Zone::SiteA | Zone::SiteB => ([0.86, 0.66, 0.48, 1.0], mat_tiles),
        };
        let size = ((b - a).abs() / CELL).extend(0.0);
        let at = ground((a + b) * 0.5, -0.05);
        add_node(document, inverses, scene, node3d("Floor", Some(map), Transform3D::at(at), mesh(Primitive3D::Cube { size: Vec3::new(size.x, 0.1, size.y) }, tint, Some(material))));
    }
    let cell_center = |cx: usize, cy: usize, w: usize, h: usize| Vec2::new(-half + (cx as f32 + w as f32 / 2.0) * CELL, half - (cy as f32 + h as f32 / 2.0) * CELL);
    for (cx, cy, w, h) in merge_rects(grid, |c| c == b'#') {
        // Walls deep inside solid areas are never seen — skip them.
        if !borders_floor(grid, cx, cy, w, h) {
            continue;
        }
        let at = ground(cell_center(cx, cy, w, h), WALL_H / 2.0);
        // Slight warm variation so long runs don't read as one flat colour.
        let v = ((cx * 7 + cy * 13) % 5) as f32 * 0.02;
        add_node(document, inverses, scene, node3d("Wall", Some(map), Transform3D::at(at), mesh(Primitive3D::Cube { size: Vec3::new(w as f32, WALL_H, h as f32) }, [0.86 - v, 0.68 - v, 0.52 - v, 1.0], Some(mat_plaster))));
    }
    for (cx, cy, w, h) in merge_rects(grid, |c| c == b'c') {
        let at = ground(cell_center(cx, cy, w, h), CRATE_H / 2.0);
        add_node(document, inverses, scene, node3d("Crate", Some(map), Transform3D::at(at), mesh(Primitive3D::Cube { size: Vec3::new(w as f32 - 0.06, CRATE_H, h as f32 - 0.06) }, [0.62, 0.42, 0.24, 1.0], Some(mat_wood))));
    }
    // Plant zones: a glowing outline on the ground.
    for (x, y, w, h) in [(1357.0, 620.0, 92.0, 104.0), (205.0, 710.0, 130.0, 90.0)] {
        let center = ground(to_world(x, y), 0.01);
        let (w, h) = (w / CELL, h / CELL);
        for (dx, dz, sx, sz) in [(0.0, -h / 2.0, w, 0.12), (0.0, h / 2.0, w, 0.12), (-w / 2.0, 0.0, 0.12, h), (w / 2.0, 0.0, 0.12, h)] {
            let edge = Mesh3D { primitive: Primitive3D::Cube { size: Vec3::new(sx, 0.03, sz) }, color: [1.0, 0.62, 0.3, 0.9], shininess: 0.0, unlit: true, material: None, animation: None, animation_loop: true, animation_overlay: None };
            add_node(document, inverses, scene, node3d("Plant Zone", Some(map), Transform3D::at(center + Vec3::new(dx, 0.0, dz)), Node3DContent::Mesh(edge)));
        }
    }

    // ---- Ability pools (the script mirrors the 2D ones onto these) ----
    let pool = |document: &mut Document, inverses: &mut Vec<Command>, id: String, primitive: Primitive3D, color: [f32; 4], unlit: bool| {
        let m = Mesh3D { primitive, color, shininess: 0.0, unlit, material: None, animation: None, animation_loop: true, animation_overlay: None };
        add_node(document, inverses, scene, with_id(hidden(node3d(&id, None, Transform3D::default(), Node3DContent::Mesh(m))), &id));
    };
    for i in 0..6 {
        pool(document, inverses, format!("smoke3d{i}"), Primitive3D::Sphere { radius: 1.0, segments: 24, rings: 12 }, [0.66, 0.68, 0.74, 0.97], false);
    }
    for i in 0..12 {
        pool(document, inverses, format!("wall3d{i}"), Primitive3D::Cube { size: Vec3::new(1.0, 3.2, 1.0) }, [0.5, 0.88, 0.9, 0.92], false);
    }
    for i in 0..4 {
        pool(document, inverses, format!("slow3d{i}"), Primitive3D::Cylinder { radius: 1.0, height: 0.04, segments: 32 }, [0.3, 0.7, 1.0, 0.45], true);
        pool(document, inverses, format!("swarm3d{i}"), Primitive3D::Cylinder { radius: 1.0, height: 0.05, segments: 32 }, [1.0, 0.82, 0.15, 0.45], true);
        pool(document, inverses, format!("device3d{i}"), Primitive3D::Cube { size: Vec3::splat(0.45) }, [1.0, 0.86, 0.2, 1.0], false);
    }
    for i in 0..8 {
        pool(document, inverses, format!("proj3d{i}"), Primitive3D::Sphere { radius: 0.18, segments: 12, rings: 6 }, [1.0, 1.0, 1.0, 1.0], true);
    }
    for i in 0..30 {
        pool(document, inverses, format!("tracer3d{i}"), Primitive3D::cube(), [1.0, 0.92, 0.6, 0.9], true);
    }
    pool(document, inverses, "spike3d".into(), Primitive3D::Cylinder { radius: 0.22, height: 0.5, segments: 6 }, [1.0, 0.35, 0.2, 1.0], true);

    // ---- Agents: Kenney "Blocky Characters" (CC0) holding a Kenney blaster ----
    let rifle = add_model(document, "blaster-f.glb", RIFLE_GLB, &[("Textures/colormap.png", RIFLE_TEXTURE)]);
    let model_mesh = |model: ModelId, animation: Option<&str>, overlay: Option<&str>| {
        Node3DContent::Mesh(Mesh3D {
            primitive: Primitive3D::Model { model },
            color: [1.0; 4],
            shininess: 0.15,
            unlit: false,
            material: None,
            animation: animation.map(AnimationName::new),
            animation_loop: true,
            animation_overlay: overlay.map(AnimationName::new),
        })
    };
    for i in 0..10 {
        let attacker = i < 5;
        let team = if attacker { [0.3, 1.0, 0.6, 1.0] } else { [1.0, 0.3, 0.3, 1.0] };
        let (glb, texture) = CHARACTERS[i];
        let character = add_model(document, &format!("character-{}.glb", (b'a' + i as u8) as char), glb, &[(&format!("Textures/texture-{}.png", (b'a' + i as u8) as char), texture)]);
        let pivot = add_node(document, inverses, scene, with_id(node3d(&format!("Agent {i}"), None, Transform3D::default(), Node3DContent::Empty), &format!("ag3d{i}")));
        // The models are 2.7 units tall facing +Z: scale to ~1.75 m and turn
        // to face the pivot's -Z (forward).
        let body = Transform3D { position: Vec3::ZERO, rotation_degrees: Vec3::new(0.0, 180.0, 0.0), scale: Vec3::splat(CHARACTER_SCALE) };
        add_node(document, inverses, scene, with_id(node3d(&format!("Agent {i} Model"), Some(pivot), body, model_mesh(character, Some("idle"), Some("holding-right"))), &format!("ag3d{i}Model")));
        // The rifle in the right hand (where the holding-right pose puts it).
        // (Kenney's blasters already point down -Z: no turn needed.)
        let gun = Transform3D { position: Vec3::new(0.27, 1.12, -0.42), rotation_degrees: Vec3::ZERO, scale: Vec3::splat(0.55) };
        add_node(document, inverses, scene, with_id(node3d("Rifle", Some(pivot), gun, model_mesh(rifle, None, None)), &format!("ag3d{i}Gun")));
        // A team ring at the feet — friend or foe at a glance.
        let ring = Mesh3D { primitive: Primitive3D::Torus { radius: 0.42, tube: 0.035, segments: 24, sides: 6 }, color: team, shininess: 0.0, unlit: true, material: None, animation: None, animation_loop: true, animation_overlay: None };
        add_node(document, inverses, scene, with_id(node3d("Team Ring", Some(pivot), Transform3D::at(Vec3::new(0.0, 0.04, 0.0)), Node3DContent::Mesh(ring)), &format!("ag3d{i}Band")));
        // A glowing marker over the head, so a distant agent still reads as
        // friend / foe (Valorant's red enemy highlight, in spirit).
        let mark = Mesh3D { primitive: Primitive3D::Cone { radius: 0.12, height: 0.2, segments: 4 }, color: team, shininess: 0.0, unlit: true, material: None, animation: None, animation_loop: true, animation_overlay: None };
        let mark_at = Transform3D { position: Vec3::new(0.0, 2.05, 0.0), rotation_degrees: Vec3::new(180.0, 0.0, 0.0), scale: Vec3::ONE };
        add_node(document, inverses, scene, with_id(node3d("Team Marker", Some(pivot), mark_at, Node3DContent::Mesh(mark)), &format!("ag3d{i}Mark")));
    }

    // ---- First person: your rifle and its muzzle flash, on the camera ----
    let cam3d = document.artboard(scene).and_then(|a| a.node_ids().find(|id| a.node(*id).is_some_and(|n| n.id == "cam3d"))).expect("camera added above");
    let view_gun = Transform3D { position: Vec3::new(0.22, -0.2, -0.5), rotation_degrees: Vec3::new(0.0, 4.0, 0.0), scale: Vec3::splat(0.3) };
    add_node(document, inverses, scene, with_id(node3d("View Rifle", Some(cam3d), view_gun, model_mesh(rifle, None, None)), "viewGun"));
    let flash = Mesh3D { primitive: Primitive3D::Sphere { radius: 0.05, segments: 10, rings: 6 }, color: [1.0, 0.85, 0.45, 1.0], shininess: 0.0, unlit: true, material: None, animation: None, animation_loop: true, animation_overlay: None };
    add_node(document, inverses, scene, with_id(hidden(node3d("Muzzle Flash", Some(cam3d), Transform3D::at(Vec3::new(0.205, -0.165, -0.76)), Node3DContent::Mesh(flash))), "muzzleFlash"));
}

/// Agents' model scale: Kenney's characters are 2.7 units tall.
const CHARACTER_SCALE: f32 = 0.65;

/// Kenney "Blocky Characters" (CC0, `assets/kenney/characters/License.txt`):
/// one look per agent slot.
const CHARACTERS: [(&[u8], &[u8]); 10] = [
    (include_bytes!("../assets/kenney/characters/character-a.glb"), include_bytes!("../assets/kenney/characters/Textures/texture-a.png")),
    (include_bytes!("../assets/kenney/characters/character-b.glb"), include_bytes!("../assets/kenney/characters/Textures/texture-b.png")),
    (include_bytes!("../assets/kenney/characters/character-c.glb"), include_bytes!("../assets/kenney/characters/Textures/texture-c.png")),
    (include_bytes!("../assets/kenney/characters/character-d.glb"), include_bytes!("../assets/kenney/characters/Textures/texture-d.png")),
    (include_bytes!("../assets/kenney/characters/character-e.glb"), include_bytes!("../assets/kenney/characters/Textures/texture-e.png")),
    (include_bytes!("../assets/kenney/characters/character-f.glb"), include_bytes!("../assets/kenney/characters/Textures/texture-f.png")),
    (include_bytes!("../assets/kenney/characters/character-g.glb"), include_bytes!("../assets/kenney/characters/Textures/texture-g.png")),
    (include_bytes!("../assets/kenney/characters/character-h.glb"), include_bytes!("../assets/kenney/characters/Textures/texture-h.png")),
    (include_bytes!("../assets/kenney/characters/character-i.glb"), include_bytes!("../assets/kenney/characters/Textures/texture-i.png")),
    (include_bytes!("../assets/kenney/characters/character-j.glb"), include_bytes!("../assets/kenney/characters/Textures/texture-j.png")),
];
/// Kenney "Blaster Kit" (CC0, `assets/kenney/blasters/License.txt`).
const RIFLE_GLB: &[u8] = include_bytes!("../assets/kenney/blasters/blaster-f.glb");
const RIFLE_TEXTURE: &[u8] = include_bytes!("../assets/kenney/blasters/Textures/colormap.png");

/// Adds an embedded model asset (+ its texture files) to the document.
fn add_model(document: &mut Document, name: &str, glb: &[u8], files: &[(&str, &[u8])]) -> ModelId {
    document.add_model(ModelAsset {
        name: name.into(),
        source: glb.to_vec(),
        files: files.iter().map(|(path, data)| ModelFile { path: (*path).into(), data: data.to_vec() }).collect(),
        deleted: false,
    })
}
