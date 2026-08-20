//! Barnes-Hut quadtree: the structure that makes repulsion affordable.
//!
//! Every star repels every other star, which is N² work — four and a half
//! trillion pairs at three million stars. Barnes-Hut replaces distant crowds
//! with their centre of mass: a cell far enough away is one interaction
//! instead of thousands, bringing the cost to N log N.
//!
//! "Far enough" is the opening angle θ. A cell is summarised when its width
//! divided by its distance is below θ; the standard 0.5 keeps the error small
//! enough that the layout is visually identical to the exact computation.
//!
//! The tree is rebuilt every iteration -- stars move -- so building it must be
//! cheap. It is stored as a flat vector with index links rather than as boxed
//! nodes: three million allocations per iteration would cost more than the
//! forces they save.

/// A node of the tree. Leaves hold one star; internal nodes hold four
/// children and the centre of mass of everything beneath them.
#[derive(Clone, Copy)]
struct Node {
    /// Centre of mass.
    x: f32,
    y: f32,
    /// Total mass beneath this node. Mass is the star's weight in the graph,
    /// so a well-connected artist pushes harder than an obscure one -- which
    /// is what keeps hubs from being buried inside their own crowds.
    mass: f32,
    /// Half-width of the square this node covers.
    half: f32,
    /// Centre of the square (not of the mass).
    cx: f32,
    cy: f32,
    /// Index of the first child; the four children are consecutive. `NONE`
    /// for a leaf.
    children: u32,
    /// How many stars are beneath this node. Zero for an empty leaf, one for
    /// a leaf holding a star.
    count: u32,
}

/// Marks "no children" without an `Option`, which would grow the node.
const NONE: u32 = u32::MAX;

/// The tree for one iteration.
pub struct Quadtree {
    nodes: Vec<Node>,
    /// Opening angle: a cell is summarised when `half * 2 / distance < theta`.
    theta: f32,
}

impl Quadtree {
    /// Builds a tree over the given positions and masses.
    ///
    /// Positions and masses are parallel slices indexed by star. Stars with
    /// zero mass still occupy space -- they are drawn -- but contribute no
    /// repulsion.
    #[must_use]
    pub fn build(xs: &[f32], ys: &[f32], masses: &[f32], theta: f32) -> Self {
        debug_assert_eq!(xs.len(), ys.len());
        debug_assert_eq!(xs.len(), masses.len());

        let (cx, cy, half) = bounds(xs, ys);
        // Capacity is a guess that avoids most reallocation: a quadtree over
        // n points settles around 2n nodes when the points are spread out.
        let mut nodes = Vec::with_capacity(xs.len() * 2 + 4);
        nodes.push(Node {
            x: 0.0,
            y: 0.0,
            mass: 0.0,
            half,
            cx,
            cy,
            children: NONE,
            count: 0,
        });

        let mut tree = Self { nodes, theta };
        for i in 0..xs.len() {
            tree.insert(xs[i], ys[i], masses[i]);
        }
        tree.summarise(0);
        tree
    }

    /// Adds one star to the tree.
    fn insert(&mut self, x: f32, y: f32, mass: f32) {
        let mut at = 0usize;
        loop {
            let node = self.nodes[at];

            // An empty leaf takes the star directly.
            if node.count == 0 && node.children == NONE {
                let node = &mut self.nodes[at];
                node.x = x;
                node.y = y;
                node.mass = mass;
                node.count = 1;
                return;
            }

            // A leaf already holding a star has to split, pushing the old
            // occupant down before the new one can be placed.
            if node.children == NONE {
                // Two stars at the very same point can never be separated by
                // subdividing, and trying costs one level per attempt until
                // the depth cap -- thirty levels of four nodes each, per
                // duplicate. Artists do land on identical coordinates (the
                // initial spiral repeats, and layouts converge), so this is
                // the difference between a tree of 2n nodes and one of 18n.
                //
                // Coincident stars share a leaf instead. They repel each
                // other through the softened force law rather than through
                // geometry, which is what the caller does anyway at zero
                // distance.
                let coincident = (node.x - x).abs() < MIN_HALF && (node.y - y).abs() < MIN_HALF;
                if coincident || node.half < MIN_HALF {
                    let node = &mut self.nodes[at];
                    // The leaf's position stays where the first star put it;
                    // its mass and count grow. Centre of mass is exact for
                    // coincident points, so nothing is lost.
                    node.count += 1;
                    node.mass += mass;
                    return;
                }
                self.split(at);
            }

            let node = self.nodes[at];
            at = node.children as usize + quadrant(x, y, node.cx, node.cy);
        }
    }

