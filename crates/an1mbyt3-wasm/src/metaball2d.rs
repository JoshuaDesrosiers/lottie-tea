//! 2D **metaballs**: the gooey "liquid drops pulling at each other" look for
//! 2D particles (`an1mbyt3_core::ParticleFluid2D`).
//!
//! Every blob contributes a smooth field, `w · (1 − d²/R²)³` (zero beyond
//! `R`, smooth to its second derivative), and the liquid is wherever the
//! summed field passes `threshold`. Two blobs far apart are two circles;
//! moving closer, their fields add up in the gap and a neck grows between
//! them until they merge into one body — exactly the vector "gooey" effect.
//!
//! The outline is extracted on the CPU with **marching squares** (edge
//! crossings linearly interpolated, one Chaikin smoothing pass) and
//! returned as closed polygons, which the editor tessellates and draws as
//! ordinary 2D shapes — so the liquid sits at its node's place in the draw
//! order, masks and lights apply, and edges stay crisp vector curves at
//! any zoom. Polygons may nest (holes); they're filled even-odd.

use std::collections::HashMap;

use glam::Vec2;

/// One blob: centre, field radius `R` (world units) and weight.
#[derive(Debug, Clone, Copy)]
pub struct Blob {
    pub center: Vec2,
    pub radius: f32,
    pub weight: f32,
}

/// The visible radius of a lone blob of weight 1 as a share of its field
/// radius at `threshold`: solve `(1 − x²)³ = threshold`.
pub fn visible_share(threshold: f32) -> f32 {
    (1.0 - threshold.clamp(0.0001, 0.9999).cbrt()).max(0.0).sqrt()
}

/// Most grid cells a frame may use (caps cost for huge spreads).
const MAX_CELLS: usize = 160_000;

/// The sampling grid for a field covering `min..max`: **locked to the
/// world** — corners sit on multiples of `cell` from the world origin, and
/// `cell` itself only ever changes by whole doublings (when a spread gets
/// huge). So a still member is sampled at exactly the same points every
/// frame and its outline doesn't shimmer, however the rest of its group
/// (and so the grid's extent) moves. Returns `(cell, origin, w, h)`.
fn world_grid(min: Vec2, max: Vec2, base_cell: f32) -> (f32, Vec2, usize, usize) {
    let mut cell = base_cell.max(0.05);
    loop {
        let origin = ((min / cell).floor() - Vec2::ONE) * cell;
        let w = ((max.x - origin.x) / cell).ceil() as usize + 2;
        let h = ((max.y - origin.y) / cell).ceil() as usize + 2;
        if w * h <= MAX_CELLS {
            return (cell, origin, w, h);
        }
        cell *= 2.0;
    }
}

/// The liquid's outline at `threshold`, as closed polygons, sampled on a
/// world-locked grid of `cell`-sized squares (see [`world_grid`]). Keep
/// `cell` the same from frame to frame (derive it from settings, not from
/// the current blobs) or outlines shimmer as sizes change.
pub fn contour(blobs: &[Blob], threshold: f32, cell: f32) -> Vec<Vec<Vec2>> {
    let blobs: Vec<&Blob> = blobs.iter().filter(|b| b.radius > 0.0 && b.weight > 0.0).collect();
    if blobs.is_empty() {
        return Vec::new();
    }
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for b in &blobs {
        min = min.min(b.center - Vec2::splat(b.radius));
        max = max.max(b.center + Vec2::splat(b.radius));
    }
    let (cell, origin, w, h) = world_grid(min, max, cell);

    // Splat every blob's field onto the grid corners it reaches.
    let mut field = vec![0.0f32; w * h];
    for b in &blobs {
        let r2 = b.radius * b.radius;
        let lo = ((b.center - Vec2::splat(b.radius) - origin) / cell).floor().max(Vec2::ZERO);
        let hi = ((b.center + Vec2::splat(b.radius) - origin) / cell).ceil().min(Vec2::new((w - 1) as f32, (h - 1) as f32));
        for j in lo.y as usize..=hi.y as usize {
            let y = origin.y + j as f32 * cell - b.center.y;
            for i in lo.x as usize..=hi.x as usize {
                let x = origin.x + i as f32 * cell - b.center.x;
                let q = 1.0 - (x * x + y * y) / r2;
                if q > 0.0 {
                    field[j * w + i] += b.weight * q * q * q;
                }
            }
        }
    }

    march(&field, w, h, origin, cell, threshold)
}

