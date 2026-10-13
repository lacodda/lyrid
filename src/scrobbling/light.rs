//! What a listen is worth (bank 35: light is paid for minutes of real
//! listening, and the new pays far more than the familiar).
//!
//! The whole rule, in one place:
//!
//! - a minute of listening gathers **one** light;
//! - a minute spent on a star you have heard **fewer than ten times** before
//!   gathers **five**;
//! - minutes are the recording's length, rounded, at least one and at most
//!   fifteen; a listen that does not say how long it was counts as three;
//! - a listen credited to several artists opens every one of them and pays
//!   once, at the rate of the newest;
//! - a listen nobody could name still pays, at the familiar rate: it was real
//!   listening, it simply opens nothing;
//! - a listen from before the link, or from a file, pays **nothing** -- but it
//!   is still listening, and it counts towards what is familiar (ADR 0020).
//!
//! Pure arithmetic over what is already known, so the rule is tested here and
//! not described to a database.

use std::collections::HashMap;

use uuid::Uuid;

/// Below this many earlier listens, a star is still new to you.
pub const NEW_UNTIL: i64 = 10;
/// Light per minute for a star that is still new.
pub const NEW_RATE: i32 = 5;
/// Light per minute for everything else.
pub const FAMILIAR_RATE: i32 = 1;
/// The minutes counted for a listen that does not report its length: about a
/// song.
pub const UNKNOWN_MINUTES: i32 = 3;
/// The most minutes one listen can count for. A two-hour "track" is a mix, a
/// podcast, or a number someone typed; the ceiling keeps one claim from being
/// worth a week of listening.
pub const MAX_MINUTES: i32 = 15;

/// The minutes a listen counts for.
pub fn minutes(seconds: Option<i32>) -> i32 {
    match seconds {
        // Rounded to the nearest minute, so a 3:40 song is four and a 3:20 one
        // is three, and never less than one: a scrobble means it was played.
        Some(seconds) => ((seconds + 30) / 60).clamp(1, MAX_MINUTES),
        None => UNKNOWN_MINUTES,
    }
}

/// One listen in the order it was heard, as the rule sees it.
#[derive(Debug, Clone, Copy)]
pub struct Heard<'a> {
    pub artists: &'a [Uuid],
    pub seconds: Option<i32>,
    /// Whether this listen is being paid for now: a new listen heard while the
    /// account was linked. A listen already in the ledger, one from before the
    /// link and one from a file all pass through the count without a price.
    pub pays: bool,
}

