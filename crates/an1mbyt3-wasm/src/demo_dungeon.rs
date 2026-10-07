//! "Dungeon Royale" — a Clash Royale x dungeon-crawler tower defense, built
//! from the 2D Pixel Dungeon Asset Pack.
//!
//! Drag cards from the hand at the bottom into the dungeon to deploy
//! troops, buildings and spells; waves of monsters pour out of portals and
//! fight their way to your keep. Every run is a fresh, randomly generated
//! dungeon (rooms + corridors + loops) with its own portal placement.
//!
//! What lives where:
//! - **Rust (this file)** builds the scene: the tilemaps (left empty — the
//!   script generates the level), small fixed *pools* of lights, effects
//!   and HUD pieces parked hidden, and three **prefabs** — `Unit` (sprite +
//!   HP bar), `Arrow` and `Bolt` (with its own Light 2D). The script spawns
//!   prefab instances as troops deploy and shots fire (`ctx.spawn`) and
//!   destroys them on death / impact (`ref.destroy()`); runtime spawns never
//!   touch the undo stack and are cleared when play stops. Edit a prefab
//!   (Resources > Prefabs, or select a placed instance and Apply) and every
//!   unit or shot uses the new version.
//! - **The Code script (`DUNGEON_SCRIPT`)** is the game: dungeon generation,
//!   A* pathfinding (8-way, octile heuristic, binary heap, no corner
//!   cutting, line-of-sight shortcutting), the bots' little state machines
//!   (aggro / chase / kite / flee / follow-the-leader / heal-the-hurt /
//!   siege), card hand + elixir economy, drag-and-drop deployment, waves,
//!   lighting cues and particles.
//!
//! Engine features it leans on (all general-purpose script APIs):
//! `ref.setTiles` (procedural tilemaps), untracked `ref.visible`,
//! `ref.lightIntensity`/`lightRadius` (flicker, flashes), `ref.fillColor`
//! on a light-only node (light color), `ref.emit` (one-shot particles),
//! Y-sorted groups (depth by feet), Light 2D *components* on ordinary
//! sprites (torches, braziers, the keep), Pixel Perfect camera + vignette.

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Command, ConditionDef, Document, Easing, InputDef, InputKind, Keyframe, PlaybackDirection, PropertyTrack,
    StateMachineLayerDef, Track, TransitionDef, EmitterShape, FontAsset, GradientDef, GradientKind, GradientStop, GroupNode, ImageId, Light2DNode,
    LightOverlapOperation, LoopMode, Node, NodeBehavior, NodeBehaviorEntry, NodeKind, ParticleRange, ParticleSystemNode, Path, PrefabDefinition, PrefabNode, ProgramDef, ShapeNode,
    SimulationSpace, SizeKeyframe, StateDef, StateMachineDef, Stroke, TilemapNode,
};
use glam::Vec2;

use crate::demo_ninja::{add_grid_frames, add_node, add_png, feet_anchored, image_node, text_node, with_id};

const TILESET_BYTES: &[u8] = include_bytes!("../assets/dungeon/tileset.png");
const UNITS_BYTES: &[u8] = include_bytes!("../assets/dungeon/units.png");
const ITEMS_BYTES: &[u8] = include_bytes!("../assets/dungeon/items.png");
const CARDS_BYTES: &[u8] = include_bytes!("../assets/dungeon/cards.png");
const CASTLE_BYTES: &[u8] = include_bytes!("../assets/dungeon/castle.png");
const ELIXIR_BYTES: &[u8] = include_bytes!("../assets/dungeon/elixir.png");
const PIXEL_FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");

/// Portrait phone-ish frame, like the game it riffs on.
const SCREEN_W: f32 = 288.0;
const SCREEN_H: f32 = 512.0;
/// The dungeon grid: 18 x 24 cells of 16px = 288 x 384, top-aligned; the
/// bottom 128px is the card tray.
pub(crate) const MAP_W: u32 = 18;
pub(crate) const MAP_H: u32 = 24;
const TILE: f32 = 16.0;
const MAP_CENTER_Y: f32 = SCREEN_H / 2.0 - MAP_H as f32 * TILE / 2.0;

/// Pool sizes — the script reads the same numbers. `UNITS`/`ARROWS`/`BOLTS`
/// are only caps now (those are spawned from prefabs), the rest are real
/// pre-built pools.
const UNITS: usize = 48;
const ARROWS: usize = 16;
const BOLTS: usize = 12;
const TORCHES: usize = 12;
const BRAZIERS: usize = 3;
const SPIKES: usize = 4;
const UNIT_LIGHTS: usize = 12;
const FLASHES: usize = 4;
const COINS: usize = 10;
const PORTALS: usize = 3;

pub(crate) struct DungeonDemo {
    pub scene: ArtboardId,
    pub images: Vec<ImageId>,
}

fn shape(name: &str, parent: Option<an1mbyt3_core::NodeId>, path: Path, color: [f32; 4], at: Vec2) -> Node {
    let mut node = Node::new(name, parent, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None }));
    node.transform.translation = at;
    node
}

fn hud(mut node: Node) -> Node {
    node.unlit = true;
    node
}

fn vertical_gradient(top: [f32; 4], bottom: [f32; 4], half_h: f32) -> GradientDef {
    GradientDef {
        kind: GradientKind::Linear,
        start: Vec2::new(0.0, half_h),
        end: Vec2::new(0.0, -half_h),
        stops: vec![GradientStop { offset: 0.0, color: top, midpoint: 0.5 }, GradientStop { offset: 1.0, color: bottom, midpoint: 0.5 }],
    }
}

/// A pixel-friendly point light: 3 crisp falloff rings, merged with
/// neighbors by Max (overlapping glows join instead of blowing out).
pub(crate) fn glow(color: [f32; 3], intensity: f32, radius: f32) -> Light2DNode {
    Light2DNode {
        color: [color[0], color[1], color[2], 1.0],
        intensity,
        inner_radius: radius * 0.12,
        outer_radius: radius,
        falloff_steps: 3,
        edge_blur: 0.0,
        falloff_strength: 0.45,
        pixel_size: 2.0,
        overlap_operation: LightOverlapOperation::Max,
        ..Light2DNode::default()
    }
}

/// Attaches components to an existing node (Unity "Add Component").
fn with_components(mut node: Node, components: Vec<NodeBehavior>) -> Node {
    node.behaviors.extend(components.into_iter().map(|behavior| NodeBehaviorEntry { behavior, enabled: true }));
    node
}

/// Registers a prefab built from `nodes` (`(parent index, node)`, parents
/// first, root first) — the demo's spawnable templates. Each node's index
/// becomes its stable prefab uid.
fn add_prefab(document: &mut Document, inverses: &mut Vec<Command>, name: &str, nodes: Vec<(Option<u32>, Node)>) {
    let nodes: Vec<PrefabNode> = nodes
        .into_iter()
        .enumerate()
        .map(|(i, (parent, mut node))| {
            node.parent = None;
            node.id = String::new();
            PrefabNode { uid: i as u32, parent, node }
        })
        .collect();
    let next_uid = nodes.len() as u32;
    inverses.push(Command::AddPrefab { definition: PrefabDefinition { name: name.into(), nodes, next_uid, deleted: false } }.apply(document));
}

fn hidden(mut node: Node) -> Node {
    node.visible = false;
    node.transform.translation = Vec2::new(100_000.0, 0.0);
    node
}

/// A one-shot, world-space burst emitter the script fires with `emit(n)`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn burst(id: &str, color: [f32; 4], speed: (f32, f32), lifetime: (f32, f32), size: (f32, f32), gravity: f32, drag: f32, grow: bool, radius: f32) -> Node {
    let mut config = ParticleSystemNode::default();
    config.main.looping = false;
    config.main.duration = 0.05;
    config.main.start_speed = ParticleRange { min: speed.0, max: speed.1 };
    config.main.start_lifetime = ParticleRange { min: lifetime.0, max: lifetime.1 };
    config.main.start_size = ParticleRange { min: size.0, max: size.1 };
    config.main.start_color = color;
    config.main.gravity_modifier = gravity;
    config.main.drag = drag;
    config.main.simulation_space = SimulationSpace::World;
    config.main.max_particles = 200;
    config.emission.rate_over_time = 0.0;
    config.emission.bursts.clear();
    config.shape = EmitterShape::Circle { radius };
    config.color_over_lifetime = vec![
        GradientStop { offset: 0.0, color: [1.0, 1.0, 1.0, 1.0], midpoint: 0.5 },
        GradientStop { offset: 0.7, color: [1.0, 1.0, 1.0, 0.8], midpoint: 0.5 },
        GradientStop { offset: 1.0, color: [1.0, 1.0, 1.0, 0.0], midpoint: 0.5 },
    ];
    config.size_over_lifetime = if grow {
        vec![SizeKeyframe { t: 0.0, size_multiplier: 0.5 }, SizeKeyframe { t: 1.0, size_multiplier: 1.8 }]
    } else {
        vec![SizeKeyframe { t: 0.0, size_multiplier: 1.0 }, SizeKeyframe { t: 1.0, size_multiplier: 0.25 }]
    };
    let mut node = with_id(Node::with_components(id, None, vec![NodeBehavior::ParticleSystem(config)]), id);
    node.transform.translation = Vec2::new(100_000.0, 0.0);
    node.unlit = true;
    node
}

