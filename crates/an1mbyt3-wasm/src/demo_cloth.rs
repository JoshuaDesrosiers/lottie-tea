//! "Cloth Lab" — the Cloth 2D component in action.
//!
//! - A **flag** on a pole (pinned along its left edge) streaming in gusty
//!   wind.
//! - A heavy velvet **curtain** on a rod (top edge) — grab it and fling it.
//! - A checkered **banner** hung by its two top corners, sagging.
//! - A pale **sheet** that tears: right-drag (or Shift + drag) slices
//!   through it, and yanking it too hard rips it.
//! - A **cape** on a little hero zipping about — pinned points follow
//!   their node, so the cape trails behind.
//!
//! Drag any cloth to pull it; 1 / 2 / 3 = calm / breeze / storm; R mends
//! everything.

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Cloth2D, ClothPatternKind, ClothPin, Command, Document, FontAsset, LoopMode, Node, NodeBehavior, NodeKind, Path, ProgramDef, ShapeNode, StateDef,
    StateMachineDef, Stroke,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");
const SCREEN_W: f32 = 960.0;
const SCREEN_H: f32 = 540.0;
const INK: [f32; 4] = [0.1, 0.08, 0.14, 1.0];

pub(crate) struct ClothDemo {
    pub scene: ArtboardId,
}

fn shape(name: &str, path: Path, color: [f32; 4], at: Vec2, outline: Option<f32>) -> Node {
    let stroke = outline.map(|width| Stroke { width, color: INK, gradient: None, image: None, cap: Default::default(), join: Default::default() });
    let mut node = Node::new(name, None, NodeKind::Shape(ShapeNode { path, fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke }));
    node.transform.translation = at;
    node
}

fn cloth(id: &str, at: Vec2, config: Cloth2D) -> Node {
    let mut node = with_id(Node::with_components(id, None, vec![NodeBehavior::Cloth2D(config)]), id);
    node.transform.translation = at;
    node
}

