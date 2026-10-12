//! "Sunset Tactics" — a top-down, pixel-art tactical shooter in the style of
//! the 5v5 hero shooters, on a recreation of the Sunset map. No image
//! assets: everything is squares.
//!
//! Two scenes, each one Code program from `demo_tactics.js`:
//! - **Tactics Select** — pick an agent (Sage, Reyna, Sova, Killjoy, Jett).
//! - **Tactics Match** — you + 4 bot teammates attack, 5 bots defend.
//!   First to 5 rounds. Plant the spike on A or B, or eliminate everyone.
//!
//! Vision is real shadow casting: your agent carries a cone-shaped
//! `Light2D` with shadows and every wall / crate is a `ShadowCaster2D`, so
//! you only see what you could actually see. Enemies are hidden unless in
//! your line of sight (or revealed by recon). Smokes are shadow casters
//! too, so they block light and sight alike.
//!
//! The map is authored as floor rectangles traced from the Sunset layout,
//! rasterized to a 104x104 grid of 8 px cells. The same grid string is
//! handed to the script (movement, line of sight, bullets, A* pathing), and
//! the solid cells are merged into large wall rectangles for rendering and
//! shadow casting.

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Command, Document, FontAsset, FontId, GroupNode, Light2DNode, Light2DType, LoopMode, Node, NodeBehavior, NodeKind, Path,
    ProgramDef, ShadowCaster2D, ShapeNode, StateDef, StateMachineDef, Stroke, TextNode,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");
const SCRIPT: &str = include_str!("demo_tactics.js");

/// The render resolution (upscaled to the window, pixel-perfect).
/// Big enough that everything a bot can see (`VISION` in the script,
/// 190 px) is also on your screen in the direction you aim (half height
/// 112 px + up to 70 px of look-ahead).
const VIEW_W: f32 = 400.0;

const VIEW_H: f32 = 225.0;
/// Grid: 104 x 104 cells of 8 world px (16 px of the traced image).
pub(crate) const GW: usize = 104;
pub(crate) const CELL: f32 = 8.0;
const IMG_CELL: f32 = 16.0;
/// World = (image - 832) * 0.5, y up.
const IMG_CENTER: f32 = 832.0;

pub(crate) struct TacticsDemo {
    pub scene: ArtboardId,
    /// With a VRM avatar: the match scene, the avatar's model asset and the
    /// enemy agents' model nodes (the loader gives them red outlines once
    /// the avatar's own materials exist).
    pub avatar: Option<(ArtboardId, an1mbyt3_core::ModelId, Vec<an1mbyt3_core::NodeId>)>,
}