/// A portal: a looping violet swirl (particles) plus its own glow — two
/// components on one node.
fn portal(i: usize) -> Node {
    let mut config = ParticleSystemNode::default();
    config.main.looping = true;
    config.main.duration = 2.0;
    config.main.start_lifetime = ParticleRange { min: 0.6, max: 1.2 };
    config.main.start_speed = ParticleRange { min: 4.0, max: 14.0 };
    config.main.start_size = ParticleRange { min: 1.0, max: 2.0 };
    config.main.start_color = [0.75, 0.4, 1.0, 1.0];
    config.main.gravity_modifier = -0.6;
    config.main.simulation_space = SimulationSpace::World;
    config.main.max_particles = 60;
    config.emission.rate_over_time = 26.0;
    config.emission.bursts.clear();
    config.shape = EmitterShape::Circle { radius: 7.0 };
    config.color_over_lifetime = vec![
        GradientStop { offset: 0.0, color: [1.0, 1.0, 1.0, 0.0], midpoint: 0.5 },
        GradientStop { offset: 0.25, color: [1.0, 1.0, 1.0, 1.0], midpoint: 0.5 },
        GradientStop { offset: 1.0, color: [0.5, 0.6, 1.0, 0.0], midpoint: 0.5 },
    ];
    let mut node = with_id(
        Node::with_components(&format!("Portal {i}"), None, vec![NodeBehavior::ParticleSystem(config), NodeBehavior::Light2D(glow([0.7, 0.35, 1.0], 1.1, 46.0))]),
        &format!("portal{i}"),
    );
    node.unlit = true;
    hidden(node)
}

/// Which game the shared battle scene is built for. Both use the same
/// art, HUD, card tray, effect pools and prefabs; they differ in their
/// script, state machine and screens.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Game {
    /// Dungeon Royale: random dungeons, Royale + Infinite modes, menus.
    Dungeon,
    /// Arena Royale: a lane arena, one match vs a utility-AI opponent.
    Arena,
}

/// Dungeon Royale: Home, Upgrades and Game Over screens plus two battle
/// scenes built on one shared kit — "Dungeon Royale" (random dungeons,
/// Royale + Infinite modes) and "Arena Royale" (a lane match vs a
/// utility-AI opponent, `demo_arena.js`).
pub(crate) fn build_demo_dungeon(document: &mut Document) -> DungeonDemo {
    let mut inverses = Vec::new();
    let kit = load_kit(document, &mut inverses);
    build_battle(document, &mut inverses, Game::Dungeon, &kit);
    build_battle(document, &mut inverses, Game::Arena, &kit);
    let art = MenuArt { tileset: &kit.tileset, castle: kit.castle.image, units: &kit.units, items: &kit.items, cards: &kit.cards, font: kit.font };
    let home = build_menu_scene(document, &mut inverses, "Royale Home", MenuKind::Home, &art);
    build_menu_scene(document, &mut inverses, "Royale Upgrades", MenuKind::Upgrades, &art);
    build_menu_scene(document, &mut inverses, "Royale Game Over", MenuKind::GameOver, &art);
    inverses.reverse();
    document.commit_batch(inverses);
    // The demo opens on its home screen; every screen switches via
    // `ctx.loadScene`.
    DungeonDemo { scene: home, images: kit.images }
}

/// Art, font and prefabs shared by every Dungeon Royale scene (loaded once).
struct Kit {
    font: an1mbyt3_core::FontId,
    tileset: crate::demo_ninja::Tileset,
    units: Vec<ImageId>,
    items: Vec<ImageId>,
    cards: Vec<ImageId>,
    castle: crate::demo_ninja::Tileset,
    elixir: crate::demo_ninja::Tileset,
    images: Vec<ImageId>,
}

fn load_kit(document: &mut Document, inverses: &mut Vec<Command>) -> Kit {
    let mut images = Vec::new();
    // ---- Assets ----
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: PIXEL_FONT_BYTES.to_vec() });
    let tileset = add_png(document, "dungeon_tileset.png", TILESET_BYTES);
    let units = add_grid_frames(document, "unit", UNITS_BYTES, 16, 16, 150);
    let items = add_grid_frames(document, "item", ITEMS_BYTES, 16, 16, 120);
    let cards = add_grid_frames(document, "card", CARDS_BYTES, 36, 46, 1000);
    let castle_art = add_png(document, "keep.png", CASTLE_BYTES);
    let elixir_art = add_png(document, "elixir.png", ELIXIR_BYTES);
    images.push(tileset.image);
    images.push(castle_art.image);
    images.push(elixir_art.image);
    for frames in [&units, &items, &cards] {
        images.extend(frames.iter().copied());
    }
    // items.png rows: coin, torch, side torch, candle, flag, spikes, flask, chest, arrow.
    let item = |row: usize, frame: usize| items[row * 4 + frame];
    // Units: a prefab, spawned into the y-sorted world as troops deploy.
    add_prefab(
        document,
        inverses,
        "Unit",
        vec![
            (None, feet_anchored(image_node("Unit", None, units[0], 16.0, 16.0), Vec2::ZERO, 16.0)),
            (Some(0), hud(shape("HP Back", None, Path::rect(14.0, 4.0), [0.06, 0.03, 0.08, 0.9], Vec2::new(0.0, 13.0)))),
            (Some(0), hud(shape("HP", None, Path::rect(12.0, 2.0), [0.45, 0.95, 0.5, 1.0], Vec2::new(0.0, 13.0)))),
        ],
    );
    // Projectiles: prefabs, spawned per shot (the bolt carries its own light).
    let mut arrow = image_node("Arrow", None, item(8, 0), 12.0, 12.0);
    arrow.unlit = true;
    add_prefab(document, inverses, "Arrow", vec![(None, arrow)]);
    let bolt = with_components(
        hud(shape("Bolt", None, Path::ellipse(5.0, 5.0), [1.0, 0.7, 0.3, 1.0], Vec2::ZERO)),
        vec![NodeBehavior::Light2D(glow([1.0, 0.6, 0.3], 1.1, 34.0))],
    );
    add_prefab(document, inverses, "Bolt", vec![(None, bolt)]);
    // Royale mode's enemy castle: the keep's art with its own hearth light
    // and HP bar, spawned at the far end of the dungeon (the script tints it).
    let enemy_castle = with_components(
        feet_anchored(image_node("Castle", None, castle_art.image, 48.0, 48.0), Vec2::ZERO, 48.0),
        vec![NodeBehavior::Light2D(glow([1.0, 0.45, 0.35], 1.3, 90.0))],
    );
    add_prefab(
        document,
        inverses,
        "Castle",
        vec![
            (None, enemy_castle),
            (Some(0), hud(shape("HP Back", None, Path::rect(40.0, 4.0), [0.08, 0.04, 0.1, 0.9], Vec2::new(0.0, 30.0)))),
            (Some(0), hud(shape("HP", None, Path::rect(38.0, 2.0), [1.0, 0.38, 0.38, 1.0], Vec2::new(0.0, 30.0)))),
        ],
    );
    Kit { font, tileset, units, items, cards, castle: castle_art, elixir: elixir_art, images }
}

