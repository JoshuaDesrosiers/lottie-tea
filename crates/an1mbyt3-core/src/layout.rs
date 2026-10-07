//! Responsive screens and flexbox UI layout — what lets one scene work on a
//! phone held upright, a tablet and a wide desktop window, the way Rive's
//! Layouts (and Unity's Canvas Scaler + layout groups) do.
//!
//! Three pieces:
//!
//! - **[`ScreenSettings`]** (per artboard): `Fixed` shows the artboard's
//!   design size letterboxed, exactly as before; `Responsive` makes the
//!   view fill the whole canvas / page, resizing the visible world area
//!   with the screen. How much it grows is Unity's *Scale With Screen
//!   Size* rule ([`responsive_view`]): the artboard size is the reference
//!   resolution and `match_width_or_height` picks which screen axis keeps
//!   the design's scale.
//! - **[`UiLayout`]** (a container) + **[`UiLayoutItem`]** (optional, on a
//!   child): CSS flexbox — direction, wrap, justify, align, gap, padding,
//!   and Hug / Fill / Fixed / Percent sizes — solved by `taffy` in
//!   [`solve_layouts`]. Rectangle shapes in a layout are *resized* to their
//!   slot (a panel that fills its column), everything else is positioned.
//!   A layout can switch to a different style when the screen is
//!   **compact** (narrower than the artboard's breakpoint) — a row of
//!   cards on desktop that becomes a column on a phone.
//! - **[`UiShowWhen`]**: shows a node only on compact / regular /
//!   portrait / landscape screens (a hamburger button on phones, a full
//!   nav bar on desktop).
//!
//! Scripts and state machines see the same screen facts (`ctx.screen`, and
//! the `screenWidth` / `screenHeight` / `isCompact` / `isPortrait` inputs),
//! so anything else can be made conditional on the layout too.

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::{Artboard, NodeBehavior, NodeId, NodeKind, PathCommand};

// ---------------------------------------------------------------- screen --

/// How a scene meets the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ScreenMode {
    /// The artboard's design size, scaled to fit and letterboxed.
    #[default]
    Fixed,
    /// The view fills the whole canvas / page; its world size follows the
    /// screen (see [`responsive_view`]). UI Anchors, UI Layouts and
    /// `ctx.screen` then adapt to it.
    Responsive,
}

/// An artboard's screen behaviour (Unity's Canvas Scaler + a breakpoint).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenSettings {
    pub mode: ScreenMode,
    /// Responsive only — 0 = keep the design's *width* scale (a wider
    /// screen shows more height), 1 = keep its *height* scale, 0.5 = blend
    /// (Unity's `matchWidthOrHeight`).
    pub match_width_or_height: f32,
    /// Screens narrower than this many CSS pixels are **compact** (phones):
    /// UI Layouts use their compact style and *Show When: Compact* nodes
    /// appear.
    pub compact_width: f32,
}

impl Default for ScreenSettings {
    fn default() -> Self {
        Self { mode: ScreenMode::Fixed, match_width_or_height: 0.5, compact_width: 700.0 }
    }
}

/// What the layout system knows about the screen this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenInfo {
    /// The visible world area (world units) — what UI Anchors pin to and a
    /// root UI Layout with *Fill* size fills.
    pub view: Vec2,
    /// The screen / canvas size in CSS pixels (breakpoints compare this).
    pub screen: Vec2,
    pub compact: bool,
    pub portrait: bool,
}

impl ScreenInfo {
    /// Classifies `screen` (CSS px) against `settings`' breakpoint.
    pub fn new(view: Vec2, screen: Vec2, settings: &ScreenSettings) -> Self {
        Self { view, screen, compact: screen.x < settings.compact_width, portrait: screen.y > screen.x }
    }
}

/// The world-unit view size for a responsive screen: `reference` is the
/// design size (artboard / camera zoom), `screen` the screen in CSS px.
/// Unity's *Scale With Screen Size*: the scale factor is the log-space
/// blend of the width and height ratios, so the design's width (match 0),
/// height (match 1) or a mix keeps its size on screen, and the other axis
/// shows more or less of the world.
pub fn responsive_view(reference: Vec2, screen: Vec2, settings: &ScreenSettings) -> Vec2 {
    let reference = reference.max(Vec2::splat(1.0));
    let screen = screen.max(Vec2::splat(1.0));
    let m = settings.match_width_or_height.clamp(0.0, 1.0);
    let log_w = (screen.x / reference.x).ln();
    let log_h = (screen.y / reference.y).ln();
    let scale = (log_w + (log_h - log_w) * m).exp();
    screen / scale.max(1e-4)
}

