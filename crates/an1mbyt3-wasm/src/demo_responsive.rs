//! "Responsive Layout" — one scene that reflows for phones and desktops
//! (Rive-style layouts):
//!
//! - The scene's **Screen** is *Responsive*: in Playground / an exported
//!   game the canvas fills the whole window, and the visible area follows
//!   it.
//! - A full-screen **UI Layout** page (column): header, body, footer.
//! - The header is a row — logo on the left, then either a nav bar
//!   (**Show When: Regular**) or a MENU button (**Show When: Compact**).
//! - The body is a row of a sidebar (desktop only) and a content area whose
//!   cards **wrap** onto more lines; on compact screens the body stacks as
//!   a column (the layout's *compact* style).
//! - The footer's text comes from the script reading `ctx.screen`.
//!
//! Try it in Design with the zoom bar's "preview on" picker (Phone /
//! Tablet / Desktop), or resize the window in Playground.

use an1mbyt3_core::{
    Animation, ArtboardId, CameraNode, Command, Document, FontAsset, FontId, GroupNode, LoopMode, Node, NodeBehavior, NodeBehaviorEntry, NodeId, NodeKind, Path,
    ProgramDef, ScreenMode, ScreenSettings, ShapeNode, StateDef, StateMachineDef, UiAlign, UiButton, UiEdges, UiFlexDirection, UiFlexStyle, UiJustify, UiLayout, UiLayoutItem,
    UiScreenCondition, UiShowWhen, UiSize, UiStateStyle,
};
use glam::Vec2;

use crate::demo_ninja::{add_node, text_node, with_id};

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PressStart2P-Regular.ttf");

pub(crate) struct ResponsiveDemo {
    pub scene: ArtboardId,
}

fn with(mut node: Node, behavior: NodeBehavior) -> Node {
    node.behaviors.push(NodeBehaviorEntry { behavior, enabled: true });
    node
}

fn rect(id: &str, parent: Option<NodeId>, size: Vec2, color: [f32; 4]) -> Node {
    with_id(Node::new(id, parent, NodeKind::Shape(ShapeNode { path: Path::rect(size.x, size.y), fill_color: color, blend_mode: Default::default(), gradient: None, image: None, stroke: None })), id)
}

fn group(id: &str, parent: Option<NodeId>) -> Node {
    with_id(Node::new(id, parent, NodeKind::Group(GroupNode::default())), id)
}

fn layout(style: UiFlexStyle, compact: Option<UiFlexStyle>, width: UiSize, height: UiSize) -> NodeBehavior {
    NodeBehavior::UiLayout(UiLayout { style, compact, width, height })
}

fn item(width: UiSize, height: UiSize) -> NodeBehavior {
    NodeBehavior::UiLayoutItem(UiLayoutItem { width, height, ..Default::default() })
}

fn show(when: UiScreenCondition) -> NodeBehavior {
    NodeBehavior::UiShowWhen(UiShowWhen { when })
}

/// A clickable rectangle with a centred label.
#[allow(clippy::too_many_arguments)]
fn add_button(document: &mut Document, inverses: &mut Vec<Command>, scene: ArtboardId, parent: NodeId, font: FontId, id: &str, label: &str, size: Vec2, color: [f32; 4], extra: Vec<NodeBehavior>) -> NodeId {
    let style = UiStateStyle { highlighted_color: [1.25, 1.25, 1.3, 1.0], pressed_scale: 0.96, ..Default::default() };
    let mut node = with(rect(id, Some(parent), size, color), NodeBehavior::UiButton(UiButton { interactable: true, style }));
    for b in extra {
        node = with(node, b);
    }
    let node_id = add_node(document, inverses, scene, node);
    add_node(document, inverses, scene, text_node(Some(node_id), &format!("{id}Label"), label, font, 8.0, [0.92, 0.94, 1.0, 1.0], Vec2::ZERO));
    node_id
}