/// The light each listen pays, in the order given, or `None` for a listen
/// that pays nothing.
///
/// `timeline` must be in the order the listens were heard; `before` holds how
/// many times each artist had been heard before the first of them. Every
/// listen raises the count of every artist it credits, paid or not, so the
/// eleventh listen of a new star pays as familiar exactly as it would have if
/// each listen had been read on its own -- and a star heard five hundred times
/// before the link is familiar on the first listen after it.
pub fn price(timeline: &[Heard<'_>], before: &HashMap<Uuid, i64>) -> Vec<Option<i32>> {
    let mut heard = before.clone();
    timeline
        .iter()
        .map(|listen| {
            let new = listen.artists.iter().any(|artist| heard.get(artist).copied().unwrap_or(0) < NEW_UNTIL);
            for artist in listen.artists {
                *heard.entry(*artist).or_insert(0) += 1;
            }
            let rate = if new { NEW_RATE } else { FAMILIAR_RATE };
            listen.pays.then(|| minutes(listen.seconds) * rate)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artist(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn paid(artists: &[Uuid], seconds: Option<i32>) -> Heard<'_> {
        Heard { artists, seconds, pays: true }
    }

    fn unpaid(artists: &[Uuid]) -> Heard<'_> {
        Heard {
            artists,
            seconds: Some(180),
            pays: false,
        }
    }

    #[test]
    fn minutes_round_and_stay_inside_their_bounds() {
        assert_eq!(minutes(Some(200)), 3);
        assert_eq!(minutes(Some(220)), 4);
        assert_eq!(minutes(Some(5)), 1, "a scrobble means it was played");
        assert_eq!(minutes(Some(7200)), MAX_MINUTES);
        assert_eq!(minutes(None), UNKNOWN_MINUTES);
    }

    #[test]
    fn a_new_star_pays_five_times_what_a_familiar_one_does() {
        let new = [artist(1)];
        let familiar = [artist(2)];
        let before = HashMap::from([(familiar[0], 400)]);

        let prices = price(&[paid(&new, Some(240)), paid(&familiar, Some(240))], &before);
        assert_eq!(prices, vec![Some(4 * NEW_RATE), Some(4 * FAMILIAR_RATE)]);
    }

    #[test]
    fn a_star_stops_being_new_on_its_tenth_earlier_listen() {
        let star = [artist(1)];
        // Nine earlier listens: still new. Ten: familiar.
        assert_eq!(
            price(&[paid(&star, Some(60))], &HashMap::from([(star[0], NEW_UNTIL - 1)])),
            vec![Some(NEW_RATE)]
        );
        assert_eq!(
            price(&[paid(&star, Some(60))], &HashMap::from([(star[0], NEW_UNTIL)])),
            vec![Some(FAMILIAR_RATE)]
        );
    }

    #[test]
    fn listens_within_one_read_count_towards_each_other() {
        // An album of a star nobody had heard, twelve tracks in one read: the
        // first ten are new, the last two are not -- the same as if each had
        // been read on its own.
        let star = [artist(1)];
        let album: Vec<Heard<'_>> = (0..12).map(|_| paid(&star, Some(60))).collect();
        let prices = price(&album, &HashMap::new());
        assert_eq!(prices[..10], [Some(NEW_RATE); 10]);
        assert_eq!(prices[10..], [Some(FAMILIAR_RATE); 2]);
    }

    #[test]
    fn a_shared_credit_pays_once_at_the_rate_of_its_newest_artist() {
        let credit = [artist(1), artist(2)];
        let prices = price(&[paid(&credit, Some(180))], &HashMap::from([(credit[0], 50)]));
        assert_eq!(prices, vec![Some(3 * NEW_RATE)]);
    }

    #[test]
    fn a_shared_credit_counts_as_a_listen_of_every_artist_on_it() {
        // The guest is heard on the first listen too, so after ten shared
        // listens the guest's own record is no longer new.
        let credit = [artist(1), artist(2)];
        let guest = [artist(2)];
        let mut listens: Vec<Heard<'_>> = (0..10).map(|_| paid(&credit, Some(60))).collect();
        listens.push(paid(&guest, Some(60)));
        let prices = price(&listens, &HashMap::new());
        assert_eq!(prices[10], Some(FAMILIAR_RATE));
    }

    #[test]
    fn a_listen_nobody_could_name_pays_as_familiar() {
        assert_eq!(price(&[paid(&[], None)], &HashMap::new()), vec![Some(UNKNOWN_MINUTES * FAMILIAR_RATE)]);
    }

    #[test]
    fn the_past_pays_nothing_and_still_makes_a_star_familiar() {
        // Ten listens from before the link, then one after it: the history is
        // free, and it has used up the star's newness.
        let star = [artist(1)];
        let mut timeline: Vec<Heard<'_>> = (0..10).map(|_| unpaid(&star)).collect();
        timeline.push(paid(&star, Some(60)));
        let prices = price(&timeline, &HashMap::new());
        assert_eq!(prices[..10], [None; 10]);
        assert_eq!(prices[10], Some(FAMILIAR_RATE));
    }

    #[test]
    fn every_paid_listen_pays_something() {
        // The ledger refuses a paid listen worth nothing (CHECK light > 0);
        // the rule must never produce one.
        for seconds in [None, Some(1), Some(29), Some(31), Some(100_000)] {
            for before in [0, NEW_UNTIL, 10_000] {
                let star = [artist(9)];
                let prices = price(&[paid(&star, seconds)], &HashMap::from([(star[0], before)]));
                assert!(prices[0].is_some_and(|light| light > 0), "{seconds:?} after {before}");
            }
        }
    }
}