/// Marching squares over a `w × h` grid of field samples (row-major,
/// corner `(i, j)` at `origin + (i, j) · cell`): the `threshold` outline as
/// closed, Chaikin-smoothed polygons.
fn march(field: &[f32], w: usize, h: usize, origin: Vec2, cell: f32, threshold: f32) -> Vec<Vec<Vec2>> {
    // Marching squares: segments between crossing points on cell edges.
    // Edge ids: horizontal (i,j)→(i+1,j) = 2·(j·w+i); vertical (i,j)→(i,j+1) = 2·(j·w+i)+1.
    let at = |i: usize, j: usize| field[j * w + i];
    let corner = |i: usize, j: usize| origin + Vec2::new(i as f32, j as f32) * cell;
    let mut points: HashMap<usize, Vec2> = HashMap::new();
    let mut point = |edge: usize| -> usize {
        points.entry(edge).or_insert_with(|| {
            let cellid = edge / 2;
            let (i, j) = (cellid % w, cellid / w);
            let (a, b, pa, pb) = if edge % 2 == 0 { (at(i, j), at(i + 1, j), corner(i, j), corner(i + 1, j)) } else { (at(i, j), at(i, j + 1), corner(i, j), corner(i, j + 1)) };
            let t = if (b - a).abs() > 1e-9 { ((threshold - a) / (b - a)).clamp(0.0, 1.0) } else { 0.5 };
            pa + (pb - pa) * t
        });
        edge
    };
    let mut links: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut link = |a: usize, b: usize| {
        links.entry(a).or_default().push(b);
        links.entry(b).or_default().push(a);
    };
    for j in 0..h - 1 {
        for i in 0..w - 1 {
            let (v0, v1, v2, v3) = (at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
            let case = (v0 > threshold) as u8 | ((v1 > threshold) as u8) << 1 | ((v2 > threshold) as u8) << 2 | ((v3 > threshold) as u8) << 3;
            if case == 0 || case == 15 {
                continue;
            }
            let bottom = 2 * (j * w + i);
            let top = 2 * ((j + 1) * w + i);
            let left = 2 * (j * w + i) + 1;
            let right = 2 * (j * w + i + 1) + 1;
            let saddle_joined = (v0 + v1 + v2 + v3) * 0.25 > threshold;
            let pairs: &[(usize, usize)] = match case {
                1 | 14 => &[(left, bottom)],
                2 | 13 => &[(bottom, right)],
                3 | 12 => &[(left, right)],
                4 | 11 => &[(right, top)],
                6 | 9 => &[(bottom, top)],
                7 | 8 => &[(left, top)],
                // Saddles: join the two "inside" corners if the centre is inside.
                5 => if saddle_joined { &[(left, top), (bottom, right)] } else { &[(left, bottom), (right, top)] },
                10 => if saddle_joined { &[(left, bottom), (right, top)] } else { &[(left, top), (bottom, right)] },
                _ => &[],
            };
            for &(a, b) in pairs {
                link(point(a), point(b));
            }
        }
    }

    // Walk the links into closed loops.
    let mut loops = Vec::new();
    let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut starts: Vec<usize> = links.keys().copied().collect();
    starts.sort_unstable();
    for start in starts {
        if seen.contains(&start) {
            continue;
        }
        let mut ring = vec![start];
        seen.insert(start);
        let mut prev = usize::MAX;
        let mut cur = start;
        loop {
            let Some(next) = links.get(&cur).and_then(|n| n.iter().copied().find(|&n| n != prev && !seen.contains(&n)).or_else(|| n.iter().copied().find(|&n| n == start && prev != usize::MAX && ring.len() > 2))) else {
                break;
            };
            if next == start {
                break;
            }
            seen.insert(next);
            ring.push(next);
            prev = cur;
            cur = next;
        }
        if ring.len() >= 3 {
            let poly: Vec<Vec2> = ring.iter().map(|e| points[e]).collect();
            loops.push(chaikin(&poly));
        }
    }
    loops
}

/// Anything that can be part of a gooey group (`an1mbyt3_core::Goo2D`), in
/// world space: a disc (particles) or filled polygons (shapes, text,
/// sprites — even-odd, so glyph holes stay holes).
#[derive(Debug, Clone)]
pub enum Source {
    Circle { center: Vec2, radius: f32, weight: f32 },
    Polygons { loops: Vec<Vec<Vec2>>, weight: f32 },
}

impl Source {
    fn bounds(&self) -> (Vec2, Vec2) {
        match self {
            Source::Circle { center, radius, .. } => (*center - Vec2::splat(*radius), *center + Vec2::splat(*radius)),
            Source::Polygons { loops, .. } => {
                let mut min = Vec2::splat(f32::INFINITY);
                let mut max = Vec2::splat(f32::NEG_INFINITY);
                for p in loops.iter().flatten() {
                    min = min.min(*p);
                    max = max.max(*p);
                }
                (min, max)
            }
        }
    }

    fn weight(&self) -> f32 {
        match self {
            Source::Circle { weight, .. } | Source::Polygons { weight, .. } => *weight,
        }
    }

    /// Signed distance from `p` to the outline (negative inside).
    fn distance(&self, p: Vec2) -> f32 {
        match self {
            Source::Circle { center, radius, .. } => p.distance(*center) - radius,
            Source::Polygons { loops, .. } => {
                let mut best = f32::INFINITY;
                let mut inside = false;
                for ring in loops {
                    let n = ring.len();
                    for k in 0..n {
                        let (a, b) = (ring[k], ring[(k + 1) % n]);
                        let ab = b - a;
                        let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
                        best = best.min(p.distance_squared(a + ab * t));
                        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
                            inside = !inside;
                        }
                    }
                }
                let d = best.sqrt();
                if inside { -d } else { d }
            }
        }
    }
}

