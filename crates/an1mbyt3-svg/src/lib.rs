//! SVG -> a flat list of [`ImportedShape`]s the editor can drop onto the
//! *current* artboard — deliberately not a whole-`Document` importer the way
//! `an1mbyt3-lottie::import_lottie` is (that one owns its own artboard,
//! sized to the Lottie composition); an SVG drop-in is closer to "paste this
//! artwork into what I'm already working on" than "open a new file," so this
//! crate hands back raw geometry/color pairs and leaves node creation
//! (`Command::AddNode`, batched as one undo step) to the caller
//! (`crates/an1mbyt3-wasm/src/editor.rs`).
//!
//! Supported: `<path d="...">`, plus the common non-path primitives
//! (`<rect>`, `<circle>`, `<ellipse>`, `<polygon>`, `<polyline>`, `<line>`),
//! nested inside any depth of `<g>` groups, each with its own `fill`/
//! `fill-opacity`/`opacity`/`transform` — as either plain presentation
//! attributes or an inline `style="fill: ...; opacity: ..."` (the latter
//! wins per the CSS cascade, see `parse_style`). `fill` inherits down from
//! an ancestor `<g>` (the common "color set once on a wrapping group"
//! pattern); `fill-opacity`/`opacity` are read per-element only (no
//! inheritance chain) — simpler, and correct for the overwhelmingly common
//! case where opacity is only ever set on the specific shape that needs it.
//!
//! `<use href="#id">`/the older `xlink:href="#id"` form both resolve — the
//! referenced element (found anywhere in the document, most commonly inside
//! a `<defs>` block) is visited as if it sat at the `<use>`'s own location,
//! with the `<use>`'s own `x`/`y`/`transform` composed on top. `<defs>` (and
//! `<symbol>`/`<clipPath>`/`<mask>`/`<pattern>`) are never walked directly —
//! only reachable through a `<use>` reference, exactly like a real renderer
//! — so a file that stashes its whole icon set in `<defs>` doesn't dump
//! every icon onto the canvas unreferenced.
//!
//! A `<!DOCTYPE ...>` declaration (near-universal in Illustrator/Inkscape
//! exports) is tolerated, not a hard failure — see [`import_svg`]'s doc
//! comment for why.
//!
//! `fill="url(#gradientId)"`/`style="fill: url(#gradientId)"` referencing a
//! `<linearGradient>`/`<radialGradient>` resolves too (see [`resolve_fill`])
//! — averaged down to one flat color across the gradient's own `<stop>`s,
//! since `ShapeNode` has no gradient-fill concept to actually paint one with.
//! Character/illustration artwork routinely fills its main body shapes with
//! a gradient rather than a flat color; before this resolved, every such
//! shape silently fell through to whatever ancestor fill was in scope
//! (usually none), which is why gradient-heavy files imported as solid black
//! silhouettes. `display: none`/`visibility: hidden` (attribute or inline
//! `style`) hides an element's whole subtree — Illustrator/Figma exports
//! commonly carry hidden alternate-color/guide layers alongside the visible
//! artwork, which would otherwise import as extra, wrong-colored geometry
//! layered on top of the real shapes.
//!
//! **Every accumulated transform (`viewBox`'s own offset, every ancestor
//! `<g transform="...">`, the element's own `transform`) is baked directly
//! into the shape's path coordinates**, not translated into a
//! `an1mbyt3_core::Transform2D` — `Transform2D` only has translation/
//! rotation/uniform-scale, with no representation for skew or the general
//! case a `matrix()`/nested-rotate transform can produce, while an affine
//! matrix applied straight to path points has no such limitation. Every
//! returned shape's node ends up with an identity `Transform2D` (aside from
//! the whole-import centering translate — see [`import_svg`]'s doc comment);
//! the shape's final position/orientation is already baked into its `Path`.
//!
//! **`<g>` groups are preserved as real hierarchy**: each `<g>` becomes an
//! [`ImportedNode::Group`] holding its children as an actual tree (not
//! flattened to siblings) — `editor.rs`'s `import_svg` wasm method turns
//! each `Group` into a real `NodeKind::Group` document node and parents the
//! imported shapes/subgroups under it (via `Command::ReparentNode`), so
//! Layers-panel hierarchy and group-level selection survive an import
//! exactly as authored. A `Group` node's own `Transform2D` is always left at
//! identity — every accumulated ancestor transform is still baked straight
//! into each leaf shape's path coordinates (see below), so a `Group` exists
//! purely for organization/masking, contributing nothing to the actual
//! composed transform.
//!
//! **`clip-path`/`mask` references are approximated via the document's real
//! masking mechanism**, not computed as a geometric intersection (this
//! crate, and `an1mbyt3-geometry`, have no path boolean-op support at all).
//! An element carrying `clip-path="url(#id)"`/`mask="url(#id)"` gets wrapped
//! in a synthetic `Group` alongside the referenced clip/mask shape, with
//! `ImportedNode::Group::mask_index` pointing at that child — `editor.rs`
//! turns this into a real `Command::SetGroupMask`, the same authored
//! relationship the Layers panel's own drag-to-mask action produces. This is
//! only a *rough* stand-in for a true clip (one simple shape, no
//! `clipPathUnits="objectBoundingBox"`/multi-shape-clip support), and — see
//! `Renderer::render_with_mask`'s own doc comment — the renderer doesn't yet
//! composite a masked group into a normal multi-object scene, so the
//! relationship is saved and undoable but doesn't visibly clip on canvas
//! yet; that's separate, renderer-level follow-up work.
//!
//! **`fill="url(#pattern)"` resolves too**, the same "approximate rather
//! than disappear" way a gradient reference does — averaged down to one
//! flat color across the pattern tile's own content shapes (see
//! `average_pattern_color`), not an actual repeating tile (`ShapeNode` has
//! no tiling-fill concept to paint one with).
//!
//! **Strokes resolve too** (`resolve_stroke`) — flat `stroke` color (no
//! paint-server/gradient stroke, same "flat color only" limit `ShapeNode`'s
//! fill already has), `stroke-width`, `stroke-linecap`, `stroke-linejoin`.
//! `stroke="none"` (or no `stroke` at all) is a real, ordinary "no stroke" —
//! unlike `fill="none"`'s "approximate rather than disappear" gray
//! fallback below, a stroke genuinely absent in the source just stays
//! absent. A stroke-only shape (`fill="none"` *with* a real `stroke`) gets
//! a properly transparent fill instead of that gray fallback, since there's
//! now something real (the stroke) to actually see it by.
//!
//! Not supported (silently skipped, not a hard import failure — matches
//! `an1mbyt3-lottie`'s own "one unsupported element in a complex file
//! shouldn't fail the whole import" convention): `<text>`, `<image>`,
//! filter effects (blur/drop-shadow/etc.). Also: external stylesheets/CSS
//! selectors (only an element's own inline `style` attribute is read, never
//! a `<style>` block or linked stylesheet), `rx`/`ry` rounded-rect corners
//! (read-and-discarded — `Path` has no rounded-rect primitive), and a
//! gradient/pattern *stroke* (only a flat stroke color resolves — same
//! "approximate rather than disappear" averaging `resolve_fill` already
//! does for a gradient *fill* would be reasonable future work here too,
//! just not done yet). A shape with genuinely neither a resolvable fill nor
//! a stroke still falls back to the dim gray fill — that edge case (truly
//! invisible in the source) keeps the original "let me see and test my
//! artwork" reasoning.

use std::collections::HashMap;

use glam::Vec2;
use svgtypes::{Color, SimplePathSegment, SimplifyingPathParser, TransformListParser, TransformListToken};

use an1mbyt3_core::{GradientDef, GradientKind, GradientStop, Path, PathCommand, Stroke, StrokeCap, StrokeJoin};

/// Why an SVG file failed to import outright — this is only for genuinely
/// unreadable input (not well-formed XML, or no `<svg>` root); an element
/// using an unsupported feature is skipped instead, see this module's doc
/// comment.
#[derive(Debug)]
pub enum SvgImportError {
    Xml(roxmltree::Error),
    NoSvgRoot,
}

impl std::fmt::Display for SvgImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SvgImportError::Xml(err) => write!(f, "not a well-formed SVG/XML file: {err}"),
            SvgImportError::NoSvgRoot => write!(f, "file has no <svg> root element"),
        }
    }
}

impl std::error::Error for SvgImportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SvgImportError::Xml(err) => Some(err),
            SvgImportError::NoSvgRoot => None,
        }
    }
}

/// One node read out of an SVG file's tree — everything `editor.rs`'s
/// `import_svg` wasm method needs to build a real `NodeKind::Shape`/
/// `NodeKind::Group` node from, preserving the source `<g>` hierarchy (see
/// this module's doc comment). No `Transform2D` on `Shape` on purpose —
/// every shape's final position is already baked into `path`; `Group`
/// likewise always corresponds to an identity `Transform2D` in the
/// document, existing purely for organization/masking.
pub enum ImportedNode {
    /// `gradient`, when present, is *in addition to* `fill_color` (which
    /// stays the flat-averaged fallback `resolve_fill` always computes) —
    /// mirrors `an1mbyt3_core::ShapeNode`'s own "gradient overrides
    /// fill_color, fill_color is the fallback if it's ever cleared"
    /// convention (see `gradient.rs`'s module doc). See `resolve_gradient`
    /// for exactly which `<linearGradient>`/`<radialGradient>` shapes
    /// resolve to `Some`.
    Shape { name: String, path: Path, fill_color: [f32; 4], gradient: Option<GradientDef>, stroke: Option<Stroke> },
    /// `mask_index`, if set, is this group's own index into `children` that
    /// should become its `GroupNode::mask` once inserted — see
    /// `resolve_clip_or_mask`'s doc comment for how/why it's populated.
    Group { name: String, children: Vec<ImportedNode>, mask_index: Option<usize> },
}

