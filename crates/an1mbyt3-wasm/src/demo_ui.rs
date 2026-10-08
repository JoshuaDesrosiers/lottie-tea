//! "UI & Sound Lab" — the UI components and the audio system together.
//!
//! - A title pinned to the top-centre and a click counter pinned to the
//!   bottom-right (**UI Anchor**), whatever the window size.
//! - A column of sound buttons laid out by a **UI Stack**; each is a
//!   **UI Button** (Unity-style state colours + press-in, fading between
//!   states) whose click the script reads with `ref.clicked`. Every button
//!   plays a different synthesized sound (`ctx.playTone`: beep, laser,
//!   coin, explosion) — no audio files needed.
//! - A **UI Slider** for the master volume (`ref.value`, `ref.changed`), a
//!   **UI Toggle** checkbox to mute, and three toggles in one radio group
//!   picking the beep's waveform.
//! - A **UI Scroll View** (clipped viewport + "Content" stack + linked
//!   **UI Scrollbar**) holding twelve note buttons — scroll with the wheel,
//!   by dragging the list, or by dragging the bar.

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Command, Document, FontAsset, FontId, GroupNode, LoopMode, Node, NodeBehavior, NodeBehaviorEntry, NodeId, NodeKind, Path,
    ProgramDef, ShapeNode, StateDef, StateMachineDef, UiAnchor, UiButton, UiScrollView, UiScrollbar, UiSlider, UiStack, UiStateStyle, UiToggle,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");

/// The twelve notes in the scroll list (name, frequency in Hz).
const NOTES: [(&str, f32); 12] = [
    ("C4", 261.63),
    ("D4", 293.66),
    ("E4", 329.63),
    ("F4", 349.23),
    ("G4", 392.0),
    ("A4", 440.0),
    ("B4", 493.88),
    ("C5", 523.25),
    ("D5", 587.33),
    ("E5", 659.25),
    ("F5", 698.46),
    ("G5", 783.99),
];

pub(crate) struct UiDemo {
    pub scene: ArtboardId,
}

fn with(mut node: Node, behavior: NodeBehavior) -> Node {
    node.behaviors.push(NodeBehaviorEntry { behavior, enabled: true });
    node
}

/// A plain rectangle shape at `at` under `parent`.
fn rect(id: &str, parent: Option<NodeId>, size: Vec2, color: [f32; 4], at: Vec2) -> Node {
    let mut node = with_id(
        Node::new(id, parent, NodeKind::Shape(ShapeNode { path: Path::rect(size.x, size.y), fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None })),
        id,
    );
    node.transform.translation = at;
    node
}

/// A toggle box with its "Checkmark" child and a label to its right.
#[allow(clippy::too_many_arguments)]
fn add_toggle(document: &mut Document, inverses: &mut Vec<Command>, scene: ArtboardId, parent: NodeId, font: FontId, id: &str, label: &str, at: Vec2, toggle: UiToggle) {
    let style = UiStateStyle { highlighted_color: [1.25, 1.25, 1.35, 1.0], ..Default::default() };
    let boxed = with(rect(id, Some(parent), Vec2::splat(18.0), [0.2, 0.22, 0.3, 1.0], at), NodeBehavior::UiToggle(UiToggle { style, ..toggle }));
    let box_id = add_node(document, inverses, scene, boxed);
    let mut check = rect(&format!("{id}Check"), Some(box_id), Vec2::splat(10.0), [0.45, 0.85, 0.55, 1.0], Vec2::ZERO);
    check.name = "Checkmark".into();
    check.visible = toggle.is_on;
    add_node(document, inverses, scene, check);
    let text_x = 14.0 + label.len() as f32 * 4.0;
    add_node(document, inverses, scene, text_node(Some(box_id), &format!("{id}Label"), label, font, 7.0, [0.85, 0.88, 1.0, 1.0], Vec2::new(text_x, 0.0)));
}