// ---------------------------------------------------------------- layout --

/// How big a layout box is along one axis.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum UiSize {
    /// Fit the content (a leaf's own bounds; a container's children).
    #[default]
    Hug,
    /// Take the remaining space (flex-grow along the parent's direction,
    /// stretch across it; a root fills the screen / its parent).
    Fill,
    /// Exactly this many world units.
    Fixed(f32),
    /// This percentage (0..100) of the parent's size.
    Percent(f32),
}

/// Flexbox main-axis direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UiFlexDirection {
    Row,
    #[default]
    Column,
    RowReverse,
    ColumnReverse,
}

impl UiFlexDirection {
    fn is_row(self) -> bool {
        matches!(self, Self::Row | Self::RowReverse)
    }
}

/// Flexbox `justify-content` (main axis).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UiJustify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Flexbox `align-items` / `align-self` (cross axis).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UiAlign {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

/// Four edge distances (padding, margin), world units.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct UiEdges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl UiEdges {
    pub fn all(v: f32) -> Self {
        Self { top: v, right: v, bottom: v, left: v }
    }
}

/// The part of a [`UiLayout`] a breakpoint can swap out.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiFlexStyle {
    pub direction: UiFlexDirection,
    /// Wrap onto more lines when the children don't fit.
    pub wrap: bool,
    pub justify: UiJustify,
    pub align: UiAlign,
    /// Space between children (and between wrapped lines).
    pub gap: f32,
    pub padding: UiEdges,
}

impl Default for UiFlexStyle {
    fn default() -> Self {
        Self { direction: UiFlexDirection::Column, wrap: false, justify: UiJustify::Start, align: UiAlign::Start, gap: 8.0, padding: UiEdges::default() }
    }
}

/// A flexbox container (Rive's Layout, CSS `display: flex`): lays out its
/// visible children. A layout whose parent isn't a layout is a **root**: it
/// stays where it's placed (or wherever its UI Anchor puts it) and, sized
/// *Fill*, fills the screen (under the Camera) or its parent shape.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiLayout {
    pub style: UiFlexStyle,
    /// Used instead of `style` on compact (phone-width) screens.
    #[serde(default)]
    pub compact: Option<UiFlexStyle>,
    /// This container's own size.
    pub width: UiSize,
    pub height: UiSize,
}

impl Default for UiLayout {
    fn default() -> Self {
        Self { style: UiFlexStyle::default(), compact: None, width: UiSize::Hug, height: UiSize::Hug }
    }
}

/// Optional per-child settings inside a [`UiLayout`] (CSS flex item).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiLayoutItem {
    pub width: UiSize,
    pub height: UiSize,
    /// Share of leftover main-axis space (flex-grow).
    pub grow: f32,
    /// Share of overflow to give up (flex-shrink).
    pub shrink: f32,
    /// Overrides the container's `align` for this child.
    pub align_self: Option<UiAlign>,
    pub margin: UiEdges,
    /// Leave this child out of the layout entirely (it keeps its position).
    pub ignore: bool,
}

impl Default for UiLayoutItem {
    fn default() -> Self {
        Self { width: UiSize::Hug, height: UiSize::Hug, grow: 0.0, shrink: 1.0, align_self: None, margin: UiEdges::default(), ignore: false }
    }
}

/// Which screens a [`UiShowWhen`] node appears on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UiScreenCondition {
    /// Narrower than the artboard's breakpoint (phones).
    #[default]
    Compact,
    /// At least the breakpoint wide (tablets landscape, desktops).
    Regular,
    /// Taller than wide.
    Portrait,
    /// Wider than tall.
    Landscape,
}

/// Shows the node only when the screen matches (else hides it — and a
/// hidden child takes no space in its layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiShowWhen {
    pub when: UiScreenCondition,
}

impl UiShowWhen {
    pub fn passes(&self, screen: &ScreenInfo) -> bool {
        match self.when {
            UiScreenCondition::Compact => screen.compact,
            UiScreenCondition::Regular => !screen.compact,
            UiScreenCondition::Portrait => screen.portrait,
            UiScreenCondition::Landscape => !screen.portrait,
        }
    }
}

/// One node moved (and maybe resized) by [`solve_layouts`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutWrite {
    pub node: NodeId,
    /// New local translation (`None` = a root, which stays put).
    pub translation: Option<Vec2>,
    /// A rectangle shape's new path size (local units), when its slot
    /// differs from its drawn size.
    pub rect_size: Option<Vec2>,
}