/// A plain 2D affine matrix, SVG's own `a b c d e f` convention
/// (`matrix(a, b, c, d, e, f)`, equivalent to the column-major
/// `[[a, c, e], [b, d, f], [0, 0, 1]]`) — used to bake every accumulated
/// `transform`/viewBox offset directly into each shape's path coordinates,
/// per this module's doc comment.
#[derive(Clone, Copy)]
struct Matrix {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Matrix {
    const IDENTITY: Matrix = Matrix { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    fn translate(tx: f64, ty: f64) -> Matrix {
        Matrix { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: tx, f: ty }
    }

    /// Composes `self` with `rhs` such that applying the result to a point
    /// is equivalent to applying `rhs` first, then `self` — i.e. `self ∘
    /// rhs`. Folding a `transform="a() b() c()"` attribute's tokens left to
    /// right via `acc = acc.mul(token)` reproduces the SVG spec's own
    /// "leftmost function is outermost" nesting; the same operation composes
    /// a child's local matrix under its parent's accumulated one while
    /// walking the XML tree (`walk`'s `matrix.mul(local)`).
    fn mul(self, rhs: Matrix) -> Matrix {
        Matrix {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            e: self.a * rhs.e + self.c * rhs.f + self.e,
            f: self.b * rhs.e + self.d * rhs.f + self.f,
        }
    }

    fn apply(self, x: f64, y: f64) -> (f64, f64) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }

    fn apply_vec2(self, x: f64, y: f64) -> Vec2 {
        let (x, y) = self.apply(x, y);
        Vec2::new(x as f32, y as f32)
    }
}

/// Parses one `transform="..."` attribute value into a single composed
/// [`Matrix`] — an unparseable token list (or one this crate's `svgtypes`
/// version doesn't recognize) degrades to whatever prefix parsed
/// successfully, same "best-effort, don't fail the whole import" spirit as
/// every other unsupported-feature case this crate documents.
fn parse_transform(value: &str) -> Matrix {
    let mut matrix = Matrix::IDENTITY;
    for token in TransformListParser::from(value) {
        let Ok(token) = token else { break };
        let local = match token {
            TransformListToken::Matrix { a, b, c, d, e, f } => Matrix { a, b, c, d, e, f },
            TransformListToken::Translate { tx, ty } => Matrix::translate(tx, ty),
            TransformListToken::Scale { sx, sy } => Matrix { a: sx, b: 0.0, c: 0.0, d: sy, e: 0.0, f: 0.0 },
            TransformListToken::Rotate { angle } => {
                let r = angle.to_radians();
                Matrix { a: r.cos(), b: r.sin(), c: -r.sin(), d: r.cos(), e: 0.0, f: 0.0 }
            }
            TransformListToken::SkewX { angle } => Matrix { a: 1.0, b: 0.0, c: angle.to_radians().tan(), d: 1.0, e: 0.0, f: 0.0 },
            TransformListToken::SkewY { angle } => Matrix { a: 1.0, b: angle.to_radians().tan(), c: 0.0, d: 1.0, e: 0.0, f: 0.0 },
        };
        matrix = matrix.mul(local);
    }
    matrix
}

/// Parses an SVG path `d` attribute into our own cubic-only [`Path`], with
/// `matrix` baked into every resulting point. Delegates almost all of the
/// real work to `svgtypes::SimplifyingPathParser`, which already collapses
/// relative-to-absolute conversion, `H`/`V` -> `L`, `S`/`T` control-point
/// reflection, and (via the `kurbo` crate internally) elliptical arcs into
/// cubic Béziers — this function only has to additionally convert the
/// *quadratic* segments `SimplifyingPathParser` still emits (Q/T) into
/// cubics itself, since `an1mbyt3_core::PathCommand` has no quadratic
/// variant. That conversion (`cp1 = start + 2/3*(ctrl-start)`, `cp2 = end +
/// 2/3*(ctrl-end)`) is an *exact* degree elevation, not an approximation,
/// and is done in the SVG's own raw coordinate space (tracked via `current`)
/// before `matrix` is applied — affine maps commute with Bézier evaluation,
/// so transforming the elevated cubic's points afterward is identical to
/// transforming the quadratic's points first.
fn parse_path(d: &str, matrix: Matrix) -> Path {
    let mut commands = Vec::new();
    let mut current = (0.0_f64, 0.0_f64);

    for segment in SimplifyingPathParser::from(d) {
        let Ok(segment) = segment else { break };
        match segment {
            SimplePathSegment::MoveTo { x, y } => {
                current = (x, y);
                commands.push(PathCommand::MoveTo(matrix.apply_vec2(x, y)));
            }
            SimplePathSegment::LineTo { x, y } => {
                current = (x, y);
                commands.push(PathCommand::LineTo(matrix.apply_vec2(x, y)));
            }
            SimplePathSegment::CurveTo { x1, y1, x2, y2, x, y } => {
                current = (x, y);
                commands.push(PathCommand::CubicTo {
                    control1: matrix.apply_vec2(x1, y1),
                    control2: matrix.apply_vec2(x2, y2),
                    to: matrix.apply_vec2(x, y),
                });
            }
            SimplePathSegment::Quadratic { x1, y1, x, y } => {
                let (sx, sy) = current;
                let c1 = (sx + 2.0 / 3.0 * (x1 - sx), sy + 2.0 / 3.0 * (y1 - sy));
                let c2 = (x + 2.0 / 3.0 * (x1 - x), y + 2.0 / 3.0 * (y1 - y));
                current = (x, y);
                commands.push(PathCommand::CubicTo {
                    control1: matrix.apply_vec2(c1.0, c1.1),
                    control2: matrix.apply_vec2(c2.0, c2.1),
                    to: matrix.apply_vec2(x, y),
                });
            }
            SimplePathSegment::ClosePath => commands.push(PathCommand::Close),
        }
    }

    Path { commands }
}

/// Builds a closed polygon/polyline path from a `points="x1,y1 x2,y2 ..."`
/// attribute value (shared by `<polygon>`/`<polyline>`) — `close` is `true`
/// only for `<polygon>` (a `<polyline>` is an open path, per SVG semantics).
fn parse_points(value: &str, matrix: Matrix, close: bool) -> Path {
    let mut commands = Vec::new();
    let mut numbers = value.split([',', ' ', '\n', '\t']).filter(|s| !s.is_empty()).filter_map(|s| s.parse::<f64>().ok());
    let mut first = true;
    while let (Some(x), Some(y)) = (numbers.next(), numbers.next()) {
        let point = matrix.apply_vec2(x, y);
        commands.push(if first { PathCommand::MoveTo(point) } else { PathCommand::LineTo(point) });
        first = false;
    }
    if close && !commands.is_empty() {
        commands.push(PathCommand::Close);
    }
    Path { commands }
}

/// Parses a plain `<color>` value (hex/named/`rgb()`/`rgba()`, whatever
/// `svgtypes::Color` understands) — NOT a `url(#...)` paint-server
/// reference, see [`resolve_fill`] for that. Used both for an element's own
/// `fill`/`style` value and for a gradient `<stop>`'s `stop-color`.
fn parse_fill(value: &str) -> Option<[f32; 4]> {
    if value == "none" {
        // No fill and no stroke support in `ShapeNode` yet (see this
        // module's doc comment) — a dim, clearly-a-placeholder gray beats
        // importing a shape the designer can't see or select at all.
        return Some([0.35, 0.35, 0.35, 1.0]);
    }
    let color: Color = value.parse().ok()?;
    Some([color.red as f32 / 255.0, color.green as f32 / 255.0, color.blue as f32 / 255.0, color.alpha as f32 / 255.0])
}

/// Resolves a `fill`/`style: fill` *value* to a flat RGBA color, handling
/// both a plain `<color>` (delegates to [`parse_fill`]) and a
/// `url(#gradientId)` paint-server reference — `an1mbyt3_core::ShapeNode`
/// has no gradient-fill concept at all (a flat color is the only thing it
/// can hold), so a referenced `<linearGradient>`/`<radialGradient>` resolves
/// to the *average* of its own `<stop>` colors (see
/// `average_gradient_color`) rather than being dropped — the same
/// "approximate rather than disappear" bias `fill="none"` already gets.
/// Without this, any shape painted via a gradient (extremely common for a
/// character/illustration SVG's main body fills) silently fell through to
/// whatever ancestor fill was in scope — usually nothing, which is exactly
/// why gradient-heavy artwork was importing as flat black shapes before
/// this existed.
fn resolve_fill(value: &str, node: roxmltree::Node) -> Option<[f32; 4]> {
    if let Some(id) = value.trim().strip_prefix("url(#").and_then(|s| s.strip_suffix(')')) {
        let target = node.document().root_element().descendants().find(|n| n.attribute("id") == Some(id))?;
        return match target.tag_name().name() {
            "pattern" => average_pattern_color(target, 0),
            _ => average_gradient_color(target, 0),
        };
    }
    parse_fill(value)
}

/// A `stroke`/`style: stroke` *color* value only (not a gradient/pattern
/// reference — see this module's own doc comment on why only a flat stroke
/// color resolves) — `None` for `"none"` or an unparseable value, since
/// `stroke="none"` is an entirely ordinary "this shape has no stroke," not
/// the "approximate rather than disappear" case `parse_fill`'s own gray
/// fallback exists for (nothing is lost by a shape genuinely having no
/// stroke; something *would* be lost by a shape silently having no fill).
fn parse_stroke_color(value: &str) -> Option<[f32; 4]> {
    if value == "none" {
        return None;
    }
    let color: Color = value.parse().ok()?;
    Some([color.red as f32 / 255.0, color.green as f32 / 255.0, color.blue as f32 / 255.0, color.alpha as f32 / 255.0])
}

fn parse_stroke_cap(value: &str) -> StrokeCap {
    match value {
        "round" => StrokeCap::Round,
        "square" => StrokeCap::Square,
        _ => StrokeCap::Butt,
    }
}

fn parse_stroke_join(value: &str) -> StrokeJoin {
    match value {
        "round" => StrokeJoin::Round,
        "bevel" => StrokeJoin::Bevel,
        _ => StrokeJoin::Miter,
    }
}

/// Resolves `child`'s own `stroke`/`stroke-width`/`stroke-opacity`/
/// `stroke-linecap`/`stroke-linejoin` (presentation attributes, each
/// overridden by the same-named `style="..."` declaration if present — same
/// cascade `visit`'s own fill resolution already follows) into a real
/// `an1mbyt3_core::Stroke`, or `None` if this element has no stroke at all.
/// `stroke-width` defaults to `1.0` (SVG's own default) when a stroke color
/// resolves but no explicit width was given.
fn resolve_stroke(child: roxmltree::Node) -> Option<Stroke> {
    let mut color = child.attribute("stroke").and_then(parse_stroke_color);
    let mut width = child.attribute("stroke-width").and_then(|v| v.parse::<f64>().ok());
    let mut cap = child.attribute("stroke-linecap").map(parse_stroke_cap);
    let mut join = child.attribute("stroke-linejoin").map(parse_stroke_join);
    let mut opacity = child.attribute("stroke-opacity").and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0);