/// The gooey outline of `sources` merged: each source's field is 1
/// inside it and falls smoothly to 0 at `reach` outside its outline,
/// shaped so a lone source's outline is exactly its own (the field there
/// equals the threshold). Where sources come within `reach` of each other
/// their fields add up and a smooth neck joins them. `level` (0..1) picks
/// an outline *inside* the liquid (1 = the plain outline; smaller values
/// sit deeper, for rim bands and highlights): it is the share of the way
/// from the threshold down to "fully inside".
pub fn contour_sources(sources: &[Source], reach: f32, inset: f32, detail: f32) -> Vec<Vec<Vec2>> {
    let sources: Vec<&Source> = sources.iter().filter(|s| s.weight() > 0.0).collect();
    if sources.is_empty() {
        return Vec::new();
    }
    const THRESHOLD: f32 = 0.5;
    let share = visible_share(THRESHOLD);
    // The kernel's radius R and the outline's distance R0 from its "core",
    // chosen so the field crosses THRESHOLD exactly on the outline and
    // reaches 0 at `reach` outside it.
    let reach = reach.max(0.5);
    let big_r = reach / (1.0 - share);
    let r0 = big_r * share;
    let kernel = |d: f32| {
        let x = ((d + r0).max(0.0) / big_r).min(1.0);
        let q = 1.0 - x * x;
        q * q * q
    };
    // An outline `inset` world units inside the liquid's edge.
    let level = kernel(-inset.clamp(0.0, r0 * 0.98));

    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for s in &sources {
        let (a, b) = s.bounds();
        min = min.min(a);
        max = max.max(b);
    }
    min -= Vec2::splat(reach);
    max += Vec2::splat(reach);
    let (cell, origin, w, h) = world_grid(min, max, (reach * detail.clamp(0.05, 1.0)).max(0.25));
    let mut field = vec![0.0f32; w * h];
    for s in &sources {
        let (a, b) = s.bounds();
        let lo = ((a - Vec2::splat(reach) - origin) / cell).floor().max(Vec2::ZERO);
        let hi = ((b + Vec2::splat(reach) - origin) / cell).ceil().min(Vec2::new((w - 1) as f32, (h - 1) as f32));
        let weight = s.weight();
        for j in lo.y as usize..=hi.y as usize {
            for i in lo.x as usize..=hi.x as usize {
                let p = origin + Vec2::new(i as f32, j as f32) * cell;
                let k = kernel(s.distance(p));
                if k > 0.0 {
                    field[j * w + i] += weight * k;
                }
            }
        }
    }
    march(&field, w, h, origin, cell, level)
}

/// A path flattened to polygons (cubics sampled), each point mapped by `to`.
pub fn flatten_path(path: &an1mbyt3_core::Path, to: impl Fn(Vec2) -> Vec2) -> Vec<Vec<Vec2>> {
    use an1mbyt3_core::PathCommand as C;
    let mut loops = Vec::new();
    let mut ring: Vec<Vec2> = Vec::new();
    let mut cur = Vec2::ZERO;
    for c in &path.commands {
        match *c {
            C::MoveTo(p) => {
                if ring.len() >= 3 {
                    loops.push(std::mem::take(&mut ring));
                }
                ring.clear();
                cur = p;
                ring.push(to(p));
            }
            C::LineTo(p) => {
                cur = p;
                ring.push(to(p));
            }
            C::CubicTo { control1, control2, to: end } => {
                const STEPS: usize = 12;
                for k in 1..=STEPS {
                    let t = k as f32 / STEPS as f32;
                    let u = 1.0 - t;
                    let p = cur * (u * u * u) + control1 * (3.0 * u * u * t) + control2 * (3.0 * u * t * t) + end * (t * t * t);
                    ring.push(to(p));
                }
                cur = end;
            }
            C::Close => {
                if ring.len() >= 3 {
                    loops.push(std::mem::take(&mut ring));
                }
                ring.clear();
            }
        }
    }
    if ring.len() >= 3 {
        loops.push(ring);
    }
    loops
}