/// A rectangle path's `(center, size)` — `Path::rect` and any axis-aligned
/// 4-corner closed path; `None` for anything else (those aren't resized).
pub fn rect_path_box(commands: &[PathCommand]) -> Option<(Vec2, Vec2)> {
    let [PathCommand::MoveTo(a), PathCommand::LineTo(b), PathCommand::LineTo(c), PathCommand::LineTo(d), PathCommand::Close] = commands else { return None };
    let pts = [*a, *b, *c, *d];
    let min = pts.iter().fold(Vec2::splat(f32::INFINITY), |m, p| m.min(*p));
    let max = pts.iter().fold(Vec2::splat(f32::NEG_INFINITY), |m, p| m.max(*p));
    // Every corner on the box's edges in both axes = axis-aligned.
    let on_edge = |v: f32, lo: f32, hi: f32| (v - lo).abs() < 1e-3 || (v - hi).abs() < 1e-3;
    if !pts.iter().all(|p| on_edge(p.x, min.x, max.x) && on_edge(p.y, min.y, max.y)) {
        return None;
    }
    Some(((min + max) * 0.5, max - min))
}

fn layout_of(artboard: &Artboard, id: NodeId) -> Option<UiLayout> {
    artboard.node(id)?.behaviors.iter().find_map(|e| match &e.behavior {
        NodeBehavior::UiLayout(l) if e.enabled => Some(*l),
        _ => None,
    })
}

fn item_of(artboard: &Artboard, id: NodeId) -> Option<UiLayoutItem> {
    artboard.node(id)?.behaviors.iter().find_map(|e| match &e.behavior {
        NodeBehavior::UiLayoutItem(l) if e.enabled => Some(*l),
        _ => None,
    })
}

/// Everything about one laid-out node the solver needs afterwards.
struct Entry {
    node: NodeId,
    taffy: taffy::NodeId,
    /// `Some(index)` of the parent entry; `None` for a root.
    parent: Option<usize>,
    /// Local offset of the node's own box centre from its origin (scaled).
    offset: Vec2,
    scale: Vec2,
    /// Its rectangle's current size (local units), if resizable.
    rect: Option<Vec2>,
    /// The size it'd have untouched (scaled), for "did it change".
    intrinsic: Vec2,
}