    if let Some(style) = child.attribute("style") {
        for declaration in style.split(';') {
            let Some((property, value)) = declaration.split_once(':') else { continue };
            let value = value.trim();
            match property.trim() {
                "stroke" => color = parse_stroke_color(value),
                "stroke-width" => width = value.parse::<f64>().ok(),
                "stroke-linecap" => cap = Some(parse_stroke_cap(value)),
                "stroke-linejoin" => join = Some(parse_stroke_join(value)),
                "stroke-opacity" => opacity = value.parse::<f32>().unwrap_or(opacity),
                _ => {}
            }
        }
    }

    let [r, g, b, a] = color?;
    Some(Stroke {
        width: width.unwrap_or(1.0) as f32,
        color: [r, g, b, a * opacity],
        gradient: None,
        image: None,
        cap: cap.unwrap_or_default(),
        join: join.unwrap_or_default(),
    })
}

/// Averages every content shape's own `fill` inside a `<pattern>` tile into
/// one flat RGBA — the same "approximate rather than disappear" fallback
/// `average_gradient_color` already applies to gradients, extended to the
/// other paint-server kind `ShapeNode` can't actually render (a repeating
/// tile, rather than one flat color). Recurses into nested `<g>`s (patterns
/// commonly group their tile content); a content element's fill is read via
/// `parse_fill` only (not the fuller `resolve_fill`), so a pattern whose own
/// content is itself gradient/pattern-filled doesn't resolve recursively —
/// real-world pattern tiles are simple enough (a couple of flat-filled
/// shapes) that this stays a reasonable approximation without this crate
/// needing full recursive paint-server resolution here too. Chases `href`/
/// `xlink:href` to `MAX_USE_DEPTH` deep if the pattern has no content shapes
/// of its own, same as `average_gradient_color`.
fn average_pattern_color(node: roxmltree::Node, depth: u32) -> Option<[f32; 4]> {
    fn collect(node: roxmltree::Node, sum: &mut [f32; 4], count: &mut u32) {
        for child in node.children().filter(|c| c.is_element()) {
            if child.tag_name().name() == "g" {
                collect(child, sum, count);
                continue;
            }
            let mut color = child.attribute("fill").and_then(parse_fill);
            if let Some(style) = child.attribute("style") {
                for declaration in style.split(';') {
                    let Some((property, val)) = declaration.split_once(':') else { continue };
                    if property.trim() == "fill" {
                        if let Some(c) = parse_fill(val.trim()) {
                            color = Some(c);
                        }
                    }
                }
            }
            let Some(color) = color else { continue };
            for i in 0..4 {
                sum[i] += color[i];
            }
            *count += 1;
        }
    }

    let mut sum = [0.0_f32; 4];
    let mut count = 0;
    collect(node, &mut sum, &mut count);

    if count > 0 {
        return Some([sum[0] / count as f32, sum[1] / count as f32, sum[2] / count as f32, sum[3] / count as f32]);
    }
    if depth >= MAX_USE_DEPTH {
        return None;
    }
    let id = href(node)?.strip_prefix('#')?;
    let target = node.document().root_element().descendants().find(|n| n.attribute("id") == Some(id))?;
    average_pattern_color(target, depth + 1)
}

/// Averages every `<stop>` child's `stop-color`/`stop-opacity` (read as
/// either plain attributes or an inline `style`) into one flat RGBA color —
/// simple unweighted mean, ignoring each stop's `offset` position, since
/// there's no way to represent *where* a gradient transitions in a flat
/// fill anyway; an average is a reasonable "what color does this mostly
/// read as" approximation for the common 2-3 stop case. If this gradient
/// element has no `<stop>` children of its own, chases its `href`/
/// `xlink:href` (Illustrator/Inkscape both commonly factor a shared stop
/// list into one `<linearGradient>` and reference it from several others
/// via `gradientTransform`-only variants) up to `MAX_USE_DEPTH` deep.
fn average_gradient_color(node: roxmltree::Node, depth: u32) -> Option<[f32; 4]> {
    let mut sum = [0.0_f32; 4];
    let mut count = 0;
    for stop in node.children().filter(|c| c.is_element() && c.tag_name().name() == "stop") {
        let mut color = stop.attribute("stop-color").and_then(parse_fill);
        let mut stop_opacity = stop.attribute("stop-opacity").and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0);
        if let Some(style) = stop.attribute("style") {
            for declaration in style.split(';') {
                let Some((property, val)) = declaration.split_once(':') else { continue };
                match property.trim() {
                    "stop-color" => {
                        if let Some(c) = parse_fill(val.trim()) {
                            color = Some(c);
                        }
                    }
                    "stop-opacity" => {
                        if let Ok(v) = val.trim().parse::<f32>() {
                            stop_opacity = v;
                        }
                    }
                    _ => {}
                }
            }
        }
        let Some(mut color) = color else { continue };
        color[3] *= stop_opacity;
        for i in 0..4 {
            sum[i] += color[i];
        }
        count += 1;
    }

    if count > 0 {
        return Some([sum[0] / count as f32, sum[1] / count as f32, sum[2] / count as f32, sum[3] / count as f32]);
    }
    if depth >= MAX_USE_DEPTH {
        return None;
    }
    let id = href(node)?.strip_prefix('#')?;
    let target = node.document().root_element().descendants().find(|n| n.attribute("id") == Some(id))?;
    average_gradient_color(target, depth + 1)
}

/// The winning raw `fill` value for `child` — its own `fill` attribute,
/// overridden by an inline `style: fill: ...` declaration per the CSS
/// cascade (same precedence `parse_style`'s color-resolving version already
/// applies). Kept as the *raw* string, unlike `resolve_fill`'s resolved
/// color, so `resolve_gradient` can inspect it once a shape's own final
/// `path` exists (needed for `objectBoundingBox` gradients' bounding-box
/// mapping) — a flat-color resolution doesn't need to wait for that, hence
/// the two separate helpers rather than one doing both jobs.
fn effective_fill_value(child: roxmltree::Node) -> Option<String> {
    let mut value = child.attribute("fill").map(str::to_string);
    if let Some(style) = child.attribute("style") {
        for declaration in style.split(';') {
            let Some((property, val)) = declaration.split_once(':') else { continue };
            if property.trim() == "fill" {
                value = Some(val.trim().to_string());
            }
        }
    }
    value
}

/// Parses a plain number or a `NN%` percentage into a `0.0..=1.0`-scaled
/// fraction either way (`"50%"` and `"0.5"` both -> `0.5`) — SVG gradient
/// attributes (`x1`/`cx`/`r`/a `<stop offset>`) routinely use either form
/// interchangeably, unlike `attr_f64`'s plain-position attributes (`x`/`y`/
/// `width`) which only ever need the percent *sign* stripped, never the
/// division, since this crate doesn't resolve percentage positions against a
/// viewport anyway.
fn parse_frac(value: &str) -> Option<f64> {
    let value = value.trim();
    match value.strip_suffix('%') {
        Some(pct) => pct.trim().parse::<f64>().ok().map(|v| v / 100.0),
        None => value.parse::<f64>().ok(),
    }
}

fn attr_frac_or(node: roxmltree::Node, name: &str, default: f64) -> f64 {
    node.attribute(name).and_then(parse_frac).unwrap_or(default)
}

/// `gradientUnits`, defaulting to SVG's own default (`objectBoundingBox`) —
/// not chased through `href` (see `resolve_gradient`'s doc comment for why
/// this crate doesn't chase coordinate-affecting attributes through a
/// gradient's `href` chain, only its `<stop>`s).
fn gradient_units<'a>(node: roxmltree::Node<'a, 'a>) -> &'a str {
    node.attribute("gradientUnits").unwrap_or("objectBoundingBox")
}

/// The axis-aligned bounding box of every point `path` actually visits
/// (control points included, not just on-curve points — a cubic Bézier
/// always lies within its control points' convex hull, so this is a safe,
/// if occasionally slightly loose, superset of the curve's *tight* bbox) —
/// `resolve_gradient`'s `objectBoundingBox`-mode mapping target. `None` for
/// an empty path (shouldn't happen for anything that reached `resolve_gradient`,
/// every caller already only calls this on a just-built, non-empty `path`).
fn path_bbox(path: &Path) -> Option<(f32, f32, f32, f32)> {
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    let mut touch = |p: Vec2| {
        min = min.min(p);
        max = max.max(p);
    };
    for command in &path.commands {
        match *command {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => touch(p),
            PathCommand::CubicTo { control1, control2, to } => {
                touch(control1);
                touch(control2);
                touch(to);
            }
            PathCommand::Close => {}
        }
    }
    if !min.x.is_finite() {
        return None;
    }
    // A zero-size axis (a perfectly flat `<line>`, or a bbox-degenerate
    // shape) would otherwise divide every `objectBoundingBox` fraction by
    // zero downstream — floor both axes to a tiny but non-zero span instead
    // of returning `None`, so a gradient on a degenerate shape still resolves
    // to *something* rather than silently falling back to the flat-color-only
    // path.
    Some((min.x, min.y, (max.x - min.x).max(0.0001), (max.y - min.y).max(0.0001)))
}