pub(crate) fn build_demo_ui(document: &mut Document) -> UiDemo {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: FONT_BYTES.to_vec() });
    let inverse = document.apply(Command::AddArtboard { name: "UI & Sound Lab".into(), width: 640.0, height: 360.0 });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    let camera = with_id(
        Node::with_components("Main Camera", None, vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.09, 0.09, 0.13, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })]),
        "camera",
    );
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));

    // Pinned title (top-centre), counter (bottom-right) and hint (bottom).
    let title = with(text_node(Some(camera_id), "title", "UI & SOUND LAB", font, 18.0, [1.0, 1.0, 1.0, 1.0], Vec2::ZERO), NodeBehavior::UiAnchor(UiAnchor { anchor: Vec2::new(0.5, 1.0), offset: Vec2::new(0.0, -30.0) }));
    add_node(document, &mut inverses, scene, title);
    let counter = with(text_node(Some(camera_id), "counter", "CLICKS: 0", font, 10.0, [0.75, 0.8, 1.0, 1.0], Vec2::ZERO), NodeBehavior::UiAnchor(UiAnchor { anchor: Vec2::new(1.0, 0.0), offset: Vec2::new(-80.0, 20.0) }));
    add_node(document, &mut inverses, scene, counter);
    let hint = with(
        text_node(Some(camera_id), "hint", "SOUND STARTS AFTER YOUR FIRST CLICK - SCROLL THE NOTES", font, 7.0, [0.55, 0.58, 0.7, 1.0], Vec2::ZERO),
        NodeBehavior::UiAnchor(UiAnchor { anchor: Vec2::new(0.5, 0.0), offset: Vec2::new(0.0, 18.0) }),
    );
    add_node(document, &mut inverses, scene, hint);

    // Left: a column of sound buttons (state colours fade, label tints too).
    let stack = with(
        with(with_id(Node::new("Buttons", Some(camera_id), NodeKind::Group(GroupNode::default())), "buttons"), NodeBehavior::UiStack(UiStack { spacing: 10.0, ..Default::default() })),
        NodeBehavior::UiAnchor(UiAnchor { anchor: Vec2::new(0.5, 0.5), offset: Vec2::new(-180.0, 90.0) }),
    );
    let stack_id = add_node(document, &mut inverses, scene, stack);
    let button_style = UiStateStyle { highlighted_color: [1.2, 1.2, 1.2, 1.0], pressed_color: [0.7, 0.7, 0.75, 1.0], highlighted_scale: 1.04, pressed_scale: 0.95, fade_duration: 0.12, ..Default::default() };
    for (id, label, color) in [
        ("beep", "BEEP", [0.27, 0.55, 0.95, 1.0]),
        ("laser", "LASER", [0.85, 0.3, 0.55, 1.0]),
        ("coin", "COIN", [0.95, 0.72, 0.2, 1.0]),
        ("boom", "EXPLOSION", [0.9, 0.38, 0.22, 1.0]),
    ] {
        let button = with(rect(id, Some(stack_id), Vec2::new(180.0, 30.0), color, Vec2::ZERO), NodeBehavior::UiButton(UiButton { interactable: true, style: button_style }));
        let button_id = add_node(document, &mut inverses, scene, button);
        add_node(document, &mut inverses, scene, text_node(Some(button_id), &format!("{id}Label"), label, font, 9.0, [1.0, 1.0, 1.0, 1.0], Vec2::ZERO));
    }

    // Right: a settings panel.
    let panel = with(rect("panel", Some(camera_id), Vec2::new(250.0, 250.0), [0.12, 0.13, 0.18, 0.9], Vec2::ZERO), NodeBehavior::UiAnchor(UiAnchor { anchor: Vec2::new(0.5, 0.5), offset: Vec2::new(120.0, 4.0) }));
    let panel_id = add_node(document, &mut inverses, scene, panel);

    // Volume slider (track + Fill + Handle).
    add_node(document, &mut inverses, scene, text_node(Some(panel_id), "volLabel", "VOLUME 80%", font, 7.0, [0.85, 0.88, 1.0, 1.0], Vec2::new(0.0, 108.0)));
    let handle_style = UiStateStyle { highlighted_scale: 1.2, pressed_scale: 1.1, ..Default::default() };
    let slider = with(rect("volume", Some(panel_id), Vec2::new(200.0, 8.0), [0.22, 0.24, 0.32, 1.0], Vec2::new(0.0, 90.0)), NodeBehavior::UiSlider(UiSlider { value: 0.8, style: handle_style, ..Default::default() }));
    let slider_id = add_node(document, &mut inverses, scene, slider);
    let mut fill = rect("volumeFill", Some(slider_id), Vec2::new(200.0, 8.0), [0.35, 0.6, 1.0, 1.0], Vec2::ZERO);
    fill.name = "Fill".into();
    add_node(document, &mut inverses, scene, fill);
    let mut handle = with_id(Node::new("Handle", Some(slider_id), NodeKind::Shape(ShapeNode { path: Path::ellipse(18.0, 18.0), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None })), "volumeHandle");
    handle.transform.translation = Vec2::new(60.0, 0.0);
    add_node(document, &mut inverses, scene, handle);

    // Mute checkbox, and a radio group for the beep's waveform.
    add_toggle(document, &mut inverses, scene, panel_id, font, "mute", "MUTE", Vec2::new(-100.0, 60.0), UiToggle::default());
    add_node(document, &mut inverses, scene, text_node(Some(panel_id), "waveLabel", "BEEP WAVE", font, 7.0, [0.6, 0.64, 0.78, 1.0], Vec2::new(-65.0, 34.0)));
    for (i, (id, label)) in [("waveSquare", "SQR"), ("waveSaw", "SAW"), ("waveSine", "SIN")].into_iter().enumerate() {
        add_toggle(document, &mut inverses, scene, panel_id, font, id, label, Vec2::new(-100.0 + i as f32 * 72.0, 14.0), UiToggle { is_on: i == 0, group: 1, ..Default::default() });
    }

    // A scroll view of note buttons with a linked scrollbar.
    let view = with(rect("notes", Some(panel_id), Vec2::new(220.0, 120.0), [0.07, 0.07, 0.1, 1.0], Vec2::new(0.0, -55.0)), NodeBehavior::UiScrollView(UiScrollView::default()));
    let mut view = view;
    view.clip_children = true;
    let view_id = add_node(document, &mut inverses, scene, view);
    let mut content = with(with_id(Node::new("Content", Some(view_id), NodeKind::Group(GroupNode::default())), "notesContent"), NodeBehavior::UiStack(UiStack { spacing: 4.0, padding: 4.0, ..Default::default() }));
    content.transform.translation = Vec2::new(-6.0, 60.0);
    let content_id = add_node(document, &mut inverses, scene, content);
    let note_style = UiStateStyle { highlighted_color: [1.3, 1.3, 1.4, 1.0], pressed_scale: 0.97, ..Default::default() };
    for (i, (name, _)) in NOTES.iter().enumerate() {
        let id = format!("note{i}");
        let shade = 0.17 + (i % 2) as f32 * 0.03;
        let row = with(rect(&id, Some(content_id), Vec2::new(196.0, 22.0), [shade, shade + 0.02, shade + 0.08, 1.0], Vec2::ZERO), NodeBehavior::UiButton(UiButton { interactable: true, style: note_style }));
        let row_id = add_node(document, &mut inverses, scene, row);
        add_node(document, &mut inverses, scene, text_node(Some(row_id), &format!("{id}Label"), &format!("PLAY {name}"), font, 7.0, [0.9, 0.92, 1.0, 1.0], Vec2::ZERO));
    }
    let bar_style = UiStateStyle { highlighted_color: [1.3, 1.3, 1.3, 1.0], ..Default::default() };
    let mut bar = with(rect("notesBar", Some(view_id), Vec2::new(8.0, 116.0), [0.14, 0.15, 0.2, 1.0], Vec2::new(104.0, 0.0)), NodeBehavior::UiScrollbar(UiScrollbar { style: bar_style, ..Default::default() }));
    bar.name = "Scrollbar".into();
    let bar_id = add_node(document, &mut inverses, scene, bar);
    let mut bar_handle = rect("notesBarHandle", Some(bar_id), Vec2::new(8.0, 116.0), [0.5, 0.53, 0.65, 1.0], Vec2::ZERO);
    bar_handle.name = "Handle".into();
    add_node(document, &mut inverses, scene, bar_handle);

    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Lab (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "UI & Sound Lab".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Play".into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(script())),
            }],
            initial_state: "Play".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));
    inverses.reverse();
    document.commit_batch(inverses);
    UiDemo { scene }
}

