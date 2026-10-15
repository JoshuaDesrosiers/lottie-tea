//! "Amyruth's Birthday" — a little town-wide treasure hunt built from the
//! Ninja Adventure pieces (`demo_ninja`): walk the ninja around a sunny
//! pixel-art town full of wandering townsfolk, chat with them, follow
//! their hints, and find the one person carrying a secret birthday letter.
//! Finding them opens the letter on a parchment card with confetti,
//! fireworks and a chiptune "Happy Birthday".
//!
//! Same building blocks as the Ninja demo: a tilemap ground autotiled from
//! a dirt mask, standing objects (houses, trees, flowers) as tiny tilemaps
//! cut from the pack's tilesets under one Y-sorted group, a camera with the
//! HUD parented under it, and the whole game as one Code program whose
//! collision / townsfolk tables are generated here in Rust.

use std::fmt::Write as _;

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Command, Document, EmitterShape, FontAsset, FontId, GradientStop, GroupNode, ImageId, LoopMode, Node, NodeId, NodeKind,
    ParticleRange, ParticleSystemNode, Path, ProgramDef, ShapeNode, SimulationSpace, SizeKeyframe, StateDef, StateMachineDef,
};
use glam::Vec2;

use crate::demo_ninja::{add_grid_frames, add_node, add_png, cell_hash, feet_anchored, image_node, set_tint, text_node, with_id, Tileset};

const WALK_BYTES: &[u8] = include_bytes!("../assets/ninja/ninja_walk.png");
const IDLE_BYTES: &[u8] = include_bytes!("../assets/ninja/ninja_idle.png");
const FLOOR_BYTES: &[u8] = include_bytes!("../assets/ninja/tileset_floor.png");
const NATURE_BYTES: &[u8] = include_bytes!("../assets/ninja/tileset_nature.png");
const HOUSE_BYTES: &[u8] = include_bytes!("../assets/ninja/tileset_house.png");
const PIXEL_FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");
const OLDMAN_BYTES: &[u8] = include_bytes!("../assets/ninja/oldman.png");
const OLDMAN_FACE_BYTES: &[u8] = include_bytes!("../assets/ninja/oldman_face.png");
const KID_BYTES: &[u8] = include_bytes!("../assets/ninja/kid.png");
const KID_FACE_BYTES: &[u8] = include_bytes!("../assets/ninja/kid_face.png");
const DIALOG_BOX_BYTES: &[u8] = include_bytes!("../assets/ninja/dialog_box.png");
const FACESET_BOX_BYTES: &[u8] = include_bytes!("../assets/ninja/faceset_box.png");
const CHEST_BYTES: &[u8] = include_bytes!("../assets/ninja/chest.png");
const HEART_BYTES: &[u8] = include_bytes!("../assets/ninja/heart.png");

/// The letter, exactly as Joshua wrote it (typos tidied).
pub(crate) const LETTER: &str = "Hey Amy, happy birthday! I appreciate you so much for being at Marcy and being an amazing person. I look up to you in so many ways! You are so strong in your faith, and so responsible! I pray you have many more years and that God will continue to bless you and guide your steps as we venture off into adulthood and job hunting. Forever grateful for you!";
pub(crate) const SIGNATURE: &str = "With love, Joshua";

const TILE: f32 = 16.0;
const PLAYER_SIZE: f32 = 32.0;
const PLAYER_FEET_SPAN: f32 = 16.0;
const VIEW_WIDTH: f32 = 320.0;
const VIEW_HEIGHT: f32 = 180.0;
/// Town size in tiles (centered on the origin): ±384 x ±272 world units.
const MAP_W: u32 = 48;
const MAP_H: u32 = 34;
/// Lines on the letter card (text nodes `card0..`).
const CARD_LINES: usize = 14;

pub(crate) struct BirthdayDemo {
    pub scene: ArtboardId,
    pub images: Vec<ImageId>,
}

/// A standing object cut out of a tileset, with a collision footprint.
#[derive(Clone, Copy)]
struct Prop {
    name: &'static str,
    col: u32,
    row: u32,
    w: u32,
    h: u32,
    solid_half_width: f32,
    solid_height: f32,
}

const PINK_TREE_BIG: Prop = Prop { name: "Cherry Blossom", col: 0, row: 18, w: 3, h: 3, solid_half_width: 9.0, solid_height: 10.0 };
const PEACH_TREE_BIG: Prop = Prop { name: "Peach Blossom", col: 9, row: 18, w: 3, h: 3, solid_half_width: 9.0, solid_height: 10.0 };
const GREEN_TREE_BIG: Prop = Prop { name: "Big Tree", col: 3, row: 18, w: 3, h: 3, solid_half_width: 9.0, solid_height: 10.0 };
const PINK_TREE: Prop = Prop { name: "Pink Tree", col: 14, row: 0, w: 2, h: 2, solid_half_width: 6.0, solid_height: 8.0 };
const ROUND_TREE: Prop = Prop { name: "Round Tree", col: 16, row: 0, w: 2, h: 2, solid_half_width: 6.0, solid_height: 8.0 };
const PINE: Prop = Prop { name: "Pine", col: 2, row: 0, w: 2, h: 2, solid_half_width: 6.0, solid_height: 8.0 };
const BUSH: Prop = Prop { name: "Bush", col: 1, row: 10, w: 1, h: 1, solid_half_width: 7.0, solid_height: 9.0 };

/// One townsperson: element id, display name, which sprite sheet, an
/// outfit tint, where they live, how far they wander, and what they say.
struct Townsperson {
    id: &'static str,
    name: &'static str,
    /// "oldman" | "kid" | "ninja".
    sheet: &'static str,
    tint: [f32; 4],
    home: (f32, f32),
    wander: f32,
    lines: &'static [&'static str],
}