/// Solves every UI Layout in `artboard` for `screen`. `bounds` gives a
/// node's current local bounds `(min, max)` (its subtree, unscaled).
/// Pure: returns the writes, the caller applies them.
pub fn solve_layouts(artboard: &Artboard, screen: &ScreenInfo, bounds: &dyn Fn(NodeId) -> (Vec2, Vec2)) -> Vec<LayoutWrite> {
    use taffy::{AlignItems, AvailableSpace, Dimension, Size, TaffyTree};

    let alive = |id: NodeId| !artboard.is_effectively_deleted(id) && artboard.node(id).is_some_and(|n| n.visible && !n.deleted);
    let camera = artboard.active_camera();
    let roots: Vec<NodeId> = artboard
        .node_ids()
        .filter(|&id| alive(id) && layout_of(artboard, id).is_some())
        .filter(|&id| artboard.node(id).and_then(|n| n.parent).is_none_or(|p| layout_of(artboard, p).is_none()))
        .collect();
    if roots.is_empty() {
        return Vec::new();
    }

    let mut tree: TaffyTree<()> = TaffyTree::new();
    let mut entries: Vec<Entry> = Vec::new();

    let dim = |size: UiSize, intrinsic: f32, container: bool| -> Dimension {
        match size {
            UiSize::Hug if container => Dimension::auto(),
            UiSize::Hug => Dimension::length(intrinsic),
            UiSize::Fixed(v) => Dimension::length(v.max(0.0)),
            UiSize::Percent(p) => Dimension::percent(p / 100.0),
            UiSize::Fill => Dimension::auto(),
        }
    };
    let align = |a: UiAlign| match a {
        UiAlign::Start => AlignItems::FLEX_START,
        UiAlign::Center => AlignItems::CENTER,
        UiAlign::End => AlignItems::FLEX_END,
        UiAlign::Stretch => AlignItems::STRETCH,
    };

    // Builds `id`'s taffy node (recursively for containers). `parent_row`:
    // the parent layout's main axis (None = root). Returns the entry index.
    fn build(
        artboard: &Artboard,
        tree: &mut taffy::TaffyTree<()>,
        entries: &mut Vec<Entry>,
        id: NodeId,
        parent: Option<usize>,
        parent_row: Option<bool>,
        root_space: Vec2,
        screen: &ScreenInfo,
        bounds: &dyn Fn(NodeId) -> (Vec2, Vec2),
        alive: &dyn Fn(NodeId) -> bool,
        dim: &dyn Fn(UiSize, f32, bool) -> taffy::Dimension,
        align: &dyn Fn(UiAlign) -> taffy::AlignItems,
    ) -> usize {
        use taffy::{AlignContent, AlignItems, Dimension, Display, FlexDirection, FlexWrap, JustifyContent, LengthPercentage, LengthPercentageAuto, Rect, Size, Style};
        let node = artboard.node(id).expect("alive node");
        let scale = node.transform.scale.abs();
        let (min, max) = bounds(id);
        let intrinsic = ((max - min) * scale).max(Vec2::ZERO);
        let layout = layout_of(artboard, id);
        let item = item_of(artboard, id).unwrap_or_default();
        let rect = match &node.kind {
            NodeKind::Shape(s) => rect_path_box(&s.path.commands).map(|(_, size)| size),
            _ => None,
        };
        let (width, height) = match layout {
            // A container's own size, unless its item settings override it.
            Some(l) if item_of(artboard, id).is_none() => (l.width, l.height),
            _ => (item.width, item.height),
        };
        let container = layout.is_some();
        let mut style = Style { flex_shrink: item.shrink.max(0.0), flex_grow: item.grow.max(0.0), ..Default::default() };
        style.margin = Rect { left: LengthPercentageAuto::length(item.margin.left), right: LengthPercentageAuto::length(item.margin.right), top: LengthPercentageAuto::length(item.margin.top), bottom: LengthPercentageAuto::length(item.margin.bottom) };
        style.size = Size { width: dim(width, intrinsic.x, container), height: dim(height, intrinsic.y, container) };
        if let Some(a) = item.align_self {
            style.align_self = Some(align(a));
        }
        match parent_row {
            // A root: Fill takes the whole available space.
            None => {
                if width == UiSize::Fill {
                    style.size.width = Dimension::length(root_space.x);
                }
                if height == UiSize::Fill {
                    style.size.height = Dimension::length(root_space.y);
                }
            }
            Some(row) => {
                for (axis_fill, is_main) in [(width == UiSize::Fill, row), (height == UiSize::Fill, !row)] {
                    if !axis_fill {
                        continue;
                    }
                    if is_main {
                        style.flex_grow = style.flex_grow.max(1.0);
                        style.flex_basis = Dimension::length(0.0);
                    } else {
                        style.align_self = Some(AlignItems::STRETCH);
                    }
                }
                // A hugging leaf never shrinks below its own size.
                if !container {
                    if width == UiSize::Hug {
                        style.min_size.width = LengthPercentageAuto::length(intrinsic.x);
                    }
                    if height == UiSize::Hug {
                        style.min_size.height = LengthPercentageAuto::length(intrinsic.y);
                    }
                }
            }
        }
        let index = entries.len();
        let offset = if container || rect.is_some() && (width != UiSize::Hug || height != UiSize::Hug) { Vec2::ZERO } else { (min + max) * 0.5 * scale };
        if let Some(l) = layout {
            let flex = if screen.compact { l.compact.unwrap_or(l.style) } else { l.style };
            style.display = Display::Flex;
            style.flex_direction = match flex.direction {
                UiFlexDirection::Row => FlexDirection::Row,
                UiFlexDirection::Column => FlexDirection::Column,
                UiFlexDirection::RowReverse => FlexDirection::RowReverse,
                UiFlexDirection::ColumnReverse => FlexDirection::ColumnReverse,
            };
            style.flex_wrap = if flex.wrap { FlexWrap::Wrap } else { FlexWrap::NoWrap };
            style.justify_content = Some(match flex.justify {
                UiJustify::Start => JustifyContent::FLEX_START,
                UiJustify::Center => JustifyContent::CENTER,
                UiJustify::End => JustifyContent::FLEX_END,
                UiJustify::SpaceBetween => JustifyContent::SPACE_BETWEEN,
                UiJustify::SpaceAround => JustifyContent::SPACE_AROUND,
                UiJustify::SpaceEvenly => JustifyContent::SPACE_EVENLY,
            });
            style.align_items = Some(align(flex.align));
            style.align_content = Some(AlignContent::FLEX_START);
            style.gap = Size { width: LengthPercentage::length(flex.gap), height: LengthPercentage::length(flex.gap) };
            style.padding = Rect { left: LengthPercentage::length(flex.padding.left), right: LengthPercentage::length(flex.padding.right), top: LengthPercentage::length(flex.padding.top), bottom: LengthPercentage::length(flex.padding.bottom) };
            let taffy_id = tree.new_leaf(style).expect("taffy leaf");
            entries.push(Entry { node: id, taffy: taffy_id, parent, offset, scale, rect, intrinsic });
            let row = flex.direction.is_row();
            let kids: Vec<NodeId> = artboard.children_of(id).map(|(cid, _)| cid).filter(|&cid| alive(cid) && !item_of(artboard, cid).is_some_and(|i| i.ignore)).collect();
            let mut children = Vec::with_capacity(kids.len());
            for cid in kids {
                let child = build(artboard, tree, entries, cid, Some(index), Some(row), root_space, screen, bounds, alive, dim, align);
                children.push(entries[child].taffy);
            }
            tree.set_children(taffy_id, &children).expect("taffy children");
        } else {
            let taffy_id = tree.new_leaf(style).expect("taffy leaf");
            entries.push(Entry { node: id, taffy: taffy_id, parent, offset, scale, rect, intrinsic });
        }
        index
    }

    let mut root_indices = Vec::new();
    for &root in &roots {
        let parent = artboard.node(root).and_then(|n| n.parent);
        // Under the Camera (the HUD) or at the top: the screen. Inside a
        // shape: that shape's box.
        let space = match parent {
            Some(p) if Some(p) != camera => {
                let (min, max) = bounds(p);
                (max - min).max(Vec2::ONE)
            }
            _ => screen.view,
        };
        let index = build(artboard, &mut tree, &mut entries, root, None, None, space, screen, bounds, &alive, &dim, &align);
        let _ = tree.compute_layout(entries[index].taffy, Size { width: AvailableSpace::Definite(space.x), height: AvailableSpace::Definite(space.y) });
        root_indices.push(index);
    }

    let mut writes = Vec::with_capacity(entries.len());
    for entry in &entries {
        let Ok(l) = tree.layout(entry.taffy) else { continue };
        let size = Vec2::new(l.size.width, l.size.height);
        // Rectangles take their slot's size (in their own local units).
        let rect_size = entry.rect.and_then(|current| {
            let wanted = size / entry.scale.max(Vec2::splat(1e-4));
            ((wanted - current).abs().max_element() > 1e-3 && (size - entry.intrinsic).abs().max_element() > 1e-3).then_some(wanted)
        });
        let translation = entry.parent.map(|p| {
            let parent_size = tree.layout(entries[p].taffy).map(|pl| Vec2::new(pl.size.width, pl.size.height)).unwrap_or(Vec2::ZERO);
            // taffy: top-left origin, y down. Ours: box centred on the
            // parent's origin, y up.
            let center = Vec2::new(l.location.x + size.x * 0.5 - parent_size.x * 0.5, parent_size.y * 0.5 - (l.location.y + size.y * 0.5));
            let offset = if rect_size.is_some() { Vec2::ZERO } else { entry.offset };
            center - offset
        });
        if translation.is_some() || rect_size.is_some() {
            writes.push(LayoutWrite { node: entry.node, translation, rect_size });
        }
    }
    writes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Document, GroupNode, Node, NodeBehaviorEntry, Path, ShapeNode};

    fn shape(name: &str, parent: Option<NodeId>, w: f32, h: f32) -> Node {
        Node::new(name, parent, NodeKind::Shape(ShapeNode { path: Path::rect(w, h), fill_color: [1.0; 4], blend_mode: Default::default(), gradient: None, image: None, stroke: None }))
    }

    fn add(document: &mut Document, artboard: crate::ArtboardId, node: Node) -> NodeId {
        let Command::SetDeleted { node: id, .. } = document.apply(Command::AddNode { artboard, node }) else { unreachable!() };
        id
    }

    /// Bounds straight from rectangle paths (what the editor's own bounds
    /// give for these test scenes).
    fn rect_bounds(artboard: &Artboard) -> impl Fn(NodeId) -> (Vec2, Vec2) + '_ {
        |id| match artboard.node(id).map(|n| &n.kind) {
            Some(NodeKind::Shape(s)) => rect_path_box(&s.path.commands).map(|(c, s)| (c - s * 0.5, c + s * 0.5)).unwrap_or((Vec2::ZERO, Vec2::ZERO)),
            _ => (Vec2::ZERO, Vec2::ZERO),
        }
    }

    #[test]
    fn responsive_view_keeps_the_matched_axis() {
        let s = ScreenSettings { mode: ScreenMode::Responsive, match_width_or_height: 1.0, ..Default::default() };
        // Match height: a twice-as-wide screen shows twice the width.
        let v = responsive_view(Vec2::new(640.0, 360.0), Vec2::new(2560.0, 720.0), &s);
        assert!((v.y - 360.0).abs() < 1e-3 && (v.x - 1280.0).abs() < 1e-2, "{v}");
        let w = ScreenSettings { match_width_or_height: 0.0, ..s };
        // Match width: a portrait phone keeps the design's width.
        let v = responsive_view(Vec2::new(640.0, 360.0), Vec2::new(390.0, 844.0), &w);
        assert!((v.x - 640.0).abs() < 1e-2, "{v}");
    }

    #[test]
    fn a_row_with_a_fill_child_splits_the_screen() {
        let mut document = Document::new();
        let Command::SetArtboardDeleted { artboard, .. } = document.apply(Command::AddArtboard { name: "A".into(), width: 400.0, height: 200.0 }) else { unreachable!() };
        let mut root = Node::new("Root", None, NodeKind::Group(GroupNode::default()));
        let row = UiFlexStyle { direction: UiFlexDirection::Row, gap: 0.0, align: UiAlign::Stretch, ..Default::default() };
        root.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::UiLayout(UiLayout { style: row, compact: Some(UiFlexStyle { direction: UiFlexDirection::Column, ..row }), width: UiSize::Fill, height: UiSize::Fill }), enabled: true });
        let root = add(&mut document, artboard, root);
        let mut sidebar = shape("Sidebar", Some(root), 100.0, 10.0);
        sidebar.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::UiLayoutItem(UiLayoutItem { height: UiSize::Fill, ..Default::default() }), enabled: true });
        let sidebar = add(&mut document, artboard, sidebar);
        let mut main = shape("Main", Some(root), 10.0, 10.0);
        main.behaviors.push(NodeBehaviorEntry { behavior: NodeBehavior::UiLayoutItem(UiLayoutItem { width: UiSize::Fill, ..Default::default() }), enabled: true });
        let main = add(&mut document, artboard, main);

        let a = document.artboard(artboard).unwrap();
        let wide = ScreenInfo { view: Vec2::new(400.0, 200.0), screen: Vec2::new(1200.0, 600.0), compact: false, portrait: false };
        let writes = solve_layouts(a, &wide, &rect_bounds(a));
        let get = |id| writes.iter().find(|w| w.node == id).copied().unwrap();
        // Sidebar: 100 wide at the left, stretched to full height.
        assert_eq!(get(sidebar).translation, Some(Vec2::new(-150.0, 0.0)));
        assert_eq!(get(sidebar).rect_size, Some(Vec2::new(100.0, 200.0)));
        // Main: the remaining 300 wide; its Hug height (10) stays, at the
        // top (CSS: only auto-sized items stretch).
        assert_eq!(get(main).translation, Some(Vec2::new(50.0, 95.0)));
        assert_eq!(get(main).rect_size, Some(Vec2::new(300.0, 10.0)));

        // Compact: the same scene stacks as a column.
        let phone = ScreenInfo { compact: true, portrait: true, ..wide };
        let writes = solve_layouts(a, &phone, &rect_bounds(a));
        let side = writes.iter().find(|w| w.node == sidebar).unwrap();
        // Column: the sidebar's "Fill" height now runs along the main axis
        // (shared with nothing else growing, so it takes the leftover
        // 190), and Main (10 tall) sits under it.
        let main_w = writes.iter().find(|w| w.node == main).unwrap();
        assert!(side.translation.unwrap().y > main_w.translation.unwrap().y, "sidebar above main");
        assert_eq!(main_w.translation.unwrap().y, -95.0);
    }

    #[test]
    fn show_when_follows_the_screen() {
        let screen = ScreenInfo::new(Vec2::ONE, Vec2::new(390.0, 844.0), &ScreenSettings::default());
        assert!(UiShowWhen { when: UiScreenCondition::Compact }.passes(&screen));
        assert!(UiShowWhen { when: UiScreenCondition::Portrait }.passes(&screen));
        assert!(!UiShowWhen { when: UiScreenCondition::Regular }.passes(&screen));
    }
}
