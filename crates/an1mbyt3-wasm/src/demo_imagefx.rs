//! **Image FX Lab** — a showcase scene for the Image Effects component:
//! six panels of vector art (sky, sun, hills, a figure, type), each a group
//! with its own effect stack, and one looping animation that keyframes the
//! effects' values — so it shows both the looks and that every effect
//! value animates like any other property.
//!
//! | Panel | Effects (animated values) |
//! |---|---|
//! | Ribbed Glass | ribbed glass (angle, wave) |
//! | ASCII | ASCII, mono (cell size) |
//! | Pattern | checker pattern (scale, power) + progressive blur |
//! | Glow | glow (intensity) + grain |
//! | Slit Scan | slit scan (timing) |
//! | Code | the Code effect's ripple template (its own `time`) + gradient map |

use an1mbyt3_core::image_effects::{Ascii, Blur, BlurMode, CodeEffect, Glow, GradientMap, Grain, Pattern, RibbedGlass, ScannerDrag, Stroke as StrokeFx, StrokePosition};
use an1mbyt3_core::{Document, FontAsset, FontId, GradientDef, GradientKind, GradientStop, ImageEffect, ImageEffectLayer, ImageEffects, InputDef, Node, NodeBehavior, NodeId, NodeKind, Path, PathCommand, ShapeNode, Stroke, StrokeCap, StrokeJoin};
use glam::Vec2;