/// Collects one `<linearGradient>`/`<radialGradient>` element's own `<stop>`
/// children into real `GradientStop`s (offset, straight RGBA, and the
/// default unbiased `midpoint: 0.5` — SVG has no per-segment midpoint-bias
/// concept of its own to read). Falls back to chasing `href`/`xlink:href` up
/// to `MAX_USE_DEPTH` deep when `node` has no `<stop>`s of its own, same
/// "shared stop list, gradient-specific geometry" inheritance pattern
/// `average_gradient_color` already established — unlike that function,
/// this preserves each stop's real offset/color rather than averaging them
/// away, since `GradientDef` can actually paint a real multi-stop gradient.
fn parse_gradient_stops(node: roxmltree::Node, depth: u32) -> Option<Vec<GradientStop>> {
    let mut stops = Vec::new();
    for stop in node.children().filter(|c| c.is_element() && c.tag_name().name() == "stop") {
        let mut color = stop.attribute("stop-color").and_then(parse_fill);
        let mut stop_opacity = stop.attribute("stop-opacity").and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0);
        let offset = stop.attribute("offset").and_then(parse_frac).map(|v| v.clamp(0.0, 1.0) as f32).unwrap_or(0.0);
        if let Some(style) = stop.attribute("style") {
            for declaration in style.split(';') {
                let Some((property, val)) = declaration.split_once(':') else { continue };
                match property.trim() {
                    "stop-color" => {
                        if let Some(c) = parse_fill(val.trim()) {
                            color = Some(c);
                        }
                    }
                    "stop-opacity" => {
                        if let Ok(v) = val.trim().parse::<f32>() {
                            stop_opacity = v;
                        }
                    }
                    _ => {}
                }
            }
        }
        let Some(mut color) = color else { continue };
        color[3] *= stop_opacity;
        stops.push(GradientStop { offset, color, midpoint: 0.5 });
    }

    if !stops.is_empty() {
        return Some(stops);
    }
    if depth >= MAX_USE_DEPTH {
        return None;
    }
    let id = href(node)?.strip_prefix('#')?;
    let target = node.document().root_element().descendants().find(|n| n.attribute("id") == Some(id))?;
    parse_gradient_stops(target, depth + 1)
}

/// Resolves a `fill`/`style: fill` value's gradient, if it references a
/// `<linearGradient>`/`<radialGradient>` — `None` for a plain color, a
/// `<pattern>` reference (`average_pattern_color` already handles that one,
/// via `resolve_fill`), or an unresolvable/under-two-stop reference, in
/// which case the shape keeps only the flat `fill_color` `resolve_fill`
/// already computed for it, same "approximate, never disappear" fallback
/// spirit as everything else in this module — this only ever *adds* a
/// gradient on top, never removes the flat-color safety net.
///
/// `world_matrix`/`path` are the shape's own already fully-baked values (see
/// this module's doc comment: every ancestor/own `transform` is baked
/// straight into path coordinates) — a `gradientUnits="userSpaceOnUse"`
/// gradient's `x1`/`y1`/etc. live in that exact same pre-element-transform
/// local space a path's own raw points start in, so they go through the
/// identical `world_matrix.apply` a path point already does (`gradientTransform`
/// composed first, same "leftmost-outermost" order `parse_transform` already
/// uses for a plain `transform` attribute). The default `objectBoundingBox`
/// mode instead maps its `0..1` fractions directly onto `path`'s own final,
/// already-transformed axis-aligned bounding box (`path_bbox`) — deliberately
/// *not* composing `gradientTransform` in that mode (spec has it apply
/// against the unit square *before* the bbox mapping, which would need a
/// from-scratch pre-element-transform bbox this crate has no other reason to
/// compute) and not chasing `x1`/`y1`/etc. through `href` inheritance either
/// (only a gradient's `<stop>`s inherit, via `parse_gradient_stops`) — both
/// documented approximations, exact for the overwhelmingly common
/// unrotated/unskewed, own-coordinates-authored case, still a large
/// improvement over the flat-averaged-color-only fallback for the rest.
fn resolve_gradient(value: &str, node: roxmltree::Node, world_matrix: Matrix, path: &Path) -> Option<GradientDef> {
    let id = value.trim().strip_prefix("url(#")?.strip_suffix(')')?;
    let target = node.document().root_element().descendants().find(|n| n.attribute("id") == Some(id))?;
    let tag = target.tag_name().name();
    if tag != "linearGradient" && tag != "radialGradient" {
        return None;
    }

    let stops = parse_gradient_stops(target, 0)?;
    if stops.len() < 2 {
        return None;
    }

    let user_space = gradient_units(target) == "userSpaceOnUse";
    let gradient_transform = target.attribute("gradientTransform").map(parse_transform).unwrap_or(Matrix::IDENTITY);
    let bbox = path_bbox(path);

    let to_world = |x: f64, y: f64| -> Vec2 {
        if user_space {
            let (gx, gy) = gradient_transform.apply(x, y);
            world_matrix.apply_vec2(gx, gy)
        } else {
            let (bx, by, bw, bh) = bbox.unwrap_or((0.0, 0.0, 1.0, 1.0));
            Vec2::new(bx + x as f32 * bw, by + y as f32 * bh)
        }
    };

    let (kind, start, end) = match tag {
        "linearGradient" => {
            let x1 = attr_frac_or(target, "x1", 0.0);
            let y1 = attr_frac_or(target, "y1", 0.0);
            let x2 = attr_frac_or(target, "x2", 1.0);
            let y2 = attr_frac_or(target, "y2", 0.0);
            (GradientKind::Linear, to_world(x1, y1), to_world(x2, y2))
        }
        "radialGradient" => {
            let cx = attr_frac_or(target, "cx", 0.5);
            let cy = attr_frac_or(target, "cy", 0.5);
            let r = attr_frac_or(target, "r", 0.5);
            // Radial's `end` only ever marks *where the radius is reached*
            // (`GradientKind::Radial`'s own doc comment) — direction from
            // `start` doesn't matter for a concentric-rings gradient, so an
            // arbitrary `+x` offset by `r` is exactly as correct as any other
            // direction would be.
            (GradientKind::Radial, to_world(cx, cy), to_world(cx + r, cy))
        }
        _ => unreachable!("tag already checked above"),
    };

    Some(GradientDef { kind, start, end, stops })
}

/// A minimal inline `style="fill: #fff; opacity: 0.5"` parser — just enough
/// to read `fill`/`fill-opacity`/`opacity` declarations, the three this
/// crate already reads as plain presentation attributes. Real-world SVGs
/// exported from web-based design tools very commonly set color via inline
/// `style` instead of (or in addition to) the presentation attribute — per
/// CSS cascade rules a `style` declaration wins over the presentation
/// attribute of the same name, which is why callers apply this *after*
/// reading the plain attributes. No selector/stylesheet support (a `<style>`
/// element's own CSS rules are skipped entirely, per this module's doc
/// comment) — only the element's own inline `style` attribute.
fn parse_style(value: &str, node: roxmltree::Node, fill: &mut Option<[f32; 4]>, opacity: &mut Option<f32>) {
    for declaration in value.split(';') {
        let Some((property, val)) = declaration.split_once(':') else { continue };
        let (property, val) = (property.trim(), val.trim());
        match property {
            "fill" => {
                if let Some(color) = resolve_fill(val, node) {
                    *fill = Some(color);
                }
            }
            "fill-opacity" | "opacity" => {
                if let Ok(v) = val.parse::<f32>() {
                    *opacity = Some(opacity.unwrap_or(1.0) * v);
                }
            }
            _ => {}
        }
    }
}

/// `true` if `node` is hidden via `display: none` or `visibility: hidden` —
/// checked as either a plain attribute or an inline `style` declaration.
/// Illustrator/Figma exports routinely carry hidden layers (alternate color
/// variants, guides, unused groups) alongside the visible artwork; importing
/// those anyway silently draws extra, wrong-colored geometry on top of or
/// under the real shapes. Both properties are treated as "skip this
/// element's entire subtree" for simplicity — `visibility` technically
/// *inherits* and a descendant can set `visibility: visible` to opt back in,
/// but that pattern is rare enough in hand-authored/tool-exported SVGs that
/// the simpler blanket skip is the right tradeoff here.
fn is_hidden(node: roxmltree::Node) -> bool {
    if node.attribute("display") == Some("none") || node.attribute("visibility") == Some("hidden") {
        return true;
    }
    if let Some(style) = node.attribute("style") {
        for declaration in style.split(';') {
            let Some((property, val)) = declaration.split_once(':') else { continue };
            match (property.trim(), val.trim()) {
                ("display", "none") | ("visibility", "hidden") => return true,
                _ => {}
            }
        }
    }
    false
}

/// `href`/`xlink:href` — SVG2 dropped the `xlink:` namespace prefix
/// `<use>` originally required, but real-world files (and every SVG1.1-era
/// export) still commonly use the old `xlink:href` form, so both are
/// checked, preferring the newer unprefixed one.
fn href<'a>(node: roxmltree::Node<'a, 'a>) -> Option<&'a str> {
    node.attribute("href").or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")))
}

