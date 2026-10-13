//! The home star: where a person's listening gathers on the sky (idea 3).
//!
//! Not the centre of mass. Someone who listens to both techno and baroque has
//! a centre of mass in the empty sky between the two, or on a star they have
//! never heard -- a home nobody lives in. The home is the star of their own
//! listening that is nearest, on the whole, to all of the rest of it: the
//! weighted medoid, each star weighing as many times as it was heard.
//!
//! The candidates are the most heard stars. A home is somewhere a person
//! returns to, and limiting the search to the stars they return to most keeps
//! it cheap -- two hundred candidates against twenty thousand heard stars is
//! four million distances, a few milliseconds.

/// A heard star where the sky puts it, and how many times it was heard.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Heard {
    pub id: i32,
    pub x: f32,
    pub y: f32,
    pub listens: i64,
}

/// How many of the most heard stars may be home.
const CANDIDATES: usize = 200;

/// The home among `stars`, or `None` when nothing has been heard.
pub fn home(stars: &[Heard]) -> Option<Heard> {
    let mut candidates: Vec<&Heard> = stars.iter().collect();
    candidates.sort_by(|a, b| b.listens.cmp(&a.listens).then(a.id.cmp(&b.id)));
    candidates.truncate(CANDIDATES);

    let mut best: Option<(f64, &Heard)> = None;
    for candidate in candidates {
        let cost: f64 = stars
            .iter()
            .map(|star| {
                #[allow(clippy::cast_precision_loss, reason = "a weight, not a count to be exact about")]
                let weight = star.listens as f64;
                weight * f64::from(star.x - candidate.x).hypot(f64::from(star.y - candidate.y))
            })
            .sum();
        // Candidates arrive most heard first, so a tie keeps the star heard
        // more -- and a home does not flicker between two equals.
        if best.is_none_or(|(lowest, _)| cost < lowest) {
            best = Some((cost, candidate));
        }
    }
    best.map(|(_, star)| *star)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn star(id: i32, x: f32, y: f32, listens: i64) -> Heard {
        Heard { id, x, y, listens }
    }

    #[test]
    fn nothing_heard_is_no_home() {
        assert_eq!(home(&[]), None);
    }

    #[test]
    fn one_star_heard_is_home() {
        assert_eq!(home(&[star(7, 3.0, 4.0, 1)]).map(|s| s.id), Some(7));
    }

    #[test]
    fn home_is_a_star_heard_and_not_the_empty_sky_between_two_tastes() {
        // Two clusters far apart, the left one heard a little more. The centre
        // of mass is at x = 0, where nothing was heard; home is in the left
        // cluster.
        let stars = [
            star(1, -100.0, 0.0, 30),
            star(2, -101.0, 1.0, 20),
            star(3, 100.0, 0.0, 25),
            star(4, 101.0, -1.0, 20),
        ];
        let found = home(&stars).unwrap();
        assert!(found.x < 0.0, "{found:?}");
    }

    #[test]
    fn home_follows_where_the_listening_is_and_not_the_single_loudest_star() {
        // One star heard forty times alone in a corner, and a cluster of ten
        // stars heard ten times each: the cluster is where the listening is.
        let mut stars = vec![star(99, 500.0, 500.0, 40)];
        for id in 0..10 {
            stars.push(star(id, f32::from(u8::try_from(id).unwrap()), 0.0, 10));
        }
        let found = home(&stars).unwrap();
        assert_ne!(found.id, 99);
        assert!(found.x < 10.0);
    }

    #[test]
    fn the_same_listening_always_has_the_same_home() {
        // Two identical stars: the one heard more, then the lower id.
        let stars = [star(2, 0.0, 0.0, 5), star(1, 0.0, 0.0, 5)];
        assert_eq!(home(&stars).map(|s| s.id), Some(1));
        let stars = [star(2, 0.0, 0.0, 6), star(1, 0.0, 0.0, 5)];
        assert_eq!(home(&stars).map(|s| s.id), Some(2));
    }
}
