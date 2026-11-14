//! Force-directed layout: turning the similarity graph into positions.
//!
//! Stars repel each other and edges pull them together. Run that to a settled
//! state and artists who are listened to alongside each other end up near each
//! other — which is the whole claim the map makes.
//!
//! **Why this is written here rather than taken from a crate.** Every ready
//! option failed for a checked reason: the `forceatlas2` crate is AGPL-3.0 and
//! this product is MIT; `annembed` (MIT, and the only Rust library with a
//! verified million-scale run) accepts a graph only through an HNSW index
//! built from vectors, and its `KGraph` fields are `pub(crate)`, so a graph
//! that already exists cannot be handed to it; Rust has no maintained sparse
//! eigensolver for a spectral embedding; `sfdp` has no Rust implementation.
//! Recorded in ADR 0008.
//!
//! **Why it is affordable.** Repulsion goes through a Barnes-Hut quadtree, so
//! the cost per iteration is N log N rather than N². Memory was measured
//! rather than hoped: the graph in CSR form is about 250 MB at three million
//! artists, the tree about as much again.
//!
//! **Determinism.** A layout is versioned, so "the same seed" has to mean the
//! same sky. Floating-point addition is not associative, so parallel force
//! accumulation gives different answers on different thread counts even with
//! a fixed seed — a documented trap in every library of this kind. Forces here
//! are accumulated in index order.

use std::collections::HashMap;

use super::quadtree::Quadtree;

/// The similarity graph, in the compact form the layout iterates over.
///
/// Compressed sparse row: `offsets[i]..offsets[i + 1]` indexes the neighbours
/// of star `i`. Both directions are stored, because a star is pulled by every
/// edge it takes part in and the input holds each pair once.
pub struct Graph {
    /// Artist id for each dense index, so positions can be written back.
    pub artist_ids: Vec<i32>,
    pub offsets: Vec<u32>,
    pub targets: Vec<u32>,
    pub weights: Vec<f32>,
    /// Repulsion mass per star: how much it pushes. Taken from the star's
    /// weight in the graph so hubs claim room rather than being buried.
    pub masses: Vec<f32>,
}

impl Graph {
    /// Builds the graph from edges keyed by artist id.
    ///
    /// Artists that appear in no edge are not included: they have no
    /// similarity, so a layout has nothing to say about where they belong.
    /// They are the map's dark matter and are placed separately.
    #[must_use]
    pub fn from_edges(edges: &[(i32, i32, f32)]) -> Self {
        // Dense indices in first-seen order. Sorted afterwards so the result
        // does not depend on hash iteration order, which would make a seeded
        // run irreproducible.
        let mut ids: Vec<i32> = Vec::with_capacity(edges.len());
        for &(a, b, _) in edges {
            ids.push(a);
            ids.push(b);
        }
        ids.sort_unstable();
        ids.dedup();
        let index: HashMap<i32, u32> = ids.iter().enumerate().map(|(i, &id)| (id, u32::try_from(i).unwrap_or(u32::MAX))).collect();

        let n = ids.len();
        let mut degree = vec![0u32; n + 1];
        for &(a, b, _) in edges {
            degree[index[&a] as usize + 1] += 1;
            degree[index[&b] as usize + 1] += 1;
        }
        for i in 0..n {
            degree[i + 1] += degree[i];
        }

        let offsets = degree;
        let mut cursor = offsets.clone();
        let mut targets = vec![0u32; offsets[n] as usize];
        let mut weights = vec![0f32; offsets[n] as usize];
        let mut masses = vec![0f32; n];

        for &(a, b, weight) in edges {
            let (ia, ib) = (index[&a], index[&b]);
            for (from, to) in [(ia, ib), (ib, ia)] {
                let at = cursor[from as usize] as usize;
                targets[at] = to;
                weights[at] = weight;
                cursor[from as usize] += 1;
            }
            masses[ia as usize] += weight;
            masses[ib as usize] += weight;
        }

        Self {
            artist_ids: ids,
            offsets,
            targets,
            weights,
            masses,
        }
    }