/// One battle scene (Dungeon or Arena) on the shared kit; returns its id.
fn build_battle(document: &mut Document, inverses: &mut Vec<Command>, game: Game, kit: &Kit) -> ArtboardId {
    let mut inverses = inverses;
    let font = kit.font;
    let tileset = &kit.tileset;
    let units = &kit.units;
    let cards = &kit.cards;
    let castle_art = &kit.castle;
    let elixir_art = &kit.elixir;
    let item = |row: usize, frame: usize| kit.items[row * 4 + frame];

    // ---- Scene ----
    let scene_name = if game == Game::Arena { "Arena Royale" } else { "Dungeon Royale" };
    let inverse = document.apply(Command::AddArtboard { name: scene_name.into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);
    // Top-down: no gravity for anyone.
    inverses.push(document.apply(Command::SetArtboardGravity { artboard: scene, value: Vec2::ZERO }));

    let camera = with_id(
        Node::with_components(
            "Main Camera",
            None,
            vec![NodeBehavior::Camera(CameraNode {
                zoom: 1.0,
                background_color: [0.05, 0.035, 0.07, 1.0],
                follow_target: None,
                follow_smoothing: 0.0,
                vignette: an1mbyt3_core::CameraVignette { enabled: true, color: [0.03, 0.0, 0.06, 1.0], intensity: 0.45, radius: 1.0, blur: 0.7 },
                pixel_perfect: an1mbyt3_core::PixelPerfect {
                    enabled: true,
                    grid_snapping: an1mbyt3_core::PixelGridSnapping::UpscaleRenderTexture,
                    crop_frame: an1mbyt3_core::PixelCropFrame::Windowbox,
                },
            })],
        ),
        "camera",
    );
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));

    // Dim violet dungeon ambience — torches, the keep and spells do the rest.
    let ambient = Light2DNode { color: [0.62, 0.56, 0.86, 1.0], intensity: 0.78, ..Light2DNode::global() };
    add_node(document, &mut inverses, scene, Node::with_components("Global Light 2D", None, vec![NodeBehavior::Light2D(ambient)]));

    // ---- Level: two empty tilemaps the script fills ----
    let map_center = Vec2::new(0.0, MAP_CENTER_Y);
    for (name, id) in [("Dungeon", "map"), ("Decor", "decor")] {
        let mut tilemap = TilemapNode::empty(tileset.image, tileset.width, tileset.height, 16, 16, MAP_W, MAP_H, TILE);
        if id == "map" {
            // Design-view placeholder: solid floor until Playground generates.
            for t in tilemap.tiles.iter_mut() {
                *t = Some(23);
            }
        }
        let mut node = with_id(Node::new(name, None, NodeKind::Tilemap(tilemap)), id);
        node.transform.translation = map_center;
        add_node(document, &mut inverses, scene, node);
    }
    // While dragging a troop: the no-deploy half, tinted red.
    let zone_h = 11.0 * TILE;
    let zone = with_id(shape("No-Deploy Zone", None, Path::rect(SCREEN_W, zone_h), [0.9, 0.15, 0.2, 0.16], Vec2::new(0.0, SCREEN_H / 2.0 - zone_h / 2.0)), "zone");
    let mut zone = hud(zone);
    zone.visible = false;
    add_node(document, &mut inverses, scene, zone);

    // Floor-level traps (below the y-sorted world).
    for i in 0..SPIKES {
        add_node(document, &mut inverses, scene, hidden(with_id(image_node("Spike Trap", None, item(5, 0), 16.0, 16.0), &format!("spike{i}"))));
    }
    for i in 0..PORTALS {
        add_node(document, &mut inverses, scene, portal(i));
    }

    // ---- The y-sorted world ----
    let world = add_node(
        document,
        &mut inverses,
        scene,
        with_id(Node::new("World", None, NodeKind::Group(GroupNode { y_sort_enabled: true, ..GroupNode::default() })), "world"),
    );
    // The keep, with its own warm hearth light (a component on the sprite).
    let keep = with_components(
        with_id(feet_anchored(image_node("Keep", Some(world), castle_art.image, 48.0, 48.0), Vec2::new(0.0, -100.0), 48.0), "castle"),
        vec![NodeBehavior::Light2D(glow([1.0, 0.8, 0.55], 1.45, 110.0))],
    );
    let keep_id = add_node(document, &mut inverses, scene, keep);
    let bar_back = shape("Keep HP Back", Some(keep_id), Path::rect(40.0, 4.0), [0.08, 0.04, 0.1, 0.9], Vec2::new(0.0, 30.0));
    add_node(document, &mut inverses, scene, hud(with_id(bar_back, "castleHpBack")));
    let bar_fill = shape("Keep HP", Some(keep_id), Path::rect(38.0, 2.0), [0.45, 0.95, 0.5, 1.0], Vec2::new(0.0, 30.0));
    add_node(document, &mut inverses, scene, hud(with_id(bar_fill, "castleHp")));
    let mut banners = Vec::new();
    for (i, x) in [-20.0f32, 20.0].into_iter().enumerate() {
        let mut flag = with_id(image_node("Banner", Some(keep_id), item(4, 0), 12.0, 12.0), &format!("banner{i}"));
        flag.transform.translation = Vec2::new(x, 18.0);
        banners.push(add_node(document, &mut inverses, scene, flag));
    }

    for i in 0..TORCHES {
        let torch = with_components(
            with_id(feet_anchored(image_node("Torch", Some(world), item(1, 0), 16.0, 16.0), Vec2::ZERO, 16.0), &format!("torch{i}")),
            vec![NodeBehavior::Light2D(glow([1.0, 0.66, 0.36], 1.35, 72.0))],
        );
        add_node(document, &mut inverses, scene, hidden(torch));
    }
    for i in 0..BRAZIERS {
        let brazier = with_components(
            with_id(feet_anchored(image_node("Brazier", Some(world), item(3, 0), 16.0, 16.0), Vec2::ZERO, 16.0), &format!("braz{i}")),
            vec![NodeBehavior::Light2D(glow([1.0, 0.55, 0.25], 1.2, 80.0))],
        );
        add_node(document, &mut inverses, scene, hidden(brazier));
    }
    for i in 0..COINS {
        let mut coin = with_id(image_node("Coin", Some(world), item(0, 0), 12.0, 12.0), &format!("coin{i}"));
        coin.unlit = true;
        add_node(document, &mut inverses, scene, hidden(coin));
    }

    // ---- Projectiles & light pools (above the world) ----
    let fireball = with_components(
        hud(with_id(shape("Fireball", None, Path::ellipse(10.0, 10.0), [1.0, 0.62, 0.25, 1.0], Vec2::ZERO), "fireball")),
        vec![NodeBehavior::Light2D(glow([1.0, 0.55, 0.2], 1.4, 60.0))],
    );
    add_node(document, &mut inverses, scene, hidden(fireball));
    for i in 0..UNIT_LIGHTS {
        add_node(document, &mut inverses, scene, hidden(with_id(Node::with_components("Unit Glow", None, vec![NodeBehavior::Light2D(glow([0.5, 0.7, 1.0], 1.0, 40.0))]), &format!("ul{i}"))));
    }
    for i in 0..FLASHES {
        add_node(document, &mut inverses, scene, hidden(with_id(Node::with_components("Flash", None, vec![NodeBehavior::Light2D(glow([1.0, 1.0, 1.0], 0.0, 60.0))]), &format!("fl{i}"))));
    }

    // ---- One-shot effects ----
    for i in 0..4 {
        add_node(document, &mut inverses, scene, burst(&format!("fxHit{i}"), [1.0, 0.92, 0.6, 1.0], (30.0, 80.0), (0.15, 0.35), (1.0, 2.0), 0.0, 3.0, false, 2.0));
    }
    for i in 0..3 {
        add_node(document, &mut inverses, scene, burst(&format!("fxPoof{i}"), [0.62, 0.5, 0.85, 0.9], (8.0, 26.0), (0.4, 0.8), (2.0, 4.0), -0.4, 2.0, true, 4.0));
    }
    for i in 0..2 {
        add_node(document, &mut inverses, scene, burst(&format!("fxDeploy{i}"), [0.55, 0.85, 1.0, 1.0], (12.0, 40.0), (0.35, 0.7), (1.0, 2.0), -1.2, 2.5, false, 6.0));
    }
    add_node(document, &mut inverses, scene, burst("fxBoom", [1.0, 0.62, 0.22, 1.0], (40.0, 120.0), (0.3, 0.75), (2.0, 4.0), -0.3, 3.5, true, 8.0));
    for i in 0..2 {
        add_node(document, &mut inverses, scene, burst(&format!("fxHeal{i}"), [0.5, 1.0, 0.6, 1.0], (6.0, 18.0), (0.5, 0.9), (1.0, 2.0), -1.6, 1.0, false, 6.0));
    }
    add_node(document, &mut inverses, scene, burst("fxTrail", [1.0, 0.7, 0.3, 1.0], (2.0, 10.0), (0.2, 0.45), (1.0, 2.5), -0.5, 1.0, false, 2.0));
    add_node(document, &mut inverses, scene, burst("fxCoin", [1.0, 0.88, 0.35, 1.0], (10.0, 30.0), (0.25, 0.5), (1.0, 1.0), -0.8, 2.0, false, 3.0));

    // ---- Drag preview ----
    let mut ghost = with_id(feet_anchored(image_node("Drag Ghost", None, units[0], 16.0, 16.0), Vec2::ZERO, 16.0), "ghost");
    ghost.unlit = true;
    add_node(document, &mut inverses, scene, hidden(ghost));
    let mut ring = hud(with_id(shape("Drop Ring", None, Path::ellipse(24.0, 24.0), [1.0, 1.0, 1.0, 0.08], Vec2::ZERO), "ghostRing"));
    if let NodeKind::Shape(s) = &mut ring.kind {
        s.stroke = Some(Stroke { width: 1.0, color: [1.0, 1.0, 1.0, 0.8], gradient: None, image: None, cap: Default::default(), join: Default::default() });
    }
    add_node(document, &mut inverses, scene, hidden(ring));

    // ---- HUD (camera children, unlit) ----
    let tray_h = SCREEN_H - MAP_H as f32 * TILE;
    let tray_y = -SCREEN_H / 2.0 + tray_h / 2.0;
    // Stone-floor texture under the tray (the dungeon's own tiles), then a
    // dark gradient wash so the cards read on top of it.
    let mut tray_stone = hud(with_id(Node::new("Tray Stone", Some(camera_id), NodeKind::Tilemap(stone_tiles(&tileset, MAP_W, (tray_h / TILE) as u32))), "trayStone"));
    tray_stone.transform.translation = Vec2::new(0.0, tray_y);
    add_node(document, &mut inverses, scene, tray_stone);
    let mut tray = hud(with_id(shape("Tray", Some(camera_id), Path::rect(SCREEN_W, tray_h), [1.0; 4], Vec2::new(0.0, tray_y)), "tray"));
    if let NodeKind::Shape(s) = &mut tray.kind {
        s.gradient = Some(vertical_gradient([0.16, 0.1, 0.2, 0.72], [0.04, 0.02, 0.06, 0.93], tray_h / 2.0));
    }
    add_node(document, &mut inverses, scene, tray);
    let tray_top = -SCREEN_H / 2.0 + tray_h;
    add_node(document, &mut inverses, scene, hud(shape("Tray Trim", Some(camera_id), Path::rect(SCREEN_W, 2.0), [0.81, 0.66, 0.36, 1.0], Vec2::new(0.0, tray_top - 1.0))));
    add_node(document, &mut inverses, scene, hud(shape("Tray Shade", Some(camera_id), Path::rect(SCREEN_W, 1.0), [0.0, 0.0, 0.0, 0.5], Vec2::new(0.0, tray_top - 2.5))));

    // Elixir bar: back, fill, 9 segment ticks, drop icon, count.
    let bar_y = tray_top - 18.0;
    let bar_w = 200.0;
    let bar_x = 14.0;
    // Candles at the tray's corners (animated by the script).
    for (i, x) in [-132.0f32, 132.0].into_iter().enumerate() {
        let mut candle = hud(with_id(image_node("Tray Candle", Some(camera_id), item(3, 0), 16.0, 16.0), &format!("trayCandle{i}")));
        candle.transform.translation = Vec2::new(x, bar_y + 2.0);
        add_node(document, &mut inverses, scene, candle);
    }
    add_node(document, &mut inverses, scene, hud(with_id(shape("Elixir Back", Some(camera_id), Path::rect(bar_w + 4.0, 12.0), [0.05, 0.02, 0.07, 1.0], Vec2::new(bar_x, bar_y)), "elxBack")));
    let mut fill = hud(with_id(shape("Elixir", Some(camera_id), Path::rect(bar_w, 8.0), [1.0; 4], Vec2::new(bar_x, bar_y)), "elxFill"));
    if let NodeKind::Shape(s) = &mut fill.kind {
        s.gradient = Some(vertical_gradient([0.98, 0.55, 1.0, 1.0], [0.62, 0.16, 0.72, 1.0], 4.0));
    }
    add_node(document, &mut inverses, scene, fill);
    for i in 1..10 {
        let x = bar_x - bar_w / 2.0 + bar_w * i as f32 / 10.0;
        add_node(document, &mut inverses, scene, hud(shape("Elixir Tick", Some(camera_id), Path::rect(1.0, 8.0), [0.05, 0.02, 0.07, 0.55], Vec2::new(x, bar_y))));
    }
    let mut drop = hud(image_node("Elixir Drop", Some(camera_id), elixir_art.image, 13.5, 16.5));
    drop.transform.translation = Vec2::new(bar_x - bar_w / 2.0 - 14.0, bar_y);
    add_node(document, &mut inverses, scene, drop);
    add_node(document, &mut inverses, scene, hud(text_node(Some(camera_id), "elxText", "5", font, 8.0, [1.0, 0.92, 1.0, 1.0], Vec2::new(bar_x - bar_w / 2.0 - 14.0, bar_y - 1.0))));

    // Hand: "next" preview + 4 cards, each with a cost label on its plate.
    let card_y = -SCREEN_H / 2.0 + 52.0;
    // Inset slot plates under every card (and the smaller "next" slot).
    let plate = |name: &str, w: f32, h: f32, at: Vec2| {
        let mut node = hud(shape(name, Some(camera_id), Path::rect(w, h), [0.03, 0.015, 0.05, 0.9], at));
        if let NodeKind::Shape(s) = &mut node.kind {
            s.stroke = Some(Stroke { width: 1.0, color: [0.81, 0.66, 0.36, 0.45], gradient: None, image: None, cap: Default::default(), join: Default::default() });
        }
        node
    };
    add_node(document, &mut inverses, scene, plate("Next Slot", 28.0, 34.0, Vec2::new(-124.0, card_y - 4.0)));
    for i in 0..4 {
        add_node(document, &mut inverses, scene, plate("Card Slot", 42.0, 52.0, Vec2::new(-72.0 + i as f32 * 46.0, card_y)));
    }
    let mut next = hud(with_id(image_node("Next Card", Some(camera_id), cards[0], 22.0, 28.0), "nextCard"));
    next.transform.translation = Vec2::new(-124.0, card_y - 4.0);
    add_node(document, &mut inverses, scene, next);
    add_node(document, &mut inverses, scene, hud(text_node(Some(camera_id), "nextLabel", "NEXT", font, 8.0, [0.75, 0.68, 0.85, 1.0], Vec2::new(-124.0, card_y + 21.0))));
    for i in 0..4 {
        let x = -72.0 + i as f32 * 46.0;
        let mut card = hud(with_id(image_node("Card", Some(camera_id), cards[i], 36.0, 46.0), &format!("card{i}")));
        card.transform.translation = Vec2::new(x, card_y);
        let card_id = add_node(document, &mut inverses, scene, card);
        add_node(document, &mut inverses, scene, hud(text_node(Some(card_id), &format!("cost{i}"), "3", font, 8.0, [1.0, 0.75, 1.0, 1.0], Vec2::new(0.0, -15.0))));
    }

    // Tips line along the tray's bottom edge (rotated by the script).
    add_node(document, &mut inverses, scene, hud(shape("Tray Base Trim", Some(camera_id), Path::rect(SCREEN_W - 24.0, 1.0), [0.81, 0.66, 0.36, 0.35], Vec2::new(0.0, -SCREEN_H / 2.0 + 19.0))));
    add_node(document, &mut inverses, scene, hud(text_node(Some(camera_id), "tipText", "", font, 8.0, [0.7, 0.64, 0.8, 1.0], Vec2::new(0.0, -SCREEN_H / 2.0 + 10.0))));

    // Top bar: a proper 24 px panel — wave, monsters left, gold.
    let bar_h = 24.0;
    let mut top_bar = hud(shape("Top Bar", Some(camera_id), Path::rect(SCREEN_W, bar_h), [1.0; 4], Vec2::new(0.0, SCREEN_H / 2.0 - bar_h / 2.0)));
    if let NodeKind::Shape(s) = &mut top_bar.kind {
        s.gradient = Some(vertical_gradient([0.14, 0.09, 0.18, 0.97], [0.05, 0.03, 0.08, 0.97], bar_h / 2.0));
    }
    add_node(document, &mut inverses, scene, top_bar);
    add_node(document, &mut inverses, scene, hud(shape("Top Bar Trim", Some(camera_id), Path::rect(SCREEN_W, 2.0), [0.81, 0.66, 0.36, 1.0], Vec2::new(0.0, SCREEN_H / 2.0 - bar_h - 1.0))));
    let top_y = SCREEN_H / 2.0 - bar_h / 2.0;
    add_node(document, &mut inverses, scene, hud(text_node(Some(camera_id), "waveText", "WAVE 1", font, 8.0, [1.0, 0.86, 0.5, 1.0], Vec2::new(-96.0, top_y - 1.0))));
    let mut skull = hud(image_node("Foes", Some(camera_id), units[24], 14.0, 14.0));
    skull.transform.translation = Vec2::new(-10.0, top_y);
    add_node(document, &mut inverses, scene, skull);
    add_node(document, &mut inverses, scene, hud(text_node(Some(camera_id), "foesText", "-", font, 8.0, [0.92, 0.82, 0.95, 1.0], Vec2::new(10.0, top_y - 1.0))));
    let mut gold_icon = hud(image_node("Gold", Some(camera_id), item(0, 0), 12.0, 12.0));
    gold_icon.transform.translation = Vec2::new(84.0, top_y);
    add_node(document, &mut inverses, scene, gold_icon);
    add_node(document, &mut inverses, scene, hud(text_node(Some(camera_id), "goldText", "0", font, 8.0, [1.0, 0.9, 0.45, 1.0], Vec2::new(108.0, top_y - 1.0))));
    add_node(document, &mut inverses, scene, hud(text_node(Some(camera_id), "msg", "", font, 16.0, [1.0, 0.9, 0.55, 1.0], Vec2::new(0.0, 90.0))));
    add_node(document, &mut inverses, scene, hud(text_node(Some(camera_id), "subMsg", "", font, 8.0, [0.85, 0.82, 0.95, 1.0], Vec2::new(0.0, 68.0))));

    // ---- The game: a layered state machine ----
    //
    // Base layer "Game Flow":  Title -> Wave Break <-> Battle -> Defeat -> Title
    // Layer "Simulation":      Running <-> Frozen        (Bool `fallen`)
    // Layer "Hand":            Elixir x1 <-> Elixir x2    (Number `wave` > 5.5)
    // Layer "Ambience":        Calm <-> Danger            (Number `keepHp` < 0.35)
    // Layer "Banners":         Sway <-> Alarm             (keyframed animations,
    //                                                      masked to the banners)
    //
    // Every Code state runs its own program each frame (base layer first);
    // the programs share one library and talk back to the machine through
    // `ctx.setInput` / `ctx.fireTrigger`.
    let add_animation = |document: &mut Document, inverses: &mut Vec<Command>, animation: Animation| {
        let inverse = document.apply(Command::AddAnimation { artboard: scene, animation });
        let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
        inverses.push(inverse);
        animation
    };
    let filler = add_animation(document, &mut inverses, Animation::new("Dungeon (code states)"));
    let sway = add_animation(document, &mut inverses, banner_animation("Banners Sway", &banners, 1.6, 0.07, None));
    let alarm = add_animation(document, &mut inverses, banner_animation("Banners Alarm", &banners, 0.36, 0.2, Some([1.0, 0.45, 0.4, 1.0])));

    let code = |name: &str, at: Vec2, transitions: Vec<TransitionDef>| StateDef {
        name: name.into(),
        animation: filler,
        loop_mode: LoopMode::Once,
        transitions,
        position: at,
        blend: None,
        program: Some(ProgramDef::Code(battle_program(game, name))),
    };
    let animated = |name: &str, animation, at: Vec2, transitions: Vec<TransitionDef>| StateDef {
        name: name.into(),
        animation,
        loop_mode: LoopMode::Loop,
        transitions,
        position: at,
        blend: None,
        program: None,
    };
    let go = |to: &str, condition: ConditionDef| TransitionDef { to: to.into(), condition, direction: PlaybackDirection::Forward, duration_seconds: 0.0, interruptible: true };
    let fade = |to: &str, condition: ConditionDef| TransitionDef { to: to.into(), condition, direction: PlaybackDirection::Forward, duration_seconds: 0.25, interruptible: true };
    let trigger = |input: &str| ConditionDef::Trigger { input: input.into() };
    let fallen = |value: bool| ConditionDef::BoolIs { input: "fallen".into(), value };
    let wave_above = |t: f32| ConditionDef::NumberGreaterThan { input: "wave".into(), threshold: t };
    let wave_below = |t: f32| ConditionDef::NumberLessThan { input: "wave".into(), threshold: t };
    let hp_above = |t: f32| ConditionDef::NumberGreaterThan { input: "keepHp".into(), threshold: t };
    let hp_below = |t: f32| ConditionDef::NumberLessThan { input: "keepHp".into(), threshold: t };

    if game == Game::Arena {
    // Arena Royale: Flow (Intro -> Battle -> Finish -> Intro), plus the
    // Simulation / Hand / Ambience layers, as in Dungeon Royale.
    let over = |value: bool| ConditionDef::BoolIs { input: "over".into(), value };
    let flow = vec![
        code("Intro", Vec2::new(0.0, 0.0), vec![go("Battle", trigger("begin"))]),
        code("Battle", Vec2::new(220.0, 0.0), vec![go("Finish", over(true))]),
        code("Finish", Vec2::new(440.0, 0.0), vec![go("Intro", trigger("rematch"))]),
    ];
    let layer = |name: &str, states: Vec<StateDef>, mask: Vec<an1mbyt3_core::NodeId>| StateMachineLayerDef { name: name.into(), initial_state: states[0].name.clone(), states, mask };
    let layers = vec![
        layer(
            "Simulation",
            vec![code("Running", Vec2::new(0.0, 0.0), vec![go("Frozen", over(true))]), code("Frozen", Vec2::new(220.0, 0.0), vec![go("Running", over(false))])],
            Vec::new(),
        ),
        layer("Hand", vec![code("Cards", Vec2::new(0.0, 0.0), Vec::new())], Vec::new()),
        layer(
            "Ambience",
            vec![code("Calm", Vec2::new(0.0, 0.0), vec![go("Danger", hp_below(0.35))]), code("Danger", Vec2::new(220.0, 0.0), vec![go("Calm", hp_above(0.35))])],
            Vec::new(),
        ),
    ];
    let input = |name: &str, kind: InputKind| InputDef { name: name.into(), kind };
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Arena Royale".into(),
            inputs: vec![input("begin", InputKind::Trigger), input("rematch", InputKind::Trigger), input("over", InputKind::Bool), input("keepHp", InputKind::Number)],
            states: flow,
            initial_state: "Intro".into(),
            layers,
            deleted: false,
        },
    }));
    let _ = (&sway, &alarm, wave_above, wave_below, fallen, fade, &animated);
    return scene;
    }

    let flow = vec![
        code("Title", Vec2::new(0.0, 0.0), vec![go("Wave Break", trigger("begin"))]),
        code("Wave Break", Vec2::new(220.0, 0.0), vec![go("Defeat", fallen(true)), go("Battle", trigger("go"))]),
        code("Battle", Vec2::new(440.0, 0.0), vec![go("Defeat", fallen(true)), go("Wave Break", trigger("cleared"))]),
        code("Defeat", Vec2::new(440.0, 160.0), vec![go("Title", trigger("rebuild"))]),
    ];
    let layer = |name: &str, states: Vec<StateDef>, mask: Vec<an1mbyt3_core::NodeId>| StateMachineLayerDef { name: name.into(), initial_state: states[0].name.clone(), states, mask };
    let layers = vec![
        layer(
            "Simulation",
            vec![code("Running", Vec2::new(0.0, 0.0), vec![go("Frozen", fallen(true))]), code("Frozen", Vec2::new(220.0, 0.0), vec![go("Running", fallen(false))])],
            Vec::new(),
        ),
        layer(
            "Hand",
            vec![code("Elixir x1", Vec2::new(0.0, 0.0), vec![go("Elixir x2", wave_above(5.5))]), code("Elixir x2", Vec2::new(220.0, 0.0), vec![go("Elixir x1", wave_below(5.5))])],
            Vec::new(),
        ),
        layer(
            "Ambience",
            vec![code("Calm", Vec2::new(0.0, 0.0), vec![go("Danger", hp_below(0.35))]), code("Danger", Vec2::new(220.0, 0.0), vec![go("Calm", hp_above(0.35))])],
            Vec::new(),
        ),
        layer(
            "Banners",
            vec![
                animated("Sway", sway, Vec2::new(0.0, 0.0), vec![fade("Alarm", hp_below(0.35))]),
                animated("Alarm", alarm, Vec2::new(220.0, 0.0), vec![fade("Sway", hp_above(0.35))]),
            ],
            banners.clone(),
        ),
    ];
    let input = |name: &str, kind: InputKind| InputDef { name: name.into(), kind };
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Dungeon Royale".into(),
            inputs: vec![
                input("begin", InputKind::Trigger),
                input("go", InputKind::Trigger),
                input("cleared", InputKind::Trigger),
                input("rebuild", InputKind::Trigger),
                input("fallen", InputKind::Bool),
                input("wave", InputKind::Number),
                input("keepHp", InputKind::Number),
            ],
            states: flow,
            initial_state: "Title".into(),
            layers,
            deleted: false,
        },
    }));

    scene
}

