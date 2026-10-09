//! "Tree Garden" — the Tree 2D component: one of every species on a
//! grassy hill, growing in from saplings when play starts, swaying in the
//! breeze and dropping leaves. Click a tree to shake it; Space shakes them
//! all; R grows them again.

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Command, Document, FontAsset, LoopMode, Node, NodeBehavior, NodeKind, Path, ProgramDef, ShapeNode, StateDef, StateMachineDef, Stroke, Tree2D,
    TreeSpecies,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");
const SCREEN_W: f32 = 960.0;
const SCREEN_H: f32 = 540.0;
const INK: [f32; 4] = [0.14, 0.12, 0.1, 1.0];
/// Where the trees stand (their feet), and the ground line.
const GROUND_Y: f32 = -170.0;

pub(crate) struct TreesDemo {
    pub scene: ArtboardId,
}

fn shape(name: &str, path: Path, color: [f32; 4], at: Vec2, outline: Option<f32>) -> Node {
    let stroke = outline.map(|width| Stroke { width, color: INK, gradient: None, image: None, cap: Default::default(), join: Default::default() });
    let mut node = Node::new(name, None, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke }));
    node.transform.translation = at;
    node
}

/// The trees: species, x, height, seed, grow time.
const TREES: [(TreeSpecies, f32, f32, u32, f32); 6] = [
    (TreeSpecies::Pine, -385.0, 230.0, 4, 2.6),
    (TreeSpecies::Oak, -235.0, 175.0, 11, 3.2),
    (TreeSpecies::Sakura, -80.0, 175.0, 5, 3.6),
    (TreeSpecies::Birch, 60.0, 180.0, 9, 3.0),
    (TreeSpecies::Willow, 215.0, 185.0, 2, 3.4),
    (TreeSpecies::Palm, 375.0, 215.0, 6, 2.8),
];

pub(crate) fn build_demo_trees(document: &mut Document) -> TreesDemo {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: FONT_BYTES.to_vec() });
    let inverse = document.apply(Command::AddArtboard { name: "Tree Garden".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    let camera = with_id(
        Node::with_components("Main Camera", None, vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.62, 0.84, 0.95, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })]),
        "camera",
    );
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));

    // ---- backdrop: sun, far hills, the meadow ----
    let hh = SCREEN_H / 2.0;
    add_node(document, &mut inverses, scene, shape("Sun", Path::ellipse(90.0, 90.0), [1.0, 0.93, 0.6, 1.0], Vec2::new(-300.0, 170.0), Some(4.0)));
    for (x, w, h, c) in [(-260.0f32, 620.0f32, 260.0f32, [0.56, 0.78, 0.62, 1.0]), (300.0, 700.0, 220.0, [0.5, 0.74, 0.58, 1.0])] {
        add_node(document, &mut inverses, scene, shape("Far Hill", Path::ellipse(w, h), c, Vec2::new(x, GROUND_Y + 10.0), Some(3.0)));
    }
    add_node(document, &mut inverses, scene, shape("Meadow", Path::rect(SCREEN_W + 40.0, hh + GROUND_Y + 40.0), [0.46, 0.72, 0.32, 1.0], Vec2::new(0.0, (GROUND_Y - hh) / 2.0), Some(4.0)));
    for i in 0..14 {
        let x = -440.0 + i as f32 * 66.0;
        add_node(document, &mut inverses, scene, shape("Tuft", Path::star(5, 9.0, 4.0), [0.36, 0.6, 0.26, 1.0], Vec2::new(x, GROUND_Y - 30.0 - (i % 3) as f32 * 22.0), None));
    }

    // ---- the trees (each on a little shadow) ----
    for (i, (species, x, height, seed, grow_time)) in TREES.into_iter().enumerate() {
        add_node(document, &mut inverses, scene, shape("Shadow", Path::ellipse(height * 0.55, 16.0), [0.32, 0.55, 0.24, 1.0], Vec2::new(x, GROUND_Y - 2.0), None));
        let tree = Tree2D { wind: 0.25, wind_sway: 1.1, falling_leaves: if species == TreeSpecies::Pine { 0.0 } else { 0.4 }, grow_on_play: true, grow_time, ..Tree2D::of(species, seed, height) };
        let id = format!("tree{i}");
        let mut node = with_id(Node::with_components(&id, None, vec![NodeBehavior::Tree2D(tree)]), &id);
        node.transform.translation = Vec2::new(x, GROUND_Y);
        add_node(document, &mut inverses, scene, node);
        add_node(document, &mut inverses, scene, text_node(None, &format!("label{i}"), &format!("{species:?}").to_uppercase(), font, 7.0, [0.2, 0.32, 0.16, 1.0], Vec2::new(x, GROUND_Y - 22.0)));
    }

    // ---- HUD ----
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "title", "TREE GARDEN", font, 18.0, [0.2, 0.32, 0.16, 1.0], Vec2::new(120.0, hh - 96.0)));
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "hint", "CLICK A TREE TO SHAKE IT  -  SPACE SHAKES ALL  -  R REGROWS", font, 8.0, [0.98, 0.98, 0.9, 1.0], Vec2::new(0.0, -hh + 20.0)));

    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Tree Garden (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    let xs = TREES.iter().map(|t| format!("[{}, {}]", t.1, t.2)).collect::<Vec<_>>().join(", ");
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Tree Garden".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Play".into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(format!("const TREES = [{xs}], GROUND_Y = {GROUND_Y}\n{SCRIPT}"))),
            }],
            initial_state: "Play".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));
    inverses.reverse();
    document.commit_batch(inverses);
    TreesDemo { scene }
}

const SCRIPT: &str = r#"// Tree Garden — shake trees (`ref.shakeTree`), regrow them (`ref.regrowTree`).
// TREES = [[x, height], …] (from Rust), feet on GROUND_Y.
const ref = (id) => getCompByID(id)
const keys = ctx.keysDown
const g = ctx.globals.garden || (ctx.globals.garden = { prev: {} })
const pressed = (k) => keys.has(k) && !g.prev[k]
const rustle = (strength) => ctx.playTone({ freq: 900 + Math.random() * 400, to: 300, duration: 0.25 + strength * 0.15, type: 'noise', volume: 0.05 + strength * 0.04 })

// Click: shake the tree under the cursor (within its canopy's reach).
if (pressed('mouse')) {
  let best = -1, bestD = Infinity
  TREES.forEach(([x, h], i) => {
    const dx = Math.abs(ctx.mouseX - x), dy = ctx.mouseY - GROUND_Y
    if (dx < h * 0.45 && dy > -10 && dy < h * 1.1 && dx < bestD) { best = i; bestD = dx }
  })
  if (best >= 0) { ref('tree' + best).shakeTree(1.4); rustle(1) }
}
if (pressed('space')) {
  TREES.forEach((_, i) => ref('tree' + i).shakeTree(2))
  rustle(2)
}
if (pressed('r')) TREES.forEach((_, i) => ref('tree' + i).regrowTree())
for (const k of ['mouse', 'space', 'r']) g.prev[k] = keys.has(k)
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plants_one_of_every_species() {
        let mut document = Document::new();
        let demo = build_demo_trees(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let trees: Vec<TreeSpecies> = artboard.node_ids().filter_map(|id| artboard.node(id)?.tree().map(|t| t.species)).collect();
        assert_eq!(trees.len(), 6);
        for s in [TreeSpecies::Oak, TreeSpecies::Pine, TreeSpecies::Birch, TreeSpecies::Sakura, TreeSpecies::Palm, TreeSpecies::Willow] {
            assert!(trees.contains(&s));
        }
    }
}
