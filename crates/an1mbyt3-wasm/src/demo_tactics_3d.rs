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
    ShaderDomain, ShaderEdgeDef, ShaderGraphDef, ShaderNodeDef, ShaderNodeKind, Transform3D, Collider3D, Collider3DShape, NodeBehavior, NodeBehaviorEntry, Rigidbody3D,
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

/// Static scenery: a collider (fitted to the mesh) and no body.
fn solid(mut node: Node) -> Node {
    node.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::Collider3D(Collider3D { friction: 0.7, restitution: 0.15, ..Default::default() }), enabled: true });
    node
}

/// A dynamic physics body (`Rigidbody3D` + `collider`).
fn physical(mut node: Node, body: Rigidbody3D, collider: Collider3D) -> Node {
    node.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::Rigidbody3D(body), enabled: true });
    node.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::Collider3D(collider), enabled: true });
    node
}

/// Where pooled physics objects wait: on a slab far below the map, still
/// "visible" so they stay in the simulation (showing / hiding a body would
/// rebuild the physics world on every shot).
pub(crate) const PARK_Y: f32 = -500.0;

fn hidden(mut node: Node) -> Node {
    node.visible = false;
    node
}

/// 2D world (pixels) -> 3D ground position (meters).
fn ground(p: Vec2, y: f32) -> Vec3 {
    Vec3::new(p.x / CELL, y, -p.y / CELL)
}