/// A VRM avatar for the agents (the bundled one is popi): the file and
/// what the builder needs to know about it.
pub(crate) struct Avatar3D {
    pub source: Vec<u8>,
    /// VRM 0.x avatars face −Z (VRM 1.0: +Z).
    pub faces_neg_z: bool,
    /// Hips height above the soles (m) — scales the clips' speeds.
    pub hips_height: f32,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Zone {
    Defender,
    Mid,
    Attacker,
    SiteA,
    SiteB,
}

/// Floor rectangles in traced-image pixels (x0, y0, x1, y1).
pub(crate) const FLOORS: &[(f32, f32, f32, f32, Zone)] = &[
    // Defender spawn and its exits.
    (570.0, 40.0, 1088.0, 235.0, Zone::Defender),
    (570.0, 235.0, 675.0, 570.0, Zone::Defender),
    (980.0, 235.0, 1050.0, 470.0, Zone::Defender),
    (723.0, 235.0, 930.0, 285.0, Zone::Defender),
    // A Alley.
    (1050.0, 255.0, 1450.0, 365.0, Zone::Defender),
    (1295.0, 365.0, 1390.0, 520.0, Zone::Defender),
    // Mid Top.
    (675.0, 465.0, 1110.0, 520.0, Zone::Defender),
    (570.0, 520.0, 880.0, 570.0, Zone::Defender),
    // A Link.
    (1050.0, 410.0, 1220.0, 470.0, Zone::Defender),
    (1110.0, 470.0, 1220.0, 555.0, Zone::Defender),
    (1110.0, 555.0, 1270.0, 752.0, Zone::Defender),
    // Behind A site, A Elbow.
    (1390.0, 465.0, 1605.0, 620.0, Zone::Defender),
    (1500.0, 620.0, 1605.0, 668.0, Zone::Defender),
    // B Boba and B Market.
    (205.0, 410.0, 620.0, 465.0, Zone::Defender),
    (205.0, 465.0, 465.0, 545.0, Zone::Defender),
    (325.0, 545.0, 390.0, 720.0, Zone::Defender),
    (413.0, 545.0, 465.0, 880.0, Zone::Defender),
    (465.0, 690.0, 600.0, 880.0, Zone::Defender),
    (285.0, 810.0, 600.0, 880.0, Zone::Defender),
    // B Site area.
    (75.0, 565.0, 390.0, 930.0, Zone::Defender),
    // Mid.
    (790.0, 570.0, 870.0, 625.0, Zone::Mid),
    (675.0, 620.0, 880.0, 930.0, Zone::Mid),
    (598.0, 725.0, 675.0, 775.0, Zone::Mid),
    (880.0, 725.0, 1008.0, 790.0, Zone::Mid),
    (930.0, 790.0, 1008.0, 935.0, Zone::Mid),
    (905.0, 930.0, 1088.0, 985.0, Zone::Mid),
    (780.0, 930.0, 880.0, 985.0, Zone::Mid),
    (690.0, 985.0, 1035.0, 1085.0, Zone::Mid),
    // B Main.
    (140.0, 930.0, 222.0, 988.0, Zone::Mid),
    (95.0, 985.0, 545.0, 1110.0, Zone::Mid),
    (390.0, 1060.0, 545.0, 1190.0, Zone::Mid),
    // A Main and A Lobby.
    (1110.0, 750.0, 1190.0, 885.0, Zone::Mid),
    (1110.0, 775.0, 1605.0, 885.0, Zone::Mid),
    (1500.0, 665.0, 1605.0, 885.0, Zone::Mid),
    (1215.0, 885.0, 1370.0, 1085.0, Zone::Mid),
    // Attacker side.
    (1085.0, 925.0, 1218.0, 1300.0, Zone::Attacker),
    (540.0, 1030.0, 575.0, 1066.0, Zone::Attacker),
    (570.0, 980.0, 690.0, 1190.0, Zone::Attacker),
    (490.0, 1140.0, 880.0, 1300.0, Zone::Attacker),
    (775.0, 1190.0, 1010.0, 1630.0, Zone::Attacker),
    (1085.0, 1290.0, 1215.0, 1335.0, Zone::Attacker),
    (1040.0, 1330.0, 1175.0, 1405.0, Zone::Attacker),
    (1005.0, 1400.0, 1110.0, 1480.0, Zone::Attacker),
    // Sites (plant zones).
    (1265.0, 515.0, 1450.0, 725.0, Zone::SiteA),
    (75.0, 620.0, 335.0, 800.0, Zone::SiteB),
];

/// Solid blocks inside floor areas: the B site's center wall, and crates.
const BLOCKS: &[(f32, f32, f32, f32)] = &[
    (180.0, 670.0, 285.0, 775.0),
    (388.0, 545.0, 413.0, 810.0),
];
const CRATES: &[(f32, f32, f32, f32)] = &[
    (143.0, 568.0, 175.0, 600.0),
    (290.0, 722.0, 322.0, 754.0),
    (362.0, 632.0, 390.0, 660.0),
    (710.0, 900.0, 742.0, 932.0),
    (765.0, 762.0, 800.0, 797.0),
    (855.0, 942.0, 882.0, 970.0),
    (1228.0, 696.0, 1260.0, 728.0),
    (1355.0, 636.0, 1400.0, 681.0),
    (1268.0, 325.0, 1298.0, 362.0),
    (450.0, 1025.0, 480.0, 1090.0),
    (165.0, 1085.0, 197.0, 1110.0),
];

/// Named points (image px) the bots use: spawns, sites, chokes, holds.
const POINTS: &[(&str, f32, f32)] = &[
    ("atkSpawn", 890.0, 1560.0),
    ("defSpawn", 830.0, 140.0),
    ("siteA", 1355.0, 610.0),
    ("siteB", 232.0, 645.0),
    ("plantA", 1320.0, 600.0),
    ("plantB", 150.0, 660.0),
    // Attack approaches (gather spots before an execute).
    ("stageA", 1290.0, 980.0),
    ("stageA2", 1555.0, 830.0),
    ("stageB", 300.0, 1045.0),
    ("stageMid", 830.0, 1040.0),
    // Site entrances.
    ("entryA", 1180.0, 700.0),
    ("entryA2", 1550.0, 690.0),
    ("entryB", 180.0, 900.0),
    ("entryB2", 420.0, 845.0),
    // Defender-side chokes into each site (smoke / slow targets).
    ("chokeA1", 1340.0, 490.0),
    ("chokeA2", 1160.0, 520.0),
    ("chokeB1", 300.0, 520.0),
    ("chokeB2", 500.0, 760.0),
    // Defender holds.
    ("holdA1", 1330.0, 680.0),
    ("holdA2", 1540.0, 560.0),
    ("holdB1", 130.0, 740.0),
    ("holdB2", 330.0, 640.0),
    ("holdMid", 830.0, 600.0),
    // Courtyard (mid control).
    ("courtyard", 770.0, 820.0),
    // More defender spots (off-angles / back site) for varied setups.
    ("holdA3", 1420.0, 545.0),
    ("holdA4", 1190.0, 640.0),
    ("holdB3", 100.0, 600.0),
    ("holdB4", 330.0, 850.0),
    ("holdMid2", 960.0, 760.0),
    // Early aggression spots (defenders pushing out for info).
    ("pushA", 1250.0, 830.0),
    ("pushB", 300.0, 1045.0),
    ("pushMid", 940.0, 960.0),
    // More attacker staging spots (route variety) and lurk spots.
    ("stageA3", 1150.0, 1010.0),
    ("stageB2", 470.0, 1120.0),
    ("lurkA", 640.0, 1080.0),
    ("lurkB", 1150.0, 960.0),
];

/// Callout labels drawn on the floor.
const CALLOUTS: &[(&str, f32, f32)] = &[
    ("DEF SPAWN", 830.0, 140.0),
    ("A ALLEY", 1300.0, 300.0),
    ("A LINK", 1160.0, 460.0),
    ("A SITE", 1360.0, 560.0),
    ("A ELBOW", 1550.0, 640.0),
    ("A MAIN", 1300.0, 835.0),
    ("A LOBBY", 1250.0, 1040.0),
    ("MID TOP", 800.0, 540.0),
    ("COURTYARD", 775.0, 700.0),
    ("TILES", 960.0, 960.0),
    ("MID BOT", 830.0, 1060.0),
    ("B BOBA", 330.0, 440.0),
    ("B MARKET", 520.0, 800.0),
    ("B SITE", 200.0, 640.0),
    ("B MAIN", 340.0, 1020.0),
    ("B LOBBY", 600.0, 1240.0),
    ("ATK SPAWN", 890.0, 1540.0),
];

pub(crate) fn to_world(x: f32, y: f32) -> Vec2 {
    Vec2::new((x - IMG_CENTER) * 0.5, (IMG_CENTER - y) * 0.5)
}

fn inside(r: &(f32, f32, f32, f32), x: f32, y: f32) -> bool {
    x >= r.0 && x < r.2 && y >= r.1 && y < r.3
}

/// The map grid, row-major from the top: `.` floor, `#` wall, `c` crate,
/// `a` / `b` plantable floor on site A / B.
/// `add_node`, parenting root-level nodes under the "World 2D" group.
fn add_world(document: &mut Document, inverses: &mut Vec<Command>, scene: an1mbyt3_core::ArtboardId, world: an1mbyt3_core::NodeId, mut node: Node) -> an1mbyt3_core::NodeId {
    if node.parent.is_none() {
        node.parent = Some(world);
    }
    add_node(document, inverses, scene, node)
}

pub(crate) fn build_grid() -> Vec<u8> {
    let mut grid = vec![b'#'; GW * GW];
    for cy in 0..GW {
        for cx in 0..GW {
            let (x, y) = ((cx as f32 + 0.5) * IMG_CELL, (cy as f32 + 0.5) * IMG_CELL);
            let mut cell = b'#';
            for f in FLOORS {
                if inside(&(f.0, f.1, f.2, f.3), x, y) {
                    cell = match f.4 {
                        Zone::SiteA => b'a',
                        Zone::SiteB => b'b',
                        _ if cell == b'a' || cell == b'b' => cell,
                        _ => b'.',
                    };
                }
            }
            if BLOCKS.iter().any(|r| inside(r, x, y)) {
                cell = b'#';
            }
            if cell != b'#' && CRATES.iter().any(|r| inside(r, x, y)) {
                cell = b'c';
            }
            grid[cy * GW + cx] = cell;
        }
    }
    grid
}

/// Greedy merge of solid cells (`want`) into rectangles: (cx, cy, w, h).
pub(crate) fn merge_rects(grid: &[u8], want: impl Fn(u8) -> bool) -> Vec<(usize, usize, usize, usize)> {
    let mut used = vec![false; grid.len()];
    let mut out = Vec::new();
    for cy in 0..GW {
        let mut cx = 0;
        while cx < GW {
            if used[cy * GW + cx] || !want(grid[cy * GW + cx]) {
                cx += 1;
                continue;
            }
            let mut w = 0;
            while cx + w < GW && !used[cy * GW + cx + w] && want(grid[cy * GW + cx + w]) {
                w += 1;
            }
            let mut h = 1;
            'grow: while cy + h < GW {
                for k in 0..w {
                    let i = (cy + h) * GW + cx + k;
                    if used[i] || !want(grid[i]) {
                        break 'grow;
                    }
                }
                h += 1;
            }
            for dy in 0..h {
                for dx in 0..w {
                    used[(cy + dy) * GW + cx + dx] = true;
                }
            }
            out.push((cx, cy, w, h));
            cx += w;
        }
    }
    out
}