/// Elements that are never directly rendered — only reachable (if at all)
/// through a `<use>` reference (`defs`/`symbol`) or a paint-server/effect
/// reference this crate doesn't resolve (`pattern`/`mask`/`clipPath`), plus
/// plain metadata/CSS elements (`style`/`title`/`desc`/`metadata`). Walking
/// into these directly would import their contents as if they were visible
/// shapes sitting wherever they happen to be declared, which is wrong (a
/// `<defs>` block especially — real-world SVGs routinely stash their entire
/// reusable icon set in there, never rendered directly at all).
fn is_never_rendered_directly(tag: &str) -> bool {
    matches!(
        tag,
        "defs" | "symbol" | "clipPath" | "mask" | "pattern" | "style" | "title" | "desc" | "metadata" | "linearGradient" | "radialGradient"
    )
}

/// How many `<use>` references deep this importer will follow before giving
/// up on that branch — real files never nest anywhere close to this deep;
/// this only exists as a hard backstop against a malformed/cyclic
/// `<use href="#self">`-style reference chain, so an unusual file degrades
/// to "some content missing" rather than a stack overflow.
const MAX_USE_DEPTH: u32 = 16;

struct Walker {
    counters: HashMap<&'static str, u32>,
}

impl Walker {
    fn next_name(&mut self, kind: &'static str) -> String {
        let count = self.counters.entry(kind).or_insert(0);
        *count += 1;
        format!("{kind} {count}")
    }

    /// Visits every element child of `node`, flattening each one's own
    /// produced node(s) into a single sibling list — the top-level entry
    /// point for both the whole document (`import_svg`) and a `<g>`'s own
    /// children (which become that `Group`'s `children`, see `visit`'s `"g"`
    /// arm).
    fn walk(&mut self, node: roxmltree::Node, matrix: Matrix, fill: [f32; 4], use_depth: u32) -> Vec<ImportedNode> {
        node.children().filter(|n| n.is_element()).flat_map(|child| self.visit(child, matrix, fill, use_depth)).collect()
    }

    /// Handles exactly one element, returning whatever `ImportedNode`(s) it
    /// produces — shared by `walk`'s normal top-down traversal and `<use>`
    /// resolution (which needs to (re-)visit whatever element it references
    /// as if it sat at the `<use>`'s own location, not as part of the normal
    /// parent-child walk order). Usually 0 or 1 nodes; an unrecognized
    /// container tag (anything but `<g>`) flattens its children straight
    /// into the caller's list rather than wrapping them, since only `<g>`
    /// is preserved as real hierarchy (see this module's doc comment).
    fn visit(&mut self, child: roxmltree::Node, matrix: Matrix, fill: [f32; 4], use_depth: u32) -> Vec<ImportedNode> {
        let tag = child.tag_name().name();
        if is_never_rendered_directly(tag) || is_hidden(child) {
            return Vec::new();
        }

        let local_matrix = child.attribute("transform").map(parse_transform).unwrap_or(Matrix::IDENTITY);
        let world_matrix = matrix.mul(local_matrix);

        // Presentation attributes first, then `style="..."` overrides them
        // (matches the CSS cascade: an inline `style` declaration always
        // wins over the same-named presentation attribute) — see
        // `parse_style`'s own doc comment.
        let mut opacity = 1.0_f32;
        if let Some(v) = child.attribute("opacity").and_then(|v| v.parse::<f32>().ok()) {
            opacity *= v;
        }
        if let Some(v) = child.attribute("fill-opacity").and_then(|v| v.parse::<f32>().ok()) {
            opacity *= v;
        }
        let mut child_fill = child.attribute("fill").and_then(|v| resolve_fill(v, child));
        if let Some(style) = child.attribute("style") {
            let mut style_fill = None;
            let mut style_opacity = None;
            parse_style(style, child, &mut style_fill, &mut style_opacity);
            if style_fill.is_some() {
                child_fill = style_fill;
            }
            if let Some(v) = style_opacity {
                opacity *= v;
            }
        }
        let child_fill = child_fill.unwrap_or(fill);
        let fill_value = effective_fill_value(child);
        let effective_stroke = resolve_stroke(child);
        // Two cases force a transparent fill instead of `parse_fill`'s
        // "approximate rather than disappear" gray fallback (or SVG's own
        // default-to-black when no `fill` was specified at all):
        // - `<line>` always, fill-specified-or-not — it has zero enclosed
        //   area, so SVG fill is never visually meaningful on it regardless
        //   of what's set (the user's own bug report: a plain line was
        //   importing as a filled blob — this tag has no explicit `fill`
        //   attribute at all in the common case, so a check for literal
        //   `fill="none"` alone wouldn't have caught it).
        // - any other shape that explicitly opted out of fill (`fill="none"`)
        //   *and* has a real stroke — genuinely a stroke-only shape, not the
        //   ambiguous "ought to be invisible, or did the author forget to
        //   set a color" case the gray fallback exists for.
        // A shape with neither fill nor stroke still gets the gray fallback,
        // same as before.
        let effective_fill = if tag == "line" || (fill_value.as_deref() == Some("none") && effective_stroke.is_some()) {
            [child_fill[0], child_fill[1], child_fill[2], 0.0]
        } else {
            [child_fill[0], child_fill[1], child_fill[2], child_fill[3] * opacity]
        };

        let name = child.attribute("id").map(str::to_string);

        let produced = match tag {
            "path" => match child.attribute("d") {
                Some(d) => {
                    let path = parse_path(d, world_matrix);
                    if path.commands.is_empty() {
                        Vec::new()
                    } else {
                        let name = name.clone().unwrap_or_else(|| self.next_name("Path"));
                        vec![ImportedNode::Shape { name, path: path.clone(), fill_color: effective_fill, gradient: fill_value.as_deref().and_then(|v| resolve_gradient(v, child, world_matrix, &path)), stroke: effective_stroke.clone() }]
                    }
                }
                None => Vec::new(),
            },
            "polygon" | "polyline" => match child.attribute("points") {
                Some(points) => {
                    let close = tag == "polygon";
                    let path = parse_points(points, world_matrix, close);
                    if path.commands.is_empty() {
                        Vec::new()
                    } else {
                        let kind = if close { "Polygon" } else { "Polyline" };
                        let name = name.clone().unwrap_or_else(|| self.next_name(kind));
                        vec![ImportedNode::Shape { name, path: path.clone(), fill_color: effective_fill, gradient: fill_value.as_deref().and_then(|v| resolve_gradient(v, child, world_matrix, &path)), stroke: effective_stroke.clone() }]
                    }
                }
                None => Vec::new(),
            },
            "rect" => {
                let w = attr_f64(child, "width").unwrap_or(0.0);
                let h = attr_f64(child, "height").unwrap_or(0.0);
                if w > 0.0 && h > 0.0 {
                    let x = attr_f64(child, "x").unwrap_or(0.0);
                    let y = attr_f64(child, "y").unwrap_or(0.0);
                    // `Path::rect` is centered on the origin — recenter by
                    // translating to the rect's own center before the rest
                    // of the accumulated matrix applies. Corner rounding
                    // (`rx`/`ry`) is read-and-discarded — `Path` has no
                    // rounded-rect primitive yet, same documented limitation
                    // `an1mbyt3-lottie`'s importer already has for Lottie's
                    // own `rc.r`.
                    let recenter = Matrix::translate(x + w / 2.0, y + h / 2.0);
                    let path = recenter_path(Path::rect(w as f32, h as f32), world_matrix.mul(recenter));
                    let name = name.clone().unwrap_or_else(|| self.next_name("Rectangle"));
                    vec![ImportedNode::Shape { name, path: path.clone(), fill_color: effective_fill, gradient: fill_value.as_deref().and_then(|v| resolve_gradient(v, child, world_matrix, &path)), stroke: effective_stroke.clone() }]
                } else {
                    Vec::new()
                }
            }
            "circle" => {
                let r = attr_f64(child, "r").unwrap_or(0.0);
                if r > 0.0 {
                    let (cx, cy) = (attr_f64(child, "cx").unwrap_or(0.0), attr_f64(child, "cy").unwrap_or(0.0));
                    let path = recenter_path(Path::ellipse((r * 2.0) as f32, (r * 2.0) as f32), world_matrix.mul(Matrix::translate(cx, cy)));
                    let name = name.clone().unwrap_or_else(|| self.next_name("Circle"));
                    vec![ImportedNode::Shape { name, path: path.clone(), fill_color: effective_fill, gradient: fill_value.as_deref().and_then(|v| resolve_gradient(v, child, world_matrix, &path)), stroke: effective_stroke.clone() }]
                } else {
                    Vec::new()
                }
            }
            "ellipse" => {
                let (rx, ry) = (attr_f64(child, "rx").unwrap_or(0.0), attr_f64(child, "ry").unwrap_or(0.0));
                if rx > 0.0 && ry > 0.0 {
                    let (cx, cy) = (attr_f64(child, "cx").unwrap_or(0.0), attr_f64(child, "cy").unwrap_or(0.0));
                    let path = recenter_path(Path::ellipse((rx * 2.0) as f32, (ry * 2.0) as f32), world_matrix.mul(Matrix::translate(cx, cy)));
                    let name = name.clone().unwrap_or_else(|| self.next_name("Ellipse"));
                    vec![ImportedNode::Shape { name, path: path.clone(), fill_color: effective_fill, gradient: fill_value.as_deref().and_then(|v| resolve_gradient(v, child, world_matrix, &path)), stroke: effective_stroke.clone() }]
                } else {
                    Vec::new()
                }
            }
            "line" => {
                let (x1, y1) = (attr_f64(child, "x1").unwrap_or(0.0), attr_f64(child, "y1").unwrap_or(0.0));
                let (x2, y2) = (attr_f64(child, "x2").unwrap_or(0.0), attr_f64(child, "y2").unwrap_or(0.0));
                let path = Path { commands: vec![PathCommand::MoveTo(world_matrix.apply_vec2(x1, y1)), PathCommand::LineTo(world_matrix.apply_vec2(x2, y2))] };
                let name = name.clone().unwrap_or_else(|| self.next_name("Line"));
                vec![ImportedNode::Shape { name, path: path.clone(), fill_color: effective_fill, gradient: fill_value.as_deref().and_then(|v| resolve_gradient(v, child, world_matrix, &path)), stroke: effective_stroke.clone() }]
            }
            "use" => {
                // `<use>`'s `x`/`y` act as an implicit extra translate on
                // top of its own `transform` (per spec, applied *before* the
                // referenced element's own transform — i.e. outermost),
                // baked in the same way every other transform here is. A
                // `<use>` element never has its own renderable children —
                // whatever its reference resolves to *is* everything it
                // produces, no further `walk` needed.
                if use_depth >= MAX_USE_DEPTH {
                    Vec::new()
                } else {
                    let (x, y) = (attr_f64(child, "x").unwrap_or(0.0), attr_f64(child, "y").unwrap_or(0.0));
                    let use_matrix = world_matrix.mul(Matrix::translate(x, y));
                    match href(child).and_then(|h| h.strip_prefix('#')) {
                        Some(id) => match child.document().root_element().descendants().find(|n| n.attribute("id") == Some(id)) {
                            Some(target) => self.visit(target, use_matrix, effective_fill, use_depth + 1),
                            None => Vec::new(),
                        },
                        None => Vec::new(),
                    }
                }
            }
            // Only `<g>` is preserved as real `NodeKind::Group` hierarchy
            // (see this module's doc comment) — an empty group (no
            // importable descendants) is pruned rather than kept as a
            // pointless empty document node.
            "g" => {
                let children = self.walk(child, world_matrix, effective_fill, use_depth);
                if children.is_empty() {
                    Vec::new()
                } else {
                    let group_name = name.clone().unwrap_or_else(|| self.next_name("Group"));
                    vec![ImportedNode::Group { name: group_name, children, mask_index: None }]
                }
            }
            // Any other unrecognized container (`<svg>` nested elements,
            // `<a>`, `<switch>`, ...) flattens transparently — its own
            // transform/fill were already folded into `world_matrix`/
            // `effective_fill` above, so descending with those is correct,
            // it just doesn't get a wrapping `Group` node of its own.
            _ => self.walk(child, world_matrix, effective_fill, use_depth),
        };

        if produced.is_empty() {
            return produced;
        }

        match self.resolve_clip_or_mask(child, world_matrix, use_depth) {
            Some(mask_node) => {
                let mut children = produced;
                children.push(mask_node);
                let mask_index = Some(children.len() - 1);
                let name = name.unwrap_or_else(|| self.next_name("Masked Group"));
                vec![ImportedNode::Group { name, children, mask_index }]
            }
            None => produced,
        }
    }