/// A floor of the dungeon's stone tiles (`cols` x `rows`), for backgrounds.
fn stone_tiles(tileset: &crate::demo_ninja::Tileset, cols: u32, rows: u32) -> TilemapNode {
    let mut map = TilemapNode::empty(tileset.image, tileset.width, tileset.height, 16, 16, cols, rows, TILE);
    for (i, t) in map.tiles.iter_mut().enumerate() {
        // Deterministic sprinkle of the cracked variants.
        let h = (i as u32).wrapping_mul(2654435761) >> 28;
        *t = Some(match h {
            0 => 22,
            1 => 6,
            2 => 16,
            _ => 23,
        });
    }
    map
}

/// A walled room filling a whole `cols` x `rows` screen: brick faces along
/// the top, side walls, bottom cap, stone floor — the menus' backdrop.
fn room_tiles(tileset: &crate::demo_ninja::Tileset, cols: u32, rows: u32) -> TilemapNode {
    let mut map = stone_tiles(tileset, cols, rows);
    for y in 0..rows {
        for x in 0..cols {
            let tile = if y < 2 {
                if y == 1 { if x % 5 == 0 { 3 } else { 2 } } else { 42 }
            } else if y == rows - 1 {
                42
            } else if x == 0 {
                10
            } else if x == cols - 1 {
                15
            } else {
                continue;
            };
            map.tiles[(y * cols + x) as usize] = Some(tile);
        }
    }
    map
}