    /// How many stars the layout will place.
    #[must_use]
    pub fn len(&self) -> usize {
        self.artist_ids.len()
    }

    /// Whether the layout has anything to place. Kept beside `len` because
    /// clippy requires the pair, and used by the tests.
    #[must_use]
    #[cfg_attr(not(test), expect(dead_code, reason = "the pair of len(); only tests need it so far"))]
    pub fn is_empty(&self) -> bool {
        self.artist_ids.is_empty()
    }
}

/// What the layout does, in numbers a run can be reproduced from.
#[derive(Clone, Copy)]
pub struct Params {
    /// How many rounds of forces to apply. Fixed rather than "until it
    /// settles": a convergence test makes the result depend on floating-point
    /// noise, and a layout version must mean one sky.
    pub iterations: u32,
    /// Barnes-Hut opening angle. 0.5 is the usual trade between speed and
    /// accuracy; smaller is more exact and slower.
    pub theta: f32,
    /// How hard stars push each other apart.
    pub repulsion: f32,
    /// How hard an edge pulls, per unit of similarity.
    pub attraction: f32,
    /// Fraction of its velocity a star keeps between iterations. Below one so
    /// the layout settles instead of ringing.
    pub damping: f32,
    /// Random seed, part of what makes a layout reproducible.
    pub seed: u64,
    /// How far the first round may move a star, as a share of what a fresh
    /// layout allows: 1 for a sky drawn from the spiral, which has its whole
    /// shape to find, and a tenth -- where a fresh run ends -- for one that
    /// starts from a settled sky and only has to make room for what is new.
    pub heat: f32,
    /// How hard a star with a place in the edition before is held to it, per
    /// unit of distance. Zero for a fresh layout. Without it a settled sky
    /// still drifts: a fresh run stops when its rounds run out, not when the
    /// forces balance, so every star still has somewhere it would rather be.
    pub anchor: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            iterations: 300,
            theta: 0.5,
            repulsion: 1.0,
            attraction: 1.0,
            damping: 0.9,
            seed: 0x5EED,
            heat: 1.0,
            anchor: 0.0,
        }
    }
}

impl Params {
    /// The forces of a layout that continues the edition before: few rounds,
    /// started cool, every star with a place held near it. Measured on edition
    /// 3 of the canon (ADR 0029).
    #[must_use]
    pub fn continued(iterations: u32, seed: u64) -> Self {
        Self {
            iterations,
            seed,
            heat: CONTINUED_HEAT,
            anchor: CONTINUED_ANCHOR,
            ..Self::default()
        }
    }
}

/// How many rounds a continued layout runs by default: by the fortieth, the
/// movement of a round on edition 3 had fallen to a fifth of the tenth's.
pub const CONTINUED_ITERATIONS: u32 = 40;

/// The heat a continued layout starts at: where a fresh run of 300 ends.
const CONTINUED_HEAT: f32 = 0.1;

/// The anchor of a continued layout, per unit of distance.
///
/// Measured on edition 3, with one star in a hundred hidden so it had to be
/// placed as new: at 1 the held stars drifted 7.5 units on the median and
/// 14.8 at the ninetieth percentile -- the whole reach of the fog's light --
/// and at 10 they moved 0.8 and 1.7, while the newcomers sat as close to the
/// stars they are similar to as the whole layout had put them (a weighted
/// distance of 297 on the median against 332).
const CONTINUED_ANCHOR: f32 = 10.0;

/// Where a run starts: a place for every star, and which of them are held.
pub struct Start {
    pub xs: Vec<f32>,
    pub ys: Vec<f32>,
    /// Whether each star had a place before. Those are held near it by the
    /// anchor; the rest are new and go where their neighbours pull them.
    pub known: Vec<bool>,
    /// New stars placed among their neighbours, and new stars with no
    /// neighbour that had a place, placed on a ring around the sky.
    pub by_neighbours: usize,
    pub by_ring: usize,
}

