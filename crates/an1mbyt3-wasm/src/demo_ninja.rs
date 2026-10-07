//! "Ninja Adventure" — a small top-down exploration demo built from the
//! CC0 Ninja Adventure asset pack (Pixel-boy & AAA), and the first demo in
//! this engine that spans **more than one scene**.
//!
//! What it exercises, end to end:
//!
//! - **Scenes** — two artboards ("Village" outdoors, "House" indoors), each
//!   with its own state machine running the *same* Code program. Walking
//!   through a door calls `ctx.loadScene(...)` (`scriptRuntime.ts`), which
//!   the editor's frame loop turns into a real scene switch after the tick.
//!   Game state that must survive the switch (coins, which pickups are
//!   gone, whether the chest is open, where to spawn) lives in
//!   `ctx.globals` — a per-Playground-run object, *not* in any node.
//! - **Tilemaps** — the ground (grass with a procedurally autotiled dirt
//!   plaza), the interior floor/walls, *and* every multi-tile world object
//!   (trees, houses): an object is just a tiny `TilemapNode` cropped out of
//!   its tileset, which avoids slicing dozens of one-off images.
//! - **Y-sort** — every standing object lives under one `y_sort_enabled`
//!   `Group`, bottom-anchored (`Transform2D::anchor` at the object's feet)
//!   so walking behind a tree's canopy / in front of its trunk just works.
//! - **Grid-sliced sprite sheets** — the ninja's `Walk.png` is a 4x4 grid
//!   (column = facing, row = step), sliced by
//!   `an1mbyt3_image::import_sprite_grid` and registered as a `SpriteGroup`
//!   so the Resources panel shows it as one multi-frame resource.
//! - **Cameras** — a Main Camera per scene with a Unity-style background
//!   color; the HUD is parented *under* the camera so it rides along with
//!   it, the same "UI as a child of the camera" trick Unity 2D games use.
//!
//! Collision is plain axis-aligned boxes — there's no physics engine to
//! lean on. The Rust side knows exactly where it placed every solid
//! object, so it *emits* the collision table straight into the script's
//! source (`ninja_script`) rather than the script re-deriving it from node
//! bounds at runtime.

use std::fmt::Write as _;

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Command, Document, FontAsset, FontId, GroupNode, ImageAsset, ImageFilterMode, ImageId, ImageNode, Light2DNode,
    ImageProcessing, LoopMode, Node, NodeId, NodeKind, PostEffect, ProgramDef, SpriteGroup, StateDef, StateMachineDef, TextNode, TilemapNode,
};
use glam::Vec2;

const WALK_BYTES: &[u8] = include_bytes!("../assets/ninja/ninja_walk.png");
const IDLE_BYTES: &[u8] = include_bytes!("../assets/ninja/ninja_idle.png");
const COIN_BYTES: &[u8] = include_bytes!("../assets/ninja/coin.png");
const GEM_BYTES: &[u8] = include_bytes!("../assets/ninja/gem.png");
const CHEST_BYTES: &[u8] = include_bytes!("../assets/ninja/chest.png");
const FLOOR_BYTES: &[u8] = include_bytes!("../assets/ninja/tileset_floor.png");
const NATURE_BYTES: &[u8] = include_bytes!("../assets/ninja/tileset_nature.png");
const HOUSE_BYTES: &[u8] = include_bytes!("../assets/ninja/tileset_house.png");
const INTERIOR_FLOOR_BYTES: &[u8] = include_bytes!("../assets/ninja/tileset_interior_floor.png");
const WALL_BYTES: &[u8] = include_bytes!("../assets/ninja/tileset_wall.png");
const PIXEL_FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");
const ATTACK_BYTES: &[u8] = include_bytes!("../assets/ninja/ninja_attack.png");
const HIT_BYTES: &[u8] = include_bytes!("../assets/ninja/ninja_hit.png");
const ROLL_BYTES: &[u8] = include_bytes!("../assets/ninja/ninja_roll.png");
const OLDMAN_BYTES: &[u8] = include_bytes!("../assets/ninja/oldman.png");
const OLDMAN_FACE_BYTES: &[u8] = include_bytes!("../assets/ninja/oldman_face.png");
const KID_BYTES: &[u8] = include_bytes!("../assets/ninja/kid.png");
const KID_FACE_BYTES: &[u8] = include_bytes!("../assets/ninja/kid_face.png");
const SLIME_BYTES: &[u8] = include_bytes!("../assets/ninja/slime.png");
const SKULL_BYTES: &[u8] = include_bytes!("../assets/ninja/skull.png");
const HEART_BYTES: &[u8] = include_bytes!("../assets/ninja/heart.png");
const SMOKE_BYTES: &[u8] = include_bytes!("../assets/ninja/smoke.png");
const SLASH_BYTES: &[u8] = include_bytes!("../assets/ninja/slash.png");
const SKELETON_BYTES: &[u8] = include_bytes!("../assets/ninja/skeleton.png");
const DIALOG_BOX_BYTES: &[u8] = include_bytes!("../assets/ninja/dialog_box.png");
const FACESET_BOX_BYTES: &[u8] = include_bytes!("../assets/ninja/faceset_box.png");

/// The pack's native tile size — and, deliberately, also one world unit per
/// source pixel: a 320x180 camera frame then shows exactly 20x11.25 tiles,
/// the classic SNES-ish framing this art was drawn for, and Nearest
/// filtering keeps it crisp at whatever integer-ish scale the canvas fits it to.
const TILE: f32 = 16.0;

/// The player's animated frames are 32x32 cells, drawn 1:1.
const PLAYER_SIZE: f32 = 32.0;
/// `feet_anchored` puts the anchor `span / 2` below center. The animated
/// cells have the feet on pixel row 24 of 32 — 8px below center, the same
/// offset a plain 16px sprite has — so passing 16 here lands the anchor
/// exactly on the feet.
const PLAYER_FEET_SPAN: f32 = 16.0;

/// Both scenes' artboard (= Main Camera frame at zoom 1) size.
const VIEW_WIDTH: f32 = 320.0;
const VIEW_HEIGHT: f32 = 180.0;

/// Village ground size, in tiles. The tilemap is centered on the origin,
/// so the walkable world spans `±(MAP_W*TILE/2) x ±(MAP_H*TILE/2)`.
const VILLAGE_MAP_W: u32 = 40;
const VILLAGE_MAP_H: u32 = 30;

/// House interior size, in tiles (walls included).
const ROOM_W: u32 = 12;
const ROOM_H: u32 = 9;

/// Where the enterable house's door sits (bottom-center of its footprint).
const HOUSE_A_BASE: Vec2 = Vec2::new(-96.0, 64.0);
const HOUSE_B_BASE: Vec2 = Vec2::new(96.0, 64.0);

/// What `build_demo_ninja_adventure` hands back to `WasmEditor::load_ninja_demo`.
pub(crate) struct NinjaDemo {
    /// The scene to open on (and the one the player starts in).
    pub village: ArtboardId,
    /// Every image this demo created — all of them must be baked before the
    /// first frame (tilesets included: a `TilemapNode` draws from the same
    /// `image_cache` an `Image` node does).
    pub images: Vec<ImageId>,
}

