//! "HTML UI Lab" — real HTML + CSS in a scene (`UiHtml`), wired three
//! ways:
//!
//! - **`data-trigger`** — the HUD's "Party mode" button fires the state
//!   machine's `celebrate` trigger with no code; the machine flips between
//!   its Play and Party states.
//! - **`ref.dom`** — the script reaches into the HUD's live DOM: it reads
//!   the speed slider and the colour button, and writes the score and the
//!   current state back into it.
//! - **World-anchored HTML** — a speech bubble follows the spinning star
//!   around the scene, through the camera like any node.
//!
//! The HUD itself is pinned to the top-left with a UI Anchor.

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Command, ConditionDef, Document, GroupNode, InputDef, InputKind, LoopMode, Node, NodeBehavior, NodeBehaviorEntry, NodeKind, Path,
    PlaybackDirection, ProgramDef, ShapeNode, StateDef, StateMachineDef, TransitionDef, UiAnchor, UiButton, UiHtml,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, with_id};

pub(crate) struct HtmlDemo {
    pub scene: ArtboardId,
}

fn with(mut node: Node, behavior: NodeBehavior) -> Node {
    node.behaviors.push(NodeBehaviorEntry { behavior, enabled: true });
    node
}

const HUD_HTML: &str = r#"<div class="hud">
  <div class="title">HTML UI <span class="tag" id="state">PLAY</span></div>
  <div class="score">Score <b id="score">0</b></div>
  <label>Spin speed <input id="speed" type="range" min="0" max="10" step="0.1" value="3"></label>
  <div class="row">
    <button id="color">New colour</button>
    <button class="party" data-trigger="celebrate">Party mode</button>
  </div>
  <p class="hint">Click the star to score.</p>
</div>"#;

const HUD_CSS: &str = r#".hud {
  box-sizing: border-box; width: 100%; height: 100%; padding: 14px 16px;
  font: 13px/1.4 system-ui, sans-serif; color: #eef0ff;
  background: rgb(20 22 40 / 0.72); border: 1px solid rgb(255 255 255 / 0.14);
  border-radius: 14px; backdrop-filter: blur(6px);
  box-shadow: 0 12px 32px rgb(0 0 0 / 0.4);
  display: flex; flex-direction: column; gap: 8px;
}
.title { font-weight: 700; font-size: 16px; display: flex; align-items: center; gap: 8px; }
.tag { font-size: 10px; letter-spacing: 0.08em; padding: 2px 7px; border-radius: 999px; background: #3b82f6; }
.tag.party { background: linear-gradient(90deg, #f43f5e, #f59e0b, #22c55e, #3b82f6, #a855f7); }
.score b { font-size: 20px; color: #fde68a; }
label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; opacity: 0.9; }
input[type=range] { accent-color: #a78bfa; }
.row { display: flex; gap: 8px; }
button {
  flex: 1; font: inherit; font-weight: 600; padding: 7px 0; border: 0; border-radius: 9px;
  color: #fff; background: #4f46e5; cursor: pointer; transition: transform 0.1s, filter 0.1s;
}
button:hover { filter: brightness(1.15); }
button:active { transform: scale(0.96); }
button.party { background: linear-gradient(90deg, #db2777, #f59e0b); }
.hint { margin: 0; font-size: 11px; opacity: 0.6; }"#;

const BUBBLE_HTML: &str = r#"<div class="bubble">Click me!</div>"#;
const BUBBLE_CSS: &str = r#".bubble {
  box-sizing: border-box; width: 100%; height: 100%; display: flex; align-items: center; justify-content: center;
  font: 700 15px system-ui, sans-serif; color: #1e1b4b; background: #fff; border-radius: 14px;
  box-shadow: 0 6px 18px rgb(0 0 0 / 0.3); position: relative;
}
.bubble::after {
  content: ''; position: absolute; left: 50%; bottom: -9px; margin-left: -9px;
  border: 9px solid transparent; border-bottom: 0; border-top-color: #fff;
}"#;

pub(crate) fn build_demo_html(document: &mut Document) -> HtmlDemo {
    let mut inverses = Vec::new();
    let inverse = document.apply(Command::AddArtboard { name: "HTML UI Lab".into(), width: 960.0, height: 540.0 });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);

    let camera = with_id(
        Node::with_components("Main Camera", None, vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.05, 0.06, 0.1, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })]),
        "camera",
    );
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));

    // The star (a UI Button, so clicks count) on a slow orbit.
    let mut star = with_id(Node::new("Star", None, NodeKind::Shape(ShapeNode { path: Path::star(5, 70.0, 30.0), fill_color: [0.98, 0.75, 0.25, 1.0], blend_mode: Default::default(), gradient: None, image: None, stroke: None })), "star");
    star.transform.translation = Vec2::new(150.0, -20.0);
    star.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::UiButton(UiButton::default()), enabled: true });
    add_node(document, &mut inverses, scene, star);

    // A speech bubble that follows the star (moved by the script).
    let mut bubble = with(
        with_id(Node::new("Bubble", None, NodeKind::Group(GroupNode::default())), "bubble"),
        NodeBehavior::UiHtml(UiHtml { html: BUBBLE_HTML.into(), css: BUBBLE_CSS.into(), width: 120.0, height: 44.0, interactive: false }),
    );
    bubble.transform.translation = Vec2::new(150.0, 80.0);
    add_node(document, &mut inverses, scene, bubble);

    // The HUD, pinned top-left.
    let hud = with(
        with(
            with_id(Node::new("HUD", Some(camera_id), NodeKind::Group(GroupNode::default())), "hud"),
            NodeBehavior::UiHtml(UiHtml { html: HUD_HTML.into(), css: HUD_CSS.into(), width: 280.0, height: 196.0, interactive: true }),
        ),
        NodeBehavior::UiAnchor(UiAnchor { anchor: Vec2::new(0.0, 1.0), offset: Vec2::new(164.0, -122.0) }),
    );
    add_node(document, &mut inverses, scene, hud);

    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Lab (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    let flip = |to: &str| TransitionDef { to: to.into(), condition: ConditionDef::Trigger { input: "celebrate".into() }, direction: PlaybackDirection::Forward, duration_seconds: 0.0, interruptible: true };
    let state = |name: &str, to: &str, x: f32| StateDef {
        name: name.into(),
        animation,
        loop_mode: LoopMode::Once,
        transitions: vec![flip(to)],
        position: Vec2::new(x, 0.0),
        blend: None,
        program: Some(ProgramDef::Code(SCRIPT.into())),
    };
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "HTML UI Lab".into(),
            inputs: vec![InputDef { name: "celebrate".into(), kind: InputKind::Trigger }],
            states: vec![state("Play", "Party", 0.0), state("Party", "Play", 220.0)],
            initial_state: "Play".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));
    inverses.reverse();
    document.commit_batch(inverses);
    HtmlDemo { scene }
}