pub(crate) fn build_demo_cloth(document: &mut Document) -> ClothDemo {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: FONT_BYTES.to_vec() });
    let inverse = document.apply(Command::AddArtboard { name: "Cloth Lab".into(), width: SCREEN_W, height: SCREEN_H });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    let camera = with_id(
        Node::with_components("Main Camera", None, vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.2, 0.22, 0.36, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })]),
        "camera",
    );
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));

    // ---- backdrop: dusk sky bands, a moon, the floor ----
    let hh = SCREEN_H / 2.0;
    for (i, c) in [[0.24, 0.26, 0.42, 1.0], [0.3, 0.29, 0.46, 1.0], [0.4, 0.32, 0.48, 1.0], [0.56, 0.38, 0.48, 1.0]].into_iter().enumerate() {
        add_node(document, &mut inverses, scene, shape("Sky", Path::rect(SCREEN_W, 70.0), c, Vec2::new(0.0, -40.0 - i as f32 * 60.0), None));
    }
    add_node(document, &mut inverses, scene, shape("Moon", Path::ellipse(70.0, 70.0), [1.0, 0.95, 0.8, 1.0], Vec2::new(150.0, 190.0), Some(4.0)));
    add_node(document, &mut inverses, scene, shape("Floor", Path::rect(SCREEN_W + 40.0, 90.0), [0.18, 0.15, 0.24, 1.0], Vec2::new(0.0, -hh + 30.0), Some(4.0)));

    // ---- the flag: a pole, the flag pinned along its left edge ----
    add_node(document, &mut inverses, scene, shape("Pole", Path::rect(10.0, 370.0), [0.78, 0.74, 0.7, 1.0], Vec2::new(-370.0, -40.0), Some(3.0)));
    add_node(document, &mut inverses, scene, shape("Pole Cap", Path::ellipse(20.0, 20.0), [1.0, 0.82, 0.3, 1.0], Vec2::new(-370.0, 148.0), Some(3.0)));
    add_node(
        document,
        &mut inverses,
        scene,
        cloth(
            "flag",
            Vec2::new(-365.0, 92.0),
            Cloth2D {
                width: 170.0,
                height: 100.0,
                columns: 20,
                rows: 12,
                pin: ClothPin::LeftEdge,
                color: [0.15, 0.65, 0.65, 1.0],
                pattern: ClothPatternKind::HorizontalStripes,
                pattern_color: [0.98, 0.92, 0.78, 1.0],
                pattern_count: 5,
                gravity: 260.0,
                wind: Vec2::new(900.0, 40.0),
                turbulence: 1.0,
                stiffness: 6,
                damping: 0.5,
                shading: 0.8,
                ..Default::default()
            },
        ),
    );

    // ---- the curtain on a rod ----
    add_node(
        document,
        &mut inverses,
        scene,
        cloth(
            "curtain",
            Vec2::new(-60.0, 70.0),
            Cloth2D {
                width: 250.0,
                height: 230.0,
                columns: 24,
                rows: 20,
                pin: ClothPin::TopEdge,
                color: [0.62, 0.08, 0.16, 1.0],
                pattern: ClothPatternKind::VerticalStripes,
                pattern_color: [0.52, 0.06, 0.13, 1.0],
                pattern_count: 14,
                gravity: 800.0,
                wind: Vec2::new(180.0, 0.0),
                turbulence: 0.8,
                stiffness: 8,
                damping: 1.2,
                shading: 1.0,
                ..Default::default()
            },
        ),
    );
    add_node(document, &mut inverses, scene, shape("Rod", Path::rect(290.0, 10.0), [0.85, 0.66, 0.3, 1.0], Vec2::new(-60.0, 186.0), Some(3.0)));
    for x in [-210.0f32, 90.0] {
        add_node(document, &mut inverses, scene, shape("Rod End", Path::ellipse(18.0, 18.0), [0.85, 0.66, 0.3, 1.0], Vec2::new(x, 186.0), Some(3.0)));
    }

    // ---- the banner, hung by its corners ----
    for x in [215.0f32, 395.0] {
        add_node(document, &mut inverses, scene, shape("Hook", Path::ellipse(12.0, 12.0), [0.8, 0.8, 0.85, 1.0], Vec2::new(x, 150.0), Some(3.0)));
    }
    add_node(
        document,
        &mut inverses,
        scene,
        cloth(
            "banner",
            Vec2::new(305.0, 110.0),
            Cloth2D {
                width: 180.0,
                height: 80.0,
                columns: 18,
                rows: 8,
                pin: ClothPin::TopCorners,
                color: [0.98, 0.84, 0.25, 1.0],
                pattern: ClothPatternKind::Checker,
                pattern_color: [0.15, 0.13, 0.2, 1.0],
                pattern_count: 6,
                gravity: 500.0,
                wind: Vec2::new(220.0, 30.0),
                turbulence: 0.9,
                stiffness: 7,
                damping: 0.8,
                shading: 0.8,
                ..Default::default()
            },
        ),
    );

    // ---- the tearable sheet ----
    add_node(document, &mut inverses, scene, shape("Line", Path::rect(200.0, 6.0), [0.75, 0.75, 0.8, 1.0], Vec2::new(305.0, -10.0), Some(2.0)));
    add_node(
        document,
        &mut inverses,
        scene,
        cloth(
            "sheet",
            Vec2::new(305.0, -80.0),
            Cloth2D {
                width: 170.0,
                height: 130.0,
                columns: 26,
                rows: 20,
                pin: ClothPin::TopEdge,
                color: [0.82, 0.9, 1.0, 1.0],
                pattern: ClothPatternKind::HorizontalStripes,
                pattern_color: [0.6, 0.75, 0.95, 1.0],
                pattern_count: 8,
                gravity: 600.0,
                wind: Vec2::new(160.0, 0.0),
                turbulence: 0.8,
                stiffness: 5,
                damping: 0.7,
                tear_stretch: 1.9,
                shading: 0.8,
                ..Default::default()
            },
        ),
    );
    add_node(document, &mut inverses, scene, text_node(None, "sliceLabel", "SLICE ME", font, 8.0, [0.85, 0.85, 0.95, 1.0], Vec2::new(305.0, -170.0)));

    // ---- the caped hero (the cape behind, then the hero on top) ----
    add_node(
        document,
        &mut inverses,
        scene,
        cloth(
            "cape",
            Vec2::new(-200.0, -120.0),
            Cloth2D {
                width: 46.0,
                height: 80.0,
                columns: 8,
                rows: 12,
                pin: ClothPin::TopEdge,
                color: [0.55, 0.25, 0.85, 1.0],
                gravity: 500.0,
                wind: Vec2::ZERO,
                turbulence: 0.2,
                stiffness: 6,
                damping: 0.6,
                shading: 0.9,
                ..Default::default()
            },
        ),
    );
    let hero = add_node(document, &mut inverses, scene, with_id(shape("hero", Path::ellipse(40.0, 40.0), [1.0, 0.85, 0.65, 1.0], Vec2::new(-200.0, -110.0), Some(3.0)), "hero"));
    for x in [-7.0f32, 7.0] {
        let mut eye = shape("Eye", Path::ellipse(6.0, 9.0), INK, Vec2::new(x, 3.0), None);
        eye.parent = Some(hero);
        add_node(document, &mut inverses, scene, eye);
    }
    let mut mask = shape("Mask", Path::rect(34.0, 8.0), [0.55, 0.25, 0.85, 1.0], Vec2::new(0.0, 4.0), None);
    mask.parent = Some(hero);
    add_node(document, &mut inverses, scene, mask);

    // ---- HUD ----
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "title", "CLOTH LAB", font, 18.0, [1.0, 0.9, 0.6, 1.0], Vec2::new(0.0, hh - 96.0)));
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "hint", "DRAG CLOTH  -  RIGHT-DRAG SLICES  -  1 2 3 WIND  -  R MEND", font, 8.0, [0.9, 0.88, 1.0, 1.0], Vec2::new(0.0, -hh + 22.0)));
    add_node(document, &mut inverses, scene, text_node(Some(camera_id), "windLabel", "WIND: BREEZE", font, 8.0, [0.7, 0.95, 1.0, 1.0], Vec2::new(0.0, hh - 122.0)));

    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Cloth Lab (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Cloth Lab".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Play".into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(SCRIPT.into())),
            }],
            initial_state: "Play".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));
    inverses.reverse();
    document.commit_batch(inverses);
    ClothDemo { scene }
}