/// A loaded tileset plus the pixel size `TilemapNode` needs alongside it.
#[derive(Clone, Copy)]
pub(crate) struct Tileset {
    pub(crate) image: ImageId,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl Tileset {
    fn columns(&self) -> u32 {
        self.width / TILE as u32
    }

    /// Flat tile index for `(col, row)` — `TilemapNode::tiles`' own
    /// `row * columns + col` convention.
    fn index(&self, col: u32, row: u32) -> u32 {
        row * self.columns() + col
    }

    fn empty_map(&self, map_w: u32, map_h: u32) -> TilemapNode {
        TilemapNode::empty(self.image, self.width, self.height, TILE as u32, TILE as u32, map_w, map_h, TILE)
    }

    /// A small tilemap holding exactly the `w`x`h` block of this tileset
    /// starting at `(col0, row0)` — how every multi-tile world object
    /// (tree, house) is built without cropping it into its own image.
    fn block(&self, col0: u32, row0: u32, w: u32, h: u32) -> TilemapNode {
        let mut map = self.empty_map(w, h);
        for row in 0..h {
            for col in 0..w {
                map.tiles[(row * w + col) as usize] = Some(self.index(col0 + col, row0 + row));
            }
        }
        map
    }
}

/// Everything about one placed solid object the script needs: an axis-
/// aligned collision box `[x0, y0, x1, y1]` (world space, +Y up).
type SolidBox = [f32; 4];

/// A door: stand within `half_width` of `x` and push past `y` in
/// `direction` (`"up"`/`"down"`) to load `to`, arriving at `spawn` facing
/// `facing` (0 down, 1 up, 2 left, 3 right — `Walk.png`'s column order).
struct Door {
    x: f32,
    y: f32,
    half_width: f32,
    direction: &'static str,
    to: &'static str,
    spawn: Vec2,
    facing: u32,
}

/// Per-scene data the script is generated with.
struct SceneData {
    name: &'static str,
    solids: Vec<SolidBox>,
    /// Feet-position clamp `[min_x, min_y, max_x, max_y]`.
    bounds: [f32; 4],
    /// Extra feet positions allowed *outside* `bounds` (a doorway gap in a
    /// wall) — `[x0, y0, x1, y1]`.
    openings: Vec<[f32; 4]>,
    /// Camera-center clamp `[min_x, min_y, max_x, max_y]` — keeps the view
    /// from ever showing past the map's edge.
    camera_bounds: [f32; 4],
    doors: Vec<Door>,
}

/// Normalizes raw pack PNG bytes (often palette-indexed) into the RGBA8 PNG
/// every `ImageAsset::source` is expected to hold — `decode_to_rgba`
/// only handles that one color type — and registers it Nearest-filtered.
pub(crate) fn add_png(document: &mut Document, name: &str, bytes: &[u8]) -> Tileset {
    let imported = an1mbyt3_image::import_image(name, bytes).expect("ninja demo asset decodes");
    let an1mbyt3_image::ImportedImage::Single { name, bytes, width, height } = imported else {
        unreachable!("ninja demo assets are all still PNGs")
    };
    let image = document.add_image(ImageAsset { name, source: bytes, processing: pixel_art_processing(), file: None });
    Tileset { image, width, height }
}

/// Slices a sprite sheet on a full `frame_w`x`frame_h` grid (row-major, see
/// `import_sprite_grid`), adds each cell as an image, and registers the set
/// as one `SpriteGroup` so it reads as a single multi-frame resource.
pub(crate) fn add_grid_frames(document: &mut Document, name: &str, bytes: &[u8], frame_w: u32, frame_h: u32, frame_ms: u32) -> Vec<ImageId> {
    let frames = an1mbyt3_image::import_sprite_grid(name, bytes, frame_w, frame_h).expect("ninja sprite sheet decodes");
    let ids: Vec<ImageId> = frames
        .into_iter()
        .map(|frame| {
            let an1mbyt3_image::ImportedImage::Single { name, bytes, .. } = frame else {
                unreachable!("import_sprite_grid only ever returns ImportedImage::Single frames")
            };
            document.add_image(ImageAsset { name, source: bytes, processing: pixel_art_processing(), file: None })
        })
        .collect();
    document.add_sprite_group(SpriteGroup { name: name.to_string(), frames: ids.iter().map(|&id| (id, frame_ms)).collect() });
    ids
}

pub(crate) fn pixel_art_processing() -> ImageProcessing {
    ImageProcessing { resize: None, filter: ImageFilterMode::Nearest, effect: PostEffect::None }
}

/// `Command::AddNode` + inverse bookkeeping, returning the new id.
pub(crate) fn add_node(document: &mut Document, inverses: &mut Vec<Command>, artboard: ArtboardId, node: Node) -> NodeId {
    let inverse = document.apply(Command::AddNode { artboard, node });
    let Command::SetDeleted { node: id, .. } = inverse else { unreachable!("AddNode's inverse is always a SetDeleted") };
    inverses.push(inverse);
    id
}

pub(crate) fn with_id(mut node: Node, id: &str) -> Node {
    node.id = id.to_string();
    node
}

/// A node whose `translation` is its *feet* (bottom-center), not its
/// center — what Y-sort compares, so depth follows where an object
/// actually stands. `height` is the node's own local height.
pub(crate) fn feet_anchored(mut node: Node, feet: Vec2, height: f32) -> Node {
    node.transform.anchor = Vec2::new(0.0, -height / 2.0);
    node.transform.translation = feet;
    node
}

pub(crate) fn image_node(name: &str, parent: Option<NodeId>, image: ImageId, width: f32, height: f32) -> Node {
    Node::new(name, parent, NodeKind::Image(ImageNode { image, width, height, fill_color: [1.0; 4], blend_mode: Default::default() }))
}

pub(crate) fn text_node(parent: Option<NodeId>, id: &str, content: &str, font: FontId, size_px: f32, color: [f32; 4], at: Vec2) -> Node {
    let mut node = with_id(
        Node::new(id, parent, NodeKind::Text(TextNode { content: content.to_string(), font, size_px, fill_color: color, gradient: None })),
        id,
    );
    node.transform.translation = at;
    node
}

/// Adds one artboard (scene) and returns its id.
fn add_scene(document: &mut Document, inverses: &mut Vec<Command>, name: &str) -> ArtboardId {
    let inverse = document.apply(Command::AddArtboard { name: name.into(), width: VIEW_WIDTH, height: VIEW_HEIGHT });
    let Command::SetArtboardDeleted { artboard, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);
    artboard
}

/// Every image the camera-parented HUD draws, bundled so both scenes'
/// HUDs are built from one call.
struct HudArt {
    coin: ImageId,
    gem: ImageId,
    /// `Heart.png` slices into 5 fill stages; the HUD starts on "full" and
    /// the script swaps frames as health changes.
    heart_full: ImageId,
    dialog_box: ImageId,
    faceset_box: ImageId,
    /// Any portrait — the script swaps in whoever is talking.
    face: ImageId,
}

/// Fully transparent tint — how the dialog panel/portrait start hidden.
/// The script "shows" them by tinting back to opaque white (an untracked
/// per-frame write), rather than toggling `Node::visible`, which is an
/// undoable command and would litter the undo stack every conversation.
const HIDDEN: [f32; 4] = [1.0, 1.0, 1.0, 0.0];

/// The Main Camera (with the HUD parented under it) — shared by both scenes
/// so the script finds the same element ids wherever it's running.
fn add_camera_and_hud(document: &mut Document, inverses: &mut Vec<Command>, artboard: ArtboardId, at: Vec2, background: [f32; 4], font: FontId, art: &HudArt) {
    let mut camera = with_id(
        Node::new("Main Camera", None, NodeKind::Camera(CameraNode { zoom: 1.0, background_color: background, follow_target: None,
            follow_smoothing: 0.0,
            // A heavy, soft night-time vignette...
            vignette: an1mbyt3_core::CameraVignette { enabled: true, color: [0.02, 0.01, 0.05, 1.0], intensity: 0.85, radius: 1.05, blur: 0.75 },
            // ...and Unity-style Pixel Perfect: whole-pixel scale, the frame
            // rendered at 320x180 and nearest-upscaled, black bars around it.
            pixel_perfect: an1mbyt3_core::PixelPerfect {
                enabled: true,
                grid_snapping: an1mbyt3_core::PixelGridSnapping::UpscaleRenderTexture,
                crop_frame: an1mbyt3_core::PixelCropFrame::Windowbox,
            },
        })),
        "camera",
    );
    camera.transform.translation = at;
    let camera_id = add_node(document, inverses, artboard, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard, value: Some(camera_id) }));

    // Everything below is camera-local (the frame spans ±160 x ±90).
    // Unlit: the HUD must stay readable if the scene gets 2D lights (it
    // would otherwise darken with everything else).
    let mut hud_node = Node::new("HUD", Some(camera_id), NodeKind::Group(GroupNode::default()));
    hud_node.unlit = true;
    let hud = add_node(document, inverses, artboard, hud_node);
    let mut coin = image_node("Coin Icon", Some(hud), art.coin, 10.0, 10.0);
    coin.transform.translation = Vec2::new(-148.0, 78.0);
    add_node(document, inverses, artboard, coin);
    let mut gem = image_node("Gem Icon", Some(hud), art.gem, 12.0, 12.0);
    gem.transform.translation = Vec2::new(-148.0, 64.0);
    add_node(document, inverses, artboard, gem);
    let white = [1.0, 1.0, 1.0, 1.0];
    add_node(document, inverses, artboard, text_node(Some(hud), "hudCoins", "0", font, 8.0, white, Vec2::new(-128.0, 74.0)));
    add_node(document, inverses, artboard, text_node(Some(hud), "hudGems", "0", font, 8.0, white, Vec2::new(-128.0, 60.0)));
    add_node(document, inverses, artboard, text_node(Some(hud), "hudMsg", "", font, 8.0, [1.0, 0.93, 0.6, 1.0], Vec2::new(0.0, -78.0)));

    // Hearts, top-right: 3 hearts x 2 HP each.
    for i in 0..3 {
        let mut heart = with_id(image_node("Heart", Some(hud), art.heart_full, 14.0, 14.0), &format!("heart{i}"));
        heart.transform.translation = Vec2::new(116.0 + i as f32 * 16.0, 76.0);
        add_node(document, inverses, artboard, heart);
    }

    // Dialog: panel + portrait frame + portrait + speaker name + 2 lines.
    let dialog = add_node(document, inverses, artboard, Node::new("Dialog", Some(hud), NodeKind::Group(GroupNode::default())));
    let mut panel = with_id(image_node("Dialog Box", Some(dialog), art.dialog_box, 304.0, 54.0), "dlgPanel");
    panel.transform.translation = Vec2::new(0.0, -60.0);
    set_tint(&mut panel, HIDDEN);
    add_node(document, inverses, artboard, panel);
    let mut frame = with_id(image_node("Portrait Frame", Some(dialog), art.faceset_box, 42.0, 42.0), "dlgFaceBox");
    frame.transform.translation = Vec2::new(-124.0, -60.0);
    set_tint(&mut frame, HIDDEN);
    add_node(document, inverses, artboard, frame);
    let mut face = with_id(image_node("Portrait", Some(dialog), art.face, 32.0, 32.0), "dlgFace");
    face.transform.translation = Vec2::new(-124.0, -60.0);
    set_tint(&mut face, HIDDEN);
    add_node(document, inverses, artboard, face);
    let ink = [0.27, 0.17, 0.13, 1.0];
    add_node(document, inverses, artboard, text_node(Some(dialog), "dlgName", "", font, 8.0, [0.75, 0.32, 0.18, 1.0], Vec2::new(18.0, -44.0)));
    add_node(document, inverses, artboard, text_node(Some(dialog), "dlgLine1", "", font, 8.0, ink, Vec2::new(18.0, -58.0)));
    add_node(document, inverses, artboard, text_node(Some(dialog), "dlgLine2", "", font, 8.0, ink, Vec2::new(18.0, -70.0)));
}

/// Pooled one-shot effect sprites, one of each per scene, drawn above the
/// world and below the HUD, hidden until the script plays them: a "poof"
/// parked on a defeated enemy, and the blade-swing arc in front of the
/// player (rotated to the swing's facing).
fn add_fx(document: &mut Document, inverses: &mut Vec<Command>, artboard: ArtboardId, smoke_frame: ImageId, slash_frame: ImageId) {
    let mut smoke = with_id(image_node("Smoke FX", None, smoke_frame, 32.0, 32.0), "fxSmoke");
    smoke.unlit = true;
    set_tint(&mut smoke, HIDDEN);
    add_node(document, inverses, artboard, smoke);
    let mut slash = with_id(image_node("Slash FX", None, slash_frame, 28.0, 25.0), "fxSlash");
    slash.unlit = true;
    set_tint(&mut slash, HIDDEN);
    add_node(document, inverses, artboard, slash);
}

/// The player's warm lantern: casts shadows from Shadow Caster 2D nodes
/// (the houses). Element id `lantern` so the script can flicker it.
fn player_lantern(player: NodeId) -> Node {
    let mut node = with_id(point_light("Lantern", Some(player), [1.0, 0.86, 0.62], 1.2, 64.0, Vec2::ZERO), "lantern");
    if let NodeKind::Light2D(light) = &mut node.kind {
        light.shadows_enabled = true;
        light.shadow_strength = 0.75;
        // Hard-edged shadows suit pixel art.
        light.shadow_softness = 0.0;
    }
    node
}

/// Ambient fireflies: a looping, world-space emitter covering the whole
/// village, slow and fading in/out — unlit, so they glow at night.
fn fireflies() -> Node {
    use an1mbyt3_core::{EmitterShape, ParticleRange, ParticleSystemNode, SimulationSpace};
    let mut config = ParticleSystemNode::default();
    config.main.looping = true;
    config.main.duration = 5.0;
    config.main.start_lifetime = ParticleRange { min: 2.5, max: 4.5 };
    config.main.start_speed = ParticleRange { min: 2.0, max: 7.0 };
    config.main.start_size = ParticleRange { min: 1.0, max: 2.0 };
    config.main.start_color = [0.85, 1.0, 0.45, 1.0];
    config.main.gravity_modifier = -1.5;
    config.main.simulation_space = SimulationSpace::World;
    config.main.max_particles = 70;
    config.emission.rate_over_time = 14.0;
    config.emission.bursts.clear();
    config.shape = EmitterShape::Box { width: VILLAGE_MAP_W as f32 * TILE, height: VILLAGE_MAP_H as f32 * TILE };
    config.color_over_lifetime = vec![
        an1mbyt3_core::GradientStop { offset: 0.0, color: [1.0, 1.0, 1.0, 0.0], midpoint: 0.5 },
        an1mbyt3_core::GradientStop { offset: 0.3, color: [1.0, 1.0, 1.0, 1.0], midpoint: 0.5 },
        an1mbyt3_core::GradientStop { offset: 0.6, color: [1.0, 1.0, 1.0, 0.25], midpoint: 0.5 },
        an1mbyt3_core::GradientStop { offset: 0.8, color: [1.0, 1.0, 1.0, 1.0], midpoint: 0.5 },
        an1mbyt3_core::GradientStop { offset: 1.0, color: [1.0, 1.0, 1.0, 0.0], midpoint: 0.5 },
    ];
    let mut node = Node::new("Fireflies", None, NodeKind::ParticleSystem(config));
    node.unlit = true;
    node
}