/// A gold-trimmed plate — the menus' buttons (the script handles hover/taps).
fn plate_button(id: &str, w: f32, h: f32, color: [f32; 4], at: Vec2) -> Node {
    let mut button = hud(with_id(shape("Button", None, Path::rect(w, h), color, at), id));
    if let NodeKind::Shape(s) = &mut button.kind {
        s.stroke = Some(Stroke { width: 1.0, color: [0.81, 0.66, 0.36, 1.0], gradient: None, image: None, cap: Default::default(), join: Default::default() });
    }
    button
}

struct MenuArt<'a> {
    tileset: &'a crate::demo_ninja::Tileset,
    castle: ImageId,
    units: &'a [ImageId],
    items: &'a [ImageId],
    cards: &'a [ImageId],
    font: an1mbyt3_core::FontId,
}

#[derive(Clone, Copy, PartialEq)]
enum MenuKind {
    Home,
    /// The card upgrade shop (its own screen, so Home stays uncluttered).
    Upgrades,
    GameOver,
}

/// The home screen or the game over screen: its own scene, camera, lights,
/// backdrop, and a one-state Code program (`demo_dungeon_menus.js`).
fn build_menu_scene(document: &mut Document, inverses: &mut Vec<Command>, name: &str, kind: MenuKind, art: &MenuArt) -> ArtboardId {
    // `home`: the warm, welcoming look (Home and Upgrades); Game Over is the
    // smoky red ruin.
    let home = kind != MenuKind::GameOver;
    let inverse = document.apply(Command::AddArtboard { name: name.into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);
    let item = |row: usize, frame: usize| art.items[row * 4 + frame];
    let unit = |row: usize| art.units[row * 4];

    let camera = with_id(
        Node::with_components(
            "Main Camera",
            None,
            vec![NodeBehavior::Camera(CameraNode {
                zoom: 1.0,
                background_color: [0.04, 0.03, 0.06, 1.0],
                follow_target: None,
                follow_smoothing: 0.0,
                vignette: an1mbyt3_core::CameraVignette {
                    enabled: true,
                    color: if home { [0.03, 0.0, 0.06, 1.0] } else { [0.12, 0.0, 0.02, 1.0] },
                    intensity: if home { 0.5 } else { 0.7 },
                    radius: 0.95,
                    blur: 0.75,
                },
                pixel_perfect: an1mbyt3_core::PixelPerfect {
                    enabled: true,
                    grid_snapping: an1mbyt3_core::PixelGridSnapping::UpscaleRenderTexture,
                    crop_frame: an1mbyt3_core::PixelCropFrame::Windowbox,
                },
            })],
        ),
        "camera",
    );
    let camera_id = add_node(document, inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));
    let ambient = if home { [0.6, 0.55, 0.85] } else { [0.75, 0.4, 0.45] };
    add_node(document, inverses, scene, Node::with_components("Global Light 2D", None, vec![NodeBehavior::Light2D(Light2DNode { color: [ambient[0], ambient[1], ambient[2], 1.0], intensity: 0.7, ..Light2DNode::global() })]));

    // Backdrop room + torches on the top wall and halfway down the sides.
    let mut room = Node::new("Room", None, NodeKind::Tilemap(room_tiles(art.tileset, MAP_W, (SCREEN_H / TILE) as u32)));
    room.transform.translation = Vec2::ZERO;
    add_node(document, inverses, scene, room);
    for (i, (x, y)) in [(-88.0f32, 228.0f32), (88.0, 228.0), (-128.0, 40.0), (128.0, 40.0)].into_iter().enumerate() {
        let torch = with_components(
            with_id(image_node("Torch", None, item(1, 0), 16.0, 16.0), &format!("torch{i}")),
            vec![NodeBehavior::Light2D(glow([1.0, 0.66, 0.36], 1.3, 80.0))],
        );
        let mut torch = torch;
        torch.transform.translation = Vec2::new(x, y);
        add_node(document, inverses, scene, torch);
    }

    // The keep: proud on the home screen, a dark ruin on game over.
    let mut keep = with_components(
        with_id(image_node("Keep", None, art.castle, 48.0, 48.0), "keep"),
        vec![NodeBehavior::Light2D(glow(if home { [1.0, 0.8, 0.55] } else { [1.0, 0.35, 0.2] }, 1.4, 110.0))],
    );
    keep.transform.translation = Vec2::new(0.0, if home { 40.0 } else { 70.0 });
    if !home {
        keep.transform.rotation = 0.14;
        if let NodeKind::Image(image) = &mut keep.kind {
            image.fill_color = [0.55, 0.45, 0.5, 1.0];
        }
    }
    let keep_id = if kind == MenuKind::Upgrades { None } else { Some(add_node(document, inverses, scene, keep)) };
    if let (MenuKind::Home, Some(keep_id)) = (kind, keep_id) {
        for (i, x) in [-20.0f32, 20.0].into_iter().enumerate() {
            let mut flag = with_id(image_node("Banner", Some(keep_id), item(4, 0), 12.0, 12.0), &format!("banner{i}"));
            flag.transform.translation = Vec2::new(x, 18.0);
            add_node(document, inverses, scene, flag);
        }
    }

    // Particles: rising embers (home) or ruin smoke (game over).
    let mut config = ParticleSystemNode::default();
    config.main.looping = true;
    config.main.duration = 3.0;
    config.main.start_lifetime = ParticleRange { min: 1.5, max: 3.0 };
    config.main.start_speed = ParticleRange { min: 6.0, max: 16.0 };
    config.main.start_size = ParticleRange { min: 1.0, max: if home { 2.0 } else { 5.0 } };
    config.main.start_color = if home { [1.0, 0.7, 0.35, 1.0] } else { [0.45, 0.38, 0.42, 0.7] };
    config.main.gravity_modifier = -0.6;
    config.main.simulation_space = SimulationSpace::World;
    config.main.max_particles = 80;
    config.emission.rate_over_time = if home { 14.0 } else { 18.0 };
    config.emission.bursts.clear();
    config.shape = if home { EmitterShape::Box { width: SCREEN_W - 40.0, height: 20.0 } } else { EmitterShape::Circle { radius: 14.0 } };
    config.color_over_lifetime = vec![
        GradientStop { offset: 0.0, color: [1.0, 1.0, 1.0, 0.0], midpoint: 0.5 },
        GradientStop { offset: 0.2, color: [1.0, 1.0, 1.0, 1.0], midpoint: 0.5 },
        GradientStop { offset: 1.0, color: [1.0, 1.0, 1.0, 0.0], midpoint: 0.5 },
    ];
    if !home {
        config.size_over_lifetime = vec![SizeKeyframe { t: 0.0, size_multiplier: 0.6 }, SizeKeyframe { t: 1.0, size_multiplier: 2.4 }];
    }
    let mut fx = Node::with_components(if home { "Embers" } else { "Smoke" }, None, vec![NodeBehavior::ParticleSystem(config)]);
    fx.transform.translation = if home { Vec2::new(0.0, -SCREEN_H / 2.0 + 30.0) } else { Vec2::new(0.0, 90.0) };
    fx.unlit = home;
    add_node(document, inverses, scene, fx);

    let text = |id: &str, content: &str, size: f32, color: [f32; 4], at: Vec2| hud(text_node(None, id, content, art.font, size, color, at));
    let shadowed = |document: &mut Document, inverses: &mut Vec<Command>, id: &str, content: &str, color: [f32; 4], at: Vec2| {
        add_node(document, inverses, scene, text(&format!("{id}Shadow"), content, 16.0, [0.08, 0.03, 0.1, 1.0], at + Vec2::new(2.0, -2.0)));
        add_node(document, inverses, scene, text(id, content, 16.0, color, at));
    };

    if kind == MenuKind::Home {
        shadowed(document, inverses, "title1", "DUNGEON", [1.0, 0.86, 0.5, 1.0], Vec2::new(0.0, 168.0));
        shadowed(document, inverses, "title2", "ROYALE", [0.95, 0.55, 1.0, 1.0], Vec2::new(0.0, 144.0));
        // Two armies face off in front of the keep.
        for (i, row) in [0usize, 3, 1].into_iter().enumerate() {
            let mut u = with_id(image_node("Ally", None, unit(row), 16.0, 16.0), &format!("ally{i}"));
            u.transform.translation = Vec2::new(-96.0 + i as f32 * 20.0, -6.0 - (i % 2) as f32 * 10.0);
            add_node(document, inverses, scene, u);
        }
        for (i, row) in [6usize, 7, 9].into_iter().enumerate() {
            let mut u = with_id(image_node("Foe", None, unit(row), 16.0, 16.0), &format!("foe{i}"));
            u.transform.translation = Vec2::new(96.0 - i as f32 * 20.0, -6.0 - (i % 2) as f32 * 10.0);
            u.transform.scale = Vec2::new(-1.0, 1.0);
            add_node(document, inverses, scene, u);
        }
        // Menu: two game modes and the upgrade shop (its own screen).
        for (id, label, y, color) in [
            ("royale", "ROYALE", -92.0f32, [0.42, 0.2, 0.22, 1.0]),
            ("arena", "ARENA VS AI", -122.0, [0.22, 0.38, 0.3, 1.0]),
            ("infinite", "INFINITE", -152.0, [0.2, 0.3, 0.45, 1.0]),
            ("upgrades", "UPGRADES", -182.0, [0.36, 0.28, 0.12, 1.0]),
        ] {
            add_node(document, inverses, scene, plate_button(&format!("{id}Btn"), 160.0, 24.0, color, Vec2::new(0.0, y)));
            add_node(document, inverses, scene, text(&format!("{id}Label"), label, 8.0, [1.0, 0.95, 0.85, 1.0], Vec2::new(0.0, y - 1.0)));
        }
        add_node(document, inverses, scene, text("bank", "GOLD 0", 8.0, [1.0, 0.86, 0.4, 1.0], Vec2::new(0.0, -208.0)));
        add_node(document, inverses, scene, text("best", "", 8.0, [0.75, 0.68, 0.85, 1.0], Vec2::new(0.0, -224.0)));
        add_node(document, inverses, scene, text("play", "PICK A MODE", 8.0, [1.0, 0.95, 0.8, 1.0], Vec2::new(0.0, -242.0)));
    } else if kind == MenuKind::Upgrades {
        shadowed(document, inverses, "title1", "UPGRADES", [1.0, 0.86, 0.5, 1.0], Vec2::new(0.0, 206.0));
        add_node(document, inverses, scene, text("bank", "GOLD 0", 8.0, [1.0, 0.86, 0.4, 1.0], Vec2::new(0.0, 182.0)));
        // One panel per card: art, name, level, bonus, and its upgrade button.
        const NAMES: [&str; 8] = ["KNIGHT", "ARCHERS", "CAPTAIN", "FIREBALL", "CLERIC", "SPIKES", "BRAZIER", "ROGUE"];
        for (i, name) in NAMES.into_iter().enumerate() {
            let (x, y) = (if i % 2 == 0 { -70.0f32 } else { 70.0 }, 128.0 - (i / 2) as f32 * 74.0);
            let mut panel = hud(with_id(shape("Card Panel", None, Path::rect(134.0, 66.0), [0.05, 0.02, 0.07, 0.9], Vec2::new(x, y)), &format!("panel{i}")));
            if let NodeKind::Shape(s) = &mut panel.kind {
                s.stroke = Some(Stroke { width: 1.0, color: [0.81, 0.66, 0.36, 0.55], gradient: None, image: None, cap: Default::default(), join: Default::default() });
            }
            add_node(document, inverses, scene, panel);
            let mut card = hud(with_id(image_node("Card", None, art.cards[i], 36.0, 46.0), &format!("card{i}")));
            card.transform.translation = Vec2::new(x - 42.0, y + 2.0);
            add_node(document, inverses, scene, card);
            add_node(document, inverses, scene, text(&format!("name{i}"), name, 8.0, [1.0, 0.95, 0.85, 1.0], Vec2::new(x + 18.0, y + 20.0)));
            add_node(document, inverses, scene, text(&format!("lv{i}"), "LV 1", 8.0, [0.95, 0.75, 1.0, 1.0], Vec2::new(x + 18.0, y + 6.0)));
            add_node(document, inverses, scene, text(&format!("bonus{i}"), "+0%", 8.0, [0.6, 0.9, 0.6, 1.0], Vec2::new(x + 18.0, y - 7.0)));
            add_node(document, inverses, scene, plate_button(&format!("up{i}Btn"), 76.0, 16.0, [0.2, 0.36, 0.2, 1.0], Vec2::new(x + 18.0, y - 22.0)));
            add_node(document, inverses, scene, text(&format!("up{i}Label"), "20G", 8.0, [1.0, 0.95, 0.85, 1.0], Vec2::new(x + 18.0, y - 23.0)));
        }
        add_node(document, inverses, scene, plate_button("backBtn", 150.0, 26.0, [0.3, 0.2, 0.42, 1.0], Vec2::new(0.0, -186.0)));
        add_node(document, inverses, scene, text("backLabel", "BACK", 8.0, [1.0, 0.95, 0.85, 1.0], Vec2::new(0.0, -187.0)));
        add_node(document, inverses, scene, text("play", "", 8.0, [1.0, 0.9, 0.5, 1.0], Vec2::new(0.0, -222.0)));
    } else {
        shadowed(document, inverses, "title1", "THE KEEP", [1.0, 0.5, 0.35, 1.0], Vec2::new(0.0, 172.0));
        shadowed(document, inverses, "title2", "HAS FALLEN", [1.0, 0.5, 0.35, 1.0], Vec2::new(0.0, 148.0));
        // Monsters celebrating around the ruin.
        for (i, (row, x)) in [(6usize, -46.0f32), (8, 0.0), (7, 46.0)].into_iter().enumerate() {
            let mut u = with_id(image_node("Foe", None, unit(row), 16.0, 16.0), &format!("foe{i}"));
            u.transform.translation = Vec2::new(x, 22.0);
            add_node(document, inverses, scene, u);
        }
        // Stats panel.
        let mut panel = hud(shape("Stats Panel", None, Path::rect(176.0, 64.0), [0.05, 0.02, 0.06, 0.88], Vec2::new(0.0, -36.0)));
        if let NodeKind::Shape(s) = &mut panel.kind {
            s.stroke = Some(Stroke { width: 1.0, color: [0.81, 0.66, 0.36, 0.7], gradient: None, image: None, cap: Default::default(), join: Default::default() });
        }
        add_node(document, inverses, scene, panel);
        add_node(document, inverses, scene, text("statWave", "WAVE 1", 8.0, [1.0, 0.86, 0.5, 1.0], Vec2::new(0.0, -16.0)));
        add_node(document, inverses, scene, text("statGold", "GOLD 0", 8.0, [1.0, 0.9, 0.45, 1.0], Vec2::new(0.0, -36.0)));
        add_node(document, inverses, scene, text("statBest", "BEST 0", 8.0, [0.8, 0.75, 0.9, 1.0], Vec2::new(0.0, -56.0)));
        // Buttons: a plate + label each; the script handles hover and taps.
        for (id, label, y, color) in [("again", "PLAY AGAIN", -112.0f32, [0.2, 0.42, 0.22, 1.0]), ("menu", "MENU", -150.0, [0.3, 0.2, 0.42, 1.0])] {
            let mut button = hud(with_id(shape("Button", None, Path::rect(150.0, 28.0), color, Vec2::new(0.0, y)), &format!("{id}Btn")));
            if let NodeKind::Shape(s) = &mut button.kind {
                s.stroke = Some(Stroke { width: 1.0, color: [0.81, 0.66, 0.36, 1.0], gradient: None, image: None, cap: Default::default(), join: Default::default() });
            }
            add_node(document, inverses, scene, button);
            add_node(document, inverses, scene, text(&format!("{id}Label"), label, 8.0, [1.0, 0.95, 0.85, 1.0], Vec2::new(0.0, y - 1.0)));
        }
    }

    // The screen's program: one Code state.
    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Menu (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    let state = match kind {
        MenuKind::Home => "Home",
        MenuKind::Upgrades => "Upgrades",
        MenuKind::GameOver => "GameOver",
    };
    // The library (everything before the first `//@program`) plus this
    // screen's own section only — never the other screens' programs.
    let mut sections = MENUS_SCRIPT.split("//@program ");
    let library = sections.next().unwrap_or_default();
    let body = sections
        .find_map(|section| {
            let (name, body) = section.split_once('\n')?;
            (name.trim() == state).then_some(body)
        })
        .unwrap_or_else(|| panic!("demo_dungeon_menus.js has no program {state:?}"));
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: name.into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: state.into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(format!("{library}\n// ---- {state} ----\n{body}"))),
            }],
            initial_state: state.into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));
    scene
}