pub(crate) fn build_demo_responsive(document: &mut Document) -> ResponsiveDemo {
    let mut inverses = Vec::new();
    let font = document.add_font(FontAsset { name: "Press Start 2P".into(), data: FONT_BYTES.to_vec() });
    let inverse = document.apply(Command::AddArtboard { name: "Responsive Layout".into(), width: 1280.0, height: 720.0 });
    let Command::SetArtboardDeleted { artboard: scene, .. } = inverse else { unreachable!("AddArtboard's inverse is always a SetArtboardDeleted") };
    inverses.push(inverse);
    inverses.push(document.apply(Command::SetArtboardScreen { artboard: scene, value: ScreenSettings { mode: ScreenMode::Responsive, match_width_or_height: 0.5, compact_width: 700.0 } }));

    let camera = with_id(
        Node::with_components("Main Camera", None, vec![NodeBehavior::Camera(CameraNode { zoom: 1.0, background_color: [0.06, 0.07, 0.1, 1.0], follow_target: None, follow_smoothing: 0.0, vignette: Default::default(), pixel_perfect: Default::default() })]),
        "camera",
    );
    let camera_id = add_node(document, &mut inverses, scene, camera);
    inverses.push(document.apply(Command::SetActiveCamera { artboard: scene, value: Some(camera_id) }));

    let column = |gap: f32, padding: f32| UiFlexStyle { direction: UiFlexDirection::Column, gap, align: UiAlign::Stretch, padding: UiEdges::all(padding), ..Default::default() };

    // The page: header / body / footer, filling the screen.
    let page = with(group("page", Some(camera_id)), layout(column(14.0, 20.0), Some(column(10.0, 10.0)), UiSize::Fill, UiSize::Fill));
    let page_id = add_node(document, &mut inverses, scene, page);

    // Header: logo, then nav (desktop) or MENU (phones).
    let header_style = UiFlexStyle { direction: UiFlexDirection::Row, justify: UiJustify::SpaceBetween, align: UiAlign::Center, gap: 10.0, padding: UiEdges { top: 0.0, right: 14.0, bottom: 0.0, left: 18.0 }, ..Default::default() };
    let header = with(with(rect("header", Some(page_id), Vec2::new(100.0, 56.0), [0.14, 0.16, 0.24, 1.0]), item(UiSize::Fill, UiSize::Fixed(56.0))), layout(header_style, None, UiSize::Fill, UiSize::Fixed(56.0)));
    let header_id = add_node(document, &mut inverses, scene, header);
    add_node(document, &mut inverses, scene, text_node(Some(header_id), "logo", "AN1MBYT3", font, 14.0, [1.0, 0.85, 0.4, 1.0], Vec2::ZERO));
    let nav = with(with(group("nav", Some(header_id)), layout(UiFlexStyle { direction: UiFlexDirection::Row, gap: 8.0, align: UiAlign::Center, padding: UiEdges::default(), ..Default::default() }, None, UiSize::Hug, UiSize::Hug)), show(UiScreenCondition::Regular));
    let nav_id = add_node(document, &mut inverses, scene, nav);
    for (id, label) in [("navHome", "HOME"), ("navGames", "GAMES"), ("navAbout", "ABOUT")] {
        add_button(document, &mut inverses, scene, nav_id, font, id, label, Vec2::new(96.0, 30.0), [0.22, 0.25, 0.36, 1.0], Vec::new());
    }
    add_button(document, &mut inverses, scene, header_id, font, "menu", "MENU", Vec2::new(80.0, 30.0), [0.3, 0.36, 0.6, 1.0], vec![show(UiScreenCondition::Compact)]);

    // Body: sidebar + wrapping cards (a column on phones).
    let row = UiFlexStyle { direction: UiFlexDirection::Row, gap: 14.0, align: UiAlign::Stretch, padding: UiEdges::default(), ..Default::default() };
    let body = with(with(group("body", Some(page_id)), layout(row, Some(column(10.0, 0.0)), UiSize::Fill, UiSize::Fill)), item(UiSize::Fill, UiSize::Fill));
    let body_id = add_node(document, &mut inverses, scene, body);
    let sidebar = with(
        with(with(rect("sidebar", Some(body_id), Vec2::new(220.0, 100.0), [0.1, 0.11, 0.16, 1.0]), item(UiSize::Fixed(220.0), UiSize::Fill)), layout(column(8.0, 12.0), None, UiSize::Fixed(220.0), UiSize::Fill)),
        show(UiScreenCondition::Regular),
    );
    let sidebar_id = add_node(document, &mut inverses, scene, sidebar);
    for (i, label) in ["DASHBOARD", "PROJECTS", "ASSETS", "SETTINGS"].into_iter().enumerate() {
        add_button(document, &mut inverses, scene, sidebar_id, font, &format!("side{i}"), label, Vec2::new(196.0, 30.0), [0.16, 0.18, 0.26, 1.0], vec![item(UiSize::Fill, UiSize::Hug)]);
    }
    let cards_style = UiFlexStyle { direction: UiFlexDirection::Row, wrap: true, gap: 12.0, justify: UiJustify::Start, align: UiAlign::Start, padding: UiEdges::default() };
    let content = with(with(group("content", Some(body_id)), layout(cards_style, Some(UiFlexStyle { justify: UiJustify::Center, ..cards_style }), UiSize::Fill, UiSize::Fill)), item(UiSize::Fill, UiSize::Fill));
    let content_id = add_node(document, &mut inverses, scene, content);
    let palette = [[0.85, 0.35, 0.45, 1.0], [0.35, 0.6, 0.95, 1.0], [0.4, 0.8, 0.55, 1.0], [0.95, 0.7, 0.3, 1.0], [0.65, 0.45, 0.9, 1.0], [0.3, 0.75, 0.8, 1.0]];
    for (i, color) in palette.into_iter().enumerate() {
        add_button(document, &mut inverses, scene, content_id, font, &format!("card{i}"), &format!("CARD {}", i + 1), Vec2::new(220.0, 130.0), color, Vec::new());
    }

    // Footer: the script writes the screen info here.
    let footer = with(rect("footer", Some(page_id), Vec2::new(100.0, 34.0), [0.12, 0.13, 0.19, 1.0]), item(UiSize::Fill, UiSize::Fixed(34.0)));
    let footer_id = add_node(document, &mut inverses, scene, footer);
    add_node(document, &mut inverses, scene, text_node(Some(footer_id), "status", "SCREEN", font, 7.0, [0.65, 0.7, 0.85, 1.0], Vec2::ZERO));

    let inverse = document.apply(Command::AddAnimation { artboard: scene, animation: Animation::new("Layout (code)") });
    let Command::SetAnimationDeleted { animation, .. } = inverse else { unreachable!("AddAnimation's inverse is always a SetAnimationDeleted") };
    inverses.push(inverse);
    inverses.push(document.apply(Command::AddStateMachine {
        artboard: scene,
        state_machine: StateMachineDef {
            name: "Responsive Layout".into(),
            inputs: Vec::new(),
            states: vec![StateDef {
                name: "Run".into(),
                animation,
                loop_mode: LoopMode::Once,
                transitions: Vec::new(),
                position: Vec2::ZERO,
                blend: None,
                program: Some(ProgramDef::Code(SCRIPT.into())),
            }],
            initial_state: "Run".into(),
            layers: Vec::new(),
            deleted: false,
        },
    }));
    inverses.reverse();
    document.commit_batch(inverses);
    ResponsiveDemo { scene }
}

