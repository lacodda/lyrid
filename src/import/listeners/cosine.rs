//! Similarity by the people two artists share.
//!
//! Each person is the set of artists among the thousand they have played
//! most. Two artists are as close as the cosine of their sets of listeners:
//! the people who have both, over the geometric mean of how many have each.
//! Every artist keeps its strongest neighbours, and the pairs are stored once.
//!
//! Why the plain set and not a weighting, as measured on the dump of
//! 2026-10-15 (99,812 people, 34 million person-artist pairs): weighting each
//! person by how many artists they list, or each pair by its listen count,
//! lost agreement with the 2020 graph (0.38-0.40 of the top ten against
//! 0.42) without gaining any genre coherence (0.75-0.77 against 0.77), and
//! weighting by person let the handful of accounts that list three artists
//! name a star's nearest neighbours. Dropping pairs played only once kept the
//! same quality and lost thirty thousand stars.

use std::num::NonZero;
use std::sync::atomic::{AtomicUsize, Ordering};

/// What a pair has to clear to be an edge, and how many each artist keeps.
#[derive(Debug, Clone, Copy)]
pub struct Params {
    /// Listeners an artist needs before its neighbours say anything about the
    /// artist rather than about the one or two people who listen to it.
    pub min_listeners: u32,
    /// People a pair must share. Two is a coincidence in a hundred thousand.
    pub min_shared: u32,
    /// Neighbours each artist keeps, strongest first. The union of the lists
    /// is the graph, so an artist can end with more edges than this: the
    /// neighbours that kept it.
    pub neighbours: usize,
}

/// As measured: 283,060 stars with edges, 6.89 million edges, and the top
/// ten neighbours sharing their main genre 0.77 of the time (the 2020 graph:
/// 0.73).
pub const PARAMS: Params = Params {
    min_listeners: 5,
    min_shared: 3,
    neighbours: 30,
};

/// An edge: two artists by their dense ids, the smaller first, and the cosine.
pub type Edge = (u32, u32, f32);

/// The graph `people` imply among `artists` artists.
///
/// `people` holds each person's artists as dense ids below `artists`, each
/// once. The result is sorted by pair, each pair once.
pub fn neighbours(people: &[Vec<u32>], artists: usize, params: Params) -> Vec<Edge> {
    let mut listeners = vec![0u32; artists];
    for person in people {
        for &artist in person {
            listeners[artist as usize] += 1;
        }
    }

    // Who listens to each artist, as one flat list cut at `starts`: a vector
    // per artist would be seven hundred thousand allocations.
    let mut starts = Vec::with_capacity(artists + 1);
    let mut total = 0usize;
    for &count in &listeners {
        starts.push(total);
        total += count as usize;
    }
    starts.push(total);
    let mut audience = vec![0u32; total];
    let mut filled = starts.clone();
    for (index, person) in people.iter().enumerate() {
        let index = u32::try_from(index).expect("fewer people than u32 can count");
        for &artist in person {
            audience[filled[artist as usize]] = index;
            filled[artist as usize] += 1;
        }
    }

    // Each artist's list is worked out on its own, so the work is split by
    // artist and the answer does not depend on how it was split.
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, NonZero::get);
    let mut edges: Vec<Edge> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut shared = vec![0u32; artists];
                    let mut touched: Vec<u32> = Vec::new();
                    let mut candidates: Vec<(u32, f32)> = Vec::new();
                    let mut found = Vec::new();
                    loop {
                        let artist = next.fetch_add(1, Ordering::Relaxed);
                        if artist >= artists {
                            break;
                        }
                        let own = listeners[artist];
                        if own < params.min_listeners {
                            continue;
                        }
                        for &person in &audience[starts[artist]..starts[artist + 1]] {
                            for &other in &people[person as usize] {
                                let slot = &mut shared[other as usize];
                                if *slot == 0 {
                                    touched.push(other);
                                }
                                *slot += 1;
                            }
                        }
                        let artist = u32::try_from(artist).expect("fewer artists than u32 can count");
                        for &other in &touched {
                            let both = std::mem::take(&mut shared[other as usize]);
                            let theirs = listeners[other as usize];
                            if other != artist && both >= params.min_shared && theirs >= params.min_listeners {
                                candidates.push((other, cosine(both, own, theirs)));
                            }
                        }
                        touched.clear();
                        // Strongest first; among equals the smaller id, so a
                        // tie is broken the same way on every run.
                        candidates.sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
                        found.extend(
                            candidates
                                .drain(..)
                                .take(params.neighbours)
                                .map(|(other, score)| (artist.min(other), artist.max(other), score)),
                        );
                    }
                    found
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("a similarity worker panicked"))
            .collect()
    });

    // A pair two artists both kept arrives twice, with one score: the cosine
    // is symmetric, and is computed from the same integers either way.
    edges.sort_unstable_by_key(|&(a, b, _)| (a, b));
    edges.dedup_by_key(|&mut (a, b, _)| (a, b));
    edges
}