/// A Spot Light 2D node (point light: 360° cone) — the demo's player /
/// enemy / lantern glows.
fn point_light(name: &str, parent: Option<NodeId>, color: [f32; 3], intensity: f32, radius: f32, at: Vec2) -> Node {
    let mut node = Node::new(
        name,
        parent,
        NodeKind::Light2D(Light2DNode {
            color: [color[0], color[1], color[2], 1.0],
            intensity,
            inner_radius: radius * 0.15,
            outer_radius: radius,
            // Pixel-art lighting: the falloff drawn as 3 crisp rings (no
            // blur), edges stepped on the 2x2 art-pixel grid.
            falloff_steps: 3,
            edge_blur: 0.0,
            falloff_strength: 0.5,
            pixel_size: 2.0,
            // Overlapping glows merge (brightest wins) instead of stacking.
            overlap_operation: an1mbyt3_core::LightOverlapOperation::Max,
            ..Light2DNode::default()
        }),
    );
    node.transform.translation = at;
    node
}

/// The scene's ambient (Global Light 2D).
fn ambient_light(color: [f32; 3], intensity: f32) -> Node {
    Node::new("Global Light 2D", None, NodeKind::Light2D(Light2DNode { color: [color[0], color[1], color[2], 1.0], intensity, ..Light2DNode::global() }))
}

/// A one-shot effect emitter the script fires with `ref.emit(n)`: no
/// rate, no bursts, simulated in world space (particles stay where they
/// spawned while the emitter is moved to the next hit), unlit so sparks
/// glow in the dark. `id` is what the script finds it by.
#[allow(clippy::too_many_arguments)]
fn fx_emitter(id: &str, color: [f32; 4], speed: (f32, f32), lifetime: (f32, f32), size: (f32, f32), gravity: f32, drag: f32, grow: bool) -> Node {
    use an1mbyt3_core::{EmitterShape, ParticleRange, ParticleSystemNode, SimulationSpace, SizeKeyframe};
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
    config.main.max_particles = 160;
    config.emission.rate_over_time = 0.0;
    config.emission.bursts.clear();
    config.shape = EmitterShape::Circle { radius: 2.0 };
    config.color_over_lifetime = vec![
        an1mbyt3_core::GradientStop { offset: 0.0, color: [1.0, 1.0, 1.0, 1.0], midpoint: 0.5 },
        an1mbyt3_core::GradientStop { offset: 1.0, color: [1.0, 1.0, 1.0, 0.0], midpoint: 0.5 },
    ];
    config.size_over_lifetime = if grow {
        vec![SizeKeyframe { t: 0.0, size_multiplier: 0.6 }, SizeKeyframe { t: 1.0, size_multiplier: 1.6 }]
    } else {
        vec![SizeKeyframe { t: 0.0, size_multiplier: 1.0 }, SizeKeyframe { t: 1.0, size_multiplier: 0.2 }]
    };
    let mut node = with_id(Node::new(id, None, NodeKind::ParticleSystem(config)), id);
    node.transform.translation = Vec2::new(100_000.0, 0.0);
    node.unlit = true;
    node
}

/// Every one-shot effect the script fires, plus a pool of floating damage
/// numbers — added to `artboard` above the world, below the HUD.
fn add_effects(document: &mut Document, inverses: &mut Vec<Command>, artboard: ArtboardId, font: FontId) {
    // Hit sparks: fast, bright, short.
    add_node(document, inverses, artboard, fx_emitter("fxSparks", [1.0, 0.95, 0.7, 1.0], (60.0, 130.0), (0.15, 0.35), (1.5, 3.0), 0.0, 6.0, false));
    // Footstep / roll dust: slow, soft, rising a little, growing.
    add_node(document, inverses, artboard, fx_emitter("fxDust", [0.75, 0.68, 0.55, 0.55], (6.0, 18.0), (0.35, 0.6), (2.0, 4.0), -12.0, 3.0, true));
    // Pickup glitter: gold, floats up.
    add_node(document, inverses, artboard, fx_emitter("fxGlitter", [1.0, 0.85, 0.3, 1.0], (20.0, 55.0), (0.4, 0.8), (1.0, 2.2), -40.0, 2.5, false));
    // Monster death: dark purple puff.
    add_node(document, inverses, artboard, fx_emitter("fxDeath", [0.55, 0.3, 0.85, 0.9], (25.0, 75.0), (0.45, 0.9), (2.5, 5.0), -20.0, 3.5, true));
    let mut damage_layer = Node::new("Damage Numbers", None, NodeKind::Group(GroupNode::default()));
    damage_layer.unlit = true;
    let layer = add_node(document, inverses, artboard, damage_layer);
    for i in 0..6 {
        let mut text = text_node(Some(layer), &format!("dmg{i}"), "", font, 8.0, [1.0, 1.0, 1.0, 1.0], Vec2::new(100_000.0, 0.0));
        text.unlit = true;
        add_node(document, inverses, artboard, text);
    }
}

pub(crate) fn set_tint(node: &mut Node, color: [f32; 4]) {
    if let NodeKind::Image(image) = &mut node.kind {
        image.fill_color = color;
    }
}

/// The one state machine every scene runs: a single `Playing` state whose
/// program is the whole game, as Code.
fn add_game_state_machine(document: &mut Document, inverses: &mut Vec<Command>, artboard: ArtboardId, script: &str) {
    let animation_inverse = document.apply(Command::AddAnimation { artboard, animation: Animation::new("Ninja filler") });
    let Command::SetAnimationDeleted { animation, .. } = animation_inverse else {
        unreachable!("AddAnimation's inverse is always a SetAnimationDeleted")
    };
    inverses.push(animation_inverse);
    let def = StateMachineDef {
        name: "Ninja Adventure".into(),
        inputs: Vec::new(),
        states: vec![StateDef {
            name: "Playing".into(),
            animation,
            loop_mode: LoopMode::Once,
            transitions: Vec::new(),
            position: Vec2::ZERO,
            blend: None,
            program: Some(ProgramDef::Code(script.into())),
        }],
        initial_state: "Playing".into(),
        layers: Vec::new(),
        deleted: false,
    };
    inverses.push(document.apply(Command::AddStateMachine { artboard, state_machine: def }));
}

/// Cheap deterministic per-cell hash in `0..100` — scatters grass tufts
/// without pulling in an RNG (and without the layout changing per load).
fn cell_hash(x: u32, y: u32) -> u32 {
    let mut h = x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    (h ^ (h >> 15)) % 100
}

/// Builds the ground tilemap: grass everywhere (with scattered tufts) and a
/// dirt region autotiled from its own mask with the pack's 3x3 dirt-on-grass
/// edge set — each dirt cell picks its edge/corner piece from which of its
/// four neighbors are also dirt. Concave (inner) corners fall back to the
/// plain center piece; the layout below only joins regions edge-on, so
/// that never shows as anything worse than a straight edge.
fn village_ground(floor: Tileset, dirt: &dyn Fn(i32, i32) -> bool) -> TilemapNode {
    let mut map = floor.empty_map(VILLAGE_MAP_W, VILLAGE_MAP_H);
    let grass = floor.index(0, 12);
    let tufts = [floor.index(2, 11), floor.index(3, 11), floor.index(1, 12), floor.index(2, 12), floor.index(3, 12), floor.index(4, 12)];
    for y in 0..VILLAGE_MAP_H {
        for x in 0..VILLAGE_MAP_W {
            let (xi, yi) = (x as i32, y as i32);
            let tile = if dirt(xi, yi) {
                let (n, s, w, e) = (dirt(xi, yi - 1), dirt(xi, yi + 1), dirt(xi - 1, yi), dirt(xi + 1, yi));
                // Dirt-on-grass 3x3 set: rows 7..=9, cols 0..=2.
                let col = if !w { 0 } else if !e { 2 } else { 1 };
                let row = if !n { 7 } else if !s { 9 } else { 8 };
                floor.index(col, row)
            } else {
                let h = cell_hash(x, y);
                if h < 14 { tufts[(h as usize) % tufts.len()] } else { grass }
            };
            map.tiles[(y * VILLAGE_MAP_W + x) as usize] = Some(tile);
        }
    }
    map
}

/// One kind of standing world object: which tileset block it is, and its
/// collision footprint (half-width, height above the feet line).
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

const TREE_BIG: Prop = Prop { name: "Big Tree", col: 3, row: 18, w: 3, h: 3, solid_half_width: 9.0, solid_height: 10.0 };
const TREE_SMALL: Prop = Prop { name: "Tree", col: 6, row: 8, w: 2, h: 2, solid_half_width: 6.0, solid_height: 8.0 };
const TREE_ROUND: Prop = Prop { name: "Round Tree", col: 0, row: 0, w: 2, h: 2, solid_half_width: 6.0, solid_height: 8.0 };
const PINE: Prop = Prop { name: "Pine", col: 2, row: 0, w: 2, h: 2, solid_half_width: 6.0, solid_height: 8.0 };
const BUSH: Prop = Prop { name: "Bush", col: 1, row: 10, w: 1, h: 1, solid_half_width: 7.0, solid_height: 9.0 };