    /// Resolves `clip-path="url(#id)"` / `mask="url(#id)"` (checked in that
    /// order — `clip-path` wins if both are somehow present) into the shape
    /// geometry to use as a mask, by importing the referenced `<clipPath>`/
    /// `<mask>` element's own first content node — see this module's doc
    /// comment for why this reuses the document's real `GroupNode::mask`
    /// stencil mechanism rather than computing a true path-boolean
    /// intersection. `None` if there's no clip/mask attribute, or it
    /// doesn't resolve to any importable content.
    fn resolve_clip_or_mask(&mut self, child: roxmltree::Node, matrix: Matrix, use_depth: u32) -> Option<ImportedNode> {
        let reference = child.attribute("clip-path").or_else(|| child.attribute("mask"))?;
        let id = reference.trim().strip_prefix("url(#")?.strip_suffix(')')?;
        let target = child.document().root_element().descendants().find(|n| n.attribute("id") == Some(id))?;
        let mut produced = self.walk(target, matrix, [0.0, 0.0, 0.0, 1.0], use_depth);
        if produced.is_empty() {
            return None;
        }
        Some(produced.remove(0))
    }
}

/// Parses a length attribute (`width`/`x`/`r`/...) as a plain number,
/// stripping a trailing `px` unit (the only unit real SVGs commonly write
/// explicitly) or `%` (a percentage's *true* meaning depends on the
/// containing viewport, which isn't always known here — treating the bare
/// number as a raw unit is a documented approximation, but keeps the shape
/// instead of silently dropping it, matching this crate's overall "import
/// something reasonable rather than nothing" bias).
fn attr_f64(node: roxmltree::Node, name: &str) -> Option<f64> {
    node.attribute(name)?.trim_end_matches("px").trim_end_matches('%').parse().ok()
}

/// Re-maps every point in `path` through `matrix` — used for the shape
/// primitives (`rect`/`circle`/`ellipse`) whose `Path::*` constructors
/// already produce local, origin-centered geometry that then needs the
/// element's own position/the accumulated ancestor matrix applied on top,
/// the same way `parse_path`/`parse_points` apply `matrix` point-by-point
/// while building their commands directly.
fn recenter_path(path: Path, matrix: Matrix) -> Path {
    let commands = path
        .commands
        .into_iter()
        .map(|command| match command {
            PathCommand::MoveTo(p) => PathCommand::MoveTo(matrix.apply_vec2(p.x as f64, p.y as f64)),
            PathCommand::LineTo(p) => PathCommand::LineTo(matrix.apply_vec2(p.x as f64, p.y as f64)),
            PathCommand::CubicTo { control1, control2, to } => PathCommand::CubicTo {
                control1: matrix.apply_vec2(control1.x as f64, control1.y as f64),
                control2: matrix.apply_vec2(control2.x as f64, control2.y as f64),
                to: matrix.apply_vec2(to.x as f64, to.y as f64),
            },
            PathCommand::Close => PathCommand::Close,
        })
        .collect();
    Path { commands }
}

/// Parses `viewBox="min-x min-y width height"` — a plain 4-number list
/// (commas or whitespace between them, SVG allows either), not delegated to
/// `svgtypes::ViewBox` since a hand-rolled split/parse is simpler than that
/// type's own API for this one-shot use.
fn parse_view_box(value: &str) -> Option<(f64, f64, f64, f64)> {
    let mut numbers = value.split([',', ' ']).filter(|s| !s.is_empty()).filter_map(|s| s.parse::<f64>().ok());
    Some((numbers.next()?, numbers.next()?, numbers.next()?, numbers.next()?))
}

/// Imports `xml` (the contents of an `.svg` file) into a tree of
/// [`ImportedNode`]s (siblings at the document root, `<g>` hierarchy
/// preserved beneath — see this module's doc comment), centered as a whole
/// around the origin and flipped from SVG's own `+y`-down convention to this
/// workspace's `+y`-up one — the same `x - w/2, h/2 - y` conversion
/// `an1mbyt3-lottie`'s importer already applies, using `<svg>`'s own
/// `viewBox` (falling back to the union of every shape's own bounds if
/// there's no `viewBox`) as the "w/h" this centers against.
pub fn import_svg(xml: &str) -> Result<Vec<ImportedNode>, SvgImportError> {
    // `allow_dtd: true` — `roxmltree` rejects any XML with a `<!DOCTYPE ...>`
    // declaration by default (a security-conscious default against DTD-based
    // attacks like billion-laughs), but a DOCTYPE pointing at the standard
    // SVG 1.1 DTD is exactly what Illustrator/Inkscape/most SVG exporters
    // still emit — rejecting those outright would fail to import a large
    // fraction of real-world files. `roxmltree` keeps its own entity-expansion
    // depth/count limits regardless of this flag, so this doesn't reopen the
    // attack this default guards against, just stops treating an extremely
    // common, benign export artifact as a hard parse failure.
    let options = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let document = roxmltree::Document::parse_with_options(xml, options).map_err(SvgImportError::Xml)?;
    let root = document.root_element();
    if root.tag_name().name() != "svg" {
        return Err(SvgImportError::NoSvgRoot);
    }

    let root_matrix = match root.attribute("viewBox").and_then(parse_view_box) {
        Some((min_x, min_y, _, _)) => Matrix::translate(-min_x, -min_y),
        None => Matrix::IDENTITY,
    };

    let mut walker = Walker { counters: HashMap::new() };
    let mut nodes = walker.walk(root, root_matrix, [0.0, 0.0, 0.0, 1.0], 0);

    let center = match root.attribute("viewBox").and_then(parse_view_box) {
        Some((_, _, width, height)) => Vec2::new((width / 2.0) as f32, (height / 2.0) as f32),
        None => {
            let mut min = Vec2::splat(f32::INFINITY);
            let mut max = Vec2::splat(f32::NEG_INFINITY);
            collect_bounds(&nodes, &mut min, &mut max);
            if min.x > max.x { Vec2::ZERO } else { (min + max) / 2.0 }
        }
    };

    recenter_tree(&mut nodes, center);

    Ok(nodes)
}

/// Recursively unions every `Shape` leaf's bounds into `min`/`max` — the
/// tree-aware equivalent of the old flat-list bounds scan, used for the
/// no-`viewBox` fallback center.
fn collect_bounds(nodes: &[ImportedNode], min: &mut Vec2, max: &mut Vec2) {
    for node in nodes {
        match node {
            ImportedNode::Shape { path, .. } => {
                let (shape_min, shape_max) = path.bounds();
                *min = min.min(shape_min);
                *max = max.max(shape_max);
            }
            ImportedNode::Group { children, .. } => collect_bounds(children, min, max),
        }
    }
}