/// The town. Lines are short (the dialog box fits two rows of ~24
/// characters; the script wraps them). The secret keeper is `npcSecret`.
const TOWNSFOLK: &[Townsperson] = &[
    Townsperson { id: "npc0", name: "BAKER", sheet: "oldman", tint: [1.0, 0.85, 0.75, 1.0], home: (-40.0, -24.0), wander: 30.0, lines: &["Happy birthday, Amyruth! I wanted to bake a cake but I ate it.", "Someone in town has a secret letter for you!"] },
    Townsperson { id: "npc1", name: "MAYA", sheet: "kid", tint: [1.0, 0.7, 0.8, 1.0], home: (60.0, 30.0), wander: 60.0, lines: &["Happy birthday!! Are you looking for the letter?", "I'm not telling who has it. Okay fine, not me."] },
    Townsperson { id: "npc2", name: "OLD TOM", sheet: "oldman", tint: [0.8, 0.85, 1.0, 1.0], home: (-180.0, 10.0), wander: 40.0, lines: &["In my day, letters came by pigeon.", "I saw someone sneaking off with an envelope... headed north, I think."] },
    Townsperson { id: "npc3", name: "LEO", sheet: "kid", tint: [0.7, 0.9, 1.0, 1.0], home: (180.0, -10.0), wander: 60.0, lines: &["Marcy wouldn't be the same without you!", "Have you tried the east side of town?"] },
    Townsperson { id: "npc4", name: "RIN", sheet: "ninja", tint: [1.0, 0.75, 0.55, 1.0], home: (-300.0, -40.0), wander: 50.0, lines: &["A ninja never reveals secrets.", "...but the letter-holder likes flowers. Pink ones."] },
    Townsperson { id: "npc5", name: "GRANNY SUE", sheet: "oldman", tint: [1.0, 0.8, 1.0, 1.0], home: (-110.0, -150.0), wander: 30.0, lines: &["Bless you on your birthday, dear!", "The cherry blossoms are lovely this time of year. Way up in the north-east corner."] },
    Townsperson { id: "npc6", name: "BEN", sheet: "kid", tint: [0.85, 1.0, 0.7, 1.0], home: (240.0, -150.0), wander: 50.0, lines: &["I'm just here for the party.", "Is there cake? Nobody will tell me if there's cake."] },
    Townsperson { id: "npc7", name: "KAI", sheet: "ninja", tint: [0.7, 0.75, 1.0, 1.0], home: (300.0, 30.0), wander: 40.0, lines: &["You're getting warmer...", "Try going north from here, then follow the pink trees."] },
    Townsperson { id: "npc8", name: "MR. LEE", sheet: "oldman", tint: [0.9, 1.0, 0.85, 1.0], home: (-250.0, 150.0), wander: 30.0, lines: &["Happy birthday! Wishing you a great year ahead.", "The letter? Not with me. Wrong side of town!"] },
    Townsperson { id: "npc9", name: "ZOE", sheet: "kid", tint: [1.0, 0.95, 0.6, 1.0], home: (0.0, 150.0), wander: 70.0, lines: &["Happy birthday Amyruth!! You're the best!", "I heard the letter is from someone who looks up to you."] },
    Townsperson { id: "npc10", name: "SAM", sheet: "ninja", tint: [0.8, 0.8, 0.8, 1.0], home: (110.0, -200.0), wander: 50.0, lines: &["Job hunting is tough. You'll do great though!", "Nope, no letter here. Keep looking!"] },
    Townsperson { id: "npc11", name: "NANA", sheet: "oldman", tint: [0.95, 0.9, 0.75, 1.0], home: (-330.0, -210.0), wander: 30.0, lines: &["Twenty-something already? Time flies!", "You're cold, sweetie. Very cold. Try the opposite corner!"] },
    Townsperson { id: "npc12", name: "JAY", sheet: "kid", tint: [0.8, 0.7, 1.0, 1.0], home: (-60.0, 90.0), wander: 60.0, lines: &["Did you talk to everyone yet?", "Someone said the letter-holder is hiding where it's pinkest."] },
    Townsperson { id: "npc13", name: "AVA", sheet: "ninja", tint: [1.0, 0.6, 0.7, 1.0], home: (150.0, 150.0), wander: 40.0, lines: &["Happy birthday! Your faith is such an inspiration.", "Psst. Further north-east. Past the houses."] },
    Townsperson { id: "npc14", name: "GUS", sheet: "oldman", tint: [0.75, 0.9, 1.0, 1.0], home: (330.0, -120.0), wander: 30.0, lines: &["I'm the town's official birthday greeter. Happy birthday!", "No secrets here, just good vibes."] },
    Townsperson { id: "npc15", name: "ELLA", sheet: "kid", tint: [0.7, 1.0, 0.9, 1.0], home: (-200.0, -230.0), wander: 50.0, lines: &["You're so responsible! My mom says I should be more like you.", "The letter is NOT down here. Up, up, up!"] },
    Townsperson { id: "npc16", name: "MILO", sheet: "ninja", tint: [0.9, 1.0, 0.6, 1.0], home: (-20.0, -230.0), wander: 60.0, lines: &["Happy birthday! Here's a high five. *slap*", "Rumor has it there's a letter by the blossoms in the north-east."] },
    Townsperson { id: "npcSecret", name: "???", sheet: "kid", tint: [1.0, 0.8, 0.92, 1.0], home: (300.0, 200.0), wander: 8.0, lines: &[] },
];

fn shape(name: &str, parent: Option<NodeId>, path: Path, color: [f32; 4], at: Vec2) -> Node {
    let mut node = Node::new(
        name,
        parent,
        NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None }),
    );
    node.transform.translation = at;
    node
}

/// A burst emitter the script fires with `ref.emit(n)`: world-space,
/// unlit, colored, falling (confetti) or exploding (fireworks).
fn burst_emitter(id: &str, color: [f32; 4], speed: (f32, f32), lifetime: (f32, f32), size: (f32, f32), gravity: f32, shape: EmitterShape, parent: Option<NodeId>) -> Node {
    let mut config = ParticleSystemNode::default();
    config.main.looping = false;
    config.main.duration = 0.05;
    config.main.start_speed = ParticleRange { min: speed.0, max: speed.1 };
    config.main.start_lifetime = ParticleRange { min: lifetime.0, max: lifetime.1 };
    config.main.start_size = ParticleRange { min: size.0, max: size.1 };
    config.main.start_color = color;
    config.main.gravity_modifier = gravity;
    config.main.drag = 1.2;
    config.main.simulation_space = SimulationSpace::World;
    config.main.max_particles = 300;
    config.emission.rate_over_time = 0.0;
    config.emission.bursts.clear();
    config.shape = shape;
    config.color_over_lifetime = vec![
        GradientStop { offset: 0.0, color: [1.0, 1.0, 1.0, 1.0], midpoint: 0.5 },
        GradientStop { offset: 0.75, color: [1.0, 1.0, 1.0, 1.0], midpoint: 0.5 },
        GradientStop { offset: 1.0, color: [1.0, 1.0, 1.0, 0.0], midpoint: 0.5 },
    ];
    config.size_over_lifetime = vec![SizeKeyframe { t: 0.0, size_multiplier: 1.0 }, SizeKeyframe { t: 1.0, size_multiplier: 0.6 }];
    let mut node = with_id(Node::new(id, parent, NodeKind::ParticleSystem(config)), id);
    node.unlit = true;
    node
}

/// Ground: grass with tufts, and dirt streets autotiled from `dirt`.
fn town_ground(floor: Tileset, dirt: &dyn Fn(i32, i32) -> bool) -> an1mbyt3_core::TilemapNode {
    let mut map = floor.empty_map(MAP_W, MAP_H);
    let grass = floor.index(0, 12);
    let tufts = [floor.index(2, 11), floor.index(3, 11), floor.index(1, 12), floor.index(2, 12), floor.index(3, 12), floor.index(4, 12)];
    for y in 0..MAP_H {
        for x in 0..MAP_W {
            let (xi, yi) = (x as i32, y as i32);
            let tile = if dirt(xi, yi) {
                let (n, s, w, e) = (dirt(xi, yi - 1), dirt(xi, yi + 1), dirt(xi - 1, yi), dirt(xi + 1, yi));
                let col = if !w { 0 } else if !e { 2 } else { 1 };
                let row = if !n { 7 } else if !s { 9 } else { 8 };
                floor.index(col, row)
            } else {
                let h = cell_hash(x, y);
                if h < 14 { tufts[(h as usize) % tufts.len()] } else { grass }
            };
            map.tiles[(y * MAP_W + x) as usize] = Some(tile);
        }
    }
    map
}

