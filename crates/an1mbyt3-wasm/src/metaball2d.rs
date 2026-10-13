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
#[allow(dead_code)] // tests / tools
pub fn contour(blobs: &[Blob], threshold: f32, cell: f32) -> Vec<Vec<Vec2>> {
    Field::from_blobs(blobs, cell).contour(threshold)
}

/// Marching squares over a `w × h` grid of field samples (row-major,
/// corner `(i, j)` at `origin + (i, j) · cell`): the `threshold` outline as
/// closed, Chaikin-smoothed polygons.
///
/// Crossing points and their links live in flat arrays indexed by cell
/// edge (no hashing — this runs several times a frame per liquid). Every
/// crossing point has exactly two neighbours on a closed outline (the grid
/// always has an empty margin), so the walk is a simple chain follow.
fn march(field: &[f32], w: usize, h: usize, origin: Vec2, cell: f32, threshold: f32) -> Vec<Vec<Vec2>> {
    const NONE: u32 = u32::MAX;
    // Edge ids: horizontal (i,j)→(i+1,j) = 2·(j·w+i); vertical (i,j)→(i,j+1) = 2·(j·w+i)+1.
    let mut point_of_edge: Vec<u32> = vec![NONE; 2 * w * h];
    let mut points: Vec<Vec2> = Vec::new();
    let mut links: Vec<[u32; 2]> = Vec::new();
    let at = |i: usize, j: usize| field[j * w + i];
    let mut point = |edge: usize, points: &mut Vec<Vec2>, links: &mut Vec<[u32; 2]>| -> u32 {
        if point_of_edge[edge] != NONE {
            return point_of_edge[edge];
        }
        let cellid = edge / 2;
        let (i, j) = (cellid % w, cellid / w);
        let (a, b, pa, pb) = if edge % 2 == 0 {
            (at(i, j), at(i + 1, j), Vec2::new(i as f32, j as f32), Vec2::new(i as f32 + 1.0, j as f32))
        } else {
            (at(i, j), at(i, j + 1), Vec2::new(i as f32, j as f32), Vec2::new(i as f32, j as f32 + 1.0))
        };
        let t = if (b - a).abs() > 1e-9 { ((threshold - a) / (b - a)).clamp(0.0, 1.0) } else { 0.5 };
        let id = points.len() as u32;
        points.push(origin + (pa + (pb - pa) * t) * cell);
        links.push([NONE, NONE]);
        point_of_edge[edge] = id;
        id
    };
    let link = |a: u32, b: u32, links: &mut Vec<[u32; 2]>| {
        for (x, y) in [(a, b), (b, a)] {
            let l = &mut links[x as usize];
            if l[0] == NONE {
                l[0] = y;
            } else if l[1] == NONE {
                l[1] = y;
            }
        }
    };
    for j in 0..h - 1 {
        let row = j * w;
        for i in 0..w - 1 {
            let (v0, v1, v2, v3) = (field[row + i], field[row + i + 1], field[row + w + i + 1], field[row + w + i]);
            let case = (v0 > threshold) as u8 | ((v1 > threshold) as u8) << 1 | ((v2 > threshold) as u8) << 2 | ((v3 > threshold) as u8) << 3;
            if case == 0 || case == 15 {
                continue;
            }
            let bottom = 2 * (row + i);
            let top = 2 * (row + w + i);
            let left = 2 * (row + i) + 1;
            let right = 2 * (row + i + 1) + 1;
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
            for &(ea, eb) in pairs {
                let pa = point(ea, &mut points, &mut links);
                let pb = point(eb, &mut points, &mut links);
                link(pa, pb, &mut links);
            }
        }
    }

    // Follow the chains into closed loops (in creation order: deterministic).
    let mut seen = vec![false; points.len()];
    let mut loops = Vec::new();
    for start in 0..points.len() {
        if seen[start] {
            continue;
        }
        let mut ring = Vec::new();
        let mut prev = NONE;
        let mut cur = start as u32;
        loop {
            seen[cur as usize] = true;
            ring.push(points[cur as usize]);
            let [n0, n1] = links[cur as usize];
            let next = if n0 != prev && n0 != NONE { n0 } else { n1 };
            if next == NONE || seen[next as usize] {
                break;
            }
            prev = cur;
            cur = next;
        }
        if ring.len() >= 3 {
            // Smooth (same point count), then drop the near-collinear
            // points the smoothing leaves along gentle curves.
            let ring = simplify(&smooth(&ring), cell * 0.1);
            if ring.len() >= 3 {
                loops.push(ring);
            }
        }
    }
    loops
}