/// One round of Chaikin corner cutting on a closed polygon (rounds off the
/// grid's tiny facets).
fn chaikin(poly: &[Vec2]) -> Vec<Vec2> {
    let n = poly.len();
    let mut out = Vec::with_capacity(n * 2);
    for k in 0..n {
        let (a, b) = (poly[k], poly[(k + 1) % n]);
        out.push(a * 0.75 + b * 0.25);
        out.push(a * 0.25 + b * 0.75);
    }
    out
}

/// Polygons as a path (for tessellation), optionally shifted.
pub fn to_path(loops: &[Vec<Vec2>], offset: Vec2) -> an1mbyt3_core::Path {
    let mut commands = Vec::new();
    for ring in loops {
        let Some(first) = ring.first() else { continue };
        commands.push(an1mbyt3_core::PathCommand::MoveTo(*first + offset));
        for p in &ring[1..] {
            commands.push(an1mbyt3_core::PathCommand::LineTo(*p + offset));
        }
        commands.push(an1mbyt3_core::PathCommand::Close);
    }
    an1mbyt3_core::Path { commands }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(poly: &[Vec2]) -> f32 {
        let mut a = 0.0;
        for k in 0..poly.len() {
            let (p, q) = (poly[k], poly[(k + 1) % poly.len()]);
            a += p.x * q.y - q.x * p.y;
        }
        (a * 0.5).abs()
    }

    #[test]
    fn a_lone_blob_is_a_circle_of_its_visible_radius() {
        let r = 10.0;
        let loops = contour(&[Blob { center: Vec2::new(5.0, -3.0), radius: r, weight: 1.0 }], 0.5, 1.0);
        assert_eq!(loops.len(), 1);
        let visible = r * visible_share(0.5);
        let expected = std::f32::consts::PI * visible * visible;
        assert!((area(&loops[0]) - expected).abs() / expected < 0.05, "{} vs {}", area(&loops[0]), expected);
    }

    #[test]
    fn a_lone_square_keeps_its_outline_and_two_close_ones_merge() {
        let square = |x: f32| Source::Polygons { loops: vec![vec![Vec2::new(x, 0.0), Vec2::new(x + 40.0, 0.0), Vec2::new(x + 40.0, 40.0), Vec2::new(x, 40.0)]], weight: 1.0 };
        let alone = contour_sources(&[square(0.0)], 20.0, 0.0, 0.1);
        assert_eq!(alone.len(), 1);
        // Corners round off a little, but the area is the square's.
        assert!((area(&alone[0]) - 1600.0).abs() / 1600.0 < 0.06, "{}", area(&alone[0]));
        assert_eq!(contour_sources(&[square(0.0), square(100.0)], 20.0, 0.0, 0.1).len(), 2, "apart");
        assert_eq!(contour_sources(&[square(0.0), square(50.0)], 20.0, 0.0, 0.1).len(), 1, "a neck joins them");
    }

    #[test]
    fn a_still_blob_outline_is_identical_however_its_neighbour_moves() {
        let still = Blob { center: Vec2::new(0.0, 0.0), radius: 10.0, weight: 1.0 };
        let outline = |other_x: f32| {
            let loops = contour(&[still, Blob { center: Vec2::new(other_x, 0.0), radius: 10.0, weight: 1.0 }], 0.5, 1.0);
            // The loop round the still blob (the one nearest the origin).
            let mut ring = loops.into_iter().min_by(|a, b| a[0].length().total_cmp(&b[0].length())).unwrap();
            ring.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
            ring
        };
        // The neighbour is far away and moving: the grid's extent changes,
        // the still blob's outline must not.
        assert_eq!(outline(60.0), outline(63.7));
    }

    #[test]
    fn far_blobs_stay_apart_close_blobs_merge() {
        let blob = |x: f32| Blob { center: Vec2::new(x, 0.0), radius: 10.0, weight: 1.0 };
        assert_eq!(contour(&[blob(0.0), blob(30.0)], 0.5, 1.0).len(), 2, "apart");
        // Visible radii ≈ 4.5 each; at 11 apart the fields join into one body.
        assert_eq!(contour(&[blob(0.0), blob(11.0)], 0.5, 1.0).len(), 1, "merged");
    }
}