    /// Turns a leaf into an internal node with four empty children, moving the
    /// star it held into the right one.
    fn split(&mut self, at: usize) {
        let node = self.nodes[at];
        let half = node.half * 0.5;
        let first = u32::try_from(self.nodes.len()).unwrap_or(u32::MAX);

        for i in 0..4 {
            let (dx, dy) = quadrant_offset(i);
            self.nodes.push(Node {
                x: 0.0,
                y: 0.0,
                mass: 0.0,
                half,
                cx: node.cx + dx * half,
                cy: node.cy + dy * half,
                children: NONE,
                count: 0,
            });
        }

        self.nodes[at].children = first;
        self.nodes[at].count = 0;

        // Re-place the star this node used to hold.
        if node.count > 0 {
            let target = first as usize + quadrant(node.x, node.y, node.cx, node.cy);
            let child = &mut self.nodes[target];
            child.x = node.x;
            child.y = node.y;
            child.mass = node.mass;
            child.count = node.count;
        }
    }

    /// Computes centres of mass bottom-up, after every star is in place.
    ///
    /// Iterative rather than recursive: three million stars make a tree deep
    /// enough that recursion risks the stack.
    fn summarise(&mut self, root: usize) {
        // Post-order traversal with an explicit stack: a node is summarised
        // only after all its children are.
        let mut stack: Vec<(usize, bool)> = vec![(root, false)];
        while let Some((at, children_done)) = stack.pop() {
            let node = self.nodes[at];
            if node.children == NONE {
                continue;
            }
            if children_done {
                let first = node.children as usize;
                let (mut mass, mut x, mut y, mut count) = (0.0f32, 0.0f32, 0.0f32, 0u32);
                for child in &self.nodes[first..first + 4] {
                    mass += child.mass;
                    x += child.x * child.mass;
                    y += child.y * child.mass;
                    count += child.count;
                }
                let node = &mut self.nodes[at];
                node.mass = mass;
                node.count = count;
                if mass > 0.0 {
                    node.x = x / mass;
                    node.y = y / mass;
                }
            } else {
                stack.push((at, true));
                let first = node.children as usize;
                for child in first..first + 4 {
                    stack.push((child, false));
                }
            }
        }
    }

    /// Accumulates the repulsive force on one star.
    ///
    /// `force` is called with the vector to a body or cell and its mass; the
    /// caller decides the force law, so this stays a spatial index rather
    /// than a physics engine.
    pub fn repulsion(&self, x: f32, y: f32, mut force: impl FnMut(f32, f32, f32)) {
        let mut stack: Vec<usize> = vec![0];
        while let Some(at) = stack.pop() {
            let node = self.nodes[at];
            if node.count == 0 || node.mass == 0.0 {
                continue;
            }

            let dx = node.x - x;
            let dy = node.y - y;
            let distance_squared = dx * dx + dy * dy;

            // A leaf, or a cell far enough to summarise. The comparison is
            // squared on both sides to avoid a square root per node, which at
            // this scale is the difference between minutes and hours.
            let width = node.half * 2.0;
            if node.children == NONE || width * width < self.theta * self.theta * distance_squared {
                // A star does not repel itself: at zero distance there is no
                // direction to push in, and the caller's force law would
                // divide by zero.
                if distance_squared > 0.0 {
                    force(dx, dy, node.mass);
                }
                continue;
            }

            let first = node.children as usize;
            for child in first..first + 4 {
                stack.push(child);
            }
        }
    }

    /// How many nodes the tree holds, for reporting and for tests.
    #[must_use]
    #[cfg_attr(not(test), expect(dead_code, reason = "a size report; only tests read it so far"))]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
}