const SCRIPT: &str = r#"// Cloth Lab — grab, fling and slice Cloth 2D components (see demo_cloth.rs).
const ref = (id) => getCompByID(id)
const keys = ctx.keysDown
const dt = Math.min(ctx.deltaSeconds || 1 / 60, 1 / 20)
// Front-most first, so a click takes the cloth on top.
const CLOTHS = ['cape', 'sheet', 'banner', 'curtain', 'flag']
// Wind presets: calm (still air), breeze (each cloth's own wind), storm.
const STORM = { flag: [1800, 160], curtain: [700, 80], banner: [900, 120], sheet: [900, 100], cape: [500, 60] }
const g = ctx.globals.clothLab || (ctx.globals.clothLab = { held: null, prev: {}, last: null, mode: 'breeze', applied: 'breeze', t: 0 })
const pressed = (k) => keys.has(k) && !g.prev[k]
const mx = ctx.mouseX, my = ctx.mouseY

// ---- grab and fling ----
const slicing = keys.has('mouseright') || (keys.has('mouseleft') && keys.has('shift'))
if (pressed('mouseleft') && !keys.has('shift')) {
  for (const id of CLOTHS) {
    if (ref(id).grabCloth(mx, my, 34)) { g.held = id; break }
  }
}
if (g.held) {
  if (keys.has('mouseleft')) ref(g.held).dragCloth(mx, my)
  else { ref(g.held).releaseCloth(); g.held = null }
}

// ---- slice: cut along the mouse's path since last frame ----
if (slicing) {
  const from = g.last || { x: mx, y: my }
  const steps = Math.max(1, Math.ceil(Math.hypot(mx - from.x, my - from.y) / 5))
  let cut = 0
  for (let s = 1; s <= steps; s++) {
    const x = from.x + ((mx - from.x) * s) / steps, y = from.y + ((my - from.y) * s) / steps
    for (const id of CLOTHS) cut += ref(id).tearCloth(x, y, 7)
  }
  if (cut) ctx.playTone({ freq: 2400 + Math.random() * 800, to: 900, duration: 0.05, type: 'noise', volume: 0.07 })
  g.last = { x: mx, y: my }
} else {
  g.last = null
}

// ---- wind presets and mending ----
const mode = pressed('1') ? 'calm' : pressed('2') ? 'breeze' : pressed('3') ? 'storm' : g.mode
if (mode !== g.mode && mode === 'storm') ctx.playTone({ freq: 140, to: 60, duration: 1.2, type: 'noise', volume: 0.12 })
g.mode = mode
// (Re)apply when it changed, or after mending (a fresh cloth forgets it).
if (g.applied !== mode) {
  g.applied = mode
  for (const id of CLOTHS) {
    if (mode === 'calm') ref(id).setClothWind(0, 0)
    else if (mode === 'storm') ref(id).setClothWind(STORM[id][0], STORM[id][1])
    else ref(id).setClothWind(null)
  }
  ref('windLabel').textContent = 'WIND: ' + mode.toUpperCase()
}
if (pressed('r')) {
  for (const id of CLOTHS) ref(id).resetCloth()
  g.held = null
  // Mended cloth takes the current wind again next frame.
  g.applied = null
}

// ---- the caped hero zips about (the cape's pinned edge follows) ----
g.t += dt
const hx = -200 + Math.sin(g.t * 1.1) * 130, hy = -120 + Math.sin(g.t * 2.2) * 45
const hero = ref('hero'), cape = ref('cape')
hero.x = hx; hero.y = hy
hero.rotation = Math.cos(g.t * 1.1) * -10
cape.x = hx; cape.y = hy - 6

for (const k of ['mouseleft', '1', '2', '3', 'r']) g.prev[k] = keys.has(k)
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_every_cloth_the_script_drives() {
        let mut document = Document::new();
        let demo = build_demo_cloth(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let cloths: Vec<&str> = artboard.node_ids().filter_map(|id| artboard.node(id)).filter(|n| n.cloth().is_some()).map(|n| n.id.as_str()).collect();
        for id in ["flag", "curtain", "banner", "sheet", "cape"] {
            assert!(cloths.contains(&id), "missing {id}");
            assert!(SCRIPT.contains(&format!("'{id}'")));
        }
    }
}