/// The lab's script, with the note table filled in.
fn script() -> String {
    let freqs = NOTES.iter().map(|(_, f)| format!("{f}")).collect::<Vec<_>>().join(", ");
    SCRIPT.replace("__NOTES__", &freqs)
}

const SCRIPT: &str = r#"// UI & Sound Lab: buttons (ref.clicked), a volume slider (ref.value),
// a mute checkbox + waveform radio group (ref.isOn), and a scroll list.
const g = ctx.globals.lab || (ctx.globals.lab = { clicks: 0 })
const c = (id) => getCompByID(id)
let clicked = false

// Volume + mute: re-apply whenever either changes (and once at start).
const volume = c('volume'), mute = c('mute')
if (!g.init || volume.changed || mute.changed) {
  g.init = true
  ctx.setMasterVolume(mute.isOn ? 0 : volume.value)
  c('volLabel').textContent = mute.isOn ? 'VOLUME (MUTED)' : 'VOLUME ' + Math.round(volume.value * 100) + '%'
}

// The radio group picks the beep's waveform.
const wave = c('waveSaw').isOn ? 'sawtooth' : c('waveSine').isOn ? 'sine' : 'square'

if (c('beep').clicked) { ctx.playTone({ freq: 660, duration: 0.15, type: wave }); clicked = true }
if (c('laser').clicked) { ctx.playTone({ freq: 1400, to: 180, duration: 0.25, type: 'sawtooth', volume: 0.25 }); clicked = true }
if (c('coin').clicked) {
  ctx.playTone({ freq: 988, duration: 0.08, type: 'square', volume: 0.25 })
  g.coinAt = ctx.time + 0.08
  clicked = true
}
if (g.coinAt && ctx.time >= g.coinAt) { ctx.playTone({ freq: 1319, duration: 0.25, type: 'square', volume: 0.25 }); g.coinAt = 0 }
if (c('boom').clicked) { ctx.playTone({ type: 'noise', duration: 0.6, volume: 0.5, release: 0.55 }); clicked = true }