/// Adds the 3D twin of the Tactics Match scene (see the module doc).
/// With `avatar` (a VRM), every agent wears it and moves with the
/// generated rifle clips (`demo_tactics_anim.rs`); returns the avatar's
/// model asset and the enemy agents' model nodes. Without one, the agents
/// are Kenney's blocky characters.
pub(crate) fn add_3d_world(document: &mut Document, inverses: &mut Vec<Command>, scene: ArtboardId, grid: &[u8], avatar: Option<crate::demo_tactics::Avatar3D>) -> Option<(ModelId, Vec<NodeId>)> {
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
    add_node(document, inverses, scene, solid(node3d("Ground", Some(map), Transform3D::at(Vec3::new(0.0, -0.12, 0.0)), mesh(Primitive3D::Plane { size: 160.0, subdivisions: 1 }, [0.36, 0.3, 0.27, 1.0], Some(mat_paving)))));
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
        add_node(document, inverses, scene, solid(node3d("Floor", Some(map), Transform3D::at(at), mesh(Primitive3D::Cube { size: Vec3::new(size.x, 0.1, size.y) }, tint, Some(material)))));
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
        add_node(document, inverses, scene, solid(node3d("Wall", Some(map), Transform3D::at(at), mesh(Primitive3D::Cube { size: Vec3::new(w as f32, WALL_H, h as f32) }, [0.86 - v, 0.68 - v, 0.52 - v, 1.0], Some(mat_plaster)))));
    }
    for (cx, cy, w, h) in merge_rects(grid, |c| c == b'c') {
        let at = ground(cell_center(cx, cy, w, h), CRATE_H / 2.0);
        add_node(document, inverses, scene, solid(node3d("Crate", Some(map), Transform3D::at(at), mesh(Primitive3D::Cube { size: Vec3::new(w as f32 - 0.06, CRATE_H, h as f32 - 0.06) }, [0.62, 0.42, 0.24, 1.0], Some(mat_wood)))));
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

    // ---- Physics pools (Rigidbody 3D), parked below the map until used ----
    let park = node3d("Parking", None, Transform3D::at(Vec3::new(0.0, PARK_Y, 0.0)), Node3DContent::Empty);
    let mut park = park;
    park.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::Collider3D(Collider3D { shape: Collider3DShape::Box { size: Vec3::new(400.0, 1.0, 80.0) }, ..Default::default() }), enabled: true });
    add_node(document, inverses, scene, park);
    let rest = PARK_Y + 0.5;
    // Thrown utility: real projectiles that arc and bank off walls.
    for i in 0..8 {
        let ball = Mesh3D { primitive: Primitive3D::Sphere { radius: 0.13, segments: 12, rings: 6 }, color: [1.0; 4], shininess: 0.6, unlit: false, material: None, animation: None, animation_loop: true, animation_overlay: None };
        let node = physical(
            with_id(node3d(&format!("Nade {i}"), None, Transform3D::at(Vec3::new(-150.0 + i as f32 * 1.5, rest + 0.13, 0.0)), Node3DContent::Mesh(ball)), &format!("nade{i}")),
            Rigidbody3D { mass: 0.4, angular_damping: 0.6, continuous: true, ..Default::default() },
            Collider3D { shape: Collider3DShape::Sphere { radius: 0.13 }, friction: 0.7, restitution: 0.4, ..Default::default() },
        );
        add_node(document, inverses, scene, node);
    }
    // Wall chips knocked off by bullet impacts.
    for i in 0..24 {
        let chip = Mesh3D { primitive: Primitive3D::Cube { size: Vec3::splat(0.08) }, color: [0.82, 0.66, 0.5, 1.0], shininess: 0.0, unlit: false, material: None, animation: None, animation_loop: true, animation_overlay: None };
        let node = physical(
            with_id(node3d("Chip", None, Transform3D::at(Vec3::new(-150.0 + i as f32 * 0.5, rest + 0.04, 10.0)), Node3DContent::Mesh(chip)), &format!("chip{i}")),
            Rigidbody3D { mass: 0.03, ..Default::default() },
            Collider3D { restitution: 0.25, ..Default::default() },
        );
        add_node(document, inverses, scene, node);
    }

    // ---- Agents: Kenney "Blocky Characters" (CC0) holding a Kenney blaster ----
    // (or, given a VRM avatar, the avatar: see `add_avatar_agents`).
    let rifle = add_model(document, "blaster-f.glb", RIFLE_GLB, &[("Textures/colormap.png", RIFLE_TEXTURE)]);
    let avatar_agents = avatar.map(|a| add_avatar_agents(document, inverses, scene, a, rifle));
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
        if avatar_agents.is_some() {
            break;
        }
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

    // A dropped rifle per agent: tumbles out of a dead agent's hands.
    for i in 0..10 {
        let node = physical(
            with_id(node3d("Dropped Rifle", None, Transform3D { position: Vec3::new(-150.0 + i as f32 * 2.0, rest + 0.2, 20.0), rotation_degrees: Vec3::ZERO, scale: Vec3::splat(0.55) }, model_mesh(rifle, None, None)), &format!("drop{i}")),
            Rigidbody3D { mass: 3.0, angular_damping: 0.3, ..Default::default() },
            Collider3D { friction: 0.8, restitution: 0.1, ..Default::default() },
        );
        add_node(document, inverses, scene, node);
    }

    // ---- First person: your rifle and its muzzle flash, on the camera ----
    let cam3d = document.artboard(scene).and_then(|a| a.node_ids().find(|id| a.node(*id).is_some_and(|n| n.id == "cam3d"))).expect("camera added above");
    let view_gun = Transform3D { position: Vec3::new(0.22, -0.2, -0.5), rotation_degrees: Vec3::new(0.0, 4.0, 0.0), scale: Vec3::splat(0.3) };
    let view_rifle = add_node(document, inverses, scene, with_id(node3d("View Rifle", Some(cam3d), view_gun, model_mesh(rifle, None, None)), "viewGun"));
    add_view_arms(document, inverses, scene, view_rifle);
    let flash = Mesh3D { primitive: Primitive3D::Sphere { radius: 0.05, segments: 10, rings: 6 }, color: [1.0, 0.85, 0.45, 1.0], shininess: 0.0, unlit: true, material: None, animation: None, animation_loop: true, animation_overlay: None };
    add_node(document, inverses, scene, with_id(hidden(node3d("Muzzle Flash", Some(cam3d), Transform3D::at(Vec3::new(0.205, -0.165, -0.76)), Node3DContent::Mesh(flash))), "muzzleFlash"));
    avatar_agents
}

