//! "UI & Sound Lab" — the UI components and the audio system together.
//!
//! - A title pinned to the top-centre and a click counter pinned to the
//!   bottom-right (**UI Anchor**), whatever the window size.
//! - A centred column of buttons laid out by a **UI Stack**; each is a
//!   **UI Button** (hover / press feedback) whose click the script reads with
//!   `ref.clicked`.
//! - Every button plays a different synthesized sound (`ctx.playTone`:
//!   beep, laser, coin, explosion) — no audio files needed — and the last one
//!   toggles the master volume.

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Command, Document, FontAsset, GroupNode, LoopMode, Node, NodeBehavior, NodeBehaviorEntry, NodeKind, Path, ProgramDef,
    ShapeNode, StateDef, StateMachineDef, UiAnchor, UiButton, UiStack,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");

pub(crate) struct UiDemo {
    pub scene: ArtboardId,
}

fn with(mut node: Node, behavior: NodeBehavior) -> Node {
    node.behaviors.push(NodeBehaviorEntry { behavior, enabled: true });
    node
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

    // Pinned title (top-centre) and counter (bottom-right).
    let title = with(text_node(Some(camera_id), "title", "UI & SOUND LAB", font, 18.0, [1.0, 1.0, 1.0, 1.0], Vec2::ZERO), NodeBehavior::UiAnchor(UiAnchor { anchor: Vec2::new(0.5, 1.0), offset: Vec2::new(0.0, -30.0) }));
    add_node(document, &mut inverses, scene, title);
    let counter = with(text_node(Some(camera_id), "counter", "CLICKS: 0", font, 10.0, [0.75, 0.8, 1.0, 1.0], Vec2::ZERO), NodeBehavior::UiAnchor(UiAnchor { anchor: Vec2::new(1.0, 0.0), offset: Vec2::new(-80.0, 20.0) }));
    add_node(document, &mut inverses, scene, counter);
    let hint = with(text_node(Some(camera_id), "hint", "CLICK A BUTTON (SOUND STARTS AFTER YOUR FIRST CLICK)", font, 7.0, [0.55, 0.58, 0.7, 1.0], Vec2::ZERO), NodeBehavior::UiAnchor(UiAnchor { anchor: Vec2::new(0.5, 0.0), offset: Vec2::new(0.0, 46.0) }));
    add_node(document, &mut inverses, scene, hint);

    // A centred column of buttons.
    let stack = with(
        with(with_id(Node::new("Buttons", Some(camera_id), NodeKind::Group(GroupNode::default())), "buttons"), NodeBehavior::UiStack(UiStack { spacing: 12.0, ..Default::default() })),
        NodeBehavior::UiAnchor(UiAnchor { anchor: Vec2::new(0.5, 0.5), offset: Vec2::new(0.0, 110.0) }),
    );
    let stack_id = add_node(document, &mut inverses, scene, stack);
    for (id, label, color) in [
        ("beep", "BEEP", [0.27, 0.55, 0.95, 1.0]),
        ("laser", "LASER", [0.85, 0.3, 0.55, 1.0]),
        ("coin", "COIN", [0.95, 0.72, 0.2, 1.0]),
        ("boom", "EXPLOSION", [0.9, 0.38, 0.22, 1.0]),
        ("mute", "MUTE", [0.35, 0.37, 0.45, 1.0]),
    ] {
        let mut button = Node::new(label, Some(stack_id), NodeKind::Shape(ShapeNode { path: Path::rect(200.0, 34.0), fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None }));
        button.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::UiButton(UiButton::default()), enabled: true });
        let button_id = add_node(document, &mut inverses, scene, with_id(button, id));
        add_node(document, &mut inverses, scene, text_node(Some(button_id), &format!("{id}Label"), label, font, 10.0, [1.0, 1.0, 1.0, 1.0], Vec2::ZERO));
    }

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
                program: Some(ProgramDef::Code(SCRIPT.into())),
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

const SCRIPT: &str = r#"// UI & Sound Lab: buttons (UI Button + ref.clicked) play synthesized sounds.
const g = ctx.globals.lab || (ctx.globals.lab = { clicks: 0, muted: false })
const btn = (id) => getCompByID(id)
let clicked = false
if (btn('beep').clicked) { ctx.playTone({ freq: 660, duration: 0.12, type: 'square' }); clicked = true }
if (btn('laser').clicked) { ctx.playTone({ freq: 1400, to: 180, duration: 0.25, type: 'sawtooth', volume: 0.25 }); clicked = true }
if (btn('coin').clicked) {
  ctx.playTone({ freq: 988, duration: 0.08, type: 'square', volume: 0.25 })
  g.coinAt = ctx.time + 0.08
  clicked = true
}
if (g.coinAt && ctx.time >= g.coinAt) { ctx.playTone({ freq: 1319, duration: 0.25, type: 'square', volume: 0.25 }); g.coinAt = 0 }
if (btn('boom').clicked) { ctx.playTone({ type: 'noise', duration: 0.6, volume: 0.5, release: 0.55 }); clicked = true }
if (btn('mute').clicked) {
  g.muted = !g.muted
  ctx.setMasterVolume(g.muted ? 0 : 1)
  getCompByID('muteLabel').textContent = g.muted ? 'UNMUTE' : 'MUTE'
  clicked = true
}
if (clicked) {
  g.clicks++
  getCompByID('counter').textContent = 'CLICKS: ' + g.clicks
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_anchored_hud_and_a_stack_of_buttons() {
        let mut document = Document::new();
        let demo = build_demo_ui(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let count = |pred: &dyn Fn(&NodeBehavior) -> bool| artboard.node_ids().filter_map(|id| artboard.node(id)).filter(|n| n.behaviors.iter().any(|e| pred(&e.behavior))).count();
        assert_eq!(count(&|b| matches!(b, NodeBehavior::UiButton(_))), 5);
        assert_eq!(count(&|b| matches!(b, NodeBehavior::UiStack(_))), 1);
        assert_eq!(count(&|b| matches!(b, NodeBehavior::UiAnchor(_))), 4);
    }
}