/// Smallest cell the tree will subdivide, which caps depth when stars land on
/// top of each other.
const MIN_HALF: f32 = 1e-6;

/// The square containing every point, with a margin so nothing sits exactly on
/// a boundary.
fn bounds(xs: &[f32], ys: &[f32]) -> (f32, f32, f32) {
    let (mut min_x, mut max_x) = (f32::MAX, f32::MIN);
    let (mut min_y, mut max_y) = (f32::MAX, f32::MIN);
    for (&x, &y) in xs.iter().zip(ys) {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    if xs.is_empty() {
        return (0.0, 0.0, 1.0);
    }

    let cx = f32::midpoint(min_x, max_x);
    let cy = f32::midpoint(min_y, max_y);
    let half = ((max_x - min_x).max(max_y - min_y) * 0.5).max(MIN_HALF) * 1.01;
    (cx, cy, half)
}

/// Which of the four children a point belongs to.
const fn quadrant(x: f32, y: f32, cx: f32, cy: f32) -> usize {
    let east = x >= cx;
    let north = y >= cy;
    match (east, north) {
        (false, false) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (true, true) => 3,
    }
}

/// The direction of each child's centre from its parent's.
const fn quadrant_offset(index: usize) -> (f32, f32) {
    match index {
        0 => (-1.0, -1.0),
        1 => (1.0, -1.0),
        2 => (-1.0, 1.0),
        _ => (1.0, 1.0),
    }
}

#[cfg(test)]
#[expect(clippy::cast_precision_loss, reason = "test fixtures build coordinates from small loop counters")]
mod tests {
    use super::*;

    /// Sums the exact repulsion over every other body, for comparison.
    fn exact(xs: &[f32], ys: &[f32], masses: &[f32], i: usize) -> (f32, f32) {
        let (mut fx, mut fy) = (0.0, 0.0);
        for j in 0..xs.len() {
            if i == j {
                continue;
            }
            let dx = xs[j] - xs[i];
            let dy = ys[j] - ys[i];
            let d2 = dx * dx + dy * dy;
            if d2 > 0.0 {
                fx += masses[j] * dx / d2;
                fy += masses[j] * dy / d2;
            }
        }
        (fx, fy)
    }

    fn approximate(tree: &Quadtree, x: f32, y: f32) -> (f32, f32) {
        let (mut fx, mut fy) = (0.0, 0.0);
        tree.repulsion(x, y, |dx, dy, mass| {
            let d2 = dx * dx + dy * dy;
            fx += mass * dx / d2;
            fy += mass * dy / d2;
        });
        (fx, fy)
    }

    #[test]
    fn a_single_star_has_a_tree_and_no_force_on_itself() {
        let tree = Quadtree::build(&[0.0], &[0.0], &[1.0], 0.5);
        let (fx, fy) = approximate(&tree, 0.0, 0.0);
        assert_eq!((fx, fy), (0.0, 0.0));
    }

    #[test]
    fn two_stars_push_apart_along_the_line_between_them() {
        let xs = [0.0, 10.0];
        let ys = [0.0, 0.0];
        let tree = Quadtree::build(&xs, &ys, &[1.0, 1.0], 0.5);

        let (fx, fy) = approximate(&tree, xs[0], ys[0]);
        // The force on the left star points towards the right one; the caller
        // negates it to repel. What matters here is direction and symmetry.
        assert!(fx > 0.0, "expected a force along x, got {fx}");
        assert!(fy.abs() < 1e-6, "expected no force along y, got {fy}");
    }

    #[test]
    fn mass_is_conserved_through_the_summary() {
        // Every star's mass must survive the tree, or the layout would drift
        // towards whichever region lost less of it.
        let xs: Vec<f32> = (0..100).map(|i| (i % 10) as f32).collect();
        let ys: Vec<f32> = (0..100).map(|i| (i / 10) as f32).collect();
        let masses: Vec<f32> = (0..100).map(|i| 1.0 + i as f32 * 0.1).collect();
        let tree = Quadtree::build(&xs, &ys, &masses, 0.5);

        let total: f32 = masses.iter().sum();
        assert!((tree.nodes[0].mass - total).abs() < total * 1e-4, "root mass {} vs {total}", tree.nodes[0].mass);
        assert_eq!(tree.nodes[0].count as usize, xs.len());
    }

    #[test]
    fn approximation_stays_close_to_the_exact_force() {
        // This is the whole justification for the tree: it must be much
        // cheaper without being visibly different.
        let n = 400;
        let xs: Vec<f32> = (0..n).map(|i| ((i * 37) % 100) as f32).collect();
        let ys: Vec<f32> = (0..n).map(|i| ((i * 53) % 100) as f32).collect();
        let masses = vec![1.0f32; n];

        let tree = Quadtree::build(&xs, &ys, &masses, 0.5);
        for i in (0..n).step_by(37) {
            let (ex, ey) = exact(&xs, &ys, &masses, i);
            let (ax, ay) = approximate(&tree, xs[i], ys[i]);
            let exact_magnitude = (ex * ex + ey * ey).sqrt().max(1e-6);
            let error = ((ax - ex).powi(2) + (ay - ey).powi(2)).sqrt() / exact_magnitude;
            assert!(error < 0.2, "star {i}: relative error {error} too large");
        }
    }

    #[test]
    fn a_smaller_angle_is_more_accurate() {
        // theta controls the trade; theta = 0 degenerates to the exact sum.
        let n = 200;
        let xs: Vec<f32> = (0..n).map(|i| ((i * 29) % 60) as f32).collect();
        let ys: Vec<f32> = (0..n).map(|i| ((i * 17) % 60) as f32).collect();
        let masses = vec![1.0f32; n];

        let (ex, ey) = exact(&xs, &ys, &masses, 0);
        let magnitude = (ex * ex + ey * ey).sqrt().max(1e-6);

        let coarse = Quadtree::build(&xs, &ys, &masses, 1.5);
        let fine = Quadtree::build(&xs, &ys, &masses, 0.1);
        let (cx, cy) = approximate(&coarse, xs[0], ys[0]);
        let (fx, fy) = approximate(&fine, xs[0], ys[0]);

        let coarse_error = ((cx - ex).powi(2) + (cy - ey).powi(2)).sqrt() / magnitude;
        let fine_error = ((fx - ex).powi(2) + (fy - ey).powi(2)).sqrt() / magnitude;
        assert!(fine_error <= coarse_error, "fine {fine_error} should not be worse than coarse {coarse_error}");
        assert!(fine_error < 0.02, "theta = 0.1 should be nearly exact, got {fine_error}");
    }

    #[test]
    fn stars_sharing_a_point_do_not_split_forever() {
        // Two artists can land on the same coordinates; without a depth cap
        // the tree would subdivide until it ran out of memory.
        let xs = vec![5.0f32; 8];
        let ys = vec![5.0f32; 8];
        let masses = vec![1.0f32; 8];
        let tree = Quadtree::build(&xs, &ys, &masses, 0.5);
        assert_eq!(tree.nodes[0].count, 8);
        assert!(tree.len() < 1000, "tree grew to {} nodes", tree.len());
    }

    #[test]
    fn massless_stars_are_placed_but_do_not_push() {
        // Artists with no edges are drawn but exert nothing: the dark matter
        // at the map's margins must not distort the map.
        let xs = [0.0, 10.0];
        let ys = [0.0, 0.0];
        let tree = Quadtree::build(&xs, &ys, &[1.0, 0.0], 0.5);
        let (fx, fy) = approximate(&tree, 0.0, 0.0);
        assert_eq!((fx, fy), (0.0, 0.0), "a massless star exerted a force");
    }

    #[test]
    fn the_tree_stays_near_linear_in_the_number_of_stars() {
        // The memory guess in `build` depends on this; a tree that grew
        // quadratically would exhaust the machine at three million stars.
        let n = 5000;
        let xs: Vec<f32> = (0..n).map(|i| ((i * 7919) % 1000) as f32).collect();
        let ys: Vec<f32> = (0..n).map(|i| ((i * 6271) % 1000) as f32).collect();
        let tree = Quadtree::build(&xs, &ys, &vec![1.0; n], 0.5);
        assert!(tree.len() < n * 4, "{} nodes for {n} stars", tree.len());
    }
}