/// The pistol grip on Kenney's blaster-f, in the model's own units (the
/// view arms' right fist sits here too).
const RIFLE_GRIP: Vec3 = Vec3::new(0.0, -0.24, 0.36);
/// The muzzle, same units.
const RIFLE_MUZZLE: Vec3 = Vec3::new(-0.05, 0.117, -0.87);
/// Rifle scale on an agent (about 0.72 m long).
const RIFLE_SCALE: f32 = 0.55;

/// The agents as a VRM avatar (`ag3d{i}` pivots, like the Kenney ones):
/// - `ag3d{i}Model`: the avatar, turned to face the pivot's -Z, with a
///   **Humanoid Animator** holding the generated rifle clips (one shared
///   `Rifle Moves.vrma` asset: Idle, Walk, Run, RunBack, RunLeft,
///   RunRight, Reload; the script picks one from how the agent moves),
///   **Foot IK** and a **Ragdoll 3D** (deaths go limp);
/// - `ag3d{i}Gun`: the rifle, a **Bone Attachment** on the right hand
///   (so it follows every step and the reload), with its muzzle flash
///   `ag3d{i}Flash`;
/// - the team ring and the head marker, as before.
fn add_avatar_agents(document: &mut Document, inverses: &mut Vec<Command>, scene: ArtboardId, avatar: crate::demo_tactics::Avatar3D, rifle: ModelId) -> (ModelId, Vec<NodeId>) {
    use an1mbyt3_core::{BoneAttachment, FootIK, HumanoidAnimator, HumanoidClipRef, Ragdoll3D};
    let model = document.add_model(ModelAsset { name: "popi.vrm".into(), source: avatar.source, files: Vec::new(), deleted: false });
    let (moves_vrma, speeds) = crate::demo_tactics_anim::rifle_moves_vrma();
    let moves = document.add_model(ModelAsset { name: "Rifle Moves.vrma".into(), source: moves_vrma, files: Vec::new(), deleted: false });
    let animator = HumanoidAnimator {
        clips: speeds.iter().map(|(name, _)| HumanoidClipRef { name: name.clone(), source: Some(moves), clip: name.clone() }).collect(),
        playing: "Idle".into(),
        looping: true,
        speed: 1.0,
        blend: 0.16,
        preview: true,
    };
    let (gun_at, gun_turn) = crate::demo_tactics_anim::rifle_in_hand(RIFLE_GRIP, RIFLE_SCALE);
    let mut gun_transform = Transform3D { position: gun_at, rotation_degrees: Vec3::ZERO, scale: Vec3::splat(RIFLE_SCALE) };
    gun_transform.set_rotation(gun_turn);
    let mut enemies = Vec::new();
    for i in 0..10 {
        let attacker = i < 5;
        let team = if attacker { [0.3, 1.0, 0.6, 1.0] } else { [1.0, 0.3, 0.3, 1.0] };
        let pivot = add_node(document, inverses, scene, with_id(node3d(&format!("Agent {i}"), None, Transform3D::default(), Node3DContent::Empty), &format!("ag3d{i}")));
        // Forward is the pivot's -Z: VRM 0.x avatars already face it.
        let body = Transform3D { position: Vec3::ZERO, rotation_degrees: Vec3::new(0.0, if avatar.faces_neg_z { 0.0 } else { 180.0 }, 0.0), scale: Vec3::ONE };
        let mut node = node3d(&format!("Agent {i} Model"), Some(pivot), body, mesh_of(model));
        for behavior in [
            NodeBehavior::HumanoidAnimator(animator.clone()),
            NodeBehavior::FootIK(FootIK::default()),
            NodeBehavior::Ragdoll3D(Ragdoll3D { start_ragdolled: false, mass: 55.0, flexibility: 0.8, thickness: 1.0 }),
        ] {
            node.behaviors.push(NodeBehaviorEntry { behavior, enabled: true });
        }
        let body = add_node(document, inverses, scene, with_id(node, &format!("ag3d{i}Model")));
        if !attacker {
            enemies.push(body);
        }
        let mut gun = with_id(node3d("Rifle", Some(body), gun_transform, mesh_of(rifle)), &format!("ag3d{i}Gun"));
        gun.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::BoneAttachment(BoneAttachment { bone: "rightHand".into() }), enabled: true });
        let gun = add_node(document, inverses, scene, gun);
        let flash = Mesh3D { primitive: Primitive3D::Sphere { radius: 0.13, segments: 10, rings: 6 }, color: [1.0, 0.85, 0.45, 1.0], shininess: 0.0, unlit: true, material: None, animation: None, animation_loop: true, animation_overlay: None };
        add_node(document, inverses, scene, with_id(hidden(node3d("Muzzle Flash", Some(gun), Transform3D::at(RIFLE_MUZZLE), Node3DContent::Mesh(flash))), &format!("ag3d{i}Flash")));
        let ring = Mesh3D { primitive: Primitive3D::Torus { radius: 0.42, tube: 0.035, segments: 24, sides: 6 }, color: team, shininess: 0.0, unlit: true, material: None, animation: None, animation_loop: true, animation_overlay: None };
        add_node(document, inverses, scene, with_id(node3d("Team Ring", Some(pivot), Transform3D::at(Vec3::new(0.0, 0.04, 0.0)), Node3DContent::Mesh(ring)), &format!("ag3d{i}Band")));
        let mark = Mesh3D { primitive: Primitive3D::Cone { radius: 0.12, height: 0.2, segments: 4 }, color: team, shininess: 0.0, unlit: true, material: None, animation: None, animation_loop: true, animation_overlay: None };
        let mark_at = Transform3D { position: Vec3::new(0.0, 1.9, 0.0), rotation_degrees: Vec3::new(180.0, 0.0, 0.0), scale: Vec3::ONE };
        add_node(document, inverses, scene, with_id(node3d("Team Marker", Some(pivot), mark_at, Node3DContent::Mesh(mark)), &format!("ag3d{i}Mark")));
    }
    (model, enemies)
}