const MENUS_SCRIPT: &str = include_str!("demo_dungeon_menus.js");

/// A looping banner sway: rotation left/right over `period` seconds by
/// `amplitude` radians, optionally flashing toward `tint` and back.
fn banner_animation(name: &str, banners: &[an1mbyt3_core::NodeId], period: f32, amplitude: f32, tint: Option<[f32; 4]>) -> Animation {
    let ease = Easing::CubicBezier { x1: 0.42, y1: 0.0, x2: 0.58, y2: 1.0 };
    let mut animation = Animation::new(name);
    animation.duration = period;
    for (i, &banner) in banners.iter().enumerate() {
        // Mirror the two banners so they sway toward each other.
        let a = if i % 2 == 0 { amplitude } else { -amplitude };
        animation.tracks.push((
            banner,
            PropertyTrack::Rotation(Track::new(vec![Keyframe::new(0.0, -a, ease), Keyframe::new(period / 2.0, a, ease), Keyframe::new(period, -a, ease)])),
        ));
        let white = [1.0, 1.0, 1.0, 1.0];
        let mid = tint.unwrap_or(white);
        animation.tracks.push((
            banner,
            PropertyTrack::FillColor(Track::new(vec![
                Keyframe::new(0.0, white, Easing::Linear),
                Keyframe::new(period / 2.0, mid, Easing::Linear),
                Keyframe::new(period, white, Easing::Linear),
            ])),
        ));
    }
    animation
}