/// Whether a solid cell touches any floor (deep rock casts no useful shadow).
pub(crate) fn borders_floor(grid: &[u8], cx: usize, cy: usize, w: usize, h: usize) -> bool {
    let floor = |x: i32, y: i32| x >= 0 && y >= 0 && (x as usize) < GW && (y as usize) < GW && matches!(grid[y as usize * GW + x as usize], b'.' | b'a' | b'b');
    for y in cy as i32 - 1..=(cy + h) as i32 {
        for x in cx as i32 - 1..=(cx + w) as i32 {
            if floor(x, y) {
                return true;
            }
        }
    }
    false
}

fn shape(id: &str, parent: Option<an1mbyt3_core::NodeId>, path: Path, color: [f32; 4], at: Vec2) -> Node {
    let node = Node::new(if id.is_empty() { "Shape" } else { id }, parent, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None }));
    let mut node = if id.is_empty() { node } else { with_id(node, id) };
    node.transform.translation = at;
    node
}

fn stroked(mut node: Node, width: f32, color: [f32; 4]) -> Node {
    if let NodeKind::Shape(s) = &mut node.kind {
        s.stroke = Some(Stroke { width, color, gradient: None, image: None, cap: Default::default(), join: Default::default() });
    }
    node
}

fn caster(mut node: Node) -> Node {
    node.behaviors.push(an1mbyt3_core::NodeBehaviorEntry { behavior: NodeBehavior::ShadowCaster2D(ShadowCaster2D::default()), enabled: true });
    node
}

fn unlit(mut node: Node) -> Node {
    node.unlit = true;
    node
}

fn hidden(mut node: Node) -> Node {
    node.visible = false;
    node
}

fn text(id: &str, parent: Option<an1mbyt3_core::NodeId>, content: &str, font: FontId, color: [f32; 4], at: Vec2) -> Node {
    let mut node = with_id(Node::new(id, parent, NodeKind::Text(TextNode { content: content.into(), font, size_px: 8.0, fill_color: color, gradient: None })), id);
    node.transform.translation = at;
    node.unlit = true;
    node
}