/// A plain (unanimated) model mesh.
fn mesh_of(model: ModelId) -> Node3DContent {
    Node3DContent::Mesh(Mesh3D { primitive: Primitive3D::Model { model }, color: [1.0; 4], shininess: 0.15, unlit: false, material: None, animation: None, animation_loop: true, animation_overlay: None })
}

/// First-person hands and forearms holding the view rifle — built from
/// primitives in the rifle's own units (its model is 1.31 long down −Z,
/// grip at the back, handguard in front; the rifle node is scaled 0.3, so
/// 1 unit ≈ 0.3 m). Children of the rifle, so they kick and bob with it.
/// Valorant-style: dark tactical gloves, rolled-up sleeves in the attacker
/// teal, a strip of wrist between them.
fn add_view_arms(document: &mut Document, inverses: &mut Vec<Command>, scene: ArtboardId, rifle: NodeId) {
    const GLOVE: [f32; 4] = [0.14, 0.14, 0.17, 1.0];
    const KNUCKLE: [f32; 4] = [0.2, 0.2, 0.24, 1.0];
    const SKIN: [f32; 4] = [0.82, 0.6, 0.47, 1.0];
    const SLEEVE: [f32; 4] = [0.16, 0.38, 0.42, 1.0];
    const CUFF: [f32; 4] = [0.1, 0.25, 0.28, 1.0];
    let arms = add_node(document, inverses, scene, node3d("View Arms", Some(rifle), Transform3D::default(), Node3DContent::Empty));
    // A capsule-ish limb from `a` to `b`: a cylinder plus rounded ends.
    let limb = |document: &mut Document, inverses: &mut Vec<Command>, name: &str, a: Vec3, b: Vec3, radius: f32, color: [f32; 4], caps: bool| {
        let d = b - a;
        let mut t = Transform3D::at((a + b) / 2.0);
        t.set_rotation(glam::Quat::from_rotation_arc(Vec3::Y, d.normalize_or_zero()));
        add_node(document, inverses, scene, node3d(name, Some(arms), t, mesh(Primitive3D::Cylinder { radius, height: d.length(), segments: 14 }, color, None)));
        if caps {
            for (end, p) in [("A", a), ("B", b)] {
                add_node(document, inverses, scene, node3d(&format!("{name} {end}"), Some(arms), Transform3D::at(p), mesh(Primitive3D::Sphere { radius, segments: 12, rings: 8 }, color, None)));
            }
        }
    };
    let block = |document: &mut Document, inverses: &mut Vec<Command>, name: &str, at: Vec3, size: Vec3, tilt: Vec3, color: [f32; 4]| {
        let t = Transform3D { position: at, rotation_degrees: tilt, scale: Vec3::ONE };
        add_node(document, inverses, scene, node3d(name, Some(arms), t, mesh(Primitive3D::Cube { size }, color, None)));
    };

    // ---- Right hand: a fist around the pistol grip (behind the magazine) ----
    let grip = Vec3::new(0.0, -0.24, 0.36);
    block(document, inverses, "R Palm", grip + Vec3::new(0.035, 0.0, 0.05), Vec3::new(0.2, 0.26, 0.2), Vec3::new(-12.0, 0.0, 0.0), GLOVE);
    for i in 0..4 {
        // Fingers curled around the front of the grip, stacked downward.
        let y = grip.y + 0.08 - i as f32 * 0.07;
        limb(document, inverses, &format!("R Finger {}", i + 1), Vec3::new(0.1, y, -0.06 + grip.z + 0.0), Vec3::new(-0.09, y, -0.07 + grip.z), 0.034, GLOVE, true);
        add_node(document, inverses, scene, node3d(&format!("R Knuckle {}", i + 1), Some(arms), Transform3D::at(Vec3::new(0.1, y, grip.z - 0.02)), mesh(Primitive3D::Sphere { radius: 0.038, segments: 10, rings: 6 }, KNUCKLE, None)));
    }
    // Trigger finger along the guard, thumb over the top of the stock.
    limb(document, inverses, "R Trigger Finger", Vec3::new(0.09, -0.11, 0.3), Vec3::new(0.085, -0.1, 0.12), 0.03, GLOVE, true);
    limb(document, inverses, "R Thumb", Vec3::new(0.1, -0.12, 0.42), Vec3::new(0.07, -0.02, 0.28), 0.034, GLOVE, true);
    // Wrist and forearm back toward the camera (down and to the right).
    let r_wrist = grip + Vec3::new(0.06, -0.12, 0.2);
    let r_elbow = Vec3::new(0.55, -0.95, 1.55);
    limb(document, inverses, "R Wrist", grip + Vec3::new(0.04, -0.05, 0.12), r_wrist, 0.085, SKIN, false);
    limb(document, inverses, "R Cuff", r_wrist, r_wrist + (r_elbow - r_wrist) * 0.12, 0.11, CUFF, false);
    limb(document, inverses, "R Sleeve", r_wrist + (r_elbow - r_wrist) * 0.1, r_elbow, 0.13, SLEEVE, false);

    // ---- Left hand: cupping the handguard from below, fingers up the near side ----
    let guard = Vec3::new(0.0, -0.17, -0.34);
    block(document, inverses, "L Palm", guard + Vec3::new(-0.02, -0.04, 0.0), Vec3::new(0.24, 0.1, 0.26), Vec3::new(0.0, 0.0, -10.0), GLOVE);
    for i in 0..4 {
        let z = guard.z - 0.1 + i as f32 * 0.066;
        limb(document, inverses, &format!("L Finger {}", i + 1), Vec3::new(0.095, -0.17, z), Vec3::new(0.1, -0.02, z - 0.01), 0.031, GLOVE, true);
    }
    limb(document, inverses, "L Thumb", Vec3::new(-0.1, -0.17, guard.z + 0.08), Vec3::new(-0.1, -0.04, guard.z - 0.06), 0.032, GLOVE, true);
    let l_wrist = guard + Vec3::new(-0.12, -0.12, 0.14);
    let l_elbow = Vec3::new(-0.95, -1.05, 1.1);
    limb(document, inverses, "L Wrist", guard + Vec3::new(-0.06, -0.07, 0.07), l_wrist, 0.08, SKIN, false);
    limb(document, inverses, "L Cuff", l_wrist, l_wrist + (l_elbow - l_wrist) * 0.12, 0.105, CUFF, false);
    limb(document, inverses, "L Sleeve", l_wrist + (l_elbow - l_wrist) * 0.1, l_elbow, 0.125, SLEEVE, false);
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