// Note buttons inside the scroll view.
const notes = [__NOTES__]
for (let i = 0; i < notes.length; i++) {
  if (c('note' + i).clicked) { ctx.playTone({ freq: notes[i], duration: 0.35, type: wave, volume: 0.35 }); clicked = true }
}

if (clicked) {
  g.clicks++
  c('counter').textContent = 'CLICKS: ' + g.clicks
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_every_kind_of_ui_control() {
        let mut document = Document::new();
        let demo = build_demo_ui(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let count = |pred: &dyn Fn(&NodeBehavior) -> bool| artboard.node_ids().filter_map(|id| artboard.node(id)).filter(|n| n.behaviors.iter().any(|e| pred(&e.behavior))).count();
        assert_eq!(count(&|b| matches!(b, NodeBehavior::UiButton(_))), 4 + NOTES.len());
        assert_eq!(count(&|b| matches!(b, NodeBehavior::UiToggle(_))), 4);
        assert_eq!(count(&|b| matches!(b, NodeBehavior::UiSlider(_))), 1);
        assert_eq!(count(&|b| matches!(b, NodeBehavior::UiScrollView(_))), 1);
        assert_eq!(count(&|b| matches!(b, NodeBehavior::UiScrollbar(_))), 1);
        assert_eq!(count(&|b| matches!(b, NodeBehavior::UiStack(_))), 2);
        // Children the controls find by name.
        for name in ["Fill", "Handle", "Checkmark", "Content", "Scrollbar"] {
            assert!(artboard.node_ids().filter_map(|id| artboard.node(id)).any(|n| n.name == name), "missing a {name} child");
        }
    }
}