impl Start {
    /// The deterministic spiral a fresh layout starts from: nothing is known,
    /// nothing is held.
    #[must_use]
    pub fn spiral(n: usize, seed: u64) -> Self {
        let (xs, ys) = initial_positions(n, seed);
        Self {
            xs,
            ys,
            known: vec![false; n],
            by_neighbours: 0,
            by_ring: 0,
        }
    }

    /// Starts from the places stars had before, keyed by artist id.
    ///
    /// A star that had a place keeps it. A new one goes to the middle of its
    /// neighbours that have a place, weighted by similarity -- in rounds, so a
    /// new star whose neighbours are all new waits for them and lands beside
    /// them -- which is close to where a whole layout would put it, and lets a
    /// handful of rounds finish the job instead of three hundred. A star with
    /// no path to any placed one -- a new island of the graph -- goes on a
    /// ring just outside the sky, where it can come in without landing on
    /// anybody.
    #[must_use]
    pub fn from_previous(graph: &Graph, previous: &HashMap<i32, (f32, f32)>) -> Self {
        let n = graph.len();
        let mut xs = vec![0f32; n];
        let mut ys = vec![0f32; n];
        let mut placed = vec![false; n];
        for (i, artist) in graph.artist_ids.iter().enumerate() {
            if let Some(&(x, y)) = previous.get(artist) {
                xs[i] = x;
                ys[i] = y;
                placed[i] = true;
            }
        }
        let known = placed.clone();

        let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
        let mut by_neighbours = 0;
        loop {
            // A round reads only what earlier rounds placed, so its answer
            // does not depend on the order stars are visited in.
            let mut round = Vec::new();
            for i in (0..n).filter(|&i| !placed[i]) {
                let (mut sx, mut sy, mut weight) = (0f64, 0f64, 0f64);
                for edge in graph.offsets[i] as usize..graph.offsets[i + 1] as usize {
                    let j = graph.targets[edge] as usize;
                    if placed[j] {
                        let w = f64::from(graph.weights[edge]);
                        sx += w * f64::from(xs[j]);
                        sy += w * f64::from(ys[j]);
                        weight += w;
                    }
                }
                if weight > 0.0 {
                    round.push((i, sx / weight, sy / weight));
                }
            }
            if round.is_empty() {
                break;
            }
            for (i, x, y) in round {
                // A step off the middle, so two newcomers drawn to the same
                // neighbour do not start on top of each other: the golden
                // angle by index spreads them as evenly as the spiral does.
                #[expect(clippy::cast_precision_loss, reason = "an index becomes an angle; f32 is the map's precision")]
                let angle = i as f32 * golden;
                #[expect(clippy::cast_possible_truncation, reason = "a weighted mean of f32 coordinates")]
                let (x, y) = (x as f32, y as f32);
                xs[i] = x + NEWCOMER_STEP * angle.cos();
                ys[i] = y + NEWCOMER_STEP * angle.sin();
                placed[i] = true;
                by_neighbours += 1;
            }
        }

        // The ring: around the middle of the stars that were placed, just
        // past the furthest of them.
        let held: Vec<usize> = (0..n).filter(|&i| known[i]).collect();
        let (cx, cy, reach) = if held.is_empty() {
            (0.0, 0.0, 0.0)
        } else {
            #[expect(clippy::cast_precision_loss, reason = "a count of stars, far below f32's exact range")]
            let count = held.len() as f32;
            let cx = held.iter().map(|&i| xs[i]).sum::<f32>() / count;
            let cy = held.iter().map(|&i| ys[i]).sum::<f32>() / count;
            let reach = held.iter().map(|&i| (xs[i] - cx).hypot(ys[i] - cy)).fold(0f32, f32::max);
            (cx, cy, reach)
        };
        let mut by_ring = 0;
        for i in (0..n).filter(|&i| !placed[i]) {
            #[expect(clippy::cast_precision_loss, reason = "a count of stars becomes a radius and an angle")]
            let k = by_ring as f32;
            let radius = reach + RING_STEP * (k + 1.0).sqrt();
            let angle = k * golden;
            xs[i] = cx + radius * angle.cos();
            ys[i] = cy + radius * angle.sin();
            by_ring += 1;
        }

        Self {
            xs,
            ys,
            known,
            by_neighbours,
            by_ring,
        }
    }