pub(crate) fn build_demo_birthday(document: &mut Document) -> BirthdayDemo {
    let mut inverses = Vec::new();
    let mut images = Vec::new();

    // ---- Assets ----
    let font: FontId = document.add_font(FontAsset { name: "Press Start 2P".into(), data: PIXEL_FONT_BYTES.to_vec() });
    let walk = add_grid_frames(document, "ninja_walk", WALK_BYTES, 32, 32, 120);
    let idle = add_grid_frames(document, "ninja_idle", IDLE_BYTES, 32, 32, 250);
    let oldman = add_grid_frames(document, "oldman", OLDMAN_BYTES, 16, 16, 150);
    let kid = add_grid_frames(document, "kid", KID_BYTES, 16, 16, 150);
    let chest = add_grid_frames(document, "chest", CHEST_BYTES, 16, 16, 100);
    let heart = add_grid_frames(document, "heart", HEART_BYTES, 16, 16, 100);
    let floor = add_png(document, "tileset_floor.png", FLOOR_BYTES);
    let nature = add_png(document, "tileset_nature.png", NATURE_BYTES);
    let houses = add_png(document, "tileset_house.png", HOUSE_BYTES);
    let oldman_face = add_png(document, "oldman_face.png", OLDMAN_FACE_BYTES);
    let kid_face = add_png(document, "kid_face.png", KID_FACE_BYTES);
    let dialog_box = add_png(document, "dialog_box.png", DIALOG_BOX_BYTES);
    let faceset_box = add_png(document, "faceset_box.png", FACESET_BOX_BYTES);
    for frames in [&walk, &idle, &oldman, &kid, &chest, &heart] {
        images.extend(frames.iter().copied());
    }
    images.extend([floor.image, nature.image, houses.image, oldman_face.image, kid_face.image, dialog_box.image, faceset_box.image]);

    // ---- Scene ----
    let inverse = document.apply(Command::AddArtboard { name: "Amyruth's Birthday".into(), width: VIEW_WIDTH, height: VIEW_HEIGHT });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);
    let mut solids: Vec<[f32; 4]> = Vec::new();

    // Tile coords: col = (x + 384) / 16, row = (272 - y) / 16.
    let dirt = |x: i32, y: i32| -> bool {
        let plaza = (18..=29).contains(&x) && (12..=21).contains(&y);
        let main_street = (2..=45).contains(&x) && (15..=18).contains(&y);
        let north_road = (22..=25).contains(&x) && (2..=12).contains(&y);
        let south_road = (22..=25).contains(&x) && (21..=31).contains(&y);
        // Little paths from each south house's door down to the lane.
        let south_lane = (2..=45).contains(&x) && (26..=27).contains(&y);
        // Each north house's door path down to the main street.
        let north_paths = (12..=14).contains(&y) && [3, 9, 14, 33, 38, 44].iter().any(|c| (*c..=*c + 1).contains(&x));
        let south_paths = (19..=25).contains(&y) && [3, 9, 14, 33, 38, 44].iter().any(|c| (*c..=*c + 1).contains(&x));
        plaza || main_street || north_road || south_road || south_lane || north_paths || south_paths
    };
    add_node(document, &mut inverses, scene, Node::new("Ground", None, NodeKind::Tilemap(town_ground(floor, &dirt))));

    let world = add_node(document, &mut inverses, scene, Node::new("Town (Y-Sort)", None, NodeKind::Group(GroupNode { y_sort_enabled: true, ..GroupNode::default() })));

    // Houses: 8 different 4x3 buildings along the main street, doors down.
    let house_cols = [0u32, 4, 8, 12, 16, 20, 24, 28];
    let north_x = [-320.0f32, -224.0, -144.0, 160.0, 240.0, 336.0];
    let south_x = [-320.0f32, -224.0, -144.0, 160.0, 240.0, 336.0];
    let mut house_i = 0;
    for (row_y, xs) in [(72.0f32, north_x), (-136.0f32, south_x)] {
        for x in xs {
            let col0 = house_cols[house_i % house_cols.len()];
            house_i += 1;
            let base = Vec2::new(x + 8.0, row_y);
            let node = feet_anchored(Node::new("House", Some(world), NodeKind::Tilemap(houses.block(col0, 0, 4, 3))), base, 3.0 * TILE);
            add_node(document, &mut inverses, scene, node);
            solids.push([base.x - 32.0, base.y, base.x + 32.0, base.y + 44.0]);
        }
    }

    let mut place = |document: &mut Document, inverses: &mut Vec<Command>, prop: Prop, feet: (f32, f32)| {
        let feet = Vec2::from(feet);
        let height = prop.h as f32 * TILE;
        let node = feet_anchored(Node::new(prop.name, Some(world), NodeKind::Tilemap(nature.block(prop.col, prop.row, prop.w, prop.h))), feet, height);
        add_node(document, inverses, scene, node);
        solids.push([feet.x - prop.solid_half_width, feet.y, feet.x + prop.solid_half_width, feet.y + prop.solid_height]);
    };
    // The cherry grove in the north-east corner (where the letter is).
    for feet in [(250.0, 170.0), (340.0, 175.0), (262.0, 236.0), (352.0, 232.0), (210.0, 215.0)] {
        place(document, &mut inverses, PINK_TREE_BIG, feet);
    }
    for feet in [(300.0, 150.0), (225.0, 248.0), (320.0, 255.0)] {
        place(document, &mut inverses, PINK_TREE, feet);
    }
    // Trees ringing the rest of town.
    for feet in [(-350.0, 230.0), (-260.0, 240.0), (-170.0, 228.0), (-90.0, 240.0), (90.0, 236.0), (150.0, 226.0)] {
        place(document, &mut inverses, GREEN_TREE_BIG, feet);
    }
    for feet in [(-360.0, -250.0), (-270.0, -258.0), (-120.0, -255.0), (60.0, -258.0), (200.0, -255.0), (350.0, -250.0)] {
        place(document, &mut inverses, PEACH_TREE_BIG, feet);
    }
    for feet in [(-365.0, 130.0), (-365.0, 40.0), (-365.0, -100.0), (365.0, 100.0), (365.0, -40.0), (365.0, -190.0)] {
        place(document, &mut inverses, PINE, feet);
    }
    for feet in [(-60.0, 200.0), (40.0, 190.0), (-300.0, 110.0), (290.0, -80.0), (-60.0, -180.0), (100.0, -110.0)] {
        place(document, &mut inverses, ROUND_TREE, feet);
    }
    for feet in [(-104.0, 70.0), (104.0, 70.0), (-104.0, -70.0), (104.0, -70.0), (-250.0, 60.0), (90.0, 120.0), (-150.0, 120.0), (280.0, 60.0)] {
        place(document, &mut inverses, BUSH, feet);
    }

    // Flowers everywhere (walk-through, no collision).
    let flower_kinds = [nature.block(0, 11, 1, 1), nature.block(1, 11, 1, 1), nature.block(3, 11, 1, 1), nature.block(6, 11, 1, 1)];
    let mut fi = 0u32;
    for gy in (-240..=240).step_by(32) {
        for gx in (-368..=368).step_by(32) {
            let h = cell_hash((gx + 400) as u32, (gy + 300) as u32);
            if h > 28 {
                continue;
            }
            let (x, y) = (gx as f32 + (h % 9) as f32, gy as f32 + (h % 7) as f32);
            let (col, row) = (((x + 384.0) / TILE) as i32, ((272.0 - y) / TILE) as i32);
            if dirt(col, row) || solids.iter().any(|s| x > s[0] - 10.0 && x < s[2] + 10.0 && y > s[1] - 10.0 && y < s[3] + 10.0) {
                continue;
            }
            let map = flower_kinds[(fi % 4) as usize].clone();
            fi += 1;
            add_node(document, &mut inverses, scene, feet_anchored(Node::new("Flower", Some(world), NodeKind::Tilemap(map)), Vec2::new(x, y), TILE));
        }
    }
    // Extra flowers in the cherry grove.
    for (i, feet) in [(280.0, 190.0), (296.0, 214.0), (318.0, 186.0), (236.0, 196.0), (330.0, 210.0), (270.0, 214.0)].iter().enumerate() {
        let map = flower_kinds[if i % 2 == 0 { 2 } else { 3 }].clone();
        add_node(document, &mut inverses, scene, feet_anchored(Node::new("Flower", Some(world), NodeKind::Tilemap(map)), Vec2::from(*feet), TILE));
    }

    // The party plaza: balloon bunches and gift boxes.
    let balloon_colors = [[1.0, 0.45, 0.6, 1.0], [0.45, 0.75, 1.0, 1.0], [1.0, 0.85, 0.35, 1.0], [0.6, 0.95, 0.55, 1.0], [0.8, 0.6, 1.0, 1.0]];
    for (bi, base) in [(-76.0f32, 44.0f32), (76.0, 44.0), (-76.0, -44.0), (76.0, -44.0)].iter().enumerate() {
        let feet = Vec2::new(base.0, base.1);
        let bunch = add_node(document, &mut inverses, scene, feet_anchored(Node::new("Balloons", Some(world), NodeKind::Group(GroupNode::default())), feet, 2.0));
        for k in 0..3 {
            let dx = (k as f32 - 1.0) * 6.0;
            let top = Vec2::new(dx, 26.0 + (k % 2) as f32 * 5.0);
            add_node(document, &mut inverses, scene, shape("String", Some(bunch), Path::rect(1.0, top.y), [0.3, 0.25, 0.25, 1.0], Vec2::new(dx * 0.5, top.y / 2.0)));
            let color = balloon_colors[(bi + k) % balloon_colors.len()];
            let mut b = shape("Balloon", Some(bunch), Path::ellipse(9.0, 11.0), color, top + Vec2::new(0.0, 5.0));
            b.classes = vec!["balloon".into()];
            add_node(document, &mut inverses, scene, b);
            add_node(document, &mut inverses, scene, shape("Shine", Some(bunch), Path::ellipse(2.0, 3.0), [1.0, 1.0, 1.0, 0.8], top + Vec2::new(-2.0, 7.0)));
        }
    }
    // (Beside the plaza's north exit — never in the road.)
    for feet in [(-62.0f32, 50.0f32), (62.0, 50.0), (-50.0, 30.0)] {
        let node = feet_anchored(image_node("Gift", Some(world), chest[0], 16.0, 16.0), Vec2::from(feet), 16.0);
        add_node(document, &mut inverses, scene, node);
        solids.push([feet.0 - 7.0, feet.1, feet.0 + 7.0, feet.1 + 8.0]);
    }

    // ---- Townsfolk ----
    for person in TOWNSFOLK {
        let first = match person.sheet {
            "oldman" => oldman[0],
            "kid" => kid[0],
            _ => walk[0],
        };
        let size = if person.sheet == "ninja" { PLAYER_SIZE } else { 16.0 };
        let span = if person.sheet == "ninja" { PLAYER_FEET_SPAN } else { 16.0 };
        let mut node = with_id(feet_anchored(image_node(person.name, Some(world), first, size, size), Vec2::from(person.home), span), person.id);
        set_tint(&mut node, person.tint);
        node.classes = vec!["npc".into()];
        add_node(document, &mut inverses, scene, node);
    }
    // A little sparkle over the secret keeper (the script moves it with them).
    add_node(
        document,
        &mut inverses,
        scene,
        burst_emitter("fxSparkle", [1.0, 0.85, 0.95, 1.0], (6.0, 16.0), (0.6, 1.1), (1.0, 2.0), -8.0, EmitterShape::Circle { radius: 6.0 }, None),
    );

    // ---- Player ----
    let start = Vec2::new(0.0, -8.0);
    add_node(document, &mut inverses, scene, with_id(feet_anchored(image_node("Amyruth", Some(world), idle[0], PLAYER_SIZE, PLAYER_SIZE), start, PLAYER_FEET_SPAN), "player"));

    // Fireworks (world space; the script moves them above the camera).
    let firework_colors = [[1.0, 0.45, 0.65, 1.0], [0.5, 0.8, 1.0, 1.0], [1.0, 0.9, 0.4, 1.0], [0.75, 0.6, 1.0, 1.0], [0.55, 1.0, 0.6, 1.0]];
    for (i, color) in firework_colors.iter().enumerate() {
        let mut node = burst_emitter(&format!("fxFirework{i}"), *color, (40.0, 95.0), (0.7, 1.3), (1.5, 2.6), 3.0, EmitterShape::Circle { radius: 1.0 }, None);
        node.transform.translation = Vec2::new(100_000.0, 0.0);
        add_node(document, &mut inverses, scene, node);
    }

    // ---- Camera + HUD ----
    let mut camera = with_id(
        Node::new(
            "Main Camera",
            None,
            NodeKind::Camera(CameraNode {
                zoom: 1.0,
                background_color: [0.36, 0.6, 0.31, 1.0],
                follow_target: None,
                follow_smoothing: 0.0,
                vignette: an1mbyt3_core::CameraVignette { enabled: true, color: [0.15, 0.05, 0.12, 1.0], intensity: 0.35, radius: 1.15, blur: 0.8 },
                pixel_perfect: an1mbyt3_core::PixelPerfect {
                    enabled: true,
                    grid_snapping: an1mbyt3_core::PixelGridSnapping::UpscaleRenderTexture,
                    crop_frame: an1mbyt3_core::PixelCropFrame::Windowbox,
                },
            }),
        ),
        "camera",
    );
    camera.transform.translation = start;
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));

    let mut hud_node = Node::new("HUD", Some(camera_id), NodeKind::Group(GroupNode::default()));
    hud_node.unlit = true;
    let hud = add_node(document, &mut inverses, scene, hud_node);
    // Confetti falls from the top of the view (camera space -> follows it).
    let confetti_colors = [[1.0, 0.4, 0.6, 1.0], [0.4, 0.75, 1.0, 1.0], [1.0, 0.9, 0.35, 1.0], [0.55, 1.0, 0.55, 1.0], [0.8, 0.55, 1.0, 1.0]];
    // (World space, like the fireworks: the script parks each emitter just
    // above the top of the view before every burst.)
    for (i, color) in confetti_colors.iter().enumerate() {
        let mut node = burst_emitter(&format!("fxConfetti{i}"), *color, (5.0, 25.0), (2.5, 3.8), (1.5, 2.5), 2.2, EmitterShape::Box { width: VIEW_WIDTH + 40.0, height: 4.0 }, None);
        node.transform.translation = Vec2::new(100_000.0, 0.0);
        add_node(document, &mut inverses, scene, node);
    }

    let white = [1.0, 1.0, 1.0, 1.0];
    let gold = [1.0, 0.92, 0.55, 1.0];
    add_node(document, &mut inverses, scene, text_node(Some(hud), "hudGoal", "FIND THE SECRET LETTER", font, 8.0, gold, Vec2::new(0.0, 80.0)));
    add_node(document, &mut inverses, scene, text_node(Some(hud), "hudMet", "", font, 8.0, white, Vec2::new(-118.0, 68.0)));
    add_node(document, &mut inverses, scene, text_node(Some(hud), "hudMsg", "", font, 8.0, [1.0, 0.95, 0.75, 1.0], Vec2::new(0.0, -80.0)));
    let mut heart_icon = with_id(image_node("Heart", Some(hud), heart[4], 12.0, 12.0), "hudHeart");
    heart_icon.transform.translation = Vec2::new(146.0, 80.0);
    add_node(document, &mut inverses, scene, heart_icon);

    // Dialog box (hidden until someone talks).
    let hidden = [1.0, 1.0, 1.0, 0.0];
    let dialog = add_node(document, &mut inverses, scene, Node::new("Dialog", Some(hud), NodeKind::Group(GroupNode::default())));
    let mut panel = with_id(image_node("Dialog Box", Some(dialog), dialog_box.image, 304.0, 54.0), "dlgPanel");
    panel.transform.translation = Vec2::new(0.0, -60.0);
    set_tint(&mut panel, hidden);
    add_node(document, &mut inverses, scene, panel);
    let mut frame = with_id(image_node("Portrait Frame", Some(dialog), faceset_box.image, 42.0, 42.0), "dlgFaceBox");
    frame.transform.translation = Vec2::new(-124.0, -60.0);
    set_tint(&mut frame, hidden);
    add_node(document, &mut inverses, scene, frame);
    let mut face = with_id(image_node("Portrait", Some(dialog), oldman_face.image, 32.0, 32.0), "dlgFace");
    face.transform.translation = Vec2::new(-124.0, -60.0);
    set_tint(&mut face, hidden);
    add_node(document, &mut inverses, scene, face);
    let ink = [0.27, 0.17, 0.13, 1.0];
    add_node(document, &mut inverses, scene, text_node(Some(dialog), "dlgName", "", font, 8.0, [0.75, 0.32, 0.18, 1.0], Vec2::new(18.0, -44.0)));
    add_node(document, &mut inverses, scene, text_node(Some(dialog), "dlgLine1", "", font, 8.0, ink, Vec2::new(18.0, -58.0)));
    add_node(document, &mut inverses, scene, text_node(Some(dialog), "dlgLine2", "", font, 8.0, ink, Vec2::new(18.0, -70.0)));

    // The card: a parchment panel for the title screen and the letter.
    let card = add_node(document, &mut inverses, scene, Node::new("Card", Some(hud), NodeKind::Group(GroupNode::default())));
    let mut shade = with_id(shape("Card Shade", Some(card), Path::rect(VIEW_WIDTH + 20.0, VIEW_HEIGHT + 20.0), [0.1, 0.05, 0.1, 0.0], Vec2::ZERO), "cardShade");
    shade.unlit = true;
    add_node(document, &mut inverses, scene, shade);
    add_node(document, &mut inverses, scene, with_id(shape("Card Border", Some(card), Path::rect(296.0, 166.0), [0.55, 0.3, 0.35, 0.0], Vec2::new(0.0, -2.0)), "cardBorder"));
    add_node(document, &mut inverses, scene, with_id(shape("Card Paper", Some(card), Path::rect(290.0, 160.0), [1.0, 0.96, 0.88, 0.0], Vec2::new(0.0, -2.0)), "cardPaper"));
    add_node(document, &mut inverses, scene, text_node(Some(card), "cardTitle", "", font, 8.0, [0.85, 0.3, 0.45, 1.0], Vec2::new(0.0, 66.0)));
    for i in 0..CARD_LINES {
        add_node(document, &mut inverses, scene, text_node(Some(card), &format!("card{i}"), "", font, 8.0, [0.25, 0.15, 0.15, 1.0], Vec2::new(0.0, 54.0 - i as f32 * 9.0)));
    }
    add_node(document, &mut inverses, scene, text_node(Some(card), "cardHint", "", font, 8.0, [0.6, 0.45, 0.45, 1.0], Vec2::new(0.0, -74.0)));
    add_node(document, &mut inverses, scene, text_node(Some(hud), "banner", "", font, 8.0, gold, Vec2::new(0.0, 80.0)));

    // ---- The game ----
    let script = birthday_script(&solids);
    let animation_inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Birthday filler") });
    let Command::SetAnimationDeleted { animation, .. } = animation_inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(animation_inverse);
    let def = StateMachineDef {
        name: "Birthday Hunt".into(),
        inputs: Vec::new(),
        states: vec![StateDef {
            name: "Playing".into(),
            animation,
            loop_mode: LoopMode::Once,
            transitions: Vec::new(),
            position: Vec2::ZERO,
            blend: None,
            program: Some(ProgramDef::Code(script)),
        }],
        initial_state: "Playing".into(),
        layers: Vec::new(),
        deleted: false,
    };
    inverses.push(document.apply(Command::AddStateMachine { artboard: scene, state_machine: def }));

    inverses.reverse();
    document.commit_batch(inverses);
    BirthdayDemo { scene, images }
}