/// The cosine of two sets of listeners from their sizes and their overlap.
///
/// The product is taken as an integer first, so `cosine(s, a, b)` and
/// `cosine(s, b, a)` are the same bits.
fn cosine(both: u32, mine: u32, theirs: u32) -> f32 {
    #[expect(clippy::cast_precision_loss, reason = "a product of two counts below 2^17 is exact in an f64")]
    let product = (u64::from(mine) * u64::from(theirs)) as f64;
    #[expect(clippy::cast_possible_truncation, reason = "a cosine lies in 0..1, well inside f32")]
    let score = (f64::from(both) / product.sqrt()) as f32;
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOOSE: Params = Params {
        min_listeners: 1,
        min_shared: 1,
        neighbours: 10,
    };

    fn score(edges: &[Edge], a: u32, b: u32) -> Option<f32> {
        edges.iter().find(|&&(x, y, _)| (x, y) == (a.min(b), a.max(b))).map(|e| e.2)
    }

    #[test]
    fn two_artists_are_as_close_as_the_cosine_of_their_listeners() {
        // 0 has four listeners, 1 has two, and they share two: 2 / sqrt(8).
        let people = vec![vec![0, 1], vec![0, 1], vec![0], vec![0]];
        let edges = neighbours(&people, 2, LOOSE);
        assert_eq!(edges.len(), 1);
        let expected = 2.0 / 8f32.sqrt();
        assert!((edges[0].2 - expected).abs() < 1e-6, "{} against {expected}", edges[0].2);
        assert_eq!((edges[0].0, edges[0].1), (0, 1), "smaller id first");
    }

    #[test]
    fn a_pair_both_artists_kept_is_one_edge() {
        let people = vec![vec![0, 1], vec![1, 0]];
        assert_eq!(neighbours(&people, 2, LOOSE), vec![(0, 1, 1.0)]);
    }

    #[test]
    fn an_artist_is_never_its_own_neighbour() {
        let people = vec![vec![0], vec![0]];
        assert_eq!(neighbours(&people, 1, LOOSE), Vec::new());
    }

    #[test]
    fn a_pair_needs_enough_people_in_common() {
        // 0 and 1 share two people, 0 and 2 share three.
        let people = vec![vec![0, 1, 2], vec![0, 1, 2], vec![0, 2], vec![1], vec![1]];
        let edges = neighbours(&people, 3, Params { min_shared: 3, ..LOOSE });
        assert_eq!(score(&edges, 0, 1), None, "two people in common is not enough");
        assert!(score(&edges, 0, 2).is_some(), "three is");
    }

    #[test]
    fn an_artist_needs_enough_listeners_to_have_neighbours() {
        // 2 has two listeners: each of its pairs is one person's taste.
        let people = vec![vec![0, 1, 2], vec![0, 1, 2], vec![0, 1], vec![0, 1]];
        let edges = neighbours(&people, 3, Params { min_listeners: 3, ..LOOSE });
        assert_eq!(edges.iter().map(|e| (e.0, e.1)).collect::<Vec<_>>(), vec![(0, 1)]);
    }

    #[test]
    fn each_artist_keeps_only_its_strongest_neighbours() {
        // Artist 0 is listened to with 1 by three people, with 2 by two and
        // with 3 by one. Keeping two, 0 keeps 1 and 2 -- but 3, which has only
        // two neighbours at all, keeps 0 from its own side.
        let people = vec![vec![0, 1, 2], vec![0, 1, 2], vec![0, 1, 3], vec![1], vec![2]];
        let edges = neighbours(&people, 4, Params { neighbours: 2, ..LOOSE });
        let from_zero: Vec<u32> = edges.iter().filter(|e| e.0 == 0).map(|e| e.1).collect();
        assert_eq!(from_zero, vec![1, 2, 3], "3 kept 0 though 0 did not keep 3");

        // Keeping one: 0 and 1 keep each other, 2 and 4 keep each other, and
        // the pair 0-2 is past the limit on both of its sides.
        let people = vec![vec![0, 1, 2], vec![0, 1, 2], vec![0, 1], vec![2, 4], vec![2, 4], vec![2, 4]];
        let edges = neighbours(&people, 5, Params { neighbours: 1, ..LOOSE });
        assert_eq!(edges.iter().map(|e| (e.0, e.1)).collect::<Vec<_>>(), vec![(0, 1), (2, 4)]);
    }

    #[test]
    fn the_score_does_not_depend_on_which_side_computed_it() {
        assert_eq!(cosine(3, 7, 11).to_bits(), cosine(3, 11, 7).to_bits());
    }

    #[test]
    fn the_answer_does_not_depend_on_the_order_people_come_in() {
        let people = vec![vec![0, 1, 2], vec![2, 3], vec![0, 3, 1], vec![1, 2, 3], vec![3, 0]];
        let mut reversed = people.clone();
        reversed.reverse();
        assert_eq!(neighbours(&people, 4, LOOSE), neighbours(&reversed, 4, LOOSE));
    }
}