/// Drops points that lie (within `tolerance`) on the line between their
/// kept neighbours — a fine grid traces smooth curves with long runs of
/// nearly collinear points, which only cost tessellation time.
fn simplify(ring: &[Vec2], tolerance: f32) -> Vec<Vec2> {
    let n = ring.len();
    if n < 8 {
        return ring.to_vec();
    }
    let tol2 = tolerance * tolerance;
    let mut out: Vec<Vec2> = Vec::with_capacity(n);
    out.push(ring[0]);
    for k in 1..n {
        let p = ring[k];
        let next = ring[(k + 1) % n];
        let prev = *out.last().unwrap();
        let ab = next - prev;
        let len2 = ab.length_squared();
        let off2 = if len2 > 1e-12 {
            let t = ((p - prev).dot(ab) / len2).clamp(0.0, 1.0);
            p.distance_squared(prev + ab * t)
        } else {
            p.distance_squared(prev)
        };
        if off2 > tol2 {
            out.push(p);
        }
    }
    out
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

    /// Signed distance from `p` to the outline (negative inside) — the
    /// reference the banded field sampling is tested against.
    #[allow(dead_code)]
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

/// A sampled field on a world-locked grid — build it once per liquid per
/// frame, then trace any number of outlines from it ([`Field::contour`]):
/// the rim, the body and the highlight all come from one field.
pub struct Field {
    data: Vec<f32>,
    w: usize,
    h: usize,
    origin: Vec2,
    cell: f32,
    /// For goo fields: the kernel's radius and outline offset (see
    /// [`Field::from_sources`]), so an inset distance maps to a level.
    big_r: f32,
    r0: f32,
}

impl Field {
    /// Grid size in samples, the first sample's world position, and the
    /// spacing — for uploading the field as a texture (`goo_gpu`).
    pub fn layout(&self) -> (usize, usize, Vec2, f32) {
        (self.w, self.h, self.origin, self.cell)
    }

    /// The samples, row-major.
    pub fn samples(&self) -> &[f32] {
        &self.data
    }

    /// Splats `blobs` into a field with *this* field's grid (anything off
    /// the grid is dropped) — a second field aligned sample-for-sample
    /// (the particle fluid's highlight, packed into the same texture).
    pub fn blobs_on_grid(&self, blobs: &[Blob]) -> Vec<f32> {
        let (w, h, origin, cell) = (self.w, self.h, self.origin, self.cell);
        let mut data = vec![0.0f32; w * h];
        if w == 0 || h == 0 {
            return data;
        }
        for b in blobs.iter().filter(|b| b.radius > 0.0 && b.weight > 0.0) {
            let r2 = b.radius * b.radius;
            let lo = ((b.center - Vec2::splat(b.radius) - origin) / cell).floor().max(Vec2::ZERO);
            let hi = ((b.center + Vec2::splat(b.radius) - origin) / cell).ceil().min(Vec2::new((w - 1) as f32, (h - 1) as f32));
            if lo.x > hi.x || lo.y > hi.y {
                continue;
            }
            for j in lo.y as usize..=hi.y as usize {
                let y = origin.y + j as f32 * cell - b.center.y;
                for i in lo.x as usize..=hi.x as usize {
                    let x = origin.x + i as f32 * cell - b.center.x;
                    let q = 1.0 - (x * x + y * y) / r2;
                    if q > 0.0 {
                        data[j * w + i] += b.weight * q * q * q;
                    }
                }
            }
        }
        data
    }

    /// The outline where the field passes `level`.
    pub fn contour(&self, level: f32) -> Vec<Vec<Vec2>> {
        if self.w < 2 || self.h < 2 {
            return Vec::new();
        }
        march(&self.data, self.w, self.h, self.origin, self.cell, level)
    }

    /// The field level of an outline `inset` world units inside a goo
    /// liquid's edge (0 = the edge itself).
    pub fn level_for_inset(&self, inset: f32) -> f32 {
        goo_kernel(-inset.clamp(0.0, self.r0 * 0.98), self.big_r, self.r0)
    }

    /// The summed metaball field of `blobs`, sampled every `cell`.
    pub fn from_blobs(blobs: &[Blob], cell: f32) -> Field {
        let blobs: Vec<&Blob> = blobs.iter().filter(|b| b.radius > 0.0 && b.weight > 0.0).collect();
        if blobs.is_empty() {
            return Field { data: Vec::new(), w: 0, h: 0, origin: Vec2::ZERO, cell, big_r: 1.0, r0: 0.5 };
        }
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for b in &blobs {
            min = min.min(b.center - Vec2::splat(b.radius));
            max = max.max(b.center + Vec2::splat(b.radius));
        }
        let (cell, origin, w, h) = world_grid(min, max, cell);
        let mut data = vec![0.0f32; w * h];
        for b in &blobs {
            let r2 = b.radius * b.radius;
            let lo = ((b.center - Vec2::splat(b.radius) - origin) / cell).floor().max(Vec2::ZERO);
            let hi = ((b.center + Vec2::splat(b.radius) - origin) / cell).ceil().min(Vec2::new((w - 1) as f32, (h - 1) as f32));
            for j in lo.y as usize..=hi.y as usize {
                let y = origin.y + j as f32 * cell - b.center.y;
                let row = j * w;
                for i in lo.x as usize..=hi.x as usize {
                    let x = origin.x + i as f32 * cell - b.center.x;
                    let q = 1.0 - (x * x + y * y) / r2;
                    if q > 0.0 {
                        data[row + i] += b.weight * q * q * q;
                    }
                }
            }
        }
        Field { data, w, h, origin, cell, big_r: 1.0, r0: 0.5 }
    }

    /// The goo field of `sources` (see [`contour_sources`]).
    ///
    /// Polygons are distance-sampled only in the band within `reach` of
    /// their edges (each edge visits just the cells near it), and filled
    /// inside by an even-odd scanline per grid row — instead of every cell
    /// measuring every edge, which made long text crawl.
    #[allow(dead_code)] // tests / tools
    pub fn from_sources(sources: &[Source], reach: f32, detail: f32) -> Field {
        Self::from_sources_cached(sources, reach, detail, None)
    }

    /// [`Field::from_sources`], reusing cached polygon patches (see
    /// [`SourceCache`]).
    pub fn from_sources_cached(sources: &[Source], reach: f32, detail: f32, mut cache: Option<&mut SourceCache>) -> Field {
        // Negative weights (Anti Goo) carve: they only lower the field, so
        // the grid spans the positive members alone, and carvers out of
        // their reach are skipped.
        let reach = reach.max(0.5);
        let share = visible_share(GOO_THRESHOLD);
        let big_r = reach / (1.0 - share);
        let r0 = big_r * share;
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for s in sources.iter().filter(|s| s.weight() > 0.0) {
            let (a, b) = s.bounds();
            min = min.min(a);
            max = max.max(b);
        }
        if min.x > max.x {
            return Field { data: Vec::new(), w: 0, h: 0, origin: Vec2::ZERO, cell: 1.0, big_r, r0 };
        }
        min -= Vec2::splat(reach);
        max += Vec2::splat(reach);
        let sources: Vec<&Source> = sources
            .iter()
            .filter(|s| {
                let w = s.weight();
                if w > 0.0 {
                    return true;
                }
                let (a, b) = s.bounds();
                w < 0.0 && a.x - reach <= max.x && b.x + reach >= min.x && a.y - reach <= max.y && b.y + reach >= min.y
            })
            .collect();
        let (cell, origin, w, h) = world_grid(min, max, (reach * detail.clamp(0.05, 1.0)).max(0.25));
        let mut data = vec![0.0f32; w * h];
        let cell_of = |p: Vec2| (p - origin) / cell;
        let clamp_i = |v: f32, n: usize| (v.max(0.0) as usize).min(n - 1);
        for s in &sources {
            let weight = s.weight();
            match s {
                Source::Circle { center, radius, .. } => {
                    let lo = cell_of(*center - Vec2::splat(radius + reach)).floor();
                    let hi = cell_of(*center + Vec2::splat(radius + reach)).ceil();
                    for j in clamp_i(lo.y, h)..=clamp_i(hi.y, h) {
                        for i in clamp_i(lo.x, w)..=clamp_i(hi.x, w) {
                            let p = origin + Vec2::new(i as f32, j as f32) * cell;
                            let k = goo_kernel(p.distance(*center) - radius, big_r, r0);
                            if k > 0.0 {
                                data[j * w + i] += weight * k;
                            }
                        }
                    }
                }
                Source::Polygons { loops, .. } => {
                    let world_i = (origin.x / cell).round() as i64;
                    let world_j = (origin.y / cell).round() as i64;
                    let key = cache.as_ref().map(|_| fingerprint(std::slice::from_ref(*s), (cell.to_bits() as u64) << 32 | reach.to_bits() as u64));
                    if let (Some(c), Some(key)) = (cache.as_deref_mut(), key) {
                        if let Some(win) = c.windows.get(&key) {
                            c.used.insert(key);
                            // Add the cached patch at its world position.
                            for wj in 0..win.h {
                                let j = win.j0 + wj as i64 - world_j;
                                if j < 0 || j >= h as i64 {
                                    continue;
                                }
                                for wi in 0..win.w {
                                    let i = win.i0 + wi as i64 - world_i;
                                    if i < 0 || i >= w as i64 {
                                        continue;
                                    }
                                    data[j as usize * w + i as usize] += win.data[wj * win.w + wi];
                                }
                            }
                            continue;
                        }
                    }
                    // This source's own window of the grid.
                    let (a, b) = s.bounds();
                    let lo = cell_of(a - Vec2::splat(reach)).floor();
                    let hi = cell_of(b + Vec2::splat(reach)).ceil();
                    let (i0, j0) = (clamp_i(lo.x, w), clamp_i(lo.y, h));
                    let (i1, j1) = (clamp_i(hi.x, w), clamp_i(hi.y, h));
                    let (rw, rh) = (i1 - i0 + 1, j1 - j0 + 1);
                    // Unsigned distance, only near edges (∞ elsewhere).
                    let mut dist = vec![f32::INFINITY; rw * rh];
                    let mut patch = vec![0.0f32; rw * rh];
                    // x crossings per row, for the even-odd inside fill.
                    let mut crossings: Vec<Vec<f32>> = vec![Vec::new(); rh];
                    for ring in loops {
                        let n = ring.len();
                        for k in 0..n {
                            let (pa, pb) = (ring[k], ring[(k + 1) % n]);
                            let ab = pb - pa;
                            let len2 = ab.length_squared().max(1e-12);
                            let elo = cell_of(pa.min(pb) - Vec2::splat(reach)).floor();
                            let ehi = cell_of(pa.max(pb) + Vec2::splat(reach)).ceil();
                            for j in clamp_i(elo.y, h).max(j0)..=clamp_i(ehi.y, h).min(j1) {
                                for i in clamp_i(elo.x, w).max(i0)..=clamp_i(ehi.x, w).min(i1) {
                                    let p = origin + Vec2::new(i as f32, j as f32) * cell;
                                    let t = ((p - pa).dot(ab) / len2).clamp(0.0, 1.0);
                                    let d = p.distance_squared(pa + ab * t);
                                    let slot = &mut dist[(j - j0) * rw + (i - i0)];
                                    if d < *slot {
                                        *slot = d;
                                    }
                                }
                            }
                            // Rows whose sample line this edge crosses.
                            let (ya, yb) = (pa.y, pb.y);
                            if (ya - yb).abs() > 1e-9 {
                                let jlo = (((ya.min(yb) - origin.y) / cell).ceil().max(j0 as f32)) as usize;
                                let jhi = (((ya.max(yb) - origin.y) / cell).floor()).min(j1 as f32);
                                if jhi >= jlo as f32 {
                                    for j in jlo..=jhi as usize {
                                        let y = origin.y + j as f32 * cell;
                                        if (ya > y) != (yb > y) {
                                            crossings[j - j0].push(pa.x + (y - ya) / (yb - ya) * (pb.x - pa.x));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    for (rj, xs) in crossings.iter_mut().enumerate() {
                        xs.sort_by(|a, b| a.total_cmp(b));
                        let j = j0 + rj;
                        let y_row = j * w;
                        let mut c = 0;
                        for ri in 0..rw {
                            let i = i0 + ri;
                            let x = origin.x + i as f32 * cell;
                            while c < xs.len() && xs[c] < x {
                                c += 1;
                            }
                            let inside = c % 2 == 1;
                            let d2 = dist[rj * rw + ri];
                            let k = if d2.is_finite() {
                                let d = d2.sqrt();
                                goo_kernel(if inside { -d } else { d }, big_r, r0)
                            } else if inside {
                                1.0
                            } else {
                                0.0
                            };
                            patch[rj * rw + ri] = weight * k;
                            let _ = y_row;
                        }
                    }
                    for rj in 0..rh {
                        let row = (j0 + rj) * w + i0;
                        for ri in 0..rw {
                            data[row + ri] += patch[rj * rw + ri];
                        }
                    }
                    if let (Some(c), Some(key)) = (cache.as_deref_mut(), key) {
                        c.used.insert(key);
                        c.windows.insert(key, Window { i0: world_i + i0 as i64, j0: world_j + j0 as i64, w: rw, h: rh, data: patch });
                    }
                }
            }
        }
        Field { data, w, h, origin, cell, big_r, r0 }
    }
}

/// Cached field patches of polygon sources (text, shapes), keyed by the
/// source's fingerprint and the grid's cell / reach. The grid is locked to
/// the world, so a member that hasn't changed contributes exactly the same
/// values at the same world cells every frame: its patch is computed once
/// and just added in afterwards — moving balls round a still word no
/// longer re-measures the word. Call [`SourceCache::end_frame`] once a
/// frame to drop patches nobody used.
#[derive(Default)]
pub struct SourceCache {
    windows: std::collections::HashMap<u64, Window>,
    used: std::collections::HashSet<u64>,
}

struct Window {
    /// The patch's first cell, in whole world cells.
    i0: i64,
    j0: i64,
    w: usize,
    h: usize,
    data: Vec<f32>,
}

impl SourceCache {
    pub fn end_frame(&mut self) {
        let used = std::mem::take(&mut self.used);
        self.windows.retain(|k, _| used.contains(k));
    }
}

/// The goo field's level that counts as "liquid".
const GOO_THRESHOLD: f32 = 0.5;

/// A goo source's field at signed distance `d` from its outline: 1 well
/// inside, exactly the threshold on the outline, 0 from `reach` outside.
fn goo_kernel(d: f32, big_r: f32, r0: f32) -> f32 {
    let x = ((d + r0).max(0.0) / big_r).min(1.0);
    let q = 1.0 - x * x;
    q * q * q
}

/// The gooey outline of `sources` merged, `inset` world units inside the
/// liquid's edge (0 = the edge). Each source's field is 1 inside it and
/// falls smoothly to 0 at `reach` outside its outline, shaped so a lone
/// source's outline is exactly its own; where sources come within reach of
/// each other their fields add up and a smooth neck joins them. (Several
/// outlines from one group: build a [`Field`] once and trace each.)
#[allow(dead_code)] // tests / tools
pub fn contour_sources(sources: &[Source], reach: f32, inset: f32, detail: f32) -> Vec<Vec<Vec2>> {
    let field = Field::from_sources(sources, reach, detail);
    field.contour(field.level_for_inset(inset))
}

/// A cheap fingerprint of `sources` (every coordinate's bits): when a
/// liquid's fingerprint (with its settings) matches last frame's, its
/// meshes are reused instead of re-contoured and re-uploaded.
pub fn fingerprint(sources: &[Source], extra: u64) -> u64 {
    // FNV-1a over the raw bits.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ extra;
    let mut mix = |v: u32| {
        h ^= v as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    for s in sources {
        match s {
            Source::Circle { center, radius, weight } => {
                mix(1);
                for v in [center.x, center.y, *radius, *weight] {
                    mix(v.to_bits());
                }
            }
            Source::Polygons { loops, weight } => {
                mix(2);
                mix(weight.to_bits());
                for ring in loops {
                    mix(ring.len() as u32);
                    for p in ring {
                        mix(p.x.to_bits());
                        mix(p.y.to_bits());
                    }
                }
            }
        }
    }
    h
}

/// [`fingerprint`] for plain blobs.
pub fn fingerprint_blobs(blobs: &[Blob], extra: u64) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ extra;
    for b in blobs {
        for v in [b.center.x, b.center.y, b.radius, b.weight] {
            h ^= v.to_bits() as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
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
#[allow(dead_code)]
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

/// One pass of neighbour averaging on a closed polygon (¼·prev + ½·self +
/// ¼·next): rounds off the grid's facets like corner cutting does, without
/// doubling the point count (tessellation time scales with points).
fn smooth(poly: &[Vec2]) -> Vec<Vec2> {
    let n = poly.len();
    (0..n).map(|k| poly[(k + n - 1) % n] * 0.25 + poly[k] * 0.5 + poly[(k + 1) % n] * 0.25).collect()
}

/// Polygons as a path (for tessellation), optionally shifted.
#[allow(dead_code)] // tests / tools
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
    fn banded_polygon_field_matches_brute_force() {
        // A glyph-like ring with a hole: the fast (edge-band + scanline)
        // field must equal measuring every edge from every cell.
        let outer: Vec<Vec2> = (0..40).map(|k| { let a = k as f32 / 40.0 * std::f32::consts::TAU; Vec2::new(a.cos() * 50.0 + 3.3, a.sin() * 30.0 - 1.7) }).collect();
        let inner: Vec<Vec2> = (0..20).map(|k| { let a = -(k as f32) / 20.0 * std::f32::consts::TAU; Vec2::new(a.cos() * 20.0, a.sin() * 12.0) }).collect();
        let src = Source::Polygons { loops: vec![outer, inner], weight: 1.0 };
        let reach = 15.0;
        let field = Field::from_sources(std::slice::from_ref(&src), reach, 0.2);
        let mut worst = 0.0f32;
        for j in 0..field.h {
            for i in 0..field.w {
                let p = field.origin + Vec2::new(i as f32, j as f32) * field.cell;
                let brute = goo_kernel(src.distance(p), field.big_r, field.r0);
                worst = worst.max((brute - field.data[j * field.w + i]).abs());
            }
        }
        assert!(worst < 1e-4, "max difference {worst}");
    }

    #[test]
    fn cached_patches_give_the_same_field() {
        let word = Source::Polygons { loops: vec![vec![Vec2::new(0.0, 0.0), Vec2::new(80.0, 0.0), Vec2::new(80.0, 30.0), Vec2::new(0.0, 30.0)]], weight: 1.0 };
        let mut cache = SourceCache::default();
        for x in [120.0, 131.5, 97.25] {
            // The word stays put, a ball moves (so the grid's extent moves).
            let sources = [word.clone(), Source::Circle { center: Vec2::new(x, 15.0), radius: 12.0, weight: 1.0 }];
            let cached = Field::from_sources_cached(&sources, 20.0, 0.2, Some(&mut cache));
            let fresh = Field::from_sources(&sources, 20.0, 0.2);
            assert_eq!((cached.w, cached.h, cached.origin), (fresh.w, fresh.h, fresh.origin));
            let worst = cached.data.iter().zip(&fresh.data).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
            assert!(worst < 1e-6, "{worst}");
            cache.end_frame();
        }
        assert_eq!(cache.windows.len(), 1, "the word's patch is kept and reused");
    }

    #[test]
    fn far_blobs_stay_apart_close_blobs_merge() {
        let blob = |x: f32| Blob { center: Vec2::new(x, 0.0), radius: 10.0, weight: 1.0 };
        assert_eq!(contour(&[blob(0.0), blob(30.0)], 0.5, 1.0).len(), 2, "apart");
        // Visible radii ≈ 4.5 each; at 11 apart the fields join into one body.
        assert_eq!(contour(&[blob(0.0), blob(11.0)], 0.5, 1.0).len(), 1, "merged");
    }
}

/// The liquid (`field` at or above `level`) eroded by `depth` world units,
/// as a value per sample on `field`'s grid: 1 at least `depth` inside the
/// edge, falling smoothly to 0 at the edge (the inside mask blurred twice,
/// so corners round off). Goo Gloss places its shine on this: unlike the
/// goo field itself, which is flat once `reach` inside, it keeps telling
/// depths apart however big the liquid is.
pub fn erode(field: &Field, level: f32, depth: f32) -> Vec<f32> {
    let (w, h, _, cell) = field.layout();
    let mut a: Vec<f32> = field.samples().iter().map(|v| if *v >= level { 1.0 } else { 0.0 }).collect();
    if w == 0 || h == 0 {
        return a;
    }
    // Two box blurs (a tent) of half the depth each: at a straight edge
    // the result reaches 1 exactly `depth` inside.
    let r = ((depth / cell) * 0.5).round().max(1.0) as usize;
    let mut b = vec![0.0f32; w * h];
    for _ in 0..2 {
        box_blur(&a, &mut b, w, h, r, true);
        box_blur(&b, &mut a, w, h, r, false);
    }
    a
}

/// One box-blur pass along rows (`horizontal`) or columns, radius `r`
/// samples; outside the grid counts as empty (0).
fn box_blur(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize, horizontal: bool) {
    let (len, lines) = if horizontal { (w, h) } else { (h, w) };
    let at = |line: usize, k: usize| if horizontal { line * w + k } else { k * w + line };
    let norm = 1.0 / (2 * r + 1) as f32;
    let mut prefix = vec![0.0f32; len + 1];
    for line in 0..lines {
        for k in 0..len {
            prefix[k + 1] = prefix[k] + src[at(line, k)];
        }
        for k in 0..len {
            let lo = k.saturating_sub(r);
            let hi = (k + r + 1).min(len);
            dst[at(line, k)] = (prefix[hi] - prefix[lo]) * norm;
        }
    }
}