/// The generated tables (collision boxes, townsfolk, the letter) + the game.
fn birthday_script(solids: &[[f32; 4]]) -> String {
    let mut out = String::from("// ---- Generated by build_demo_birthday ----\n");
    let boxes = solids.iter().map(|b| format!("[{},{},{},{}]", b[0], b[1], b[2], b[3])).collect::<Vec<_>>().join(",");
    let _ = writeln!(out, "const SOLIDS = [{boxes}]");
    let half_w = MAP_W as f32 * TILE / 2.0;
    let half_h = MAP_H as f32 * TILE / 2.0;
    let _ = writeln!(out, "const BOUNDS = [{},{},{},{}]", -half_w + 8.0, -half_h + 2.0, half_w - 8.0, half_h - 14.0);
    let _ = writeln!(out, "const CAM_BOUNDS = [{},{},{},{}]", -half_w + VIEW_WIDTH / 2.0, -half_h + VIEW_HEIGHT / 2.0, half_w - VIEW_WIDTH / 2.0, half_h - VIEW_HEIGHT / 2.0);
    out.push_str("const TOWNSFOLK = {\n");
    for p in TOWNSFOLK {
        let lines = p.lines.iter().map(|l| format!("{:?}", l)).collect::<Vec<_>>().join(", ");
        let _ = writeln!(out, "  {}: {{ name: {:?}, sheet: '{}', wander: {}, lines: [{}] }},", p.id, p.name, p.sheet, p.wander, lines);
    }
    out.push_str("}\n");
    let _ = writeln!(out, "const LETTER = {:?}", LETTER);
    let _ = writeln!(out, "const SIGNATURE = {:?}", SIGNATURE);
    let _ = writeln!(out, "const CARD_LINES = {CARD_LINES}");
    out.push_str(BIRTHDAY_SCRIPT_BODY);
    out
}