    /// How many stars kept the place they had.
    #[must_use]
    pub fn held(&self) -> usize {
        self.known.iter().filter(|&&known| known).count()
    }
}

/// How far off the middle of its neighbours a new star starts, in layout
/// units: enough for the repulsion to tell two newcomers apart, and a small
/// part of the 14 units the fog lights around an open star.
const NEWCOMER_STEP: f32 = 1.0;

/// How far apart the ring places stars with no placed neighbour: about the
/// spacing of the spiral a fresh layout starts from.
const RING_STEP: f32 = 1.0;

/// Positions produced by a run.
pub struct Positions {
    pub xs: Vec<f32>,
    pub ys: Vec<f32>,
}

/// Runs the layout from the spiral: a sky drawn fresh.
///
/// `progress` is called once per iteration with the iteration number and the
/// total movement of that round, so a caller can report and so a run that is
/// going nowhere is visible rather than silent.
pub fn run(graph: &Graph, params: &Params, progress: impl FnMut(u32, f32)) -> Positions {
    run_from(graph, params, &Start::spiral(graph.len(), params.seed), progress)
}

/// Runs the layout from `start`, holding its known stars by the anchor.
pub fn run_from(graph: &Graph, params: &Params, start: &Start, mut progress: impl FnMut(u32, f32)) -> Positions {
    let n = graph.len();
    debug_assert_eq!(start.xs.len(), n, "a start is a place for every star of the graph");
    let (mut xs, mut ys) = (start.xs.clone(), start.ys.clone());
    let mut vxs = vec![0f32; n];
    let mut vys = vec![0f32; n];

    for iteration in 0..params.iterations {
        let tree = Quadtree::build(&xs, &ys, &graph.masses, params.theta);
        let mut movement = 0f32;

        // Global cooling: the layout takes big steps while it is finding the
        // shape and small ones while it settles into it. Without it a
        // force-directed run oscillates around its answer instead of
        // arriving, which is what "movement stopped falling" looks like from
        // outside.
        #[expect(clippy::cast_precision_loss, reason = "iteration counts are small")]
        let elapsed = iteration as f32 / params.iterations.max(1) as f32;
        let cooling = params.heat * (1.0 - elapsed * 0.9);

        // Accumulated in index order: parallel accumulation would make the
        // result depend on thread scheduling, and a versioned layout cannot
        // afford that.
        for i in 0..n {
            let (mut fx, mut fy) = (0f32, 0f32);

            // Repulsion, from the tree.
            tree.repulsion(xs[i], ys[i], |dx, dy, mass| {
                let distance_squared = (dx * dx + dy * dy).max(SOFTENING);
                let strength = params.repulsion * mass / distance_squared;
                fx -= dx * strength;
                fy -= dy * strength;
            });

            // Attraction, along the star's own edges.
            let from = graph.offsets[i] as usize;
            let to = graph.offsets[i + 1] as usize;
            for edge in from..to {
                let j = graph.targets[edge] as usize;
                let weight = graph.weights[edge];
                let dx = xs[j] - xs[i];
                let dy = ys[j] - ys[i];
                fx += dx * weight * params.attraction;
                fy += dy * weight * params.attraction;
            }

            // Divided by the star's degree, as ForceAtlas2 does. Without
            // this a hub with thousands of edges gathers a force hundreds of
            // times larger than an ordinary star's and pins itself against
            // the step cap forever: measured on the real graph, the top
            // degree is 5650 against a mean of 58, and the layout stopped
            // converging entirely -- movement fell only 1% per ten rounds
            // while the average star ran at 77% of the cap.
            //
            // Dividing makes a star's position depend on where its
            // neighbours are rather than on how many it has.
            // A degree is a small count -- the largest in the real graph is
            // 5650 -- so f32 represents it exactly.
            #[expect(clippy::cast_precision_loss, reason = "a vertex degree is far below f32's exact-integer range")]
            let degree = (to - from).max(1) as f32;
            fx /= degree;
            fy /= degree;

            // The anchor, after the division: it holds a hub and a leaf to
            // their old places alike, whatever their degree.
            if start.known[i] {
                fx += params.anchor * (start.xs[i] - xs[i]);
                fy += params.anchor * (start.ys[i] - ys[i]);
            }

            vxs[i] = (vxs[i] + fx) * params.damping * cooling;
            vys[i] = (vys[i] + fy) * params.damping * cooling;

            // A star cannot cross the whole map in one round: an unbounded
            // step turns a dense cluster into an explosion the layout never
            // recovers from.
            let speed = (vxs[i] * vxs[i] + vys[i] * vys[i]).sqrt();
            if speed > MAX_STEP {
                let scale = MAX_STEP / speed;
                vxs[i] *= scale;
                vys[i] *= scale;
            }
            movement += vxs[i].abs() + vys[i].abs();
        }

        for i in 0..n {
            xs[i] += vxs[i];
            ys[i] += vys[i];
        }

        progress(iteration + 1, movement);
    }

    Positions { xs, ys }
}