/// Recursively recenters every `Shape` leaf's path in the tree — the
/// tree-aware equivalent of the old flat-list final pass `import_svg` ran
/// once the overall center was known. `Group` nodes hold no geometry of
/// their own (always identity `Transform2D`, per this module's doc comment)
/// so there's nothing to recenter on them directly, only recurse through.
fn recenter_tree(nodes: &mut [ImportedNode], center: Vec2) {
    for node in nodes {
        match node {
            ImportedNode::Shape { path, gradient, .. } => {
                path.commands = path.commands.drain(..).map(|command| recenter_point(command, center)).collect();
                // A gradient's `start`/`end` were computed in this same
                // raw, uncentered/un-flipped SVG-space `world_matrix` the
                // path's own points started in (see `resolve_gradient`'s doc
                // comment) — they need the identical recenter-and-flip this
                // pass already applies to every path point, or the gradient
                // would end up pointing at the wrong place relative to the
                // now-recentered shape it's painted on.
                if let Some(gradient) = gradient {
                    gradient.start = recenter_vec2(gradient.start, center);
                    gradient.end = recenter_vec2(gradient.end, center);
                }
            }
            ImportedNode::Group { children, .. } => recenter_tree(children, center),
        }
    }
}

/// Shifts one `PathCommand`'s points by `-center` and flips `y` (SVG's
/// `+y`-down -> this workspace's `+y`-up) — the final pass `import_svg` runs
/// over every shape once the overall center is known.
fn recenter_point(command: PathCommand, center: Vec2) -> PathCommand {
    let map = |p: Vec2| recenter_vec2(p, center);
    match command {
        PathCommand::MoveTo(p) => PathCommand::MoveTo(map(p)),
        PathCommand::LineTo(p) => PathCommand::LineTo(map(p)),
        PathCommand::CubicTo { control1, control2, to } => PathCommand::CubicTo { control1: map(control1), control2: map(control2), to: map(to) },
        PathCommand::Close => PathCommand::Close,
    }
}