const BIRTHDAY_SCRIPT_BODY: &str = r#"
const SPEED = 80
const FEET_HALF_WIDTH = 5
const FEET_HEIGHT = 6
const TALK_RANGE = 32
const BODY_X = 12
const BODY_Y = 9
const DIRS = [[0, -1], [0, 1], [-1, 0], [1, 0]]
const WHITE = [1, 1, 1, 1]
const CLEAR = [1, 1, 1, 0]
const SECRET = 'npcSecret'

const G = ctx.globals
const dt = Math.min(ctx.deltaSeconds, 0.05)

if (!G.started) {
  G.started = true
  G.clock = 0
  G.npcs = {}
  G.met = {}
  G.dialog = null
  G.card = 'intro'
  G.cardPage = 0
  G.reveal = 0
  G.found = false
  G.party = 0
  G.spacePrev = true
  G.facing = 0
  G.imgKeys = {}
  G.tints = {}
  G.frames = {}
  G.camX = null
  for (const img of ctx.images) G.frames[img.name] = img.id
}
G.clock += dt

function frame(name) {
  return G.frames[name] || G.frames[name + '.png'] || null
}
function setImage(ref, name) {
  if (G.imgKeys[ref.id] === name) return
  const id = frame(name)
  if (!id) return
  G.imgKeys[ref.id] = name
  ref.image = id
}
function tint(ref, color) {
  if (!ref) return
  const key = color.join(',')
  if (G.tints[ref.id] === key) return
  G.tints[ref.id] = key
  ref.fillColor = color
}
// Remembers what was written (a ref's textContent can lag a frame behind),
// so clearing a line back to '' really clears it.
function setText(id, value) {
  G.texts = G.texts || {}
  if (G.texts[id] === value) return
  const ref = getCompByID(id)
  if (!ref) return
  G.texts[id] = value
  ref.textContent = value
}
function facingToward(x, y) {
  return Math.abs(x) > Math.abs(y) ? (x < 0 ? 2 : 3) : y > 0 ? 1 : 0
}
// Word-wrap `text` to lines of at most `width` characters.
function wrap(text, width) {
  const words = text.split(' ')
  const lines = []
  let line = ''
  for (const w of words) {
    if ((line + ' ' + w).trim().length > width) {
      lines.push(line.trim())
      line = w
    } else line += ' ' + w
  }
  if (line.trim()) lines.push(line.trim())
  return lines
}
function blip(freq, length, type, volume) {
  try {
    G.audio = G.audio || new (window.AudioContext || window.webkitAudioContext)()
    const t0 = G.audio.currentTime
    const osc = G.audio.createOscillator()
    const gain = G.audio.createGain()
    osc.type = type || 'square'
    osc.frequency.setValueAtTime(freq, t0)
    gain.gain.setValueAtTime(volume || 0.05, t0)
    gain.gain.exponentialRampToValueAtTime(0.001, t0 + length)
    osc.connect(gain).connect(G.audio.destination)
    osc.start(t0)
    osc.stop(t0 + length)
  } catch (e) {}
}
// "Happy Birthday" as a little chiptune.
function playSong() {
  try {
    G.audio = G.audio || new (window.AudioContext || window.webkitAudioContext)()
    const A = G.audio
    const n = { G4: 392, A4: 440, B4: 494, C5: 523, D5: 587, E5: 659, F5: 698, G5: 784 }
    const tune = [
      ['G4', 0.75], ['G4', 0.25], ['A4', 1], ['G4', 1], ['C5', 1], ['B4', 2],
      ['G4', 0.75], ['G4', 0.25], ['A4', 1], ['G4', 1], ['D5', 1], ['C5', 2],
      ['G4', 0.75], ['G4', 0.25], ['G5', 1], ['E5', 1], ['C5', 1], ['B4', 1], ['A4', 2],
      ['F5', 0.75], ['F5', 0.25], ['E5', 1], ['C5', 1], ['D5', 1], ['C5', 3],
    ]
    const beat = 0.36
    let t = A.currentTime + 0.1
    for (const [note, len] of tune) {
      const osc = A.createOscillator()
      const gain = A.createGain()
      osc.type = 'triangle'
      osc.frequency.setValueAtTime(n[note], t)
      gain.gain.setValueAtTime(0.0001, t)
      gain.gain.exponentialRampToValueAtTime(0.09, t + 0.02)
      gain.gain.exponentialRampToValueAtTime(0.001, t + len * beat * 0.95)
      osc.connect(gain).connect(A.destination)
      osc.start(t)
      osc.stop(t + len * beat)
      t += len * beat
    }
  } catch (e) {}
}
function inside(x, y, b) {
  return x >= b[0] && x <= b[2] && y >= b[1] && y <= b[3]
}
function blocked(x, y, self) {
  if (!inside(x, y, BOUNDS)) return true
  for (const s of SOLIDS) {
    if (x + FEET_HALF_WIDTH > s[0] && x - FEET_HALF_WIDTH < s[2] && y + FEET_HEIGHT > s[1] && y < s[3]) return true
  }
  for (const id in G.bodies) {
    if (id === self) continue
    const n = G.bodies[id]
    if (Math.abs(x - n.x) < BODY_X && Math.abs(y - n.y) < BODY_Y) return true
  }
  return false
}
function moveActor(s, vx, vy, self) {
  const nx = s.x + vx * dt
  let moved = false
  if (!blocked(nx, s.y, self)) { s.x = nx; moved = moved || vx !== 0 }
  const ny = s.y + vy * dt
  if (!blocked(s.x, ny, self)) { s.y = ny; moved = moved || vy !== 0 }
  return moved
}
function burst(id, x, y, count) {
  const fx = getCompByID(id)
  if (!fx) return
  if (x !== null) { fx.x = x; fx.y = y }
  fx.emit(count)
}
function say(text, seconds) {
  G.message = text
  G.messageTime = seconds || 2.5
}