use crate::demo_lite::{circle_cw, round_rect, st_loop, v, Anim, Kit, LiteDemo, E};
use crate::demo_ninja::{text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/RussoOne-Regular.ttf");

const W: f32 = 380.0;
const H: f32 = 280.0;

fn poly(points: &[(f32, f32)]) -> Path {
    let mut commands = vec![PathCommand::MoveTo(v(points[0].0, points[0].1))];
    commands.extend(points[1..].iter().map(|p| PathCommand::LineTo(v(p.0, p.1))));
    commands.push(PathCommand::Close);
    Path { commands }
}

fn rgb(hex: u32) -> [f32; 4] {
    [((hex >> 16) & 255) as f32 / 255.0, ((hex >> 8) & 255) as f32 / 255.0, (hex & 255) as f32 / 255.0, 1.0]
}

/// A panel: a group carrying `effects`, at `at`.
fn panel(kit: &mut Kit, id: &str, at: Vec2, effects: Vec<ImageEffect>) -> NodeId {
    // Clipped to the panel's rounded shape: glass, drag and glow stay inside it.
    let fx = ImageEffects { effects: effects.into_iter().map(|effect| ImageEffectLayer { effect, enabled: true }).collect(), clip_to_shape: true, ..Default::default() };
    let mut node = with_id(Node::with_components(id, None, vec![NodeBehavior::ImageEffects(fx)]), id);
    node.transform.translation = at;
    kit.add(node)
}

fn shape(kit: &mut Kit, id: &str, parent: NodeId, path: Path, color: [f32; 4], at: Vec2) -> NodeId {
    kit.fill(id, Some(parent), path, color, at)
}

fn stroked(kit: &mut Kit, id: &str, parent: NodeId, path: Path, color: [f32; 4], width: f32, at: Vec2) -> NodeId {
    let mut node = with_id(
        Node::new(id, Some(parent), NodeKind::Shape(ShapeNode { path, fill_color: [0.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: Some(Stroke { width, color, gradient: None, image: None, cap: StrokeCap::Round, join: StrokeJoin::Round }) })),
        id,
    );
    node.transform.translation = at;
    kit.add(node)
}

/// A gradient: `stops` as `(offset, colour)`.
fn grad(kind: GradientKind, start: Vec2, end: Vec2, stops: &[(f32, [f32; 4])]) -> GradientDef {
    GradientDef { kind, start, end, stops: stops.iter().map(|(offset, color)| GradientStop { offset: *offset, color: *color, midpoint: 0.5 }).collect() }
}

/// A shape filled with a gradient.
fn gshape(kit: &mut Kit, id: &str, parent: NodeId, path: Path, gradient: GradientDef, at: Vec2) -> NodeId {
    let mut node = with_id(Node::new(id, Some(parent), NodeKind::Shape(ShapeNode { path, fill_color: [1.0; 4], blend_mode: Default::default(), gradient: Some(gradient), image: None, stroke: None })), id);
    node.transform.translation = at;
    kit.add(node)
}

fn alpha(c: [f32; 4], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

/// A lit landscape — graded sky, a glowing sun, layered hills fading into
/// haze, a water band with the sun's reflection — the shared "photo".
fn landscape(kit: &mut Kit, p: NodeId, tag: &str, top: u32, horizon: u32, sun: u32, far: u32, near: u32) {
    let (top, horizon, sun_c, far, near) = (rgb(top), rgb(horizon), rgb(sun), rgb(far), rgb(near));
    gshape(kit, &format!("{tag}_sky"), p, round_rect(W, H, 14.0), grad(GradientKind::Linear, v(0.0, 140.0), v(0.0, -40.0), &[(0.0, top), (1.0, horizon)]), v(0.0, 0.0));
    // (Panel-sized, so the glow stays inside the panel.)
    gshape(kit, &format!("{tag}_halo"), p, round_rect(W, H, 14.0), grad(GradientKind::Radial, v(60.0, 18.0), v(210.0, 18.0), &[(0.0, alpha(sun_c, 0.55)), (0.35, alpha(sun_c, 0.18)), (1.0, alpha(sun_c, 0.0))]), v(0.0, 0.0));
    gshape(kit, &format!("{tag}_sun"), p, circle_cw(42.0), grad(GradientKind::Radial, v(-8.0, 10.0), v(42.0, 0.0), &[(0.0, [1.0, 1.0, 0.96, 1.0]), (0.6, sun_c), (1.0, sun_c)]), v(60.0, 18.0));
    let hazy = |c: [f32; 4]| [c[0] * 0.6 + horizon[0] * 0.4, c[1] * 0.6 + horizon[1] * 0.4, c[2] * 0.6 + horizon[2] * 0.4, 1.0];
    gshape(
        kit,
        &format!("{tag}_far"),
        p,
        poly(&[(-190.0, -10.0), (-140.0, 22.0), (-95.0, 6.0), (-40.0, 40.0), (10.0, 12.0), (70.0, 34.0), (120.0, 4.0), (190.0, 24.0), (190.0, -50.0), (-190.0, -50.0)]),
        grad(GradientKind::Linear, v(0.0, 40.0), v(0.0, -50.0), &[(0.0, hazy(far)), (1.0, far)]),
        v(0.0, 0.0),
    );
    gshape(
        kit,
        &format!("{tag}_near"),
        p,
        poly(&[(-190.0, -38.0), (-120.0, -18.0), (-60.0, -44.0), (0.0, -26.0), (80.0, -50.0), (190.0, -30.0), (190.0, -60.0), (-190.0, -60.0)]),
        grad(GradientKind::Linear, v(0.0, -18.0), v(0.0, -60.0), &[(0.0, near), (1.0, [near[0] * 0.6, near[1] * 0.6, near[2] * 0.6, 1.0])]),
        v(0.0, 0.0),
    );
    // Water: the sky mirrored, darker, with a glittering sun path.
    gshape(
        kit,
        &format!("{tag}_water"),
        p,
        poly(&[(-190.0, -60.0), (190.0, -60.0), (190.0, -126.0), (176.0, -140.0), (-176.0, -140.0), (-190.0, -126.0)]),
        grad(GradientKind::Linear, v(0.0, -60.0), v(0.0, -140.0), &[(0.0, [horizon[0] * 0.75, horizon[1] * 0.75, horizon[2] * 0.75, 1.0]), (1.0, [top[0] * 0.5, top[1] * 0.5, top[2] * 0.5, 1.0])]),
        v(0.0, 0.0),
    );
    for i in 0..6 {
        let y = -68.0 - i as f32 * 11.0;
        let wdt = 70.0 - i as f32 * 9.0;
        gshape(
            kit,
            &format!("{tag}_glint{i}"),
            p,
            round_rect(wdt, 3.0, 1.5),
            grad(GradientKind::Linear, v(-wdt / 2.0, 0.0), v(wdt / 2.0, 0.0), &[(0.0, alpha(sun_c, 0.0)), (0.5, alpha(sun_c, 0.85 - i as f32 * 0.12)), (1.0, alpha(sun_c, 0.0))]),
            v(60.0 + (i as f32 * 1.7).sin() * 6.0, y),
        );
    }
}

/// A figure (head, torso, legs), side-lit with gradients.
fn figure(kit: &mut Kit, p: NodeId, tag: &str, at: Vec2, skin: u32, shirt: u32, pants: u32) {
    let lit = |c: [f32; 4], k: f32| [(c[0] * k).min(1.0), (c[1] * k).min(1.0), (c[2] * k).min(1.0), 1.0];
    let side = |c: u32, w: f32| {
        let c = rgb(c);
        grad(GradientKind::Linear, v(-w / 2.0, 0.0), v(w / 2.0, 0.0), &[(0.0, lit(c, 0.6)), (0.65, lit(c, 1.05)), (1.0, lit(c, 1.25))])
    };
    gshape(kit, &format!("{tag}_legs"), p, round_rect(46.0, 70.0, 10.0), side(pants, 46.0), at + v(0.0, -60.0));
    gshape(kit, &format!("{tag}_torso"), p, round_rect(70.0, 86.0, 24.0), side(shirt, 70.0), at + v(0.0, 10.0));
    gshape(kit, &format!("{tag}_head"), p, circle_cw(26.0), side(skin, 52.0), at + v(0.0, 78.0));
}

/// A dark panel background with a soft coloured light in it.
fn backdrop(kit: &mut Kit, p: NodeId, tag: &str, base: u32, light: u32, at: Vec2) {
    shape(kit, &format!("{tag}_bg"), p, round_rect(W, H, 14.0), rgb(base), v(0.0, 0.0));
    gshape(kit, &format!("{tag}_light"), p, round_rect(W, H, 14.0), grad(GradientKind::Radial, at, at + v(220.0, 0.0), &[(0.0, alpha(rgb(light), 0.55)), (1.0, alpha(rgb(light), 0.0))]), v(0.0, 0.0));
}

fn label(kit: &mut Kit, font: FontId, id: &str, text: &str, at: Vec2) {
    let node = text_node(None, id, text, font, 18.0, [0.85, 0.86, 0.9, 1.0], at);
    kit.add(node);
}

pub(crate) fn build_demo_imagefx(document: &mut Document) -> LiteDemo {
    let font = document.add_font(FontAsset { name: "Russo One".into(), data: FONT_BYTES.to_vec() });
    let mut kit = Kit::new(document, "Image FX Lab", 1280.0, 720.0);
    let cols = [-420.0, 0.0, 420.0];
    let rows = [170.0, -165.0];
    let fx_path = |i: usize, kind: &str, field: &str| format!("behavior/ImageEffects/effects/{i}/effect/{kind}/{field}");

    // 1. Ribbed glass over a sunset.
    let glass = panel(&mut kit, "ribbed", v(cols[0], rows[0]), vec![ImageEffect::RibbedGlass(RibbedGlass { size: 40.0, distortion: 0.55, echo: 0.12, curve: 0.85, wave: 0.35, wavelength: 300.0, ..Default::default() })]);
    landscape(&mut kit, glass, "rg", 0x2a1458, 0xff8a4c, 0xffd27a, 0x8a3a6e, 0x2c1238);
    kit.add(text_node(Some(glass), "rg_word", "RIBBED", font, 54.0, rgb(0xffffff), v(-60.0, -100.0)));
    label(&mut kit, font, "label_ribbed", "Ribbed Glass", v(cols[0], rows[0] - 160.0));

    // 2. ASCII portrait.
    let ascii = panel(&mut kit, "ascii", v(cols[1], rows[0]), vec![ImageEffect::Ascii(Ascii { size: 12.0, contrast: 0.25, background: [0.035, 0.045, 0.075, 1.0], ink: [0.82, 0.9, 1.0, 1.0], ..Default::default() })]);
    backdrop(&mut kit, ascii, "as", 0x07080c, 0x8fa8ff, v(0.0, 30.0));
    figure(&mut kit, ascii, "as_fig", v(0.0, -30.0), 0xf3e2d0, 0xffffff, 0x9aa3b5);
    label(&mut kit, font, "label_ascii", "ASCII", v(cols[1], rows[0] - 160.0));

    // 3. Pattern + progressive blur: type and a figure.
    let pattern = panel(
        &mut kit,
        "pattern",
        v(cols[2], rows[0]),
        vec![ImageEffect::Pattern(Pattern { scale: 16.0, ..Default::default() }), ImageEffect::Blur(Blur { mode: BlurMode::Progressive, radius: 10.0, start: [0.5, 0.45], end: [0.5, 1.0], ..Default::default() })],
    );
    gshape(&mut kit, "pt_bg", pattern, round_rect(W, H, 14.0), grad(GradientKind::Radial, v(60.0, 40.0), v(300.0, 0.0), &[(0.0, rgb(0xffffff)), (1.0, rgb(0x9a9a9a))]), v(0.0, 0.0));
    kit.add(text_node(Some(pattern), "pt_top", "toasty", font, 64.0, rgb(0x0b0b0b), v(-40.0, 70.0)));
    kit.add(text_node(Some(pattern), "pt_bottom", "studio", font, 64.0, rgb(0x0b0b0b), v(-30.0, -100.0)));
    figure(&mut kit, pattern, "pt_fig", v(110.0, -20.0), 0x6b4a3a, 0x2b2b2b, 0x111111);
    label(&mut kit, font, "label_pattern", "Pattern + Progressive Blur", v(cols[2], rows[0] - 160.0));

    // 4. Neon glow + grain.
    let glow = panel(
        &mut kit,
        "glow",
        v(cols[0], rows[1]),
        vec![ImageEffect::Glow(Glow { intensity: 1.6, radius: 22.0, threshold: 0.35, ..Default::default() }), ImageEffect::Grain(Grain { amount: 0.12, ..Default::default() })],
    );
    backdrop(&mut kit, glow, "gl", 0x06040c, 0x3a1060, v(0.0, 0.0));
    stroked(&mut kit, "gl_ring", glow, circle_cw(70.0), rgb(0xff3df2), 9.0, v(-70.0, 10.0));
    stroked(&mut kit, "gl_tri", glow, poly(&[(0.0, 70.0), (64.0, -45.0), (-64.0, -45.0)]), rgb(0x3df5ff), 9.0, v(80.0, 0.0));
    kit.add(text_node(Some(glow), "gl_word", "NEON", font, 44.0, rgb(0xfff3a0), v(0.0, -105.0)));
    label(&mut kit, font, "label_glow", "Glow + Grain", v(cols[0], rows[1] - 160.0));

    // 5. Slit scan on a figure.
    let scanner = panel(&mut kit, "scanner", v(cols[1], rows[1]), vec![ImageEffect::ScannerDrag(ScannerDrag { intensity: 0.45, ..Default::default() })]);
    backdrop(&mut kit, scanner, "sc", 0x0a1220, 0x2f6fd0, v(40.0, -10.0));
    kit.add(text_node(Some(scanner), "sc_word", "drift", font, 56.0, rgb(0xffffff), v(-40.0, 95.0)));
    figure(&mut kit, scanner, "sc_fig", v(30.0, -20.0), 0xf1c9a5, 0xe8322f, 0x2f5fa8);
    label(&mut kit, font, "label_scanner", "Slit Scan", v(cols[1], rows[1] - 160.0));

    // 6. Code (ripple) + gradient map + stroke.
    let code = panel(
        &mut kit,
        "code",
        v(cols[2], rows[1]),
        vec![ImageEffect::GradientMap(GradientMap::default()), ImageEffect::Code(CodeEffect::default()), ImageEffect::Stroke(StrokeFx { width: 3.0, color: [1.0, 1.0, 1.0, 0.9], position: StrokePosition::Inside })],
    );
    landscape(&mut kit, code, "cd", 0x0b2a5a, 0x7fc8ff, 0xf4fbff, 0x35587e, 0x14243a);
    figure(&mut kit, code, "cd_fig", v(-90.0, -30.0), 0xffffff, 0xdddddd, 0x888888);
    kit.add(text_node(Some(code), "cd_word", "RIPPLE", font, 46.0, rgb(0xffffff), v(50.0, -105.0)));
    label(&mut kit, font, "label_code", "Code (custom shader) + Gradient Map", v(cols[2], rows[1] - 160.0));

    // One looping animation keyframing the effects.
    let anim = Anim::new("Effects", 6.0)
        .field(glass, &fx_path(0, "RibbedGlass", "angle"), &[(0.0, 20.0, E::InOut), (3.0, 75.0, E::InOut), (6.0, 20.0, E::InOut)])
        .field(glass, &fx_path(0, "RibbedGlass", "wave"), &[(0.0, 0.0, E::InOut), (3.0, 0.8, E::InOut), (6.0, 0.0, E::InOut)])
        .field(ascii, &fx_path(0, "Ascii", "size"), &[(0.0, 9.0, E::InOut), (3.0, 20.0, E::InOut), (6.0, 9.0, E::InOut)])
        .field(pattern, &fx_path(0, "Pattern", "scale"), &[(0.0, 10.0, E::InOut), (3.0, 34.0, E::InOut), (6.0, 10.0, E::InOut)])
        .field(pattern, &fx_path(0, "Pattern", "angle"), &[(0.0, 0.0, E::Linear), (6.0, 90.0, E::Linear)])
        .field(glow, &fx_path(0, "Glow", "intensity"), &[(0.0, 0.6, E::InOut), (1.5, 2.6, E::InOut), (3.0, 0.6, E::InOut), (4.5, 2.6, E::InOut), (6.0, 0.6, E::InOut)])
        .field(scanner, &fx_path(0, "ScannerDrag", "timing"), &[(0.0, 0.15, E::InOut), (3.0, 0.9, E::InOut), (6.0, 0.15, E::InOut)])
        .field(code, &fx_path(0, "GradientMap", "offset"), &[(0.0, -0.3, E::InOut), (3.0, 0.3, E::InOut), (6.0, -0.3, E::InOut)]);
    let a = kit.anim(anim);
    kit.finish(Vec::<InputDef>::new(), vec![st_loop("Play", a, Vec::new())], Vec::new())
}