const SCRIPT: &str = r#"// HTML UI Lab. The HUD's "Party mode" button needs no code — its
// data-trigger="celebrate" fires the state machine's trigger, which flips
// between the Play and Party states (this same script runs in both).
const g = ctx.globals.html || (ctx.globals.html = { score: 0, speed: 3, hue: 40, angle: 0, wired: null })
const party = ctx.state === 'Party'
const star = getCompByID('star')
const dom = getCompByID('hud').dom

// Reach into the HUD's live DOM once it exists (ref.dom is a ShadowRoot).
if (dom && g.wired !== dom) {
  g.wired = dom
  dom.querySelector('#speed').addEventListener('input', (e) => { g.speed = Number(e.target.value) })
  dom.querySelector('#color').addEventListener('click', () => { g.hue = (g.hue + 70) % 360 })
}

// Spin and orbit the star; party mode spins faster with a rainbow.
g.angle += (party ? g.speed * 3 : g.speed) * ctx.deltaSeconds
star.rotation = g.angle * 20
star.x = 150 + Math.cos(ctx.time * 0.6) * 120
star.y = -20 + Math.sin(ctx.time * 0.9) * 60
const hue = party ? (ctx.time * 240) % 360 : g.hue
const k = (n) => (n + hue / 30) % 12
const f = (n) => 0.55 - 0.45 * Math.max(-1, Math.min(k(n) - 3, 9 - k(n), 1))
star.fillColor = [f(0), f(8), f(4), 1]

// The bubble follows the star (an HTML element anchored in the world).
const bubble = getCompByID('bubble')
bubble.x = star.x
bubble.y = star.y + 95

if (star.clicked) g.score += party ? 5 : 1

// Write back into the HUD.
if (dom) {
  const score = dom.querySelector('#score')
  if (score.textContent !== String(g.score)) score.textContent = String(g.score)
  const tag = dom.querySelector('#state')
  const label = party ? 'PARTY' : 'PLAY'
  if (tag.textContent !== label) { tag.textContent = label; tag.className = party ? 'tag party' : 'tag' }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_hud_a_bubble_and_two_states() {
        let mut document = Document::new();
        let demo = build_demo_html(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        let html = artboard.node_ids().filter_map(|id| artboard.node(id)).filter(|n| n.behaviors.iter().any(|e| matches!(e.behavior, NodeBehavior::UiHtml(_)))).count();
        assert_eq!(html, 2);
        let (_, machine) = artboard.state_machines().next().unwrap();
        assert_eq!(machine.states.len(), 2);
    }
}