/// Builds the whole two-scene demo as one undoable batch.
pub(crate) fn build_demo_ninja_adventure(document: &mut Document) -> NinjaDemo {
    let mut inverses = Vec::new();
    let mut images = Vec::new();

    // ---- Assets ----
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: PIXEL_FONT_BYTES.to_vec() });
    // The player uses the pack's *animated* character set
    // (`Actor/CharacterAnimated/NinjaGreen`): 32x32 cells with the 16px
    // character centered and its feet on pixel row 24, column = facing,
    // row = frame — a breathing idle, a 4-frame attack, hit flinch and a
    // dodge roll on top of the walk cycle. See `PLAYER_SIZE`.
    let walk = add_grid_frames(document, "ninja_walk", WALK_BYTES, 32, 32, 120);
    let idle = add_grid_frames(document, "ninja_idle", IDLE_BYTES, 32, 32, 250);
    let hit = add_grid_frames(document, "ninja_hit", HIT_BYTES, 32, 32, 100);
    let roll = add_grid_frames(document, "ninja_roll", ROLL_BYTES, 32, 32, 100);
    let coin = add_grid_frames(document, "coin", COIN_BYTES, 10, 10, 100);
    let chest = add_grid_frames(document, "chest", CHEST_BYTES, 16, 16, 100);
    let gem = add_png(document, "gem.png", GEM_BYTES);
    let floor = add_png(document, "tileset_floor.png", FLOOR_BYTES);
    let nature = add_png(document, "tileset_nature.png", NATURE_BYTES);
    let house_tiles = add_png(document, "tileset_house.png", HOUSE_BYTES);
    let interior_floor = add_png(document, "tileset_interior_floor.png", INTERIOR_FLOOR_BYTES);
    let wall = add_png(document, "tileset_wall.png", WALL_BYTES);
    let attack = add_grid_frames(document, "ninja_attack", ATTACK_BYTES, 32, 32, 70);
    // NPC/monster sheets share the player's layout: column = facing, row = step.
    let oldman = add_grid_frames(document, "oldman", OLDMAN_BYTES, 16, 16, 150);
    let kid = add_grid_frames(document, "kid", KID_BYTES, 16, 16, 150);
    let slime = add_grid_frames(document, "slime", SLIME_BYTES, 16, 16, 150);
    let skull = add_grid_frames(document, "skull", SKULL_BYTES, 16, 16, 120);
    // A full character sheet (4x7): rows 0-3 walk, row 4 the 4-way attack
    // pose — the one enemy with a real weapon swing.
    let skeleton = add_grid_frames(document, "skeleton", SKELETON_BYTES, 16, 16, 150);
    let heart = add_grid_frames(document, "heart", HEART_BYTES, 16, 16, 100);
    let smoke = add_grid_frames(document, "smoke", SMOKE_BYTES, 32, 32, 80);
    // `SpriteSheetArc.png`: 6 frames of 38x34, the arc drawn bulging *down*
    // (a downward swing) — the script rotates it for the other 3 facings.
    let slash = add_grid_frames(document, "slash", SLASH_BYTES, 38, 34, 45);
    let oldman_face = add_png(document, "oldman_face.png", OLDMAN_FACE_BYTES);
    let kid_face = add_png(document, "kid_face.png", KID_FACE_BYTES);
    let dialog_box = add_png(document, "dialog_box.png", DIALOG_BOX_BYTES);
    let faceset_box = add_png(document, "faceset_box.png", FACESET_BOX_BYTES);
    for frames in [&walk, &idle, &hit, &roll, &coin, &chest, &attack, &oldman, &kid, &slime, &skull, &skeleton, &heart, &smoke, &slash] {
        images.extend(frames.iter().copied());
    }
    images.extend([
        gem.image, floor.image, nature.image, house_tiles.image, interior_floor.image, wall.image, oldman_face.image, kid_face.image, dialog_box.image,
        faceset_box.image,
    ]);
    let hud_art = HudArt {
        coin: coin[0],
        gem: gem.image,
        heart_full: heart[4],
        dialog_box: dialog_box.image,
        faceset_box: faceset_box.image,
        face: oldman_face.image,
    };

    // ================= Village =================
    let village = add_scene(document, &mut inverses, "Village");
    let mut village_solids: Vec<SolidBox> = Vec::new();

    // Dirt: a central plaza, a short path up to each house door, and a road
    // running off the south edge. Tile coords: col = (x + 320) / 16,
    // row = (240 - y) / 16.
    let dirt = |x: i32, y: i32| -> bool {
        let plaza = (10..=29).contains(&x) && (12..=18).contains(&y);
        let path_a = (13..=14).contains(&x) && y == 11;
        let path_b = (25..=26).contains(&x) && y == 11;
        let road = (18..=21).contains(&x) && (19..VILLAGE_MAP_H as i32).contains(&y);
        plaza || path_a || path_b || road
    };
    add_node(document, &mut inverses, village, Node::new("Ground", None, NodeKind::Tilemap(village_ground(floor, &dirt))));

    // Everything that stands up goes under one Y-sorted group.
    let world = add_node(
        document,
        &mut inverses,
        village,
        Node::new("World (Y-Sort)", None, NodeKind::Group(GroupNode { y_sort_enabled: true, ..GroupNode::default() })),
    );

    // Flowers: no collision (you walk right through them), but they still
    // stand up off the ground, so they join the Y-sort like everything else
    // — feet-anchored, so the ninja's legs pass behind a flower below them
    // and in front of one above.
    let flower = nature.block(0, 11, 1, 1);
    let daisy = nature.block(6, 11, 1, 1);
    let flower_spots = [
        (-200.0, 20.0), (-216.0, 4.0), (184.0, -20.0), (200.0, -36.0), (-40.0, 96.0), (40.0, 100.0), (-280.0, -60.0),
        (-264.0, -76.0), (280.0, 120.0), (110.0, -210.0), (-120.0, 210.0), (230.0, 10.0), (-180.0, -100.0),
    ];
    for (i, &(x, y)) in flower_spots.iter().enumerate() {
        let map = if i % 2 == 0 { flower.clone() } else { daisy.clone() };
        let node = feet_anchored(Node::new("Flower", Some(world), NodeKind::Tilemap(map)), Vec2::new(x, y), TILE);
        add_node(document, &mut inverses, village, node);
    }

    let mut place_prop = |document: &mut Document, inverses: &mut Vec<Command>, prop: Prop, feet: Vec2| {
        let height = prop.h as f32 * TILE;
        let node = feet_anchored(Node::new(prop.name, Some(world), NodeKind::Tilemap(nature.block(prop.col, prop.row, prop.w, prop.h))), feet, height);
        add_node(document, inverses, village, node);
        village_solids.push([feet.x - prop.solid_half_width, feet.y, feet.x + prop.solid_half_width, feet.y + prop.solid_height]);
    };
    for feet in [(-250.0, 130.0), (240.0, 160.0), (-270.0, -170.0), (260.0, -120.0), (0.0, 150.0), (-180.0, -200.0), (150.0, -190.0)] {
        place_prop(document, &mut inverses, TREE_BIG, Vec2::from(feet));
    }
    for feet in [(-200.0, 60.0), (208.0, 40.0), (-60.0, 196.0), (296.0, 40.0), (-300.0, 30.0), (100.0, -140.0)] {
        place_prop(document, &mut inverses, TREE_SMALL, Vec2::from(feet));
    }
    for feet in [(-140.0, 160.0), (140.0, 196.0), (-290.0, 190.0)] {
        place_prop(document, &mut inverses, TREE_ROUND, Vec2::from(feet));
    }
    for feet in [(180.0, 110.0), (-200.0, 110.0), (60.0, -120.0), (-80.0, -150.0), (300.0, -30.0)] {
        place_prop(document, &mut inverses, PINE, Vec2::from(feet));
    }
    for feet in [(0.0, 80.0), (-60.0, -110.0), (90.0, -90.0), (-230.0, -60.0), (230.0, -70.0), (-130.0, -120.0), (40.0, 80.0), (-40.0, 80.0)] {
        place_prop(document, &mut inverses, BUSH, Vec2::from(feet));
    }

    // Houses: 4x3-tile blocks, door at bottom-center. Only House A has an
    // interior; its collision box is the whole footprint, and the door is a
    // trigger just in front of it.
    for (name, base, col0) in [("House", HOUSE_A_BASE, 0), ("House (Locked)", HOUSE_B_BASE, 4)] {
        let mut node = feet_anchored(Node::new(name, Some(world), NodeKind::Tilemap(house_tiles.block(col0, 0, 4, 3))), base, 3.0 * TILE);
        // Houses block the player's lantern light (Shadow Caster 2D).
        node.shadow_caster = Some(an1mbyt3_core::ShadowCaster2D::default());
        add_node(document, &mut inverses, village, node);
        village_solids.push([base.x - 32.0, base.y, base.x + 32.0, base.y + 44.0]);
    }
    // Warm glow spilling out of the enterable house's doorway.
    add_node(document, &mut inverses, village, point_light("Doorway Glow", None, [1.0, 0.7, 0.35], 1.3, 46.0, HOUSE_A_BASE + Vec2::new(0.0, 6.0)));
    // Night: everything starts dim and blue; the lights do the rest.
    add_node(document, &mut inverses, village, ambient_light([0.42, 0.48, 0.8], 0.42));

    // Pickups — element ids are unique across *both* scenes so
    // `ctx.globals.collected` can key on them alone.
    let coin_spots = [
        (-120.0, 0.0), (-60.0, 20.0), (60.0, -30.0), (120.0, 10.0), (-250.0, -20.0), (270.0, 70.0), (0.0, -150.0), (-150.0, -170.0),
        (200.0, -160.0), (-280.0, 100.0),
    ];
    for (i, &(x, y)) in coin_spots.iter().enumerate() {
        let mut node = feet_anchored(image_node("Coin", Some(world), coin[0], 10.0, 10.0), Vec2::new(x, y), 10.0);
        node.id = format!("coin{i}");
        node.classes = vec!["pickup".into(), "coin".into()];
        add_node(document, &mut inverses, village, node);
    }
    for (i, &(x, y)) in [(280.0, 200.0), (-300.0, -220.0), (30.0, 212.0)].iter().enumerate() {
        let mut node = feet_anchored(image_node("Gem", Some(world), gem.image, 12.0, 12.0), Vec2::new(x, y), 12.0);
        node.id = format!("gem{i}");
        node.classes = vec!["pickup".into(), "gem".into()];
        let gem_id = add_node(document, &mut inverses, village, node);
        add_node(document, &mut inverses, village, point_light("Gem Glow", Some(gem_id), [0.4, 1.0, 0.55], 1.0, 26.0, Vec2::ZERO));
    }

    let player_start = Vec2::new(0.0, -16.0);
    let player = with_id(feet_anchored(image_node("Ninja", Some(world), idle[0], PLAYER_SIZE, PLAYER_SIZE), player_start, PLAYER_FEET_SPAN), "player");
    let player_id = add_node(document, &mut inverses, village, player);
    add_node(document, &mut inverses, village, player_lantern(player_id));

    // NPCs: they wander near home and stop to talk (Space). Their lines live
    // in the script (`NPC_INFO`), keyed by element id.
    for (id, name, frames, feet) in [("npcElder", "Elder", &oldman, Vec2::new(-24.0, 24.0)), ("npcKid", "Kid", &kid, Vec2::new(110.0, -20.0))] {
        let mut node = with_id(feet_anchored(image_node(name, Some(world), frames[0], 16.0, 16.0), feet, 16.0), id);
        node.classes = vec!["npc".into()];
        let npc_id = add_node(document, &mut inverses, village, node);
        if id == "npcElder" {
            add_node(document, &mut inverses, village, point_light("Elder's Lantern", Some(npc_id), [1.0, 0.8, 0.45], 1.1, 40.0, Vec2::ZERO));
        }
    }

    // Enemies roam the south half; they wander until you get close, then chase.
    let enemy_spots: [(&str, &str, &Vec<ImageId>, (f32, f32)); 7] = [
        ("slime0", "slime", &slime, (-200.0, -140.0)),
        ("slime1", "slime", &slime, (180.0, -125.0)),
        ("slime2", "slime", &slime, (-40.0, -195.0)),
        ("skull0", "skull", &skull, (-100.0, -215.0)),
        ("skull1", "skull", &skull, (235.0, -200.0)),
        ("skeleton0", "skeleton", &skeleton, (40.0, -225.0)),
        ("skeleton1", "skeleton", &skeleton, (-270.0, -110.0)),
    ];
    for (id, kind, frames, feet) in enemy_spots {
        let mut node = with_id(feet_anchored(image_node(kind, Some(world), frames[0], 16.0, 16.0), Vec2::from(feet), 16.0), id);
        node.classes = vec!["enemy".into(), kind.into()];
        let enemy_id = add_node(document, &mut inverses, village, node);
        // Each monster carries an eerie glow of its own colour.
        let glow = match kind {
            "slime" => [0.35, 0.85, 1.0],
            "skull" => [1.0, 0.3, 0.2],
            _ => [0.75, 0.6, 1.0],
        };
        add_node(document, &mut inverses, village, point_light("Monster Glow", Some(enemy_id), glow, 0.9, 30.0, Vec2::ZERO));
    }
    // Fireflies drifting over the whole village (unlit — they glow).
    add_node(document, &mut inverses, village, fireflies());

    add_fx(document, &mut inverses, village, smoke[0], slash[0]);
    add_effects(document, &mut inverses, village, font);
    add_camera_and_hud(document, &mut inverses, village, player_start, [0.36, 0.6, 0.31, 1.0], font, &hud_art);

    let half_w = VILLAGE_MAP_W as f32 * TILE / 2.0;
    let half_h = VILLAGE_MAP_H as f32 * TILE / 2.0;
    let village_data = SceneData {
        name: "Village",
        solids: village_solids,
        bounds: [-half_w + 8.0, -half_h + 2.0, half_w - 8.0, half_h - 12.0],
        openings: Vec::new(),
        camera_bounds: [-half_w + VIEW_WIDTH / 2.0, -half_h + VIEW_HEIGHT / 2.0, half_w - VIEW_WIDTH / 2.0, half_h - VIEW_HEIGHT / 2.0],
        doors: vec![Door { x: HOUSE_A_BASE.x, y: HOUSE_A_BASE.y - 8.0, half_width: 10.0, direction: "up", to: "House", spawn: Vec2::new(0.0, -50.0), facing: 1 }],
    };

    // ================= House =================
    let house = add_scene(document, &mut inverses, "House");
    let mut floor_map = interior_floor.empty_map(ROOM_W, ROOM_H);
    let floor_tile = interior_floor.index(5, 13);
    floor_map.tiles.iter_mut().for_each(|t| *t = Some(floor_tile));
    add_node(document, &mut inverses, house, Node::new("Floor", None, NodeKind::Tilemap(floor_map)));

    // Wall ring, built from TilesetWallSimple's 5x5 frame piece set, with
    // the bottom-middle two cells left open as the doorway.
    let mut wall_map = wall.empty_map(ROOM_W, ROOM_H);
    for y in 0..ROOM_H {
        for x in 0..ROOM_W {
            let (left, right, top, bottom) = (x == 0, x == ROOM_W - 1, y == 0, y == ROOM_H - 1);
            let doorway = bottom && (x == ROOM_W / 2 - 1 || x == ROOM_W / 2);
            let piece = match (left, right, top, bottom) {
                _ if doorway => None,
                (true, _, true, _) => Some((0, 0)),
                (_, true, true, _) => Some((4, 0)),
                (true, _, _, true) => Some((0, 4)),
                (_, true, _, true) => Some((4, 4)),
                (_, _, true, _) => Some((1, 0)),
                (_, _, _, true) => Some((1, 4)),
                (true, _, _, _) => Some((0, 1)),
                (_, true, _, _) => Some((4, 1)),
                _ => None,
            };
            wall_map.tiles[(y * ROOM_W + x) as usize] = piece.map(|(c, r)| wall.index(c, r));
        }
    }
    add_node(document, &mut inverses, house, Node::new("Walls", None, NodeKind::Tilemap(wall_map)));

    let room = add_node(
        document,
        &mut inverses,
        house,
        Node::new("Room (Y-Sort)", None, NodeKind::Group(GroupNode { y_sort_enabled: true, ..GroupNode::default() })),
    );
    let chest_feet = Vec2::new(0.0, 30.0);
    let chest_node = with_id(feet_anchored(image_node("Chest", Some(room), chest[0], 16.0, 16.0), chest_feet, 16.0), "chest");
    add_node(document, &mut inverses, house, chest_node);
    for (i, &(x, y)) in [(-56.0, -10.0), (56.0, -10.0)].iter().enumerate() {
        let mut node = feet_anchored(image_node("Coin", Some(room), coin[0], 10.0, 10.0), Vec2::new(x, y), 10.0);
        node.id = format!("coinH{i}");
        node.classes = vec!["pickup".into(), "coin".into()];
        add_node(document, &mut inverses, house, node);
    }
    let house_player = with_id(feet_anchored(image_node("Ninja", Some(room), idle[1], PLAYER_SIZE, PLAYER_SIZE), Vec2::new(0.0, -50.0), PLAYER_FEET_SPAN), "player");
    let house_player_id = add_node(document, &mut inverses, house, house_player);
    add_node(document, &mut inverses, house, player_lantern(house_player_id));
    // Candle-lit interior: a dim warm ambient, two flickering torches
    // (class "torch" — the script flickers them) and the chest's glint.
    add_node(document, &mut inverses, house, ambient_light([0.75, 0.55, 0.45], 0.28));
    for x in [-64.0, 64.0] {
        let mut torch = point_light("Torch", None, [1.0, 0.62, 0.28], 1.5, 90.0, Vec2::new(x, 50.0));
        torch.classes = vec!["torch".into()];
        add_node(document, &mut inverses, house, torch);
    }
    add_node(document, &mut inverses, house, point_light("Chest Glint", None, [1.0, 0.85, 0.35], 0.8, 28.0, chest_feet + Vec2::new(0.0, 8.0)));
    add_fx(document, &mut inverses, house, smoke[0], slash[0]);
    add_effects(document, &mut inverses, house, font);
    add_camera_and_hud(document, &mut inverses, house, Vec2::ZERO, [0.06, 0.05, 0.07, 1.0], font, &hud_art);

    // Room interior spans ±80 x ±56 inside the 16px wall ring.
    let house_data = SceneData {
        name: "House",
        solids: vec![[chest_feet.x - 8.0, chest_feet.y, chest_feet.x + 8.0, chest_feet.y + 10.0]],
        bounds: [-75.0, -56.0, 75.0, 42.0],
        openings: vec![[-11.0, -84.0, 11.0, -56.0]],
        camera_bounds: [0.0, 0.0, 0.0, 0.0],
        doors: vec![Door { x: 0.0, y: -70.0, half_width: 12.0, direction: "down", to: "Village", spawn: HOUSE_A_BASE - Vec2::new(0.0, 12.0), facing: 0 }],
    };

    let script = ninja_script(&[village_data, house_data]);
    add_game_state_machine(document, &mut inverses, village, &script);
    add_game_state_machine(document, &mut inverses, house, &script);

    inverses.reverse();
    document.commit_batch(inverses);

    NinjaDemo { village, images }
}

