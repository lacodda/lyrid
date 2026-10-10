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
//!   listening, it simply opens nothing.
//!
//! Pure arithmetic over what is already known, so the rule is tested here and
//! not described to a database.

use std::collections::HashMap;

use uuid::Uuid;

use super::client::Listen;

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

/// The light each listen pays, in the order given.
///
/// `listens` must be in the order they were heard; `heard` holds how many
/// times each artist had been heard before them. Each listen raises the count
/// of every artist it credits, so the eleventh listen of a new star in one
/// batch pays as familiar exactly as it would have in eleven separate reads.
pub fn price(listens: &[Listen], heard: &HashMap<Uuid, i64>) -> Vec<i32> {
    let mut heard = heard.clone();
    listens
        .iter()
        .map(|listen| {
            let new = listen.artists.iter().any(|artist| heard.get(artist).copied().unwrap_or(0) < NEW_UNTIL);
            for artist in &listen.artists {
                *heard.entry(*artist).or_insert(0) += 1;
            }
            let rate = if new { NEW_RATE } else { FAMILIAR_RATE };
            minutes(listen.seconds) * rate
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::*;

    fn artist(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn heard(artists: &[Uuid], seconds: Option<i32>) -> Listen {
        Listen {
            listened_at: OffsetDateTime::UNIX_EPOCH,
            recording_msid: Uuid::nil(),
            artists: artists.to_vec(),
            seconds,
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
        let new = artist(1);
        let familiar = artist(2);
        let before = HashMap::from([(familiar, 400)]);

        let paid = price(&[heard(&[new], Some(240)), heard(&[familiar], Some(240))], &before);
        assert_eq!(paid, vec![4 * NEW_RATE, 4 * FAMILIAR_RATE]);
    }

    #[test]
    fn a_star_stops_being_new_on_its_tenth_earlier_listen() {
        let star = artist(1);
        // Nine earlier listens: still new. Ten: familiar.
        assert_eq!(price(&[heard(&[star], Some(60))], &HashMap::from([(star, NEW_UNTIL - 1)])), vec![NEW_RATE]);
        assert_eq!(price(&[heard(&[star], Some(60))], &HashMap::from([(star, NEW_UNTIL)])), vec![FAMILIAR_RATE]);
    }

    #[test]
    fn listens_within_one_read_count_towards_each_other() {
        // An album of a star nobody had heard, twelve tracks in one read: the
        // first ten are new, the last two are not -- the same as if each had
        // been read on its own.
        let star = artist(1);
        let album: Vec<Listen> = (0..12).map(|_| heard(&[star], Some(60))).collect();
        let paid = price(&album, &HashMap::new());
        assert_eq!(paid[..10], [NEW_RATE; 10]);
        assert_eq!(paid[10..], [FAMILIAR_RATE; 2]);
    }

    #[test]
    fn a_shared_credit_pays_once_at_the_rate_of_its_newest_artist() {
        let old = artist(1);
        let guest = artist(2);
        let paid = price(&[heard(&[old, guest], Some(180))], &HashMap::from([(old, 50)]));
        assert_eq!(paid, vec![3 * NEW_RATE]);
    }

    #[test]
    fn a_shared_credit_counts_as_a_listen_of_every_artist_on_it() {
        // The guest is heard on the first listen too, so after ten shared
        // listens the guest's own record is no longer new.
        let host = artist(1);
        let guest = artist(2);
        let mut listens: Vec<Listen> = (0..10).map(|_| heard(&[host, guest], Some(60))).collect();
        listens.push(heard(&[guest], Some(60)));
        let paid = price(&listens, &HashMap::new());
        assert_eq!(paid[10], FAMILIAR_RATE);
    }

    #[test]
    fn a_listen_nobody_could_name_pays_as_familiar() {
        assert_eq!(price(&[heard(&[], None)], &HashMap::new()), vec![UNKNOWN_MINUTES * FAMILIAR_RATE]);
    }

    #[test]
    fn every_listen_pays_something() {
        // The ledger refuses a listen worth nothing (CHECK light > 0); the
        // rule must never produce one.
        for seconds in [None, Some(1), Some(29), Some(31), Some(100_000)] {
            for before in [0, NEW_UNTIL, 10_000] {
                let star = artist(9);
                let paid = price(&[heard(&[star], seconds)], &HashMap::from([(star, before)]));
                assert!(paid[0] > 0, "{seconds:?} after {before}");
            }
        }
    }
}