const player = getCompByID('player')
const cam = getCompByID('camera')
const npcRefs = getCompByClass('npc')
const k = ctx.keysDown
const spaceDown = k.has('space') || k.has('e') || k.has('enter') || ctx.tap
const spacePressed = spaceDown && !G.spacePrev
G.spacePrev = spaceDown && !ctx.tap

if (player) {
  if (G.px === undefined) {
    G.px = player.x
    G.py = player.y
  }
  G.bodies = { player: { x: G.px, y: G.py } }
  for (const n of npcRefs) {
    const s = G.npcs[n.id]
    G.bodies[n.id] = s ? { x: s.x, y: s.y } : { x: n.x, y: n.y }
  }

  // ---- Input ----
  let dx = (k.has('right') || k.has('d') ? 1 : 0) - (k.has('left') || k.has('a') ? 1 : 0)
  let dy = (k.has('up') || k.has('w') ? 1 : 0) - (k.has('down') || k.has('s') ? 1 : 0)
  // Touch / mouse: hold to walk toward the pointer.
  if (k.has('mouse') && !G.card && !G.dialog) {
    const mx = ctx.mouseX - G.px
    const my = ctx.mouseY - (G.py + 8)
    if (Math.hypot(mx, my) > 10) {
      dx = Math.abs(mx) > 6 ? Math.sign(mx) : 0
      dy = Math.abs(my) > 6 ? Math.sign(my) : 0
    }
  }
  if (G.card || G.dialog) { dx = 0; dy = 0 }
  const moving = dx !== 0 || dy !== 0
  if (moving) G.facing = dx < 0 ? 2 : dx > 0 ? 3 : dy > 0 ? 1 : 0
  if (dx && dy) { dx *= Math.SQRT1_2; dy *= Math.SQRT1_2 }

  // Nearest townsperson in reach.
  let talkTo = null
  let best = TALK_RANGE
  for (const n of npcRefs) {
    const b = G.bodies[n.id]
    const d = Math.hypot(b.x - G.px, b.y - G.py)
    if (d < best) { best = d; talkTo = n.id }
  }

  // ---- Space: card > dialog > talk ----
  if (spacePressed) {
    if (G.card === 'intro') {
      G.card = null
      blip(660, 0.08)
      say('ARROWS / WASD TO WALK, SPACE TO TALK', 4)
    } else if (G.card === 'letter') {
      if (G.reveal < 1) G.reveal = 1
      else if (G.cardPage + 1 < G.letterPages.length) { G.cardPage += 1; G.reveal = 0; blip(700, 0.04) }
      else { G.card = null; say('EVERYONE IN TOWN HAS A WISH FOR YOU!', 6) }
    } else if (G.dialog) {
      G.dialog.page += 1
      blip(700, 0.03)
      if (G.dialog.page >= G.dialog.pages.length) {
        const wasSecret = G.dialog.npc === SECRET
        G.dialog = null
        if (wasSecret) {
          // The big moment.
          G.found = true
          G.card = 'letter'
          G.cardPage = 0
          G.reveal = 0
          G.party = 12
          const lines = wrap(LETTER, 33)
          lines.push('')
          lines.push(SIGNATURE)
          G.letterPages = []
          for (let i = 0; i < lines.length; i += CARD_LINES) G.letterPages.push(lines.slice(i, i + CARD_LINES))
          playSong()
        }
      }
    } else if (talkTo) {
      const info = TOWNSFOLK[talkTo]
      G.met[talkTo] = true
      let pages
      if (talkTo === SECRET) {
        pages = G.found
          ? [['YOU FOUND ME!', 'HAPPY BIRTHDAY, AMY!']]
          : [['OH! YOU FOUND ME!', ''], ['I HAVE A LETTER...', 'IT HAS YOUR NAME ON IT.'], ['HERE, READ IT!', '']]
      } else {
        // Each person takes turns between their lines.
        G.lineIndex = G.lineIndex || {}
        const i = G.lineIndex[talkTo] || 0
        G.lineIndex[talkTo] = (i + 1) % info.lines.length
        const lines = wrap(info.lines[i].toUpperCase(), 22)
        pages = []
        for (let j = 0; j < lines.length; j += 2) pages.push([lines[j], lines[j + 1] || ''])
      }
      G.dialog = { npc: talkTo, pages, page: 0 }
      const b = G.bodies[talkTo]
      G.facing = facingToward(b.x - G.px, b.y - G.py)
      blip(600, 0.05)
    }
  }

  // ---- Player ----
  // You walk past people (only buildings / trees stop you); they still
  // step around you.
  const me = { x: G.px, y: G.py }
  const others = G.bodies
  G.bodies = {}
  moveActor(me, dx * SPEED, dy * SPEED, 'player')
  G.bodies = others
  G.px = me.x
  G.py = me.y
  G.bodies.player = { x: G.px, y: G.py }
  player.x = G.px
  player.y = G.py
  const step = Math.floor(G.clock * 8) % 4
  if (moving) setImage(player, 'ninja_walk ' + (step * 4 + G.facing))
  else setImage(player, 'ninja_idle ' + ((Math.floor(G.clock * 3) % 4) * 4 + G.facing))

  // ---- Townsfolk: wander around home, stop and look at you nearby ----
  for (const n of npcRefs) {
    const info = TOWNSFOLK[n.id]
    if (!info) continue
    let s = G.npcs[n.id]
    if (!s) s = G.npcs[n.id] = { x: n.x, y: n.y, homeX: n.x, homeY: n.y, facing: 0, vx: 0, vy: 0, wait: Math.random() * 2 }
    // They stop and wait for you once you come close.
    const near = Math.hypot(G.px - s.x, G.py - s.y) < TALK_RANGE + (n.id === SECRET ? 60 : 24)
    let vx = 0
    let vy = 0
    if (near || (G.dialog && G.dialog.npc === n.id)) {
      s.facing = facingToward(G.px - s.x, G.py - s.y)
    } else {
      s.wait -= dt
      if (s.wait <= 0) {
        s.wait = 1 + Math.random() * 2.5
        const hx = s.homeX - s.x
        const hy = s.homeY - s.y
        const pace = n.id === SECRET ? 10 : 22
        if (Math.hypot(hx, hy) > info.wander) {
          const d = facingToward(hx, hy)
          s.vx = DIRS[d][0] * pace
          s.vy = DIRS[d][1] * pace
        } else if (Math.random() < 0.35) {
          s.vx = 0
          s.vy = 0
        } else {
          const d = Math.floor(Math.random() * 4)
          s.vx = DIRS[d][0] * pace
          s.vy = DIRS[d][1] * pace
        }
      }
      vx = s.vx
      vy = s.vy
    }
    const walked = moveActor(s, vx, vy, n.id)
    if (!walked && (vx || vy)) s.wait = 0
    if (vx || vy) s.facing = facingToward(vx, vy)
    G.bodies[n.id] = { x: s.x, y: s.y }
    const walking = walked
    if (info.sheet === 'ninja') {
      const f = walking ? 'ninja_walk ' + ((Math.floor(G.clock * 8 + s.homeX) % 4 + 4) % 4 * 4 + s.facing) : 'ninja_idle ' + s.facing
      setImage(n, f)
    } else {
      const rows = info.sheet === 'kid' ? 2 : 4
      const st = walking ? Math.floor(G.clock * 6) % rows : 0
      setImage(n, info.sheet + ' ' + (st * 4 + s.facing))
    }
    n.x = s.x
    n.y = s.y
  }

  // The secret keeper's sparkle (subtle until you've met a few people).
  const keeper = G.npcs[SECRET]
  if (keeper && !G.found) {
    G.sparkleT = (G.sparkleT || 0) - dt
    if (G.sparkleT <= 0) {
      G.sparkleT = Object.keys(G.met).length >= 4 ? 0.25 : 0.9
      burst('fxSparkle', keeper.x, keeper.y + 14, 1)
    }
  }

  // ---- Party: confetti + fireworks after the letter is found ----
  if (G.party > 0 || G.found) {
    G.party = Math.max(0, G.party - dt)
    G.confT = (G.confT || 0) - dt
    if (G.confT <= 0) {
      G.confT = G.party > 0 ? 0.12 : 0.6
      for (let i = 0; i < 5; i++) burst('fxConfetti' + i, G.camX || G.px, (G.camY || G.py) + 96, G.party > 0 ? 3 : 1)
    }
    G.fwT = (G.fwT || 0) - dt
    if (G.fwT <= 0) {
      G.fwT = G.party > 0 ? 0.45 : 1.6
      const i = Math.floor(Math.random() * 5)
      burst('fxFirework' + i, (G.camX || G.px) + (Math.random() - 0.5) * 260, (G.camY || G.py) + 20 + Math.random() * 60, 50)
      blip(120 + Math.random() * 80, 0.25, 'triangle', 0.03)
    }
  }

  // ---- Camera ----
  if (cam) {
    const tx = Math.min(Math.max(G.px, CAM_BOUNDS[0]), CAM_BOUNDS[2])
    const ty = Math.min(Math.max(G.py + 8, CAM_BOUNDS[1]), CAM_BOUNDS[3])
    if (G.camX === null) { G.camX = tx; G.camY = ty }
    const t = 1 - Math.exp(-8 * dt)
    G.camX += (tx - G.camX) * t
    G.camY += (ty - G.camY) * t
    cam.x = Math.round(G.camX)
    cam.y = Math.round(G.camY)
  }

  // Balloons bob.
  for (const b of getCompByClass('balloon')) {
    G.balloonY = G.balloonY || {}
    if (G.balloonY[b.id] === undefined) G.balloonY[b.id] = b.y
    b.y = G.balloonY[b.id] + Math.round(Math.sin(G.clock * 2 + b.x * 0.3) * 1.2)
  }

  // ---- HUD ----
  const metCount = Object.keys(G.met).filter((id) => id !== SECRET).length
  const total = Object.keys(TOWNSFOLK).length - 1
  setText('hudMet', G.found ? '' : 'MET ' + metCount + '/' + total)
  setText('hudGoal', G.found || G.card ? '' : 'FIND THE SECRET LETTER')
  if (!G.dialog && !G.card && (G.messageTime || 0) <= 0 && talkTo) say('SPACE: TALK', 0.1)
  G.messageTime = (G.messageTime || 0) - dt
  setText('hudMsg', !G.dialog && !G.card && G.messageTime > 0 ? G.message : '')
  const hue = G.clock * 2
  const rainbow = [0.75 + 0.25 * Math.sin(hue), 0.75 + 0.25 * Math.sin(hue + 2.1), 0.75 + 0.25 * Math.sin(hue + 4.2), 1]
  const banner = getCompByID('banner')
  setText('banner', G.found && !G.card ? 'HAPPY BIRTHDAY AMYRUTH!' : '')
  if (banner && G.found) banner.fillColor = rainbow

  // Dialog box
  const open = !!G.dialog
  tint(getCompByID('dlgPanel'), open ? WHITE : CLEAR)
  const face = getCompByID('dlgFace')
  const info = open ? TOWNSFOLK[G.dialog.npc] : null
  const hasFace = open && info.sheet !== 'ninja'
  tint(getCompByID('dlgFaceBox'), hasFace ? WHITE : CLEAR)
  if (face) {
    if (hasFace) setImage(face, info.sheet === 'kid' ? 'kid_face' : 'oldman_face')
    tint(face, hasFace ? WHITE : CLEAR)
  }
  const page = open ? G.dialog.pages[G.dialog.page] : ['', '']
  setText('dlgName', open ? (G.dialog.npc === SECRET && G.dialog.page > 0 ? 'JOSHUA' : info.name) : '')
  setText('dlgLine1', page[0])
  setText('dlgLine2', page[1])

  // Card: title screen / the letter
  const cardOn = !!G.card
  tint(getCompByID('cardShade'), cardOn ? [0.1, 0.05, 0.1, 0.55] : [0.1, 0.05, 0.1, 0])
  tint(getCompByID('cardBorder'), cardOn ? [0.85, 0.45, 0.55, 1] : [0.85, 0.45, 0.55, 0])
  tint(getCompByID('cardPaper'), cardOn ? [1.0, 0.96, 0.88, 1] : [1.0, 0.96, 0.88, 0])
  let title = ''
  let lines = []
  let hint = ''
  if (G.card === 'intro') {
    title = 'HAPPY BIRTHDAY, AMYRUTH!'
    lines = ['', 'Someone in this town', 'has a secret letter', 'just for you.', '', 'Talk to the townsfolk,', 'follow their hints,', 'and find who has it!', '', 'Arrows/WASD: walk', 'Space: talk']
    hint = Math.floor(G.clock * 2) % 2 ? 'PRESS SPACE TO START' : ''
  } else if (G.card === 'letter') {
    title = 'A LETTER FOR AMY'
    G.reveal = Math.min(1, G.reveal + dt * 0.35)
    const pageLines = G.letterPages[G.cardPage]
    const totalChars = pageLines.reduce((n, l) => n + l.length, 0)
    let budget = Math.floor(totalChars * G.reveal)
    for (const l of pageLines) {
      const shown = l.slice(0, Math.max(0, budget))
      budget -= l.length
      lines.push(shown)
    }
    if (G.reveal >= 1) hint = G.cardPage + 1 < G.letterPages.length ? 'SPACE: NEXT PAGE' : 'SPACE: CLOSE'
    if (G.reveal < 1 && Math.floor(G.clock * 30) % 3 === 0) blip(1200, 0.01, 'square', 0.015)
  }
  setText('cardTitle', title)
  const titleRef = getCompByID('cardTitle')
  if (titleRef && G.card === 'letter') titleRef.fillColor = [0.85, 0.3 + 0.15 * Math.sin(hue), 0.45, 1]
  for (let i = 0; i < CARD_LINES; i++) setText('card' + i, lines[i] || '')
  setText('cardHint', hint)
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_town_with_a_secret_keeper_and_the_letter() {
        let mut document = Document::new();
        let demo = build_demo_birthday(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        assert!(artboard.active_camera().is_some());
        let (_, machine) = artboard.state_machines().next().unwrap();
        let Some(ProgramDef::Code(source)) = &machine.states[0].program else { panic!("expected Code") };
        assert!(source.contains("npcSecret") && source.contains("With love, Joshua"));
    }
}