/// Emits the per-scene data tables as JS, then appends the fixed game
/// logic (`NINJA_SCRIPT_BODY`). `write!` into a `String` can't fail, hence
/// the `let _ =`.
fn ninja_script(scenes: &[SceneData]) -> String {
    let mut out = String::from("// ---- Generated by build_demo_ninja_adventure: per-scene layout data ----\nconst SCENES = {\n");
    for scene in scenes {
        let _ = writeln!(out, "  {}: {{", scene.name);
        let boxes = |list: &[[f32; 4]]| list.iter().map(|b| format!("[{},{},{},{}]", b[0], b[1], b[2], b[3])).collect::<Vec<_>>().join(",");
        let _ = writeln!(out, "    solids: [{}],", boxes(&scene.solids));
        let _ = writeln!(out, "    openings: [{}],", boxes(&scene.openings));
        let b = scene.bounds;
        let _ = writeln!(out, "    bounds: [{},{},{},{}],", b[0], b[1], b[2], b[3]);
        let c = scene.camera_bounds;
        let _ = writeln!(out, "    cameraBounds: [{},{},{},{}],", c[0], c[1], c[2], c[3]);
        let doors = scene
            .doors
            .iter()
            .map(|d| {
                format!(
                    "{{x:{},y:{},halfWidth:{},direction:'{}',to:'{}',spawnX:{},spawnY:{},facing:{}}}",
                    d.x, d.y, d.half_width, d.direction, d.to, d.spawn.x, d.spawn.y, d.facing
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let _ = writeln!(out, "    doors: [{doors}],");
        out.push_str("  },\n");
    }
    out.push_str("}\n");
    out.push_str(NINJA_SCRIPT_BODY);
    out
}

/// The game itself — movement, doors, NPC dialog, enemy AI and combat,
/// pickups, HUD. Runs once per frame in whichever scene is active.
///
/// State that has to outlive a scene switch lives on `ctx.globals` (`G`);
/// everything else is re-derived from the scene each frame. Player
/// position is owned by the script (`G.px/G.py`) rather than read back off
/// the node every frame, so a scene switch can hand the player a spawn
/// point before the new scene's node has ever been written to.
const NINJA_SCRIPT_BODY: &str = r#"
// =====================================================================
// Tuning
// =====================================================================
const SPEED = 72            // world units (= source pixels) per second
const WALK_FPS = 8
const FEET_HALF_WIDTH = 5   // every actor's collision box: a short strip at the feet
const FEET_HEIGHT = 6
const PICKUP_RADIUS_X = 9
const PICKUP_RADIUS_Y = 10
const OFFSCREEN = 100000    // collected pickups / defeated enemies are parked out here
const MAX_HP = 6            // 3 hearts x 2 halves
const ATTACK_TIME = 0.28
const ATTACK_REACH = 12     // how far in front of the feet the swing lands
const ATTACK_RADIUS = 14
const INVULN_TIME = 1.0
const TALK_RANGE = 24
const ROLL_TIME = 0.3       // Shift: a quick dodge roll, untouchable while it lasts
const ROLL_SPEED = 170
const ROLL_COOLDOWN = 0.35
const HIT_TIME = 0.3        // flinch after taking a hit
const START = { scene: 'Village', x: 0, y: -16 }   // where you wake up after fainting
const WHITE = [1, 1, 1, 1]
const CLEAR = [1, 1, 1, 0]
// Facing index -> unit vector. Same order as every sheet's columns:
// 0 down, 1 up, 2 left, 3 right.
const DIRS = [[0, -1], [0, 1], [-1, 0], [1, 0]]

// How each enemy kind fights. They never walk onto you: each one claims
// a free side of you (above/below/left/right, so its 4-way attack lines
// up) at its own `reach`, walks to that spot, and only then attacks -
// a telegraphed `windup` (flashing), the `strike`, then `cooldown`.
//   lunge: true  -> monsters with walk-only sheets: the strike is a dash
//                   at you, hitting if it connects.
//   attackRow    -> sheets with a real attack row (row 4 = the 4-way
//                   attack pose): the strike swings in front of it.
const ENEMY_SPEC = {
  slime: { hp: 2, speed: 26, chaseRange: 70, reach: 18, windup: 0.45, strike: 0.2, cooldown: 1.3, lunge: true, fps: 6 },
  skull: { hp: 3, speed: 40, chaseRange: 90, reach: 20, windup: 0.3, strike: 0.18, cooldown: 1.0, lunge: true, fps: 8 },
  skeleton: { hp: 3, speed: 34, chaseRange: 90, reach: 15, windup: 0.32, strike: 0.25, cooldown: 1.1, attackRow: 4, fps: 6 },
}
const ENEMY_KINDS = Object.keys(ENEMY_SPEC)
// Actors keep at least this far apart (roughly a sprite-width side by
// side, a bit less front-to-back since sprites overlap upward).
const BODY_X = 12
const BODY_Y = 9

// NPCs, keyed by element id. `rows` = how many walk frames the sheet has
// (the Child sheet only has 2).
const NPC_INFO = {
  npcElder: { name: 'ELDER', sheet: 'oldman', face: 'oldman_face', rows: 4, wander: 18 },
  npcKid: { name: 'KID', sheet: 'kid', face: 'kid_face', rows: 2, wander: 48 },
}

// What each NPC says right now — pages of two pre-wrapped lines (the
// pixel font is 8px a glyph, so ~24 characters fit beside the portrait).
function npcPages(id) {
  if (id === 'npcElder') {
    if (G.rewarded) return [['THE ROAD IS SAFE AGAIN.', 'THANK YOU, NINJA.']]
    if (enemiesLeft() === 0) return [['YOU DEFEATED THEM ALL!', 'TAKE THESE GEMS.'], ['GOT 5 GEMS!', '']]
    return [
      ['WELCOME, YOUNG NINJA.', ''],
      ['MONSTERS ROAM THE', 'SOUTH ROAD.'],
      ['PRESS SPACE TO SWING', 'YOUR BLADE. CAREFUL!'],
      [enemiesLeft() + ' MONSTERS REMAIN.', ''],
    ]
  }
  if (id === 'npcKid') {
    return [['I DROPPED MY COINS', 'ALL OVER TOWN!'], ['THE OLD HOUSE HAS A', 'CHEST INSIDE...']]
  }
  return [['...', '']]
}

const G = ctx.globals
const scene = SCENES[ctx.scene]
const realDt = Math.min(ctx.deltaSeconds, 0.05)
// Hit-stop: a few frames of near-freeze on a hit sells the impact (game
// time crawls; particles/sound keep going in real time).
G.hitStop = Math.max(0, (G.hitStop || 0) - realDt)
const dt = G.hitStop > 0 ? realDt * 0.08 : realDt
const decay = Math.exp(-10 * dt)

// =====================================================================
// One-time per Playground run
// =====================================================================
if (!G.started) {
  G.started = true
  G.coins = 0
  G.gems = 0
  G.hp = MAX_HP
  G.collected = {}
  G.dead = {}
  G.enemies = {}
  G.enemyIds = {}
  G.npcs = {}
  G.dialog = null
  G.chestOpen = false
  G.rewarded = false
  G.spawn = null
  G.sceneName = null
  G.clock = 0
  G.attackTime = 0
  G.rollTime = 0
  G.rollCooldown = 0
  G.hitTime = 0
  G.shiftPrev = false
  G.invuln = 0
  G.kx = 0
  G.ky = 0
  G.spacePrev = false
  G.message = 'SPACE: ACT  SHIFT: ROLL'
  G.messageTime = 3.5
  G.frames = {}
  for (const img of ctx.images) G.frames[img.name] = img.id
}
G.clock += dt

// =====================================================================
// Helpers
// =====================================================================
function frame(name) {
  return G.frames[name] || G.frames[name + '.png'] || null
}
// Only write when it changes — every write is a call into the engine.
function setImage(ref, name) {
  if (G.imgKeys[ref.id] === name) return
  const id = frame(name)
  if (!id) return
  G.imgKeys[ref.id] = name
  ref.image = id
}
function tint(ref, color) {
  const key = color.join(',')
  if (G.tints[ref.id] === key) return
  G.tints[ref.id] = key
  ref.fillColor = color
}
function setText(id, value) {
  const ref = getCompByID(id)
  if (ref && ref.textContent !== value) ref.textContent = value
}
function facingToward(x, y) {
  return Math.abs(x) > Math.abs(y) ? (x < 0 ? 2 : 3) : y > 0 ? 1 : 0
}
function enemiesLeft() {
  let left = 0
  for (const id in G.enemyIds) if (!G.dead[id]) left++
  return left
}
// ---- Juice: particle bursts, screen shake, floating numbers ----
function burst(id, x, y, count) {
  const fx = getCompByID(id)
  if (!fx) return
  fx.x = x
  fx.y = y
  fx.emit(count)
}
function shake(amount) {
  G.shake = Math.max(G.shake || 0, amount)
}
function popText(text, x, y, color) {
  G.pops = G.pops || []
  G.popIndex = ((G.popIndex || 0) + 1) % 6
  G.pops[G.popIndex] = { text, x, y, t: 0, color }
}

function say(text, seconds) {
  G.message = text
  G.messageTime = seconds || 2.5
}
// Tiny procedural blips (WebAudio), so hits and pickups feel like something.
function blip(freq, length, type) {
  try {
    G.audio = G.audio || new (window.AudioContext || window.webkitAudioContext)()
    const osc = G.audio.createOscillator()
    const gain = G.audio.createGain()
    osc.type = type || 'square'
    osc.frequency.setValueAtTime(freq, G.audio.currentTime)
    osc.frequency.exponentialRampToValueAtTime(freq * 1.5, G.audio.currentTime + length)
    gain.gain.setValueAtTime(0.06, G.audio.currentTime)
    gain.gain.exponentialRampToValueAtTime(0.001, G.audio.currentTime + length)
    osc.connect(gain).connect(G.audio.destination)
    osc.start()
    osc.stop(G.audio.currentTime + length)
  } catch (e) {}
}
function inside(x, y, b) {
  return x >= b[0] && x <= b[2] && y >= b[1] && y <= b[3]
}
// Would an actor whose feet sit at (x, y) overlap anything solid? `self`
// is the asking actor's own element id, so it doesn't collide with itself.
function blocked(x, y, self) {
  const b = scene.bounds
  if (!inside(x, y, b) && !scene.openings.some((o) => inside(x, y, o))) return true
  for (const s of scene.solids) {
    if (x + FEET_HALF_WIDTH > s[0] && x - FEET_HALF_WIDTH < s[2] && y + FEET_HEIGHT > s[1] && y < s[3]) return true
  }
  // Other actors (player, NPCs, living enemies) are solid to each other -
  // this is what stops enemies piling onto one spot.
  for (const id in G.bodies) {
    if (id === self) continue
    const n = G.bodies[id]
    if (Math.abs(x - n.x) < BODY_X && Math.abs(y - n.y) < BODY_Y) return true
  }
  return false
}
// Axis-separated move, so actors slide along walls instead of sticking.
function moveActor(s, vx, vy, self) {
  const nx = s.x + vx * dt
  if (!blocked(nx, s.y, self)) s.x = nx
  const ny = s.y + vy * dt
  if (!blocked(s.x, ny, self)) s.y = ny
}

const player = getCompByID('player')
const cam = getCompByID('camera')
const npcRefs = getCompByClass('npc')
const enemyRefs = getCompByClass('enemy')

if (scene && player) {
  // ===================================================================
  // Scene entry: place the player (door spawn point, or the authored
  // start) and forget per-node write caches — this scene's nodes were just
  // reset to their authored state.
  // ===================================================================
  if (G.sceneName !== ctx.scene) {
    G.sceneName = ctx.scene
    if (G.spawn && G.spawn.scene === ctx.scene) {
      G.px = G.spawn.x
      G.py = G.spawn.y
      G.facing = G.spawn.facing
    } else {
      G.px = player.x
      G.py = player.y
      G.facing = G.facing || 0
    }
    G.aim = DIRS[G.facing]
    G.spawn = null
    G.leaving = false
    G.camX = null
    G.imgKeys = {}
    G.tints = {}
    G.smoke = null
    G.slash = null
    G.pops = null
    G.shake = 0
    G.dialog = null
  }
  for (const e of enemyRefs) G.enemyIds[e.id] = true

  // Where every NPC stands this frame (previous frame's positions) — what
  // `blocked` checks against.
  G.npcHere = {}
  for (const n of npcRefs) {
    const s = G.npcs[n.id]
    G.npcHere[n.id] = s ? { x: s.x, y: s.y } : { x: n.x, y: n.y }
  }
  // Every actor's body this frame - what `blocked` checks against. Kept up
  // to date as each actor moves below.
  G.bodies = { player: { x: G.px, y: G.py } }
  for (const id in G.npcHere) G.bodies[id] = G.npcHere[id]
  for (const id in G.enemies) {
    const s = G.enemies[id]
    if (!G.dead[id] && s.scene === ctx.scene) G.bodies[id] = { x: s.x, y: s.y }
  }

  // ===================================================================
  // Input
  // ===================================================================
  const k = ctx.keysDown
  const spaceDown = k.has('space') || k.has('e')
  const spacePressed = spaceDown && !G.spacePrev
  G.spacePrev = spaceDown
  let dx = (k.has('right') || k.has('d') ? 1 : 0) - (k.has('left') || k.has('a') ? 1 : 0)
  let dy = (k.has('up') || k.has('w') ? 1 : 0) - (k.has('down') || k.has('s') ? 1 : 0)
  // Dodge roll: in the held direction, or straight ahead if standing still.
  const shiftDown = k.has('shift')
  if (shiftDown && !G.shiftPrev && !G.dialog && !G.leaving && G.attackTime <= 0 && G.rollTime <= 0 && G.rollCooldown <= 0) {
    const len = Math.hypot(dx, dy)
    G.rollDir = len > 0 ? [dx / len, dy / len] : DIRS[G.facing]
    if (len > 0) G.facing = dx < 0 ? 2 : dx > 0 ? 3 : dy > 0 ? 1 : 0
    G.rollTime = ROLL_TIME
    burst('fxDust', G.px, G.py + 2, 10)
    G.rollCooldown = ROLL_TIME + ROLL_COOLDOWN
    blip(180, 0.12, 'triangle')
  }
  G.shiftPrev = shiftDown
  if (G.dialog || G.attackTime > 0 || G.leaving || G.rollTime > 0 || G.hitTime > 0) {
    dx = 0
    dy = 0
  }
  const moving = dx !== 0 || dy !== 0
  if (moving) G.facing = dx < 0 ? 2 : dx > 0 ? 3 : dy > 0 ? 1 : 0
  if (dx !== 0 && dy !== 0) {
    dx *= Math.SQRT1_2
    dy *= Math.SQRT1_2
  }
  // 8-way aim: the sheets only draw 4 facings (diagonals reuse the
  // side-facing pose), but attacks aim along the real input direction, so
  // a diagonal swing hits — and shows its arc — diagonally.
  if (moving) G.aim = [dx, dy]

  // Nearest NPC within talking range, if any.
  let talkTo = null
  let talkDist = TALK_RANGE
  for (const id in G.npcHere) {
    const n = G.npcHere[id]
    const d = Math.hypot(n.x - G.px, n.y - G.py)
    if (d < talkDist) {
      talkDist = d
      talkTo = id
    }
  }
  const chest = getCompByID('chest')
  const nearChest = chest && !G.chestOpen && Math.abs(G.px - chest.x) < 14 && G.py < chest.y && G.py > chest.y - 22

  // ===================================================================
  // Space: advance dialog > talk > open chest > attack
  // ===================================================================
  if (spacePressed) {
    if (G.dialog) {
      G.dialog.page += 1
      if (G.dialog.page >= G.dialog.pages.length) {
        if (G.dialog.reward) {
          G.rewarded = true
          G.gems += 5
          blip(520, 0.35)
        }
        G.dialog = null
      } else {
        blip(700, 0.03)
      }
    } else if (talkTo) {
      const reward = talkTo === 'npcElder' && !G.rewarded && enemiesLeft() === 0
      G.dialog = { npc: talkTo, pages: npcPages(talkTo), page: 0, reward }
      const n = G.npcHere[talkTo]
      G.facing = facingToward(n.x - G.px, n.y - G.py)
      blip(700, 0.03)
    } else if (nearChest) {
      G.chestOpen = true
      burst('fxGlitter', chest.x, chest.y + 10, 30)
      G.gems += 3
      blip(520, 0.3)
      say('FOUND 3 GEMS!')
    } else if (G.attackTime <= 0 && G.rollTime <= 0) {
      G.attackTime = ATTACK_TIME
      const aim = G.aim || DIRS[G.facing]
      G.slash = { t: 0, aim }
      blip(1200, 0.06, 'sawtooth')
      // The swing lands once, at its start, in a circle just in front of you
      // along the 8-way aim.
      const dir = aim
      const hx = G.px + dir[0] * ATTACK_REACH
      const hy = G.py + 6 + dir[1] * ATTACK_REACH
      for (const e of enemyRefs) {
        const s = G.enemies[e.id]
        if (!s || G.dead[e.id]) continue
        if (Math.hypot(s.x - hx, s.y + 6 - hy) < ATTACK_RADIUS) {
          s.hp -= 1
          s.hurt = 0.25
          s.kx = dir[0] * 180
          s.ky = dir[1] * 180
          blip(220, 0.08)
          burst('fxSparks', s.x, s.y + 8, 10)
          G.hitStop = 0.05
          shake(1.5)
          if (s.hp <= 0) {
            G.dead[e.id] = true
            G.coins += 2
            burst('fxDeath', s.x, s.y + 8, 24)
            popText('+2', s.x, s.y + 14, [1, 0.85, 0.3])
            shake(3)
            G.smoke = { x: s.x, y: s.y + 8, t: 0 }
            e.x = OFFSCREEN
            blip(420, 0.2, 'triangle')
            say(enemiesLeft() === 0 ? 'ALL CLEAR! SEE THE ELDER' : '+2 COINS', 2)
          }
        }
      }
    }
  }
  G.attackTime -= dt

  // ===================================================================
  // Player movement (+ knockback from getting hit)
  // ===================================================================
  G.kx *= decay
  G.ky *= decay
  const me = { x: G.px, y: G.py }
  const rolling = G.rollTime > 0
  const runX = rolling ? G.rollDir[0] * ROLL_SPEED : dx * SPEED
  const runY = rolling ? G.rollDir[1] * ROLL_SPEED : dy * SPEED
  moveActor(me, runX + G.kx, runY + G.ky, 'player')
  G.px = me.x
  G.py = me.y
  G.bodies.player = { x: G.px, y: G.py }
  G.invuln -= dt
  G.rollTime -= dt
  G.rollCooldown -= dt
  G.hitTime -= dt

  function hurtPlayer(fromX, fromY) {
    // Rolling through an attack dodges it completely.
    if (G.invuln > 0 || G.rollTime > 0) return
    G.hp -= 1
    G.invuln = INVULN_TIME
    G.hitTime = HIT_TIME
    G.attackTime = 0
    const ax = G.px - fromX
    const ay = G.py - fromY
    const len = Math.hypot(ax, ay) || 1
    G.kx = (ax / len) * 160
    G.ky = (ay / len) * 160
    blip(140, 0.2, 'sawtooth')
    shake(4)
    popText('-1', G.px, G.py + 16, [1, 0.35, 0.3])
    burst('fxSparks', G.px, G.py + 8, 6)
    if (G.hp <= 0) {
      // Faint: wake up at the start with full hearts; monsters go home.
      G.hp = MAX_HP
      G.invuln = 2
      G.kx = 0
      G.ky = 0
      for (const id in G.enemies) {
        const s = G.enemies[id]
        s.x = s.homeX
        s.y = s.homeY
        s.state = 'roam'
      }
      say('YOU FAINTED...', 3)
      if (ctx.scene === START.scene) {
        G.px = START.x
        G.py = START.y
        G.camX = null
      } else if (!G.leaving) {
        G.leaving = true
        G.spawn = { scene: START.scene, x: START.x, y: START.y, facing: 0 }
        ctx.loadScene(START.scene)
      }
    }
  }

  // ===================================================================
  // Doors — a thin band at the door, not a half-plane.
  // ===================================================================
  const DOOR_BAND = 8
  for (const door of scene.doors) {
    const lined = Math.abs(G.px - door.x) <= door.halfWidth
    const through = door.direction === 'up'
      ? dy > 0 && G.py >= door.y && G.py <= door.y + DOOR_BAND
      : dy < 0 && G.py <= door.y && G.py >= door.y - DOOR_BAND
    if (lined && through && !G.leaving) {
      G.leaving = true
      G.spawn = { scene: door.to, x: door.spawnX, y: door.spawnY, facing: door.facing }
      blip(330, 0.12)
      ctx.loadScene(door.to)
    }
  }

  // ===================================================================
  // NPCs: amble around home; stop and face you when you're close.
  // ===================================================================
  for (const n of npcRefs) {
    const info = NPC_INFO[n.id]
    if (!info) continue
    let s = G.npcs[n.id]
    if (!s) s = G.npcs[n.id] = { x: n.x, y: n.y, homeX: n.x, homeY: n.y, facing: 0, vx: 0, vy: 0, wait: 1 + Math.random() * 2 }
    const near = Math.hypot(G.px - s.x, G.py - s.y) < TALK_RANGE + 10
    let vx = 0
    let vy = 0
    if (near || (G.dialog && G.dialog.npc === n.id)) {
      s.facing = facingToward(G.px - s.x, G.py - s.y)
    } else {
      s.wait -= dt
      if (s.wait <= 0) {
        s.wait = 1.5 + Math.random() * 2.5
        const hx = s.homeX - s.x
        const hy = s.homeY - s.y
        if (Math.hypot(hx, hy) > info.wander) {
          // Strayed too far: head back toward home.
          const d = facingToward(hx, hy)
          s.vx = DIRS[d][0] * 18
          s.vy = DIRS[d][1] * 18
        } else if (Math.random() < 0.5) {
          s.vx = 0
          s.vy = 0
        } else {
          const d = Math.floor(Math.random() * 4)
          s.vx = DIRS[d][0] * 18
          s.vy = DIRS[d][1] * 18
        }
      }
      vx = s.vx
      vy = s.vy
      if (vx || vy) s.facing = facingToward(vx, vy)
    }
    // Don't walk into the player either.
    const before = { x: s.x, y: s.y }
    moveActor(s, vx, vy, n.id)
    if (Math.abs(s.x - G.px) < FEET_HALF_WIDTH * 2 && Math.abs(s.y - G.py) < FEET_HEIGHT) {
      s.x = before.x
      s.y = before.y
    }
    G.npcHere[n.id] = { x: s.x, y: s.y }
    G.bodies[n.id] = G.npcHere[n.id]
    const walking = vx !== 0 || vy !== 0
    const step = walking ? Math.floor(G.clock * 6) % info.rows : 0
    setImage(n, info.sheet + ' ' + (step * 4 + s.facing))
    n.x = s.x
    n.y = s.y
  }

  // ===================================================================
  // Enemies. States: 'roam' (wander near home) -> 'approach' (walk to a
  // free side of you, at attack reach) -> 'windup' -> 'strike' ->
  // 'recover' -> back to 'approach'. Getting hit staggers any of them.
  // ===================================================================
  // The 4 attack slots around you: the spot `reach` away on each side.
  // Each enemy claims the nearest free one, so a pack surrounds you instead
  // of stacking on one point.
  const slotTaken = {}
  for (const e of enemyRefs) {
    if (G.dead[e.id]) {
      if (e.x < OFFSCREEN / 2) e.x = OFFSCREEN
      continue
    }
    const kind = ENEMY_KINDS.find((k) => e.classes.includes(k)) || 'slime'
    const spec = ENEMY_SPEC[kind]
    let s = G.enemies[e.id]
    if (!s) {
      s = G.enemies[e.id] = { scene: ctx.scene, x: e.x, y: e.y, homeX: e.x, homeY: e.y, hp: spec.hp, facing: 0, vx: 0, vy: 0, wait: Math.random() * 2, hurt: 0, kx: 0, ky: 0, state: 'roam', timer: 0 }
      G.bodies[e.id] = { x: s.x, y: s.y }
    }
    const toX = G.px - s.x
    const toY = G.py - s.y
    const dist = Math.hypot(toX, toY) || 1
    let vx = 0
    let vy = 0
    let striking = false

    if (s.hurt > 0) {
      // Staggered: any attack in progress is cancelled.
      s.hurt -= dt
      s.state = 'recover'
      s.timer = Math.max(s.timer, 0.35)
    } else if (G.dialog) {
      // Everyone politely waits while you're reading.
    } else if (s.state === 'roam') {
      if (dist < spec.chaseRange) s.state = 'approach'
      s.wait -= dt
      if (s.wait <= 0) {
        s.wait = 1 + Math.random() * 2
        const hx = s.homeX - s.x
        const hy = s.homeY - s.y
        const hl = Math.hypot(hx, hy)
        if (hl > 60) {
          s.vx = (hx / hl) * spec.speed * 0.5
          s.vy = (hy / hl) * spec.speed * 0.5
        } else if (Math.random() < 0.4) {
          s.vx = 0
          s.vy = 0
        } else {
          const a = Math.random() * Math.PI * 2
          s.vx = Math.cos(a) * spec.speed * 0.5
          s.vy = Math.sin(a) * spec.speed * 0.5
        }
      }
      vx = s.vx
      vy = s.vy
      if (vx || vy) s.facing = facingToward(vx, vy)
    } else if (s.state === 'approach') {
      if (dist > spec.chaseRange * 1.6) {
        s.state = 'roam'
      } else {
        // Nearest free side of the player, `reach` away.
        let best = -1
        let bestD = Infinity
        for (let side = 0; side < 4; side++) {
          if (slotTaken[side]) continue
          const sx = G.px + DIRS[side][0] * spec.reach
          const sy = G.py + DIRS[side][1] * spec.reach
          const d = Math.hypot(sx - s.x, sy - s.y)
          if (d < bestD) {
            bestD = d
            best = side
          }
        }
        if (best < 0) {
          // All 4 sides taken: hang back until one frees up.
          if (dist < spec.reach * 2.2) {
            vx = (-toX / dist) * spec.speed * 0.5
            vy = (-toY / dist) * spec.speed * 0.5
          }
          s.facing = facingToward(toX, toY)
        } else {
          slotTaken[best] = e.id
          const gx = G.px + DIRS[best][0] * spec.reach - s.x
          const gy = G.py + DIRS[best][1] * spec.reach - s.y
          const gd = Math.hypot(gx, gy)
          if (gd > 3) {
            vx = (gx / gd) * spec.speed
            vy = (gy / gd) * spec.speed
            s.facing = facingToward(vx, vy)
          } else {
            // In position: face you and start the attack.
            s.facing = facingToward(toX, toY)
            s.state = 'windup'
            s.timer = spec.windup
          }
        }
      }
    } else if (s.state === 'windup') {
      s.facing = facingToward(toX, toY)
      s.timer -= dt
      if (s.timer <= 0) {
        s.state = 'strike'
        s.timer = spec.strike
        s.hitDone = false
        s.strikeDir = s.facing
        blip(kind === 'skeleton' ? 900 : 300, 0.08, 'sawtooth')
        if (!spec.lunge) {
          // Weapon swing: one hit check just in front of it, as it starts.
          const dir = DIRS[s.facing]
          const hx = s.x + dir[0] * 13
          const hy = s.y + dir[1] * 13
          if (Math.hypot(G.px - hx, G.py - hy) < 11) {
            s.hitDone = true
            hurtPlayer(s.x, s.y)
          }
        }
      }
    } else if (s.state === 'strike') {
      striking = true
      s.timer -= dt
      if (spec.lunge) {
        // Dash along the direction it committed to - step aside to dodge.
        const dir = DIRS[s.strikeDir]
        const dashSpeed = (spec.reach * 1.4) / spec.strike
        vx = dir[0] * dashSpeed
        vy = dir[1] * dashSpeed
        if (!s.hitDone && Math.abs(toX) < 13 && Math.abs(toY) < 11) {
          s.hitDone = true
          hurtPlayer(s.x, s.y)
        }
      }
      if (s.timer <= 0) {
        s.state = 'recover'
        s.timer = spec.cooldown
      }
    } else if (s.state === 'recover') {
      // Back off to arm's length while recovering, so the next approach
      // is a visible step in, not a jitter in place.
      if (dist < spec.reach) {
        vx = (-toX / dist) * spec.speed * 0.4
        vy = (-toY / dist) * spec.speed * 0.4
      }
      s.facing = facingToward(toX, toY)
      s.timer -= dt
      if (s.timer <= 0) {
        s.state = 'approach'
        s.timer = 0
      }
    }

    s.kx *= decay
    s.ky *= decay
    moveActor(s, vx + s.kx, vy + s.ky, e.id)
    G.bodies[e.id] = { x: s.x, y: s.y }
    e.x = s.x
    e.y = s.y

    // Sprite: attack pose (sheets that have one) > walk cycle > standing.
    // Monsters bob in place even when still - their sheets have no idle.
    const moving = Math.abs(vx) + Math.abs(vy) > 1
    const phase = e.id.charCodeAt(e.id.length - 1)
    const look = striking ? s.strikeDir : s.facing
    let row
    if (striking && spec.attackRow !== undefined) row = spec.attackRow
    else if (moving || spec.lunge) row = Math.floor(G.clock * (striking ? spec.fps * 2 : spec.fps) + phase) % 4
    else row = 0
    setImage(e, kind + ' ' + (row * 4 + look))
    // Hurt = red; winding up = a bright flicker (the "about to hit you" tell).
    const flicker = s.state === 'windup' && Math.floor(G.clock * 20) % 2 === 0
    tint(e, s.hurt > 0 ? [1, 0.35, 0.35, 1] : flicker ? [1, 1, 0.45, 1] : WHITE)
  }

  // ===================================================================
  // Player sprite: attack > walk > idle; blink while invulnerable.
  // ===================================================================
  player.x = G.px
  player.y = G.py
  // Every sheet: column = facing, row = frame -> index = frame * 4 + facing.
  // Priority: roll > hit flinch > attack > walk > (breathing) idle.
  // `progress` maps a timed action onto its frames, so the animation
  // always plays start to finish over the action's own duration.
  const progress = (left, total, frames) => Math.min(frames - 1, Math.max(0, Math.floor(((total - left) / total) * frames)))
  let pose
  if (G.rollTime > 0) pose = 'ninja_roll ' + (progress(G.rollTime, ROLL_TIME, 3) * 4 + G.facing)
  else if (G.hitTime > 0) pose = 'ninja_hit ' + (progress(G.hitTime, HIT_TIME, 2) * 4 + G.facing)
  else if (G.attackTime > 0) pose = 'ninja_attack ' + (progress(G.attackTime, ATTACK_TIME, 4) * 4 + G.facing)
  else if (moving) pose = 'ninja_walk ' + ((Math.floor(G.clock * WALK_FPS) % 4) * 4 + G.facing)
  else pose = 'ninja_idle ' + ((Math.floor(G.clock * 4) % 4) * 4 + G.facing)
  setImage(player, pose)
  tint(player, G.invuln > 0 && G.hitTime <= 0 && Math.floor(G.clock * 16) % 2 ? [1, 1, 1, 0.3] : WHITE)

  // ===================================================================
  // Pickups
  // ===================================================================
  for (const item of getCompByClass('pickup')) {
    if (G.collected[item.id]) {
      if (item.x < OFFSCREEN / 2) item.x = OFFSCREEN
      continue
    }
    const isCoin = item.classes.includes('coin')
    if (isCoin) {
      const phase = item.id.length + item.x * 0.05
      setImage(item, 'coin ' + (Math.floor(G.clock * 8 + phase) % 4))
    }
    if (Math.abs(item.x - G.px) < PICKUP_RADIUS_X && Math.abs(item.y - G.py) < PICKUP_RADIUS_Y) {
      G.collected[item.id] = true
      item.x = OFFSCREEN
      burst('fxGlitter', item.x, item.y + 5, isCoin ? 10 : 20)
      if (isCoin) {
        G.coins += 1
        blip(880, 0.08)
      } else {
        G.gems += 1
        blip(660, 0.18)
        say('GEM!')
      }
    }
  }

  // ===================================================================
  // Chest, prompts
  // ===================================================================
  if (chest) setImage(chest, G.chestOpen ? 'chest 1' : 'chest 0')
  if (!G.dialog && G.messageTime <= 0) {
    if (talkTo) say('SPACE: TALK', 0.1)
    else if (nearChest) say('SPACE: OPEN', 0.1)
  }

  // ===================================================================
  // Smoke puff on a defeated enemy (one pooled sprite per scene)
  // ===================================================================
  const fx = getCompByID('fxSmoke')
  if (fx) {
    if (G.smoke) {
      G.smoke.t += dt
      const idx = Math.floor(G.smoke.t / 0.08)
      if (idx >= 6) {
        G.smoke = null
        tint(fx, CLEAR)
      } else {
        fx.x = G.smoke.x
        fx.y = G.smoke.y
        setImage(fx, 'smoke ' + idx)
        tint(fx, WHITE)
      }
    } else {
      tint(fx, CLEAR)
    }
  }

  // ===================================================================
  // Blade arc: follows the player through the swing, rotated to the 8-way
  // aim it started with (the sheet's arc bulges down = aiming down).
  // ===================================================================
  const slashFx = getCompByID('fxSlash')
  if (slashFx) {
    if (G.slash) {
      G.slash.t += dt
      const idx = Math.floor(G.slash.t / 0.045)
      if (idx >= 6) {
        G.slash = null
        tint(slashFx, CLEAR)
      } else {
        const dir = G.slash.aim
        slashFx.x = G.px + dir[0] * 11
        slashFx.y = G.py + 7 + dir[1] * 11
        // The arc is drawn swinging down (0, -1); rotating CCW by theta maps
        // that to (sin theta, -cos theta), so theta = atan2(x, -y).
        const angle = Math.round((Math.atan2(dir[0], -dir[1]) * 180) / Math.PI)
        if (slashFx.rotation !== angle) slashFx.rotation = angle
        setImage(slashFx, 'slash ' + idx)
        tint(slashFx, WHITE)
      }
    } else {
      tint(slashFx, CLEAR)
    }
  }

  // ===================================================================
  // Camera: smoothed follow, clamped so the view never leaves the map
  // ===================================================================
  if (cam) {
    const cb = scene.cameraBounds
    const tx = Math.min(Math.max(G.px, cb[0]), cb[2])
    const ty = Math.min(Math.max(G.py + 8, cb[1]), cb[3])
    if (G.camX === null) {
      G.camX = tx
      G.camY = ty
    }
    const t = 1 - Math.exp(-8 * dt)
    G.camX += (tx - G.camX) * t
    G.camY += (ty - G.camY) * t
    // Whole-pixel camera: sub-pixel offsets make Nearest-filtered pixel
    // art shimmer as it moves.
    G.shake = Math.max(0, (G.shake || 0) - realDt * 18)
    const shakeX = G.shake > 0.3 ? Math.round((Math.random() - 0.5) * G.shake * 2) : 0
    const shakeY = G.shake > 0.3 ? Math.round((Math.random() - 0.5) * G.shake * 2) : 0
    cam.x = Math.round(G.camX) + shakeX
    cam.y = Math.round(G.camY) + shakeY
  }

  // ===================================================================
  // Footstep dust, floating numbers, flickering flames
  // ===================================================================
  if (moving && G.rollTime <= 0) {
    G.stepT = (G.stepT || 0) - dt
    if (G.stepT <= 0) {
      G.stepT = 0.22
      burst('fxDust', G.px, G.py + 1, 2)
    }
  }
  for (let i = 0; i < 6; i++) {
    const ref = getCompByID('dmg' + i)
    if (!ref) continue
    const p = G.pops && G.pops[i]
    if (!p) {
      if (ref.x < OFFSCREEN / 2) ref.x = OFFSCREEN
      continue
    }
    p.t += realDt
    if (p.t > 0.9) {
      G.pops[i] = null
      ref.x = OFFSCREEN
      continue
    }
    ref.x = p.x
    ref.y = p.y + p.t * 24
    if (ref.textContent !== p.text) ref.textContent = p.text
    ref.fillColor = [p.color[0], p.color[1], p.color[2], 1 - Math.max(0, (p.t - 0.5) / 0.4)]
  }
  for (const torch of getCompByClass('torch')) {
    const f = 0.82 + Math.sin(G.clock * 13 + torch.x) * 0.08 + Math.random() * 0.12
    torch.fillColor = [1.0 * f, 0.62 * f, 0.28 * f, 1]
  }
  const lantern = getCompByID('lantern')
  if (lantern) {
    const f = 0.95 + Math.sin(G.clock * 9) * 0.03 + Math.random() * 0.04
    lantern.fillColor = [1.0 * f, 0.86 * f, 0.62 * f, 1]
  }

  // ===================================================================
  // HUD
  // ===================================================================
  for (let i = 0; i < 3; i++) {
    const heart = getCompByID('heart' + i)
    // Heart.png frames: 0 empty, 2 half, 4 full.
    if (heart) setImage(heart, 'heart ' + Math.min(Math.max(G.hp - i * 2, 0), 2) * 2)
  }
  const open = !!G.dialog
  for (const id of ['dlgPanel', 'dlgFaceBox', 'dlgFace']) {
    const ref = getCompByID(id)
    if (ref) tint(ref, open ? WHITE : CLEAR)
  }
  const face = getCompByID('dlgFace')
  if (open && face) setImage(face, NPC_INFO[G.dialog.npc].face)
  const page = open ? G.dialog.pages[G.dialog.page] : ['', '']
  setText('dlgName', open ? NPC_INFO[G.dialog.npc].name : '')
  setText('dlgLine1', page[0])
  setText('dlgLine2', page[1])
}

G.messageTime -= dt
setText('hudCoins', String(G.coins))
setText('hudGems', String(G.gems))
setText('hudMsg', G.messageTime > 0 && !G.dialog ? G.message : '')
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_two_scenes_each_with_a_main_camera_and_a_code_state_machine() {
        let mut document = Document::new();
        let demo = build_demo_ninja_adventure(&mut document);
        let names: Vec<&str> = document.artboards().map(|(_, a)| a.name.as_str()).collect();
        assert!(names.contains(&"Village") && names.contains(&"House"));
        for (_, artboard) in document.artboards() {
            assert!(artboard.active_camera().is_some(), "{} needs a Main Camera", artboard.name);
            let (_, machine) = artboard.state_machines().next().expect("each scene runs the game script");
            assert!(matches!(machine.states[0].program, Some(ProgramDef::Code(_))));
        }
        assert!(document.artboard(demo.village).is_some());
        // walk 16, idle 16, hit 8, roll 12, coin 4, chest 2, attack 16, oldman 28, kid 8, slime 16,
        // skull 16, skeleton 28, heart 5, smoke 6, slash 6, plus 10 single images.
        assert_eq!(demo.images.len(), 16 + 16 + 8 + 12 + 4 + 2 + 16 + 28 + 8 + 16 + 16 + 28 + 5 + 6 + 6 + 10);
    }

    #[test]
    fn generated_script_carries_both_scenes_data() {
        let mut document = Document::new();
        build_demo_ninja_adventure(&mut document);
        let (_, village) = document.artboards().find(|(_, a)| a.name == "Village").unwrap();
        let (_, machine) = village.state_machines().next().unwrap();
        let Some(ProgramDef::Code(source)) = &machine.states[0].program else { panic!("expected Code") };
        assert!(source.contains("Village: {") && source.contains("House: {"));
        assert!(source.contains("to:'House'") && source.contains("to:'Village'"));
    }

    #[test]
    fn undo_removes_both_scenes() {
        let mut document = Document::new();
        let before = document.artboards().count();
        build_demo_ninja_adventure(&mut document);
        assert_eq!(document.artboards().count(), before + 2);
        document.undo();
        assert_eq!(document.artboards().count(), before);
    }
}