/// Distance below which the repulsion stops growing, so two stars in the same
/// place do not produce an infinite force.
const SOFTENING: f32 = 0.01;

/// The furthest a star may move in one iteration.
const MAX_STEP: f32 = 10.0;

/// Starting positions: a deterministic spiral rather than random scatter.
///
/// The spiral spreads stars evenly with no clumps for the first iterations to
/// undo, and being a closed form it needs no random number generator — one
/// less thing to keep reproducible across versions.
fn initial_positions(n: usize, seed: u64) -> (Vec<f32>, Vec<f32>) {
    let mut xs = Vec::with_capacity(n);
    let mut ys = Vec::with_capacity(n);

    // The seed rotates the whole spiral. It changes the sky without changing
    // its structure, which is what a seed should do here.
    #[expect(clippy::cast_precision_loss, reason = "the seed is reduced to a degree in 0..360 first")]
    let phase = (seed % 360) as f32 * std::f32::consts::PI / 180.0;
    // The golden angle spaces successive points as unevenly as possible, so
    // no spokes or rings appear in the starting arrangement.
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());

    for i in 0..n {
        // Precision beyond f32 is meaningless here: these are pixels on a
        // map, and the spiral only has to spread stars evenly.
        #[expect(clippy::cast_precision_loss, reason = "a star index becomes a coordinate; f32 is the map's precision")]
        let index = i as f32;
        let radius = index.sqrt();
        let angle = index * golden + phase;
        xs.push(radius * angle.cos());
        ys.push(radius * angle.sin());
    }
    (xs, ys)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(iterations: u32) -> Params {
        Params {
            iterations,
            ..Params::default()
        }
    }

    /// Distance between two stars in the finished layout.
    fn distance(positions: &Positions, i: usize, j: usize) -> f32 {
        let dx = positions.xs[i] - positions.xs[j];
        let dy = positions.ys[i] - positions.ys[j];
        (dx * dx + dy * dy).sqrt()
    }

    #[test]
    fn builds_a_graph_with_both_directions_of_every_edge() {
        // The input holds each pair once; a star is pulled by every edge it
        // takes part in, so the layout needs both.
        let graph = Graph::from_edges(&[(10, 20, 0.5)]);
        assert_eq!(graph.len(), 2);
        assert_eq!(graph.targets.len(), 2, "one edge should appear twice");
        assert_eq!(graph.artist_ids, vec![10, 20]);
    }

    #[test]
    fn indexes_artists_in_sorted_order_not_hash_order() {
        // Hash iteration order varies between runs, which would make a seeded
        // layout irreproducible.
        let a = Graph::from_edges(&[(30, 10, 1.0), (20, 30, 1.0)]);
        let b = Graph::from_edges(&[(20, 30, 1.0), (30, 10, 1.0)]);
        assert_eq!(a.artist_ids, vec![10, 20, 30]);
        assert_eq!(a.artist_ids, b.artist_ids);
    }

    #[test]
    fn mass_grows_with_the_edges_a_star_takes_part_in() {
        // A hub must push harder than an obscurity, or it ends up buried in
        // its own crowd.
        let graph = Graph::from_edges(&[(1, 2, 1.0), (1, 3, 1.0), (1, 4, 1.0), (2, 3, 0.1)]);
        let hub = graph.artist_ids.iter().position(|&id| id == 1).unwrap();
        let leaf = graph.artist_ids.iter().position(|&id| id == 4).unwrap();
        assert!(
            graph.masses[hub] > graph.masses[leaf] * 2.0,
            "hub {} vs leaf {}",
            graph.masses[hub],
            graph.masses[leaf]
        );
    }

    #[test]
    fn the_same_seed_gives_the_same_sky() {
        // This is what makes a layout version mean something.
        let graph = Graph::from_edges(&[(1, 2, 0.9), (2, 3, 0.8), (3, 4, 0.7), (4, 1, 0.6)]);
        let first = run(&graph, &params(50), |_, _| {});
        let second = run(&graph, &params(50), |_, _| {});
        assert_eq!(first.xs, second.xs);
        assert_eq!(first.ys, second.ys);
    }

    #[test]
    fn a_different_seed_gives_a_different_sky() {
        let graph = Graph::from_edges(&[(1, 2, 0.9), (2, 3, 0.8), (3, 4, 0.7)]);
        let a = run(&graph, &params(20), |_, _| {});
        let b = run(
            &graph,
            &Params {
                iterations: 20,
                seed: 12345,
                ..Params::default()
            },
            |_, _| {},
        );
        assert_ne!(a.xs, b.xs);
    }

    #[test]
    fn connected_stars_end_up_closer_than_unconnected_ones() {
        // The claim the whole map rests on. Two tight pairs, no edge between
        // them: within a pair must beat across pairs.
        let graph = Graph::from_edges(&[(1, 2, 1.0), (3, 4, 1.0)]);
        let positions = run(&graph, &params(200), |_, _| {});

        let idx = |id: i32| graph.artist_ids.iter().position(|&a| a == id).unwrap();
        let within_a = distance(&positions, idx(1), idx(2));
        let within_b = distance(&positions, idx(3), idx(4));
        let across = distance(&positions, idx(1), idx(3));

        assert!(within_a < across, "pair 1-2 ({within_a}) should be tighter than 1-3 ({across})");
        assert!(within_b < across, "pair 3-4 ({within_b}) should be tighter than 1-3 ({across})");
    }

    #[test]
    fn two_clusters_separate() {
        // The realistic shape: two genres, densely connected inside, joined by
        // one weak edge. They should form visibly distinct regions.
        let mut edges = Vec::new();
        for i in 0..6 {
            for j in (i + 1)..6 {
                edges.push((i, j, 1.0));
                edges.push((100 + i, 100 + j, 1.0));
            }
        }
        edges.push((0, 100, 0.05));

        let graph = Graph::from_edges(&edges);
        let positions = run(&graph, &params(300), |_, _| {});
        let idx = |id: i32| graph.artist_ids.iter().position(|&a| a == id).unwrap();

        let inside: f32 = (0..6)
            .flat_map(|i| ((i + 1)..6).map(move |j| (i, j)))
            .map(|(i, j)| distance(&positions, idx(i), idx(j)))
            .sum::<f32>()
            / 15.0;
        let between: f32 = (0..6).map(|i| distance(&positions, idx(i), idx(100 + i))).sum::<f32>() / 6.0;

        assert!(between > inside * 1.5, "clusters did not separate: inside {inside}, between {between}");
    }

    #[test]
    fn stronger_similarity_pulls_harder() {
        let graph = Graph::from_edges(&[(1, 2, 1.0), (1, 3, 0.05)]);
        let positions = run(&graph, &params(200), |_, _| {});
        let idx = |id: i32| graph.artist_ids.iter().position(|&a| a == id).unwrap();

        let strong = distance(&positions, idx(1), idx(2));
        let weak = distance(&positions, idx(1), idx(3));
        assert!(strong < weak, "strong edge {strong} should be shorter than weak {weak}");
    }

    #[test]
    fn the_layout_settles_rather_than_growing_without_bound() {
        // Movement in the last rounds must be a fraction of the first, or the
        // sky is still flying apart when the run stops.
        let mut edges = Vec::new();
        for i in 0..20 {
            edges.push((i, (i + 1) % 20, 1.0));
        }
        let graph = Graph::from_edges(&edges);

        let mut movements = Vec::new();
        run(&graph, &params(200), |_, movement| movements.push(movement));

        let early: f32 = movements[..10].iter().sum::<f32>() / 10.0;
        let late: f32 = movements[movements.len() - 10..].iter().sum::<f32>() / 10.0;
        assert!(late < early, "movement grew: early {early}, late {late}");
    }

    #[test]
    fn positions_stay_finite() {
        // A force law that divides by zero produces NaN, and a NaN coordinate
        // spreads through the tree into every other star.
        let graph = Graph::from_edges(&[(1, 2, 1.0), (2, 3, 1.0), (1, 3, 1.0)]);
        let positions = run(&graph, &params(100), |_, _| {});
        assert!(positions.xs.iter().all(|v| v.is_finite()), "non-finite x");
        assert!(positions.ys.iter().all(|v| v.is_finite()), "non-finite y");
    }

    #[test]
    fn an_empty_graph_produces_an_empty_layout() {
        let graph = Graph::from_edges(&[]);
        assert!(graph.is_empty());
        let positions = run(&graph, &params(10), |_, _| {});
        assert_eq!(positions.xs, Vec::<f32>::new());
    }

    /// Two dense clusters of six joined by one weak edge: ids 0..6 and
    /// 100..106.
    fn two_clusters() -> Vec<(i32, i32, f32)> {
        let mut edges = Vec::new();
        for i in 0..6 {
            for j in (i + 1)..6 {
                edges.push((i, j, 1.0));
                edges.push((100 + i, 100 + j, 1.0));
            }
        }
        edges.push((0, 100, 0.05));
        edges
    }

    /// Every star's place in a finished layout, by artist id.
    fn places(graph: &Graph, positions: &Positions) -> HashMap<i32, (f32, f32)> {
        graph
            .artist_ids
            .iter()
            .enumerate()
            .map(|(i, &id)| (id, (positions.xs[i], positions.ys[i])))
            .collect()
    }

    fn at(graph: &Graph, start: &Start, id: i32) -> (f32, f32) {
        let i = graph.artist_ids.iter().position(|&a| a == id).unwrap();
        (start.xs[i], start.ys[i])
    }

    fn apart(a: (f32, f32), b: (f32, f32)) -> f32 {
        (a.0 - b.0).hypot(a.1 - b.1)
    }

    #[test]
    fn a_star_with_a_place_keeps_it_and_a_new_one_starts_among_its_neighbours() {
        let graph = Graph::from_edges(&two_clusters());
        let mut previous = places(&graph, &run(&graph, &params(300), |_, _| {}));
        let had = previous.remove(&3).unwrap();

        let start = Start::from_previous(&graph, &previous);
        assert_eq!(start.held(), 11);
        assert_eq!((start.by_neighbours, start.by_ring), (1, 0));
        assert_eq!(at(&graph, &start, 100), previous[&100], "a star with a place moved before the run began");

        // The newcomer starts in the middle of its own cluster, not the other.
        let landed = at(&graph, &start, 3);
        let own: f32 = (0..6).filter(|&i| i != 3).map(|i| apart(landed, previous[&i])).sum::<f32>() / 5.0;
        let other: f32 = (100..106).map(|i| apart(landed, previous[&i])).sum::<f32>() / 6.0;
        assert!(own * 3.0 < other, "the newcomer started {own} from its cluster and {other} from the other");
        assert!(
            apart(landed, had) < other,
            "it started further from where a whole layout put it than from the other cluster"
        );
    }

    #[test]
    fn a_newcomer_whose_neighbours_are_all_new_waits_for_them() {
        // 1 had a place; 2 is new beside it; 3 is new beside 2 only. 3 can only
        // be placed once 2 is, and must land beside 2 rather than at 0,0.
        let graph = Graph::from_edges(&[(1, 2, 1.0), (2, 3, 1.0)]);
        let previous = HashMap::from([(1, (500.0, -500.0))]);
        let start = Start::from_previous(&graph, &previous);
        assert_eq!((start.held(), start.by_neighbours, start.by_ring), (1, 2, 0));
        assert!(apart(at(&graph, &start, 3), (500.0, -500.0)) < 3.0, "{:?}", at(&graph, &start, 3));
    }

    #[test]
    fn an_island_with_no_placed_star_starts_on_a_ring_outside_the_sky() {
        // 1-2 had places ten apart; 7-8 is a new island of the graph.
        let graph = Graph::from_edges(&[(1, 2, 1.0), (7, 8, 1.0)]);
        let previous = HashMap::from([(1, (-5.0, 0.0)), (2, (5.0, 0.0))]);
        let start = Start::from_previous(&graph, &previous);
        assert_eq!((start.held(), start.by_neighbours, start.by_ring), (2, 0, 2));
        for id in [7, 8] {
            let distance = apart(at(&graph, &start, id), (0.0, 0.0));
            assert!(distance > 5.0, "the island star {id} started inside the sky, {distance} from its middle");
        }
        assert_ne!(at(&graph, &start, 7), at(&graph, &start, 8), "two island stars started on one spot");
    }

    #[test]
    fn a_continued_layout_holds_the_old_sky_and_a_fresh_one_does_not() {
        // The claim of v0.24, small: lay a sky out, add a star, continue it
        // from the old places, and the old stars barely move -- while running
        // the same rounds without the anchor and the cooling moves them far.
        let mut edges = two_clusters();
        let graph = Graph::from_edges(&edges);
        let previous = places(&graph, &run(&graph, &params(300), |_, _| {}));
        edges.push((2, 50, 1.0));
        edges.push((4, 50, 1.0));
        let grown = Graph::from_edges(&edges);

        let start = Start::from_previous(&grown, &previous);
        let continued = run_from(&grown, &Params::continued(CONTINUED_ITERATIONS, 0), &start, |_, _| {});
        let loose = run_from(
            &grown,
            &Params {
                iterations: CONTINUED_ITERATIONS,
                ..Params::default()
            },
            &start,
            |_, _| {},
        );

        let moved = |positions: &Positions| {
            places(&grown, positions)
                .iter()
                .filter_map(|(id, &place)| previous.get(id).map(|&old| apart(place, old)))
                .fold(0f32, f32::max)
        };
        let (held, free) = (moved(&continued), moved(&loose));
        assert!(held < 2.0, "an old star moved {held} in a continued layout");
        assert!(free > held * 3.0, "the anchor and the cooling did nothing: held {held}, free {free}");

        // The newcomer sits with the stars it is similar to.
        let newcomer = places(&grown, &continued)[&50];
        assert!(
            apart(newcomer, previous[&2]) < apart(newcomer, previous[&102]),
            "the newcomer drifted to the other cluster"
        );
    }
}