const SCRIPT: &str = r#"// Responsive Layout: the layout itself is all components (UI Layout,
// UI Layout Item, UI Show When). The script only reports the screen and
// reacts to clicks, using ctx.screen like a CSS media query.
const g = ctx.globals.layout || (ctx.globals.layout = { last: '' })
const s = ctx.screen
for (let i = 0; i < 6; i++) if (getCompByID('card' + i).clicked) g.last = 'CARD ' + (i + 1)
if (getCompByID('menu').clicked) g.last = 'MENU'
const kind = s.compact ? 'COMPACT' : 'REGULAR'
const turn = s.portrait ? 'PORTRAIT' : 'LANDSCAPE'
const text = Math.round(s.screenWidth) + 'X' + Math.round(s.screenHeight) + ' PX  ' + kind + '  ' + turn + (g.last ? '  CLICKED ' + g.last : '')
const status = getCompByID('status')
if (status.textContent !== text) status.textContent = text
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_responsive_page() {
        let mut document = Document::new();
        let demo = build_demo_responsive(&mut document);
        let artboard = document.artboard(demo.scene).unwrap();
        assert_eq!(artboard.screen.mode, ScreenMode::Responsive);
        let count = |pred: &dyn Fn(&NodeBehavior) -> bool| artboard.node_ids().filter_map(|id| artboard.node(id)).filter(|n| n.behaviors.iter().any(|e| pred(&e.behavior))).count();
        assert_eq!(count(&|b| matches!(b, NodeBehavior::UiLayout(_))), 6);
        assert_eq!(count(&|b| matches!(b, NodeBehavior::UiShowWhen(_))), 3);
    }
}