fn group(id: &str, parent: Option<an1mbyt3_core::NodeId>, at: Vec2) -> Node {
    let mut node = with_id(Node::new(id, parent, NodeKind::Group(GroupNode::default())), id);
    node.transform.translation = at;
    node
}

fn new_scene(document: &mut Document, inverses: &mut Vec<Command>, name: &str, ambient: f32) -> (ArtboardId, an1mbyt3_core::NodeId) {
    let inverse = document.apply(Command::AddArtboard { name: name.into(), width: VIEW_W, height: VIEW_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);
    let camera = with_id(
        Node::with_components(
            "Main Camera",
            None,
            vec![NodeBehavior::Camera(CameraNode {
                zoom: 1.0,
                background_color: [0.02, 0.02, 0.03, 1.0],
                follow_target: None,
                follow_smoothing: 0.0,
                vignette: an1mbyt3_core::CameraVignette { enabled: true, color: [0.0, 0.0, 0.0, 1.0], intensity: 0.35, radius: 1.0, blur: 0.7 },
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
    // (Id'd so the match script can switch 2D lighting off in the 3D view.)
    add_node(document, inverses, scene, with_id(Node::with_components("Global Light 2D", None, vec![NodeBehavior::Light2D(Light2DNode { intensity: ambient, ..Light2DNode::global() })]), "globalLight2d"));
    (scene, camera_id)
}

fn add_program(document: &mut Document, inverses: &mut Vec<Command>, scene: ArtboardId, machine: &str, state: &str, prefix: &str) {
    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Code") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    let mut sections = SCRIPT.split("//@program ");
    let library = sections.next().unwrap_or_default();
    let body = sections
        .find_map(|section| {
            let (name, body) = section.split_once('\n')?;
            (name.trim() == state).then_some(body)
        })
        .unwrap_or_else(|| panic!("demo_tactics.js has no program {state:?}"));
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: machine.into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: state.into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(format!("{prefix}\n{library}\n// ---- {state} ----\n{body}"))),
            }],
            initial_state: state.into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));
}

/// The five agents: (id, name, color).
pub(crate) const AGENTS: [(&str, &str, [f32; 4]); 5] = [
    ("sage", "SAGE", [0.35, 0.92, 0.85, 1.0]),
    ("reyna", "REYNA", [0.72, 0.36, 0.95, 1.0]),
    ("sova", "SOVA", [0.35, 0.58, 1.0, 1.0]),
    ("kj", "KILLJOY", [1.0, 0.86, 0.2, 1.0]),
    ("jett", "JETT", [0.82, 0.93, 1.0, 1.0]),
];

pub(crate) fn build_demo_tactics(document: &mut Document, avatar: Option<Avatar3D>) -> TacticsDemo {
    let avatar_out;
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: FONT_BYTES.to_vec() });

    // ========================================================= Select
    let (select, _) = new_scene(document, &mut inverses, "Tactics Select", 1.0);
    {
        let (document, inverses, scene) = (&mut *document, &mut inverses, select);
        add_node(document, inverses, scene, shape("bg", None, Path::rect(VIEW_W, VIEW_H), [0.07, 0.06, 0.09, 1.0], Vec2::ZERO));
        // The whole menu is one HTML + CSS node (`demo_tactics_ui.rs`),
        // 1600×900 CSS px scaled to the 400×225 view; the script fills it in
        // and handles its clicks.
        let mut menu = Node::with_components(
            "Select Menu",
            None,
            vec![NodeBehavior::UiHtml(an1mbyt3_core::UiHtml {
                html: crate::demo_tactics_ui::SELECT_HTML.into(),
                css: crate::demo_tactics_ui::SELECT_CSS.into(),
                width: 1600.0,
                height: 900.0,
                interactive: true,
            })],
        );
        menu.transform.scale = Vec2::splat(VIEW_W / 1600.0);
        add_node(document, inverses, scene, with_id(menu, "selectUi"));
        // The menu's agent cards are tinted with each agent's color.
        let colors = format!("const AGENT_COLORS = {{ {} }}", AGENTS.iter().map(|(id, _, c)| format!("{id}: [{}, {}, {}, 1]", c[0], c[1], c[2])).collect::<Vec<_>>().join(", "));
        add_program(document, inverses, scene, "Tactics Select", "Select", &colors);
    }

    // ========================================================== Match
    let (matchs, camera) = new_scene(document, &mut inverses, "Tactics Match", 0.22);
    {
        let (document, inverses, scene) = (&mut *document, &mut inverses, matchs);
        let grid = build_grid();
        // Everything top-down (map, agents, ability pools, the vision lights)
        // lives under this group, which the script hides in the 3D view —
        // the HUD (camera children) and the 3D twin stay.
        let world2d = add_node(document, inverses, scene, with_id(group("World 2D", None, Vec2::ZERO), "world2d"));
        // Floors, by zone color.
        for f in FLOORS {
            let color = match f.4 {
                Zone::Defender => [0.44, 0.3, 0.32, 1.0],
                Zone::Mid => [0.38, 0.38, 0.4, 1.0],
                Zone::Attacker => [0.27, 0.42, 0.38, 1.0],
                Zone::SiteA | Zone::SiteB => [0.58, 0.42, 0.33, 1.0],
            };
            let (a, b) = (to_world(f.0, f.1), to_world(f.2, f.3));
            add_world(document, inverses, scene, world2d, shape("", None, Path::rect((b.x - a.x).abs(), (b.y - a.y).abs()), color, (a + b) * 0.5));
        }
        // A faint pixel grid over the floor (every 4th cell).
        for cx in (0..GW).step_by(4) {
            let x = -(GW as f32) * CELL / 2.0 + cx as f32 * CELL;
            add_world(document, inverses, scene, world2d, shape("", None, Path::rect(1.0, GW as f32 * CELL), [0.0, 0.0, 0.0, 0.08], Vec2::new(x, 0.0)));
        }
        for cy in (0..GW).step_by(4) {
            let y = GW as f32 * CELL / 2.0 - cy as f32 * CELL;
            add_world(document, inverses, scene, world2d, shape("", None, Path::rect(GW as f32 * CELL, 1.0), [0.0, 0.0, 0.0, 0.08], Vec2::new(0.0, y)));
        }
        for (label, x, y) in CALLOUTS {
            let mut t = text("", None, label, font, [1.0, 1.0, 1.0, 0.16], to_world(*x, *y));
            t.id = String::new();
            add_world(document, inverses, scene, world2d, t);
        }
        // Walls (merged, only ones next to floor) and crates: both cast shadows.
        let half = GW as f32 * CELL / 2.0;
        let cell_rect = |cx: usize, cy: usize, w: usize, h: usize| (Vec2::new(-half + (cx as f32 + w as f32 / 2.0) * CELL, half - (cy as f32 + h as f32 / 2.0) * CELL), Vec2::new(w as f32 * CELL, h as f32 * CELL));
        for (cx, cy, w, h) in merge_rects(&grid, |c| c == b'#') {
            let (at, size) = cell_rect(cx, cy, w, h);
            let solid = shape("", None, Path::rect(size.x, size.y), [0.07, 0.07, 0.09, 1.0], at);
            if borders_floor(&grid, cx, cy, w, h) {
                add_world(document, inverses, scene, world2d, caster(solid));
            } else {
                add_world(document, inverses, scene, world2d, solid);
            }
        }
        for (cx, cy, w, h) in merge_rects(&grid, |c| c == b'c') {
            let (at, size) = cell_rect(cx, cy, w, h);
            add_world(document, inverses, scene, world2d, caster(stroked(shape("", None, Path::rect(size.x - 1.0, size.y - 1.0), [0.42, 0.33, 0.22, 1.0], at), 1.0, [0.2, 0.15, 0.1, 1.0])));
        }
        // Plant zone outlines.
        for (id, (x, y)) in [("zoneA", (1357.0, 620.0)), ("zoneB", (205.0, 710.0))] {
            let (w, h) = if id == "zoneA" { (92.0, 104.0) } else { (130.0, 90.0) };
            add_world(document, inverses, scene, world2d, unlit(stroked(shape(id, None, Path::rect(w, h), [1.0, 0.6, 0.3, 0.05], to_world(x, y)), 1.0, [1.0, 0.65, 0.3, 0.35])));
        }

        // ---- Ability objects (pools; the script places / shows them) ----
        for i in 0..6 {
            // Smokes block light (shadow casters) and sight.
            add_world(document, inverses, scene, world2d, hidden(caster(unlit(shape(&format!("smoke{i}"), None, Path::polygon(12, 26.0), [0.62, 0.66, 0.72, 0.96], Vec2::ZERO)))));
        }
        for i in 0..12 {
            add_world(document, inverses, scene, world2d, hidden(caster(unlit(stroked(shape(&format!("wall{i}"), None, Path::rect(8.0, 8.0), [0.45, 0.85, 0.85, 1.0], Vec2::ZERO), 1.0, [0.15, 0.4, 0.4, 1.0])))));
        }
        for i in 0..4 {
            add_world(document, inverses, scene, world2d, hidden(unlit(stroked(shape(&format!("slow{i}"), None, Path::ellipse(60.0, 60.0), [0.3, 0.7, 1.0, 0.18], Vec2::ZERO), 1.0, [0.5, 0.85, 1.0, 0.7]))));
            add_world(document, inverses, scene, world2d, hidden(unlit(stroked(shape(&format!("swarm{i}"), None, Path::ellipse(56.0, 56.0), [1.0, 0.8, 0.1, 0.16], Vec2::ZERO), 1.0, [1.0, 0.85, 0.2, 0.8]))));
            add_world(document, inverses, scene, world2d, hidden(unlit(stroked(shape(&format!("ring{i}"), None, Path::ellipse(2.0, 2.0), [1.0, 1.0, 1.0, 0.0], Vec2::ZERO), 1.0, [1.0, 1.0, 1.0, 0.9]))));
        }
        for i in 0..8 {
            add_world(document, inverses, scene, world2d, hidden(unlit(shape(&format!("proj{i}"), None, Path::rect(4.0, 4.0), [1.0, 1.0, 1.0, 1.0], Vec2::ZERO))));
        }
        for i in 0..4 {
            add_world(document, inverses, scene, world2d, hidden(stroked(shape(&format!("device{i}"), None, Path::rect(7.0, 7.0), [1.0, 0.86, 0.2, 1.0], Vec2::ZERO), 1.0, [0.3, 0.2, 0.0, 1.0])));
        }
        for i in 0..3 {
            add_world(document, inverses, scene, world2d, hidden(unlit(shape(&format!("beam{i}"), None, Path::rect(1.0, 6.0), [0.4, 0.75, 1.0, 0.7], Vec2::ZERO))));
        }
        for i in 0..30 {
            add_world(document, inverses, scene, world2d, hidden(unlit(shape(&format!("tracer{i}"), None, Path::rect(1.0, 1.0), [1.0, 0.95, 0.7, 0.85], Vec2::ZERO))));
        }
        // The spike.
        let spike = Node::with_components("Spike", None, vec![NodeBehavior::Light2D(Light2DNode { color: [1.0, 0.3, 0.2, 1.0], intensity: 0.0, outer_radius: 40.0, inner_radius: 4.0, ..Light2DNode::default() })]);
        let mut spike = with_id(spike, "spikeLight");
        spike.visible = false;
        add_world(document, inverses, scene, world2d, spike);
        add_world(document, inverses, scene, world2d, hidden(unlit(stroked(shape("spike", None, Path::rect(6.0, 6.0), [1.0, 0.35, 0.2, 1.0], Vec2::ZERO), 1.0, [0.3, 0.05, 0.0, 1.0]))));

        // ---- Agents: 0-4 attackers (0 = you), 5-9 defenders ----
        for i in 0..10 {
            // Unlit: whether an agent shows is decided by line of sight in
            // the script — once you can see one, it's drawn clearly, not
            // left near-black at the dim edge of your light.
            let root = add_world(document, inverses, scene, world2d, unlit(group(&format!("ag{i}"), None, Vec2::new(0.0, 0.0))));
            let attacker = i < 5;
            let outline = if attacker { [0.3, 1.0, 0.6, 1.0] } else { [1.0, 0.3, 0.3, 1.0] };
            let body = stroked(shape(&format!("ag{i}Body"), Some(root), Path::rect(7.0, 7.0), [1.0, 1.0, 1.0, 1.0], Vec2::ZERO), 1.0, outline);
            let body_id = add_world(document, inverses, scene, world2d, body);
            add_world(document, inverses, scene, world2d, shape(&format!("ag{i}Gun"), Some(body_id), Path::rect(5.0, 2.0), [0.12, 0.12, 0.14, 1.0], Vec2::new(5.5, 0.0)));
            add_world(document, inverses, scene, world2d, unlit(shape(&format!("ag{i}HpB"), Some(root), Path::rect(10.0, 2.0), [0.0, 0.0, 0.0, 0.7], Vec2::new(0.0, 7.0))));
            add_world(document, inverses, scene, world2d, unlit(shape(&format!("ag{i}Hp"), Some(root), Path::rect(10.0, 2.0), outline, Vec2::new(0.0, 7.0))));
            add_world(document, inverses, scene, world2d, unlit(shape(&format!("ag{i}Mark"), Some(root), Path::rect(3.0, 3.0), [1.0, 0.3, 0.3, 1.0], Vec2::new(0.0, 11.0))));
            // A dead agent is drawn as an X in its team colour (the script
            // rotates the bars to ±45° and swaps them in for the body), so a
            // body never reads as a living, standing agent.
            for bar in ["Xa", "Xb"] {
                let dim = [outline[0] * 0.75, outline[1] * 0.75, outline[2] * 0.75, 1.0];
                add_world(document, inverses, scene, world2d, hidden(unlit(shape(&format!("ag{i}{bar}"), Some(root), Path::rect(10.0, 2.0), dim, Vec2::ZERO))));
            }
        }
        // Your vision: a shadow-casting cone + a short all-round aura.
        let cone = Light2DNode {
            light_type: Light2DType::Spot,
            color: [1.0, 0.97, 0.9, 1.0],
            intensity: 1.15,
            inner_radius: 120.0,
            outer_radius: 200.0,
            inner_spot_angle: 94.0,
            outer_spot_angle: 108.0,
            falloff_strength: 0.3,
            shadows_enabled: true,
            shadow_strength: 1.0,
            falloff_steps: 4,
            ..Light2DNode::default()
        };
        add_world(document, inverses, scene, world2d, with_id(Node::with_components("Vision Cone", None, vec![NodeBehavior::Light2D(cone)]), "visionCone"));
        let aura = Light2DNode { color: [0.9, 0.95, 1.0, 1.0], intensity: 0.75, inner_radius: 8.0, outer_radius: 34.0, shadows_enabled: true, shadow_strength: 1.0, falloff_steps: 3, ..Light2DNode::default() };
        add_world(document, inverses, scene, world2d, with_id(Node::with_components("Vision Aura", None, vec![NodeBehavior::Light2D(aura)]), "visionAura"));
        add_world(document, inverses, scene, world2d, unlit(stroked(shape("cross", None, Path::rect(5.0, 5.0), [1.0, 1.0, 1.0, 0.0], Vec2::ZERO), 1.0, [0.4, 1.0, 0.7, 1.0])));

        // ---- HUD (camera children) ----
        let hud = |document: &mut Document, inverses: &mut Vec<Command>, node: Node| add_node(document, inverses, scene, unlit(node));
        // First-person crosshair (the 3D view).
        // A small dot (white, dark rim so it reads on bright walls and sky).
        hud(document, inverses, hidden(stroked(shape("rcCross", Some(camera), Path::ellipse(2.2, 2.2), [1.0, 1.0, 1.0, 1.0], Vec2::ZERO), 0.6, [0.05, 0.05, 0.08, 0.8])));
        let mut flash = shape("flash", Some(camera), Path::rect(VIEW_W, VIEW_H), [1.0, 1.0, 1.0, 0.0], Vec2::ZERO);
        flash.visible = false;
        hud(document, inverses, flash);
        let mut near = stroked(shape("nearsight", Some(camera), Path::ellipse(VIEW_W * 2.6, VIEW_W * 2.6), [0.0, 0.0, 0.0, 0.0], Vec2::ZERO), VIEW_W * 2.1, [0.12, 0.0, 0.2, 0.93]);
        near.visible = false;
        hud(document, inverses, near);
        let mut html_hud = Node::with_components(
            "HUD",
            Some(camera),
            vec![NodeBehavior::UiHtml(an1mbyt3_core::UiHtml {
                html: crate::demo_tactics_ui::HUD_HTML.into(),
                css: crate::demo_tactics_ui::HUD_CSS.into(),
                width: 1600.0,
                height: 900.0,
                interactive: false,
            })],
        );
        html_hud.transform.scale = Vec2::splat(VIEW_W / 1600.0);
        hud(document, inverses, with_id(html_hud, "hud"));
        // The original pixel-font HUD, kept (hidden) so older script paths
        // that still name its nodes keep working; the HTML HUD above is what
        // shows.
        let legacy = hud(document, inverses, hidden(group("Pixel HUD (unused)", Some(camera), Vec2::ZERO)));
        let camera = legacy;
        hud(document, inverses, shape("", Some(camera), Path::rect(124.0, 22.0), [0.0, 0.0, 0.0, 0.55], Vec2::new(0.0, VIEW_H / 2.0 - 11.0)));
        hud(document, inverses, text("score", Some(camera), "0  -  0", font, [1.0, 1.0, 1.0, 1.0], Vec2::new(0.0, VIEW_H / 2.0 - 7.0)));
        hud(document, inverses, text("atkLabel", Some(camera), "ATK", font, [0.4, 1.0, 0.65, 1.0], Vec2::new(-46.0, VIEW_H / 2.0 - 7.0)));
        hud(document, inverses, text("defLabel", Some(camera), "DEF", font, [1.0, 0.4, 0.4, 1.0], Vec2::new(46.0, VIEW_H / 2.0 - 7.0)));
        hud(document, inverses, text("timer", Some(camera), "1:40", font, [1.0, 0.9, 0.7, 1.0], Vec2::new(0.0, VIEW_H / 2.0 - 17.0)));
        for k in 0..5 {
            hud(document, inverses, shape(&format!("pipA{k}"), Some(camera), Path::rect(5.0, 5.0), [0.3, 1.0, 0.6, 1.0], Vec2::new(-90.0 + k as f32 * 7.0, VIEW_H / 2.0 - 7.0)));
            hud(document, inverses, shape(&format!("pipD{k}"), Some(camera), Path::rect(5.0, 5.0), [1.0, 0.3, 0.3, 1.0], Vec2::new(62.0 + k as f32 * 7.0, VIEW_H / 2.0 - 7.0)));
        }
        hud(document, inverses, text("msg", Some(camera), "", font, [1.0, 0.95, 0.85, 1.0], Vec2::new(0.0, 34.0)));
        hud(document, inverses, text("sub", Some(camera), "", font, [0.85, 0.82, 0.9, 1.0], Vec2::new(0.0, 22.0)));
        for k in 0..4 {
            hud(document, inverses, text(&format!("feed{k}"), Some(camera), "", font, [0.95, 0.95, 0.95, 1.0], Vec2::new(98.0, VIEW_H / 2.0 - 30.0 - k as f32 * 9.0)));
        }
        hud(document, inverses, shape("", Some(camera), Path::rect(VIEW_W, 20.0), [0.0, 0.0, 0.0, 0.55], Vec2::new(0.0, -VIEW_H / 2.0 + 10.0)));
        hud(document, inverses, text("hpText", Some(camera), "100", font, [1.0, 1.0, 1.0, 1.0], Vec2::new(-138.0, -VIEW_H / 2.0 + 10.0)));
        hud(document, inverses, text("armorText", Some(camera), "50", font, [0.6, 0.8, 1.0, 1.0], Vec2::new(-112.0, -VIEW_H / 2.0 + 10.0)));
        hud(document, inverses, text("agentText", Some(camera), "", font, [0.8, 0.8, 0.85, 1.0], Vec2::new(-78.0, -VIEW_H / 2.0 + 10.0)));
        for (k, key) in ["C", "Q", "E", "X"].into_iter().enumerate() {
            let x = -24.0 + k as f32 * 22.0;
            hud(document, inverses, stroked(shape(&format!("slot{k}"), Some(camera), Path::rect(18.0, 14.0), [0.15, 0.15, 0.18, 1.0], Vec2::new(x, -VIEW_H / 2.0 + 10.0)), 1.0, [0.6, 0.6, 0.65, 1.0]));
            hud(document, inverses, text(&format!("slotKey{k}"), Some(camera), key, font, [1.0, 1.0, 1.0, 1.0], Vec2::new(x - 3.0, -VIEW_H / 2.0 + 10.0)));
            hud(document, inverses, text(&format!("slotN{k}"), Some(camera), "1", font, [1.0, 0.9, 0.5, 1.0], Vec2::new(x + 5.0, -VIEW_H / 2.0 + 10.0)));
        }
        hud(document, inverses, text("ammoText", Some(camera), "25/25", font, [1.0, 1.0, 1.0, 1.0], Vec2::new(128.0, -VIEW_H / 2.0 + 10.0)));
        hud(document, inverses, text("spikeText", Some(camera), "", font, [1.0, 0.45, 0.3, 1.0], Vec2::new(0.0, VIEW_H / 2.0 - 30.0)));
        hud(document, inverses, shape("progBack", Some(camera), Path::rect(80.0, 5.0), [0.0, 0.0, 0.0, 0.7], Vec2::new(0.0, -36.0)));
        hud(document, inverses, shape("progFill", Some(camera), Path::rect(80.0, 5.0), [1.0, 0.45, 0.3, 1.0], Vec2::new(0.0, -36.0)));
        hud(document, inverses, text("progText", Some(camera), "", font, [1.0, 0.9, 0.8, 1.0], Vec2::new(0.0, -26.0)));
        hud(document, inverses, text("help", Some(camera), "", font, [0.7, 0.7, 0.75, 0.9], Vec2::new(0.0, -VIEW_H / 2.0 + 28.0)));

        // The script gets the grid and named points as constants.
        let grid_str = String::from_utf8(grid.clone()).expect("grid is ASCII");
        let mut points = String::new();
        for (name, x, y) in POINTS {
            let w = to_world(*x, *y);
            points += &format!("{name}: {{ x: {:.1}, y: {:.1} }}, ", w.x, w.y);
        }
        let mut callouts = String::new();
        for (name, x, y) in CALLOUTS {
            let w = to_world(*x, *y);
            callouts += &format!("{{ name: {name:?}, x: {:.1}, y: {:.1} }}, ", w.x, w.y);
        }
        // VRM agents: the generated rifle clips' design speeds (hips
        // heights / s) and the avatar's hips height, for the script.
        let avatar_consts = match &avatar {
            Some(a) => {
                let speeds = crate::demo_tactics_anim::rifle_clips().iter().map(|(c, s)| format!("{}: {s:.3}", c.name)).collect::<Vec<_>>().join(", ");
                format!("const AVATAR_3D = true, AVATAR_HIPS = {:.3}\nconst RIFLE_SPEEDS = {{ {speeds} }}", a.hips_height)
            }
            None => "const AVATAR_3D = false, AVATAR_HIPS = 1\nconst RIFLE_SPEEDS = {}".into(),
        };
        let prefix = format!(
            "const GW = {GW}, CELL = {CELL}, ORIGIN_X = {ox:.1}, ORIGIN_Y = {oy:.1}\nconst VIEW_W = {VIEW_W}, VIEW_H = {VIEW_H}\nconst GRID = \"{grid_str}\"\nconst P = {{ {points} }}\nconst CALLOUT_AREAS = [{callouts}]\nconst AGENT_COLORS = {colors}\n{avatar_consts}",
            ox = -(GW as f32) * CELL / 2.0,
            oy = GW as f32 * CELL / 2.0,
            colors = format!("{{ {} }}", AGENTS.iter().map(|(id, _, c)| format!("{id}: [{}, {}, {}, 1]", c[0], c[1], c[2])).collect::<Vec<_>>().join(", ")),
        );
        add_program(document, inverses, scene, "Tactics Match", "Match", &prefix);
        let agents = crate::demo_tactics_3d::add_3d_world(document, inverses, scene, &grid, avatar);
        avatar_out = agents.map(|(model, enemies)| (scene, model, enemies));
    }

    inverses.reverse();
    document.commit_batch(inverses);
    let _ = matchs;
    TacticsDemo { scene: select, avatar: avatar_out }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flood fill from attacker spawn must reach both sites and the
    /// defender spawn — the traced map is connected.
    #[test]
    fn the_map_connects_spawns_and_sites() {
        let grid = build_grid();
        let cell = |name: &str| {
            let (_, x, y) = POINTS.iter().find(|p| p.0 == name).copied().unwrap();
            ((x / IMG_CELL) as usize, (y / IMG_CELL) as usize)
        };
        let walk = |c: u8| matches!(c, b'.' | b'a' | b'b');
        let start = cell("atkSpawn");
        assert!(walk(grid[start.1 * GW + start.0]), "spawn is floor");
        let mut seen = vec![false; GW * GW];
        let mut stack = vec![start];
        while let Some((x, y)) = stack.pop() {
            let i = y * GW + x;
            if seen[i] || !walk(grid[i]) {
                continue;
            }
            seen[i] = true;
            if x > 0 { stack.push((x - 1, y)); }
            if y > 0 { stack.push((x, y - 1)); }
            if x + 1 < GW { stack.push((x + 1, y)); }
            if y + 1 < GW { stack.push((x, y + 1)); }
        }
        for name in POINTS.iter().map(|p| p.0) {
            let (x, y) = cell(name);
            assert!(seen[y * GW + x], "{name} unreachable from attacker spawn (cell {x},{y} = {})", grid[y * GW + x] as char);
        }
        assert!(grid.iter().filter(|&&c| c == b'a').count() > 30);
        assert!(grid.iter().filter(|&&c| c == b'b').count() > 30);
    }

    #[test]
    fn builds_both_scenes() {
        let mut document = Document::new();
        let demo = build_demo_tactics(&mut document, None);
        assert_eq!(document.artboard(demo.scene).unwrap().name, "Tactics Select");
        let (_, m) = document.artboards().find(|(_, a)| a.name == "Tactics Match").unwrap();
        let casters = m.node_ids().filter_map(|id| m.node(id)).filter(|n| n.shadow_caster().is_some()).count();
        assert!(casters > 20 && casters < 400, "shadow casters: {casters}");
    }
}
