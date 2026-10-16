//! **Toasty Lite components, set two** — twelve more UI animations in the
//! same style and conventions as [`crate::demo_lite`] (96 × 96, no
//! scripts, `ink` line colour, `hover` from Pointer Listeners): toggle,
//! checkbox, play ↔ pause, volume, bookmark, eye (show / hide password),
//! lock, trash, send, refresh, chevron and mic.

use an1mbyt3_core::{ActionKind, BindTarget, Document, InputKind, NodeId, Path, PathCommand, PropKind};
use glam::Vec2;

use crate::demo_lite::{
    circle_cw, fade, go, hover_layer, input, line, on_trigger, round_rect, st, st_loop, v, when, Anim, Kit, LiteDemo, St, AMBER, E, GREEN, INK, INK_JSON, RED, WHITE,
};

type Layer = (&'static str, Vec<NodeId>, Vec<St>);

/// An open circular arc of radius `r` from `a0` to `a1` (degrees, counter-
/// clockwise from +x), as cubic segments.
fn arc(r: f32, a0: f32, a1: f32) -> Path {
    let segments = (((a1 - a0).abs() / 90.0).ceil() as usize).max(1);
    let step = (a1 - a0).to_radians() / segments as f32;
    let k = 4.0 / 3.0 * (step / 4.0).tan() * r;
    let p = |a: f32| v(r * a.cos(), r * a.sin());
    let t = |a: f32| v(-a.sin(), a.cos());
    let mut commands = vec![PathCommand::MoveTo(p(a0.to_radians()))];
    for i in 0..segments {
        let s0 = a0.to_radians() + step * i as f32;
        let s1 = s0 + step;
        commands.push(PathCommand::CubicTo { control1: p(s0) + t(s0) * k, control2: p(s1) - t(s1) * k, to: p(s1) });
    }
    Path { commands }
}

/// A closed polygon.
fn poly(points: &[(f32, f32)]) -> Path {
    let mut p = line(points);
    p.commands.push(PathCommand::Close);
    p
}

/// Two closed quads — the play triangle (split down the middle) and the
/// pause bars share this structure, so one morphs into the other.
fn quads(a: [(f32, f32); 4], b: [(f32, f32); 4]) -> Path {
    let mut commands = Vec::new();
    for q in [a, b] {
        commands.push(PathCommand::MoveTo(v(q[0].0, q[0].1)));
        for p in &q[1..] {
            commands.push(PathCommand::LineTo(v(p.0, p.1)));
        }
        commands.push(PathCommand::Close);
    }
    Path { commands }
}

fn set_scale(kit: &mut Kit, id: NodeId, scale: Vec2) {
    if let Some(n) = kit.document.artboard_mut(kit.scene).and_then(|a| a.node_mut(id)) {
        n.transform.scale = scale;
    }
}

/// A hover layer from two explicit animations (`Rest ⇄ Over`).
fn hover_pair(mask: Vec<NodeId>, rest: usize, over: usize, over_loops: bool) -> Layer {
    let over_state = if over_loops { st_loop("Over", over, vec![go("Rest", when("hover", false), 0.2)]) } else { st("Over", over, vec![go("Rest", when("hover", false), 0.2)]) };
    ("Hover", mask, vec![st("Rest", rest, vec![go("Over", when("hover", true), 0.15)]), over_state])
}

/// `Off ⇄ On` on a Bool input (click toggles it), plus an optional hover layer.
fn bool_machine(kit: Kit, name: &str, off: (&'static str, usize), on: (&'static str, usize), fade_back: f32, hover: Option<Layer>) -> LiteDemo {
    let mut inputs = vec![input(name, InputKind::Bool)];
    if hover.is_some() {
        inputs.push(input("hover", InputKind::Bool));
    }
    kit.finish(inputs, vec![st(off.0, off.1, vec![go(on.0, when(name, true), 0.0)]), st(on.0, on.1, vec![go(off.0, when(name, false), fade_back)])], hover.into_iter().collect())
}

/// `Idle → Go ⇄ Go again` on a trigger (each fire replays the animation).
fn trigger_machine(kit: Kit, trigger: &str, idle: usize, run: usize, hover: Option<Layer>) -> LiteDemo {
    let mut inputs = vec![input(trigger, InputKind::Trigger)];
    if hover.is_some() {
        inputs.push(input("hover", InputKind::Bool));
    }
    kit.finish(
        inputs,
        vec![st("Idle", idle, vec![go("Go", on_trigger(trigger), 0.0)]), st("Go", run, vec![go("Go again", on_trigger(trigger), 0.0)]), st("Go again", run, vec![go("Go", on_trigger(trigger), 0.0)])],
        hover.into_iter().collect(),
    )
}

fn toggle_on(input: &str) -> Vec<ActionKind> {
    vec![ActionKind::ToggleInput { input: input.into() }]
}

fn fire_and_tell(trigger: &str) -> Vec<ActionKind> {
    vec![ActionKind::FireTrigger { input: trigger.into() }, ActionKind::CallFunction { function: trigger.into(), arg: String::new() }]
}

/// **Toggle**: a switch. Bool `on`; the knob slides with a stretch and the
/// track fills green.
pub(crate) fn build_toggle(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Toggle", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let fill = kit.fill("fill", None, round_rect(64.0, 36.0, 18.0), GREEN, Vec2::ZERO);
    let track = kit.stroke("track", None, round_rect(64.0, 36.0, 18.0), INK, 4.0, Vec2::ZERO, (0.0, 1.0));
    let knob = kit.fill("knob", None, Path::ellipse(24.0, 24.0), INK, v(-14.0, 0.0));
    kit.ink(&[track]);
    set_scale(&mut kit, fill, v(0.0, 1.0));
    kit.hit(round_rect(80.0, 56.0, 28.0), toggle_on("on"), None);
    let off = kit.anim(Anim::new("Off", 0.3).pos(knob, &[(0.0, -14.0, 0.0, E::Linear)]).scale_xy(knob, &[(0.0, 1.0, 1.0, E::Linear)]).scale_xy(fill, &[(0.0, 0.0, 1.0, E::Linear)]).fill(knob, &[(0.0, INK, E::Linear)]));
    let on = kit.anim(
        Anim::new("On", 0.45)
            .pos(knob, &[(0.0, -14.0, 0.0, E::Out), (0.3, 14.0, 0.0, E::Linear)])
            .scale_xy(knob, &[(0.0, 1.0, 1.0, E::Out), (0.12, 1.35, 0.85, E::Out), (0.3, 1.0, 1.0, E::Linear)])
            .scale_xy(fill, &[(0.0, 0.0, 1.0, E::Out), (0.25, 1.0, 1.0, E::Linear)])
            .fill(knob, &[(0.0, INK, E::Linear), (0.15, WHITE, E::Linear)]),
    );
    bool_machine(kit, "on", ("Off", off), ("On", on), 0.25, None)
}

/// **Checkbox**: Bool `checked`; the box fills and a check draws on.
pub(crate) fn build_checkbox(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Checkbox", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let frame = kit.stroke("box", None, round_rect(40.0, 40.0, 10.0), INK, 4.0, Vec2::ZERO, (0.0, 1.0));
    let fill = kit.fill("fill", None, round_rect(40.0, 40.0, 10.0), GREEN, Vec2::ZERO);
    let check = kit.stroke("check", None, line(&[(-10.0, 0.0), (-3.0, -8.0), (11.0, 8.0)]), WHITE, 5.0, Vec2::ZERO, (0.0, 0.0));
    kit.ink(&[frame]);
    set_scale(&mut kit, fill, Vec2::ZERO);
    kit.hit(round_rect(80.0, 80.0, 16.0), toggle_on("checked"), Some("hover"));
    let off = kit.anim(Anim::new("Unchecked", 0.3).scale(fill, &[(0.0, 0.0, E::Linear)]).trim_end(check, &[(0.0, 0.0, E::Linear)]));
    let on = kit.anim(Anim::new("Checked", 0.6).scale(fill, &[(0.0, 0.0, E::Back), (0.22, 1.0, E::Linear)]).trim_end(check, &[(0.0, 0.0, E::Linear), (0.16, 0.0, E::Out), (0.45, 1.0, E::Linear)]));
    let hover = hover_layer(&mut kit, frame, 1.08);
    bool_machine(kit, "checked", ("Unchecked", off), ("Checked", on), 0.15, Some(hover))
}

/// **Play**: the triangle morphs into pause bars. Bool `playing`.
pub(crate) fn build_play(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Play", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let play = || quads([(-11.0, 16.0), (2.0, 8.0), (2.0, -8.0), (-11.0, -16.0)], [(2.0, 8.0), (15.0, 0.0), (15.0, 0.0), (2.0, -8.0)]);
    let pause = || quads([(-12.0, 15.0), (-3.0, 15.0), (-3.0, -15.0), (-12.0, -15.0)], [(3.0, 15.0), (12.0, 15.0), (12.0, -15.0), (3.0, -15.0)]);
    let ring = kit.stroke("ring", None, circle_cw(34.0), INK, 4.0, Vec2::ZERO, (0.0, 1.0));
    let icon = kit.fill("icon", None, play(), INK, Vec2::ZERO);
    kit.ink(&[ring]);
    kit.bind(&[icon], "ink", BindTarget::FillColor);
    kit.hit(Path::ellipse(80.0, 80.0), toggle_on("playing"), Some("hover"));
    let paused = kit.anim(Anim::new("Paused", 0.3).morph(icon, vec![(0.0, play(), E::Linear)]).rot(icon, &[(0.0, 0.0, E::Linear)]));
    let playing = kit.anim(Anim::new("Playing", 0.4).morph(icon, vec![(0.0, play(), E::Out), (0.3, pause(), E::Linear)]).rot(icon, &[(0.0, -90.0, E::Out), (0.3, 0.0, E::Linear)]));
    let hover = hover_layer(&mut kit, ring, 1.08);
    bool_machine(kit, "playing", ("Paused", paused), ("Playing", playing), 0.25, Some(hover))
}

/// **Volume**: a speaker with sound waves; Bool `muted` trims them away
/// and draws a red cross.
pub(crate) fn build_volume(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Volume", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let speaker = kit.stroke("speaker", None, poly(&[(-22.0, 7.0), (-12.0, 7.0), (0.0, 18.0), (0.0, -18.0), (-12.0, -7.0), (-22.0, -7.0)]), INK, 4.5, v(-4.0, 0.0), (0.0, 1.0));
    let w1 = kit.stroke("wave1", None, arc(10.0, -50.0, 50.0), INK, 4.5, v(2.0, 0.0), (0.0, 1.0));
    let w2 = kit.stroke("wave2", None, arc(19.0, -50.0, 50.0), INK, 4.5, v(2.0, 0.0), (0.0, 1.0));
    let x1 = kit.stroke("crossA", None, line(&[(12.0, 9.0), (28.0, -9.0)]), RED, 4.5, Vec2::ZERO, (0.0, 0.0));
    let x2 = kit.stroke("crossB", None, line(&[(12.0, -9.0), (28.0, 9.0)]), RED, 4.5, Vec2::ZERO, (0.0, 0.0));
    kit.ink(&[speaker, w1, w2]);
    kit.hit(round_rect(80.0, 80.0, 16.0), toggle_on("muted"), Some("hover"));
    let mut sound = Anim::new("Sound", 0.5);
    let mut muted = Anim::new("Muted", 0.5);
    for (w, d) in [(w1, 0.0), (w2, 0.08)] {
        sound = sound.trim_start(w, &[(0.0, 0.5, E::Linear), (d, 0.5, E::Out), (d + 0.25, 0.0, E::Linear)]).trim_end(w, &[(0.0, 0.5, E::Linear), (d, 0.5, E::Out), (d + 0.25, 1.0, E::Linear)]);
        muted = muted.trim_start(w, &[(0.0, 0.0, E::In), (0.18, 0.5, E::Linear)]).trim_end(w, &[(0.0, 1.0, E::In), (0.18, 0.5, E::Linear)]);
    }
    sound = sound.trim_end(x1, &[(0.0, 0.0, E::Linear)]).trim_end(x2, &[(0.0, 0.0, E::Linear)]);
    muted = muted.trim_end(x1, &[(0.0, 0.0, E::Linear), (0.15, 0.0, E::Out), (0.32, 1.0, E::Linear)]).trim_end(x2, &[(0.0, 0.0, E::Linear), (0.24, 0.0, E::Out), (0.42, 1.0, E::Linear)]);
    let sound = kit.anim(sound);
    let muted = kit.anim(muted);
    let hover = hover_layer(&mut kit, speaker, 1.08);
    bool_machine(kit, "muted", ("Sound", sound), ("Muted", muted), 0.0, Some(hover))
}

/// **Bookmark**: Bool `saved`; the ribbon fills top-down with a little
/// drop-and-settle.
pub(crate) fn build_bookmark(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Bookmark", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let ribbon = || poly(&[(-14.0, 22.0), (14.0, 22.0), (14.0, -22.0), (0.0, -12.0), (-14.0, -22.0)]);
    let g = kit.group("ribbonGroup", None, Vec2::ZERO);
    let fill = kit.fill("fill", Some(g), ribbon(), AMBER, Vec2::ZERO);
    let outline = kit.stroke("outline", Some(g), ribbon(), INK, 4.5, Vec2::ZERO, (0.0, 1.0));
    kit.ink(&[outline]);
    if let Some(n) = kit.document.artboard_mut(kit.scene).and_then(|a| a.node_mut(fill)) {
        // Pivot on the top edge so it fills downward.
        n.transform.anchor = v(0.0, 22.0);
        n.transform.translation = v(0.0, 22.0);
        n.transform.scale = v(1.0, 0.0);
    }
    kit.hit(round_rect(80.0, 80.0, 16.0), toggle_on("saved"), Some("hover"));
    let off = kit.anim(Anim::new("Unsaved", 0.3).scale_xy(fill, &[(0.0, 1.0, 0.0, E::Linear)]).pos(g, &[(0.0, 0.0, 0.0, E::Linear)]));
    let on = kit.anim(
        Anim::new("Saved", 0.55)
            .scale_xy(fill, &[(0.0, 1.0, 0.0, E::Out), (0.28, 1.0, 1.0, E::Linear)])
            .pos(g, &[(0.0, 0.0, 0.0, E::Out), (0.12, 0.0, -5.0, E::InOut), (0.32, 0.0, 1.5, E::InOut), (0.45, 0.0, 0.0, E::Linear)]),
    );
    let hover = hover_layer(&mut kit, g, 1.08);
    bool_machine(kit, "saved", ("Unsaved", off), ("Saved", on), 0.2, Some(hover))
}

/// **Eye**: show / hide password. Bool `hidden` blinks the eye nearly
/// shut and draws a slash; hovering makes the pupil look around.
pub(crate) fn build_eye(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Eye", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let eye = kit.group("eye", None, Vec2::ZERO);
    let almond = Path {
        commands: vec![
            PathCommand::MoveTo(v(-26.0, 0.0)),
            PathCommand::CubicTo { control1: v(-14.0, 18.0), control2: v(14.0, 18.0), to: v(26.0, 0.0) },
            PathCommand::CubicTo { control1: v(14.0, -18.0), control2: v(-14.0, -18.0), to: v(-26.0, 0.0) },
            PathCommand::Close,
        ],
    };
    let outline = kit.stroke("outline", Some(eye), almond, INK, 4.5, Vec2::ZERO, (0.0, 1.0));
    let pupil = kit.fill("pupil", Some(eye), Path::ellipse(15.0, 15.0), INK, Vec2::ZERO);
    let slash = kit.stroke("slash", None, line(&[(-24.0, 20.0), (24.0, -20.0)]), INK, 4.5, Vec2::ZERO, (0.0, 0.0));
    kit.ink(&[outline, slash]);
    kit.bind(&[pupil], "ink", BindTarget::FillColor);
    kit.hit(round_rect(80.0, 80.0, 16.0), toggle_on("hidden"), Some("hover"));
    let shown = kit.anim(Anim::new("Shown", 0.3).scale_xy(eye, &[(0.0, 1.0, 1.0, E::Linear)]).trim_end(slash, &[(0.0, 0.0, E::Linear)]).scale(pupil, &[(0.0, 1.0, E::Linear)]));
    let hidden = kit.anim(
        Anim::new("Hidden", 0.5)
            .scale_xy(eye, &[(0.0, 1.0, 1.0, E::In), (0.14, 1.0, 0.12, E::Out), (0.3, 1.0, 0.7, E::Linear)])
            .scale(pupil, &[(0.0, 1.0, E::In), (0.14, 0.4, E::Linear)])
            .trim_end(slash, &[(0.0, 0.0, E::Linear), (0.14, 0.0, E::Out), (0.4, 1.0, E::Linear)]),
    );
    let rest = kit.anim(Anim::new("Hover off", 0.2).pos(pupil, &[(0.0, 0.0, 0.0, E::Linear)]));
    let look = kit.anim(Anim::new("Hover on", 1.6).pos(pupil, &[(0.0, 0.0, 0.0, E::InOut), (0.4, -6.0, 1.0, E::InOut), (1.0, 6.0, 1.0, E::InOut), (1.6, 0.0, 0.0, E::Linear)]));
    bool_machine(kit, "hidden", ("Shown", shown), ("Hidden", hidden), 0.2, Some(hover_pair(vec![pupil], rest, look, true)))
}

/// **Lock**: Bool `locked` (starts unlocked); the shackle drops in and the
/// lock gives a small kick.
pub(crate) fn build_lock(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Lock", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let lock = kit.group("lock", None, v(0.0, -6.0));
    let body = kit.stroke("body", Some(lock), round_rect(40.0, 30.0, 7.0), INK, 4.5, v(0.0, -6.0), (0.0, 1.0));
    let hole = kit.stroke("keyhole", Some(lock), line(&[(0.0, -2.0), (0.0, -10.0)]), INK, 4.5, Vec2::ZERO, (0.0, 1.0));
    // The shackle: up the left side, over the top arc, down the right.
    let mut shackle_path = line(&[(-11.0, 9.0), (-11.0, 17.0)]);
    for c in arc(11.0, 180.0, 0.0).commands.into_iter().skip(1) {
        if let PathCommand::CubicTo { control1, control2, to } = c {
            shackle_path.commands.push(PathCommand::CubicTo { control1: control1 + v(0.0, 17.0), control2: control2 + v(0.0, 17.0), to: to + v(0.0, 17.0) });
        }
    }
    shackle_path.commands.push(PathCommand::LineTo(v(11.0, 9.0)));
    let shackle = kit.stroke("shackle", Some(lock), shackle_path, INK, 4.5, v(0.0, 7.0), (0.0, 1.0));
    kit.ink(&[body, hole, shackle]);
    kit.hit(round_rect(80.0, 80.0, 16.0), toggle_on("locked"), Some("hover"));
    let open = kit.anim(Anim::new("Unlocked", 0.3).pos(shackle, &[(0.0, 0.0, 7.0, E::Linear)]).rot(lock, &[(0.0, 0.0, E::Linear)]));
    let locked = kit.anim(
        Anim::new("Locked", 0.5)
            .pos(shackle, &[(0.0, 0.0, 7.0, E::In), (0.16, 0.0, 0.0, E::Linear)])
            .rot(lock, &[(0.0, 0.0, E::Linear), (0.16, 0.0, E::InOut), (0.24, -7.0, E::InOut), (0.33, 5.0, E::InOut), (0.42, 0.0, E::Linear)]),
    );
    let hover = hover_layer(&mut kit, body, 1.06);
    bool_machine(kit, "locked", ("Unlocked", open), ("Locked", locked), 0.2, Some(hover))
}

/// **Trash**: hover lifts the lid; trigger `trash` (click, also an event)
/// flips it open, shakes the can and settles.
pub(crate) fn build_trash(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Trash", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let can = kit.group("can", None, Vec2::ZERO);
    let body = kit.stroke("body", Some(can), poly(&[(-15.0, 12.0), (15.0, 12.0), (12.0, -22.0), (-12.0, -22.0)]), INK, 4.5, Vec2::ZERO, (0.0, 1.0));
    let l1 = kit.stroke("line1", Some(can), line(&[(-5.0, 4.0), (-4.0, -14.0)]), INK, 4.0, Vec2::ZERO, (0.0, 1.0));
    let l2 = kit.stroke("line2", Some(can), line(&[(5.0, 4.0), (4.0, -14.0)]), INK, 4.0, Vec2::ZERO, (0.0, 1.0));
    // The lid hinges on its left end.
    let lid = kit.group("lid", Some(can), v(-19.0, 19.0));
    let lid_line = kit.stroke("lidLine", Some(lid), line(&[(0.0, 0.0), (38.0, 0.0)]), INK, 4.5, Vec2::ZERO, (0.0, 1.0));
    let handle = kit.stroke("handle", Some(lid), line(&[(13.0, 0.0), (13.0, 5.0), (25.0, 5.0), (25.0, 0.0)]), INK, 4.0, Vec2::ZERO, (0.0, 1.0));
    kit.ink(&[body, l1, l2, lid_line, handle]);
    kit.hit(round_rect(80.0, 80.0, 16.0), fire_and_tell("trash"), Some("hover"));
    let idle = kit.anim(Anim::new("Idle", 0.3).rot(lid, &[(0.0, 0.0, E::Linear)]).rot(can, &[(0.0, 0.0, E::Linear)]));
    let toss = kit.anim(
        Anim::new("Toss", 0.9)
            .rot(lid, &[(0.0, 0.0, E::Out), (0.15, 40.0, E::InOut), (0.55, 40.0, E::InOut), (0.75, 0.0, E::Linear)])
            .rot(can, &[(0.0, 0.0, E::Linear), (0.2, 0.0, E::InOut), (0.3, -6.0, E::InOut), (0.4, 6.0, E::InOut), (0.5, -3.0, E::InOut), (0.6, 0.0, E::Linear)]),
    );
    let rest = kit.anim(Anim::new("Hover off", 0.2).pos(lid, &[(0.0, -19.0, 19.0, E::Linear)]));
    let lift = kit.anim(Anim::new("Hover on", 0.2).pos(lid, &[(0.0, -19.0, 24.0, E::Linear)]));
    trigger_machine(kit, "trash", idle, toss, Some(hover_pair(vec![lid], rest, lift, false)))
}

/// **Send**: trigger `send` (click, also an event) launches the paper
/// plane off to the top right with a trail; it slides back in from the left.
pub(crate) fn build_send(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Send", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let trail = kit.stroke("trail", None, line(&[(-40.0, -30.0), (-4.0, -4.0)]), AMBER, 3.0, Vec2::ZERO, (0.0, 0.0));
    let plane = kit.group("plane", None, Vec2::ZERO);
    let wing = kit.stroke("wing", Some(plane), poly(&[(-22.0, -2.0), (22.0, 16.0), (2.0, -20.0), (-3.0, -6.0)]), INK, 4.0, Vec2::ZERO, (0.0, 1.0));
    let fold = kit.stroke("fold", Some(plane), line(&[(22.0, 16.0), (-3.0, -6.0), (-6.0, -18.0)]), INK, 4.0, Vec2::ZERO, (0.0, 1.0));
    kit.ink(&[wing, fold]);
    kit.hit(round_rect(80.0, 80.0, 16.0), fire_and_tell("send"), Some("hover"));
    let idle = kit.anim(Anim::new("Idle", 0.3).pos(plane, &[(0.0, 0.0, 0.0, E::Linear)]).scale(plane, &[(0.0, 1.0, E::Linear)]).trim_end(trail, &[(0.0, 0.0, E::Linear)]).trim_start(trail, &[(0.0, 0.0, E::Linear)]));
    let fly = kit.anim(
        Anim::new("Fly", 1.2)
            .pos(plane, &[(0.0, 0.0, 0.0, E::InOut), (0.1, -4.0, -3.0, E::In), (0.42, 60.0, 50.0, E::Linear), (0.43, -60.0, 0.0, E::Out), (0.9, 0.0, 0.0, E::Linear)])
            .scale(plane, &[(0.0, 1.0, E::Linear), (0.42, 0.5, E::Linear), (0.43, 0.6, E::Out), (0.9, 1.0, E::Linear)])
            .trim_end(trail, &[(0.0, 0.0, E::Linear), (0.12, 0.0, E::Out), (0.35, 1.0, E::Linear)])
            .trim_start(trail, &[(0.0, 0.0, E::Linear), (0.25, 0.0, E::Out), (0.55, 1.0, E::Linear)]),
    );
    let rest = kit.anim(Anim::new("Hover off", 0.2).rot(plane, &[(0.0, 0.0, E::Linear)]));
    let tilt = kit.anim(Anim::new("Hover on", 0.2).rot(plane, &[(0.0, 8.0, E::Linear)]));
    trigger_machine(kit, "send", idle, fly, Some(hover_pair(vec![plane], rest, tilt, false)))
}

/// **Refresh**: two arrows round a circle; trigger `refresh` (click, also
/// an event) spins them a full turn with overshoot.
pub(crate) fn build_refresh(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Refresh", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let g = kit.group("arrows", None, Vec2::ZERO);
    let a = kit.stroke("arcA", Some(g), arc(20.0, 20.0, 160.0), INK, 4.5, Vec2::ZERO, (0.0, 1.0));
    let b = kit.stroke("arcB", Some(g), arc(20.0, 200.0, 340.0), INK, 4.5, Vec2::ZERO, (0.0, 1.0));
    let tip = |deg: f32| {
        let r = deg.to_radians();
        let p = v(r.cos() * 20.0, r.sin() * 20.0);
        let t = v(-r.sin(), r.cos());
        let n = v(r.cos(), r.sin());
        let (l, rr) = (p - t * 7.0 + n * 6.0, p - t * 7.0 - n * 6.0);
        line(&[(l.x, l.y), (p.x, p.y), (rr.x, rr.y)])
    };
    let ha = kit.stroke("headA", Some(g), tip(160.0), INK, 4.5, Vec2::ZERO, (0.0, 1.0));
    let hb = kit.stroke("headB", Some(g), tip(340.0), INK, 4.5, Vec2::ZERO, (0.0, 1.0));
    kit.ink(&[a, b, ha, hb]);
    kit.hit(Path::ellipse(80.0, 80.0), fire_and_tell("refresh"), Some("hover"));
    let idle = kit.anim(Anim::new("Idle", 0.3).rot(g, &[(0.0, 0.0, E::Linear)]));
    let spin = kit.anim(Anim::new("Spin", 0.9).rot(g, &[(0.0, 0.0, E::Back), (0.8, -360.0, E::Linear)]));
    let rest = kit.anim(Anim::new("Hover off", 0.2).scale(g, &[(0.0, 1.0, E::Linear)]));
    let over = kit.anim(Anim::new("Hover on", 0.2).scale(g, &[(0.0, 1.1, E::Linear)]));
    trigger_machine(kit, "refresh", idle, spin, Some(hover_pair(vec![g], rest, over, false)))
}

/// **Chevron**: for accordions and dropdowns. Bool `open` flips it with
/// overshoot; hovering bobs it.
pub(crate) fn build_chevron(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Chevron", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let chev = kit.stroke("chevron", None, line(&[(-14.0, 6.0), (0.0, -8.0), (14.0, 6.0)]), INK, 5.0, Vec2::ZERO, (0.0, 1.0));
    kit.ink(&[chev]);
    kit.hit(round_rect(80.0, 80.0, 16.0), toggle_on("open"), Some("hover"));
    let closed = kit.anim(Anim::new("Closed", 0.3).rot(chev, &[(0.0, 0.0, E::Linear)]));
    let open = kit.anim(Anim::new("Open", 0.4).rot(chev, &[(0.0, 0.0, E::Back), (0.35, 180.0, E::Linear)]));
    let rest = kit.anim(Anim::new("Hover off", 0.2).pos(chev, &[(0.0, 0.0, 0.0, E::Linear)]));
    let bob = kit.anim(Anim::new("Hover on", 0.8).pos(chev, &[(0.0, 0.0, 0.0, E::InOut), (0.4, 0.0, -4.0, E::InOut), (0.8, 0.0, 0.0, E::Linear)]));
    bool_machine(kit, "open", ("Closed", closed), ("Open", open), 0.25, Some(hover_pair(vec![chev], rest, bob, true)))
}

/// **Mic**: Bool `recording` fills the capsule red and sends pulse rings
/// out on a loop.
pub(crate) fn build_mic(document: &mut Document) -> LiteDemo {
    let mut kit = Kit::new(document, "Mic", 96.0, 96.0);
    kit.property("ink", PropKind::Color, INK_JSON);
    let p1 = kit.stroke("pulse1", None, circle_cw(18.0), RED, 3.0, v(0.0, 8.0), (0.0, 1.0));
    let p2 = kit.stroke("pulse2", None, circle_cw(18.0), RED, 3.0, v(0.0, 8.0), (0.0, 1.0));
    let fill = kit.fill("capsuleFill", None, round_rect(18.0, 30.0, 9.0), RED, v(0.0, 8.0));
    let capsule = kit.stroke("capsule", None, round_rect(18.0, 30.0, 9.0), INK, 4.5, v(0.0, 8.0), (0.0, 1.0));
    let cup = kit.stroke("cup", None, arc(15.0, 180.0, 360.0), INK, 4.5, v(0.0, 6.0), (0.0, 1.0));
    let stand = kit.stroke("stand", None, line(&[(0.0, -9.0), (0.0, -18.0)]), INK, 4.5, Vec2::ZERO, (0.0, 1.0));
    kit.ink(&[capsule, cup, stand]);
    for id in [fill, p1, p2] {
        set_scale(&mut kit, id, Vec2::ZERO);
    }
    kit.hit(round_rect(80.0, 80.0, 16.0), toggle_on("recording"), Some("hover"));
    let idle = kit.anim(Anim::new("Idle", 0.3).scale(fill, &[(0.0, 0.0, E::Linear)]).scale(p1, &[(0.0, 0.0, E::Linear)]).scale(p2, &[(0.0, 0.0, E::Linear)]));
    let rec = kit.anim(
        Anim::new("Recording", 1.4)
            .scale(fill, &[(0.0, 1.0, E::Linear)])
            .scale(p1, &[(0.0, 0.9, E::Out), (1.4, 2.2, E::Linear)])
            .stroke_color(p1, &[(0.0, fade(RED, 0.9), E::Linear), (1.4, fade(RED, 0.0), E::Linear)])
            .scale(p2, &[(0.0, 0.9, E::Linear), (0.7, 0.9, E::Out), (1.4, 1.55, E::Linear)])
            .stroke_color(p2, &[(0.0, fade(RED, 0.0), E::Linear), (0.7, fade(RED, 0.9), E::Linear), (1.4, fade(RED, 0.45), E::Linear)]),
    );
    let hover = hover_layer(&mut kit, capsule, 1.08);
    kit.finish(
        vec![input("recording", InputKind::Bool), input("hover", InputKind::Bool)],
        vec![st("Idle", idle, vec![go("Recording", when("recording", true), 0.0)]), st_loop("Recording", rec, vec![go("Idle", when("recording", false), 0.25)])],
        vec![hover],
    )
}

/// Builds one of this set's components by name.
pub(crate) fn build(document: &mut Document, kind: &str) -> Option<LiteDemo> {
    Some(match kind {
        "toggle" => build_toggle(document),
        "checkbox" => build_checkbox(document),
        "play" => build_play(document),
        "volume" => build_volume(document),
        "bookmark" => build_bookmark(document),
        "eye" => build_eye(document),
        "lock" => build_lock(document),
        "trash" => build_trash(document),
        "send" => build_send(document),
        "refresh" => build_refresh(document),
        "chevron" => build_chevron(document),
        "mic" => build_mic(document),
        _ => return None,
    })
}
