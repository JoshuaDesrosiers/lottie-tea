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

/// The liquid's outline at `threshold`, as closed polygons. `detail` is the
/// grid cell size relative to the smallest blob's field radius (smaller =
/// smoother, costlier); `0.2` is a good default.
pub fn contour(blobs: &[Blob], threshold: f32, detail: f32) -> Vec<Vec<Vec2>> {
    let blobs: Vec<&Blob> = blobs.iter().filter(|b| b.radius > 0.0 && b.weight > 0.0).collect();
    if blobs.is_empty() {
        return Vec::new();
    }
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    let mut smallest = f32::INFINITY;
    for b in &blobs {
        min = min.min(b.center - Vec2::splat(b.radius));
        max = max.max(b.center + Vec2::splat(b.radius));
        smallest = smallest.min(b.radius);
    }
    // One extra cell of margin so outlines always close inside the grid.
    let mut cell = (smallest * detail.clamp(0.05, 1.0)).max(0.0001);
    let span = max - min;
    while ((span.x / cell) as usize + 3) * ((span.y / cell) as usize + 3) > MAX_CELLS {
        cell *= 1.25;
    }
    let origin = min - Vec2::splat(cell);
    let w = (span.x / cell) as usize + 3;
    let h = (span.y / cell) as usize + 3;

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
        let loops = contour(&[Blob { center: Vec2::new(5.0, -3.0), radius: r, weight: 1.0 }], 0.5, 0.1);
        assert_eq!(loops.len(), 1);
        let visible = r * visible_share(0.5);
        let expected = std::f32::consts::PI * visible * visible;
        assert!((area(&loops[0]) - expected).abs() / expected < 0.05, "{} vs {}", area(&loops[0]), expected);
    }

    #[test]
    fn far_blobs_stay_apart_close_blobs_merge() {
        let blob = |x: f32| Blob { center: Vec2::new(x, 0.0), radius: 10.0, weight: 1.0 };
        assert_eq!(contour(&[blob(0.0), blob(30.0)], 0.5, 0.1).len(), 2, "apart");
        // Visible radii ≈ 4.5 each; at 11 apart the fields join into one body.
        assert_eq!(contour(&[blob(0.0), blob(11.0)], 0.5, 0.1).len(), 1, "merged");
    }
}