/// Shifts `p` by `-center` and flips `y` (SVG's `+y`-down -> this
/// workspace's `+y`-up) — shared by `recenter_point` (path points) and
/// `recenter_tree` (a `Shape`'s own `gradient.start`/`end`, which need the
/// identical mapping).
fn recenter_vec2(p: Vec2, center: Vec2) -> Vec2 {
    Vec2::new(p.x - center.x, center.y - p.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Depth-first flatten of every `Shape` leaf in a tree, in traversal
    /// order — most tests care about "which shapes came out and in what
    /// order/color," not the group nesting itself, so this restores the old
    /// flat-list-of-shapes ergonomics for those assertions; the handful of
    /// tests that DO care about hierarchy inspect `ImportedNode::Group`
    /// directly instead.
    fn flatten_shapes(nodes: &[ImportedNode]) -> Vec<(&str, &Path, [f32; 4])> {
        let mut out = Vec::new();
        for node in nodes {
            match node {
                ImportedNode::Shape { name, path, fill_color, .. } => out.push((name.as_str(), path, *fill_color)),
                ImportedNode::Group { children, .. } => out.extend(flatten_shapes(children)),
            }
        }
        out
    }

    #[test]
    fn a_plain_rect_becomes_one_shape_with_the_right_color() {
        let svg = r##"<svg viewBox="0 0 100 100"><rect x="0" y="0" width="100" height="100" fill="#ff0000"/></svg>"##;
        let nodes = import_svg(svg).unwrap();
        let shapes = flatten_shapes(&nodes);
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].2, [1.0, 0.0, 0.0, 1.0]);
        assert!(!shapes[0].1.commands.is_empty());
    }

    #[test]
    fn a_rect_filling_the_whole_viewbox_ends_up_centered_on_the_origin() {
        let svg = r##"<svg viewBox="0 0 100 100"><rect x="0" y="0" width="100" height="100" fill="#000000"/></svg>"##;
        let nodes = import_svg(svg).unwrap();
        let shapes = flatten_shapes(&nodes);
        let (min, max) = shapes[0].1.bounds();
        assert!((min.x - -50.0).abs() < 0.01);
        assert!((max.x - 50.0).abs() < 0.01);
        assert!((min.y - -50.0).abs() < 0.01);
        assert!((max.y - 50.0).abs() < 0.01);
    }

    #[test]
    fn a_group_transform_is_baked_into_its_childrens_paths() {
        let svg = r##"<svg viewBox="0 0 200 200"><g transform="translate(50, 0)"><rect x="0" y="0" width="10" height="10" fill="#00ff00"/></g></svg>"##;
        let nodes = import_svg(svg).unwrap();
        let shapes = flatten_shapes(&nodes);
        assert_eq!(shapes.len(), 1);
        // The rect's own center (5,5) shifted by the group's translate(50,0)
        // -> (55,5) in raw SVG space, then centered against the 200x200
        // viewBox (subtract (100,100)) and y-flipped -> (-45, 95).
        let (min, max) = shapes[0].1.bounds();
        let center = (min + max) / 2.0;
        assert!((center.x - -45.0).abs() < 0.01, "center: {center:?}");
        assert!((center.y - 95.0).abs() < 0.01, "center: {center:?}");
    }

    #[test]
    fn a_group_becomes_a_real_group_node_wrapping_its_child() {
        let svg = r##"<svg viewBox="0 0 100 100"><g id="Layer 1"><rect x="0" y="0" width="10" height="10" fill="#00ff00"/></g></svg>"##;
        let nodes = import_svg(svg).unwrap();
        assert_eq!(nodes.len(), 1, "the <g> must produce exactly one top-level node, not flatten its child to a sibling");
        let ImportedNode::Group { name, children, mask_index } = &nodes[0] else { panic!("expected a Group") };
        assert_eq!(name, "Layer 1");
        assert_eq!(children.len(), 1);
        assert!(matches!(children[0], ImportedNode::Shape { .. }));
        assert_eq!(*mask_index, None);
    }

    #[test]
    fn nested_groups_preserve_their_full_depth() {
        let svg = r##"<svg viewBox="0 0 100 100"><g id="Outer"><g id="Inner"><rect x="0" y="0" width="10" height="10" fill="#00ff00"/></g></g></svg>"##;
        let nodes = import_svg(svg).unwrap();
        let ImportedNode::Group { name: outer_name, children: outer_children, .. } = &nodes[0] else { panic!("expected outer Group") };
        assert_eq!(outer_name, "Outer");
        let ImportedNode::Group { name: inner_name, children: inner_children, .. } = &outer_children[0] else { panic!("expected inner Group") };
        assert_eq!(inner_name, "Inner");
        assert_eq!(inner_children.len(), 1);
    }

    #[test]
    fn an_empty_group_is_pruned_rather_than_kept_as_a_pointless_node() {
        let svg = r##"<svg viewBox="0 0 100 100"><g id="Empty"></g><rect x="0" y="0" width="10" height="10" fill="#ff0000"/></svg>"##;
        let nodes = import_svg(svg).unwrap();
        assert_eq!(nodes.len(), 1, "the empty group must not appear at all");
        assert!(matches!(nodes[0], ImportedNode::Shape { .. }));
    }

    #[test]
    fn a_fill_set_on_a_group_is_inherited_by_its_children() {
        let svg = r##"<svg viewBox="0 0 100 100"><g fill="#0000ff"><rect x="0" y="0" width="10" height="10"/></g></svg>"##;
        let nodes = import_svg(svg).unwrap();
        assert_eq!(flatten_shapes(&nodes)[0].2, [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn a_path_with_a_curve_parses_into_cubic_commands() {
        let svg = r##"<svg viewBox="0 0 100 100"><path d="M10 10 C 20 20, 40 20, 50 10 Z" fill="#ffffff"/></svg>"##;
        let nodes = import_svg(svg).unwrap();
        let shapes = flatten_shapes(&nodes);
        assert_eq!(shapes.len(), 1);
        assert!(shapes[0].1.commands.iter().any(|c| matches!(c, PathCommand::CubicTo { .. })));
        assert!(matches!(shapes[0].1.commands.last(), Some(PathCommand::Close)));
    }

    #[test]
    fn an_svg_with_no_svg_root_is_rejected() {
        assert!(matches!(import_svg("<not-svg></not-svg>"), Err(SvgImportError::NoSvgRoot)));
    }

    #[test]
    fn fill_none_falls_back_to_a_visible_gray_rather_than_disappearing() {
        let svg = r##"<svg viewBox="0 0 100 100"><rect x="0" y="0" width="10" height="10" fill="none"/></svg>"##;
        let nodes = import_svg(svg).unwrap();
        assert_eq!(flatten_shapes(&nodes)[0].2, [0.35, 0.35, 0.35, 1.0]);
    }

    #[test]
    fn a_doctype_declaration_does_not_fail_the_import() {
        let svg = r##"<?xml version="1.0"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<svg viewBox="0 0 100 100"><rect x="0" y="0" width="10" height="10" fill="#ff0000"/></svg>"##;
        let nodes = import_svg(svg).unwrap();
        assert_eq!(flatten_shapes(&nodes).len(), 1);
    }

    #[test]
    fn defs_contents_are_not_imported_unless_referenced_by_use() {
        let svg = r##"<svg viewBox="0 0 100 100"><defs><rect id="icon" x="0" y="0" width="10" height="10" fill="#ff0000"/></defs></svg>"##;
        let nodes = import_svg(svg).unwrap();
        assert!(nodes.is_empty(), "a <defs> shape with no <use> reference should not be imported");
    }

    #[test]
    fn a_use_element_resolves_its_defs_reference() {
        let svg = r##"<svg viewBox="0 0 100 100">
            <defs><rect id="icon" x="0" y="0" width="10" height="10" fill="#00ff00"/></defs>
            <use href="#icon" x="20" y="0"/>
        </svg>"##;
        let nodes = import_svg(svg).unwrap();
        let shapes = flatten_shapes(&nodes);
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].2, [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn a_use_element_resolves_the_older_xlink_href_form() {
        let svg = r##"<svg viewBox="0 0 100 100" xmlns:xlink="http://www.w3.org/1999/xlink">
            <defs><rect id="icon" x="0" y="0" width="10" height="10" fill="#0000ff"/></defs>
            <use xlink:href="#icon"/>
        </svg>"##;
        let nodes = import_svg(svg).unwrap();
        let shapes = flatten_shapes(&nodes);
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].2, [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn an_inline_style_fill_overrides_the_presentation_attribute() {
        let svg = r##"<svg viewBox="0 0 100 100"><rect x="0" y="0" width="10" height="10" fill="#ff0000" style="fill: #00ff00;"/></svg>"##;
        let nodes = import_svg(svg).unwrap();
        assert_eq!(flatten_shapes(&nodes)[0].2, [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn a_style_element_and_a_title_element_are_not_imported_as_shapes() {
        let svg = r##"<svg viewBox="0 0 100 100"><title>My Icon</title><style>rect { fill: red; }</style><rect x="0" y="0" width="10" height="10" fill="#0000ff"/></svg>"##;
        let nodes = import_svg(svg).unwrap();
        let shapes = flatten_shapes(&nodes);
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].2, [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn a_gradient_fill_resolves_to_the_average_of_its_stops_instead_of_falling_back_to_black() {
        let svg = r##"<svg viewBox="0 0 100 100">
            <defs><linearGradient id="g"><stop offset="0" stop-color="#000000"/><stop offset="1" stop-color="#ffffff"/></linearGradient></defs>
            <rect x="0" y="0" width="10" height="10" fill="url(#g)"/>
        </svg>"##;
        let nodes = import_svg(svg).unwrap();
        let shapes = flatten_shapes(&nodes);
        assert_eq!(shapes.len(), 1);
        // Average of black and white -> mid gray, not the black a failed
        // `url(...)` parse used to silently fall back to.
        assert!((shapes[0].2[0] - 0.5).abs() < 0.01, "got {:?}", shapes[0].2);
    }

    /// Depth-first search for the first `Shape`'s own `gradient`, if any —
    /// `flatten_shapes` deliberately drops it (most tests don't care), so
    /// gradient-specific tests need their own accessor.
    fn first_gradient(nodes: &[ImportedNode]) -> Option<&an1mbyt3_core::GradientDef> {
        for node in nodes {
            match node {
                ImportedNode::Shape { gradient: Some(g), .. } => return Some(g),
                ImportedNode::Shape { .. } => {}
                ImportedNode::Group { children, .. } => {
                    if let Some(g) = first_gradient(children) {
                        return Some(g);
                    }
                }
            }
        }
        None
    }

    /// Depth-first search for the first `Shape`'s own `stroke`, if any — same
    /// reasoning as `first_gradient`.
    fn first_stroke(nodes: &[ImportedNode]) -> Option<&Stroke> {
        for node in nodes {
            match node {
                ImportedNode::Shape { stroke: Some(s), .. } => return Some(s),
                ImportedNode::Shape { .. } => {}
                ImportedNode::Group { children, .. } => {
                    if let Some(s) = first_stroke(children) {
                        return Some(s);
                    }
                }
            }
        }
        None
    }

    #[test]
    fn a_stroked_line_imports_as_a_real_stroke_with_a_transparent_fill() {
        // The user's own bug report: a plain `<line>` (no fill by nature,
        // stroke-only) was importing as a filled gray blob instead of an
        // actual line — `fill="none"` without a real stroke is genuinely
        // ambiguous ("invisible, or did the author forget to set a color?"),
        // but a stroke-only shape isn't ambiguous at all.
        let svg = r##"<svg viewBox="0 0 100 100">
            <line x1="0" y1="0" x2="10" y2="10" stroke="#ff0000" stroke-width="2" stroke-linecap="round"/>
        </svg>"##;
        let nodes = import_svg(svg).unwrap();
        let shapes = flatten_shapes(&nodes);
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].2[3], 0.0, "a stroke-only shape should have a transparent fill, not the gray fallback");
        let stroke = first_stroke(&nodes).expect("line should resolve a real stroke");
        assert_eq!(stroke.color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(stroke.width, 2.0);
        assert_eq!(stroke.cap, StrokeCap::Round);
    }

    #[test]
    fn fill_none_with_no_stroke_still_falls_back_to_gray() {
        // Same `fill="none"` case the existing
        // `fill_none_falls_back_to_a_visible_gray_rather_than_disappearing`
        // test covers, just re-asserted here to pin down that adding stroke
        // support didn't change behavior for the genuinely-no-paint-at-all
        // case.
        let svg = r##"<svg viewBox="0 0 100 100"><rect x="0" y="0" width="10" height="10" fill="none"/></svg>"##;
        let nodes = import_svg(svg).unwrap();
        assert_eq!(flatten_shapes(&nodes)[0].2, [0.35, 0.35, 0.35, 1.0]);
        assert!(first_stroke(&nodes).is_none());
    }

    #[test]
    fn a_linear_gradient_fill_produces_a_real_multi_stop_gradient() {
        let svg = r##"<svg viewBox="0 0 100 100">
            <defs><linearGradient id="g"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient></defs>
            <rect x="0" y="0" width="100" height="100" fill="url(#g)"/>
        </svg>"##;
        let nodes = import_svg(svg).unwrap();
        let gradient = first_gradient(&nodes).expect("rect fill should resolve to a real gradient");
        assert_eq!(gradient.kind, an1mbyt3_core::GradientKind::Linear);
        assert_eq!(gradient.stops.len(), 2);
        assert_eq!(gradient.stops[0].color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(gradient.stops[1].color, [0.0, 0.0, 1.0, 1.0]);
        // Default `objectBoundingBox` `x1/y1/x2/y2` (0,0 -> 1,0) mapped onto
        // the rect's own full-bleed bbox, then through the same recenter/
        // y-flip pass the rect's own path went through: left edge -> right
        // edge, at the (flipped) top of the shape.
        assert!((gradient.start - Vec2::new(-50.0, 50.0)).length() < 0.01, "got {:?}", gradient.start);
        assert!((gradient.end - Vec2::new(50.0, 50.0)).length() < 0.01, "got {:?}", gradient.end);
    }

    #[test]
    fn a_radial_gradient_fill_produces_a_centered_gradient_with_the_right_radius() {
        let svg = r##"<svg viewBox="0 0 100 100">
            <defs><radialGradient id="g"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#000000"/></radialGradient></defs>
            <rect x="0" y="0" width="100" height="100" fill="url(#g)"/>
        </svg>"##;
        let nodes = import_svg(svg).unwrap();
        let gradient = first_gradient(&nodes).expect("rect fill should resolve to a real gradient");
        assert_eq!(gradient.kind, an1mbyt3_core::GradientKind::Radial);
        // Default `cx/cy/r` (50%, 50%, 50%) center on the rect's own bbox
        // center, which is exactly the artboard's own center here — so after
        // recentering, `start` lands on the origin, with radius 50.
        assert!(gradient.start.length() < 0.01, "got {:?}", gradient.start);
        assert!(((gradient.end - gradient.start).length() - 50.0).abs() < 0.01, "got {:?}", gradient.end);
    }

    #[test]
    fn a_gradient_with_no_stops_of_its_own_inherits_them_via_href() {
        let svg = r##"<svg viewBox="0 0 100 100">
            <defs>
                <linearGradient id="base"><stop offset="0" stop-color="#ff0000"/></linearGradient>
                <linearGradient id="derived" href="#base"/>
            </defs>
            <rect x="0" y="0" width="10" height="10" fill="url(#derived)"/>
        </svg>"##;
        let nodes = import_svg(svg).unwrap();
        assert_eq!(flatten_shapes(&nodes)[0].2, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn a_pattern_fill_resolves_to_the_average_of_its_content_shapes() {
        let svg = r##"<svg viewBox="0 0 100 100">
            <defs><pattern id="p"><rect width="5" height="5" fill="#000000"/><rect width="5" height="5" fill="#ffffff"/></pattern></defs>
            <rect x="0" y="0" width="10" height="10" fill="url(#p)"/>
        </svg>"##;
        let nodes = import_svg(svg).unwrap();
        let shapes = flatten_shapes(&nodes);
        assert_eq!(shapes.len(), 1);
        assert!((shapes[0].2[0] - 0.5).abs() < 0.01, "got {:?}", shapes[0].2);
    }

    #[test]
    fn a_clip_path_reference_wraps_the_element_in_a_masked_group() {
        let svg = r##"<svg viewBox="0 0 100 100">
            <defs><clipPath id="c"><circle cx="5" cy="5" r="4" fill="#ffffff"/></clipPath></defs>
            <rect id="Clipped" x="0" y="0" width="10" height="10" fill="#ff0000" clip-path="url(#c)"/>
        </svg>"##;
        let nodes = import_svg(svg).unwrap();
        assert_eq!(nodes.len(), 1);
        let ImportedNode::Group { children, mask_index, .. } = &nodes[0] else { panic!("expected a synthetic masked Group") };
        assert_eq!(children.len(), 2, "the clipped shape plus the imported clip geometry");
        assert_eq!(*mask_index, Some(1));
        assert!(matches!(children[0], ImportedNode::Shape { .. }));
        assert!(matches!(children[1], ImportedNode::Shape { .. }));
    }

    #[test]
    fn display_none_hides_the_whole_subtree() {
        let svg = r##"<svg viewBox="0 0 100 100">
            <g display="none"><rect x="0" y="0" width="10" height="10" fill="#ff0000"/></g>
            <rect x="20" y="0" width="10" height="10" fill="#00ff00"/>
        </svg>"##;
        let nodes = import_svg(svg).unwrap();
        let shapes = flatten_shapes(&nodes);
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].2, [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn visibility_hidden_via_inline_style_hides_the_element() {
        let svg = r##"<svg viewBox="0 0 100 100"><rect x="0" y="0" width="10" height="10" fill="#ff0000" style="visibility: hidden;"/></svg>"##;
        let nodes = import_svg(svg).unwrap();
        assert!(nodes.is_empty());
    }
}