/// One state's Code program: layout constants + the shared library (the
/// script up to its first `//@program`) + that state's own section.
fn battle_program(game: Game, state: &str) -> String {
    let script = if game == Game::Arena { ARENA_SCRIPT } else { DUNGEON_SCRIPT };
    let mut parts = script.split("//@program ");
    let library = parts.next().unwrap_or_default();
    let body = parts
        .find_map(|section| {
            let (name, rest) = section.split_once('\n')?;
            (name.trim() == state).then_some(rest)
        })
        .unwrap_or_else(|| panic!("battle script has no program for state {state:?}"));
    format!(
        "const MAP_W = {MAP_W}, MAP_H = {MAP_H}, TILE = {TILE}, MAP_TOP = {top}, MAP_LEFT = {left}\n\
         const POOL = {{ units: {UNITS}, arrows: {ARROWS}, bolts: {BOLTS}, torches: {TORCHES}, braziers: {BRAZIERS}, spikes: {SPIKES}, lights: {UNIT_LIGHTS}, flashes: {FLASHES}, coins: {COINS}, portals: {PORTALS} }}\n\
         {library}\n// ---- state: {state} ----\n{body}",
        top = SCREEN_H / 2.0,
        left = -SCREEN_W / 2.0,
    )
}

const DUNGEON_SCRIPT: &str = include_str!("demo_dungeon.js");
const ARENA_SCRIPT: &str = include_str!("demo_arena.js");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_arena_with_its_own_machine() {
        let mut document = Document::new();
        build_demo_dungeon(&mut document);
        let (_, artboard) = document.artboards().find(|(_, a)| a.name == "Arena Royale").expect("the arena is one of Dungeon Royale's scenes");
        assert_eq!(document.artboards().count(), 5, "home, upgrades, dungeon, arena, game over");
        assert_eq!(document.prefabs().count(), 4, "one shared set of prefabs");
        let (_, machine) = artboard.state_machines().next().unwrap();
        assert_eq!(machine.states.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Intro", "Battle", "Finish"]);
        let Some(ProgramDef::Code(source)) = &machine.states[1].program else { panic!("expected Code") };
        assert!(source.contains("function aiThink("));
        assert!(!source.contains("function generateDungeon("), "arena programs come from the arena script");
        assert!(document.prefabs().any(|(_, p)| p.name == "Castle"));
    }

    #[test]
    fn builds_scene_with_pools_and_script() {
        let mut document = Document::new();
        let demo = build_demo_dungeon(&mut document);
        // The demo opens on its home screen; the game is its own scene.
        assert_eq!(document.artboard(demo.scene).unwrap().name, "Royale Home");
        assert!(document.artboards().any(|(_, a)| a.name == "Royale Game Over"));
        let (_, artboard) = document.artboards().find(|(_, a)| a.name == "Dungeon Royale").unwrap();
        let count = |prefix: &str| artboard.node_ids().filter_map(|id| artboard.node(id)).filter(|n| n.id.starts_with(prefix) && n.id[prefix.len()..].parse::<u32>().is_ok()).count();
        // Units and projectiles are prefabs, not pre-built pools.
        assert_eq!(count("u"), 0);
        let prefab_names: Vec<&str> = document.prefabs().map(|(_, p)| p.name.as_str()).collect();
        assert_eq!(prefab_names, ["Unit", "Arrow", "Bolt", "Castle"]);
        let unit = document.prefabs().find(|(_, p)| p.name == "Unit").unwrap().1;
        assert_eq!(unit.nodes.iter().map(|n| n.node.name.as_str()).collect::<Vec<_>>(), ["Unit", "HP Back", "HP"]);
        assert_eq!(unit.nodes[1].parent, Some(0));
        // Each menu screen runs only its own program.
        for (scene, own, other) in [("Royale Home", "---- Home ----", "againBtn"), ("Royale Upgrades", "---- Upgrades ----", "royaleBtn"), ("Royale Game Over", "---- GameOver ----", "royaleBtn")] {
            let (_, a) = document.artboards().find(|(_, a)| a.name == scene).unwrap();
            let (_, m) = a.state_machines().next().unwrap();
            let Some(ProgramDef::Code(src)) = &m.states[0].program else { panic!("expected Code") };
            assert!(src.contains(own) && !src.contains(other), "{scene} program has another screen's code");
        }
        assert_eq!(count("torch"), TORCHES);
        assert_eq!(count("card"), 4);
        assert!(artboard.active_camera().is_some());
        let (_, machine) = artboard.state_machines().next().unwrap();
        assert_eq!(machine.states.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Title", "Wave Break", "Battle", "Defeat"]);
        assert_eq!(machine.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["Simulation", "Hand", "Ambience", "Banners"]);
        let Some(ProgramDef::Code(source)) = &machine.states[2].program else { panic!("expected Code") };
        assert!(source.starts_with("const MAP_W = 18"));
        assert!(source.contains("function aStar("));
        assert!(source.contains("// ---- state: Battle ----"));
        assert!(!source.contains("//@program"), "only the state's own section is included");
        // The banner layer is pure keyframed animation, masked to the banners.
        let banners = &machine.layers[3];
        assert_eq!(banners.mask.len(), 2);
        assert!(banners.states.iter().all(|s| s.program.is_none()));
    }
}
