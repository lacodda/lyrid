//! The weekly letter: "your week in the sky" (idea 11).
//!
//! Once a week, to those who asked for it, a short plain-text letter of what
//! their listening came to: how much, the stars it opened for the first time,
//! the ones heard most, and where their home star stands. A week with no
//! listening sends nothing -- a letter that says "nothing happened" is a
//! letter that teaches people to stop reading the next one.
//!
//! The rules that make it a letter people keep, all enforced here:
//!
//! - **Asked for, never assumed.** A row in `weekly_letter` is the asking; no
//!   row, no letter. Only a confirmed address can ask.
//! - **Once a week at most.** The week is taken by one UPDATE before the letter
//!   is written, so two servers cannot both send it, and a crash between the
//!   two loses a letter rather than doubling one. A server that was down for a
//!   month sends last week's letter, not four old ones.
//! - **One click to stop.** Every letter carries a link that stops it without
//!   signing in, and the same address in `List-Unsubscribe` for the mail
//!   client's own button (RFC 8058).
//! - **In the reader's language.** The interface keeps its language per
//!   browser, and a letter has no browser, so the language is the one the
//!   person was reading in when they asked. The words live in `letter/*.json`,
//!   checked against each other by the tests below, as the interface's are.
//!
//! Weeks are ISO weeks in UTC: Monday to Monday. lyrid does not know where its
//! readers live, and a week that began at someone's local midnight would need
//! to.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use sqlx::{PgConnection, PgExecutor, PgPool};
use time::{Date, OffsetDateTime, Weekday};

use crate::mail::Mailer;
use crate::scrobbling::{self, home, light};

/// The languages a letter can be written in, as the column allows.
pub const LANGUAGES: [&str; 2] = ["en", "ru"];

/// How many newly opened stars a letter names before saying "and N more".
const NAMED: usize = 8;

/// How many of the most heard stars a letter names.
const LOUDEST: i64 = 3;

/// How often the sender looks for letters due. A letter goes out within an
/// hour of its week ending, and an idle hour costs one query.
const LOOK_EVERY: Duration = Duration::from_hours(1);

// ------------------------------------------------------------------ words

/// The words of one language.
pub struct Locale {
    language: &'static str,
    strings: HashMap<String, String>,
}

fn locales() -> &'static HashMap<&'static str, Locale> {
    static LOCALES: OnceLock<HashMap<&'static str, Locale>> = OnceLock::new();
    LOCALES.get_or_init(|| {
        let read = |language: &'static str, text: &str| {
            let strings: HashMap<String, String> = serde_json::from_str(text).expect("the letter's words are valid JSON");
            (language, Locale { language, strings })
        };
        HashMap::from([read("en", include_str!("letter/en.json")), read("ru", include_str!("letter/ru.json"))])
    })
}

/// The words of `language`, or English for a language with none.
pub fn locale(language: &str) -> &'static Locale {
    let all = locales();
    all.get(language).or_else(|| all.get("en")).expect("English is always there")
}

/// The CLDR plural category of `n` in `language`, for the two languages a
/// letter is written in.
fn category(language: &str, n: i64) -> &'static str {
    let n = n.abs();
    match language {
        "ru" => {
            let (ten, hundred) = (n % 10, n % 100);
            if ten == 1 && hundred != 11 {
                "one"
            } else if (2..=4).contains(&ten) && !(12..=14).contains(&hundred) {
                "few"
            } else {
                "many"
            }
        }
        _ if n == 1 => "one",
        _ => "other",
    }
}

impl Locale {
    fn get<'a>(&'a self, key: &'a str) -> &'a str {
        self.strings.get(key).map_or(key, String::as_str)
    }

    /// The string under `key` with each `{{name}}` filled.
    fn fill(&self, key: &str, values: &[(&str, &str)]) -> String {
        let mut text = self.get(key).to_string();
        for (name, value) in values {
            text = text.replace(&format!("{{{{{name}}}}}"), value);
        }
        text
    }

    /// The plural form of `key` for `n`, with `{{number}}` filled.
    fn count(&self, key: &str, n: i64) -> String {
        let form = format!("{key}_{}", category(self.language, n));
        let key = if self.strings.contains_key(&form) { form } else { format!("{key}_other") };
        self.fill(&key, &[("number", &self.number(n))])
    }

    /// A whole number with its thousands grouped the way the language groups
    /// them.
    fn number(&self, n: i64) -> String {
        let digits = n.unsigned_abs().to_string();
        let mut grouped = String::new();
        for (index, digit) in digits.chars().enumerate() {
            if index > 0 && (digits.len() - index).is_multiple_of(3) {
                grouped.push_str(self.get("group"));
            }
            grouped.push(digit);
        }
        if n < 0 { format!("-{grouped}") } else { grouped }
    }

    fn month(&self, date: Date) -> &str {
        let index = usize::from(u8::from(date.month())) - 1;
        self.get("months").split('|').nth(index).unwrap_or_default()
    }

    /// "6–12 October", or "29 September – 5 October" across a month's end.
    pub fn week(&self, monday: Date) -> String {
        let sunday = monday + time::Duration::days(6);
        let (from, to) = (monday.day().to_string(), sunday.day().to_string());
        if monday.month() == sunday.month() {
            self.fill("week_one_month", &[("from", &from), ("to", &to), ("month", self.month(sunday))])
        } else {
            self.fill(
                "week_two_months",
                &[
                    ("from", &from),
                    ("from_month", self.month(monday)),
                    ("to", &to),
                    ("to_month", self.month(sunday)),
                ],
            )
        }
    }
}

// ------------------------------------------------------------------ weeks

/// The Monday that began the week `now` is in.
pub fn monday_of(now: OffsetDateTime) -> Date {
    let date = now.date();
    date - time::Duration::days(i64::from(date.weekday().number_days_from_monday()))
}

/// The Monday of the last week that has ended by `now`: the week a letter
/// sent now is about.
pub fn last_week(now: OffsetDateTime) -> Date {
    monday_of(now) - time::Duration::days(7)
}

fn start(monday: Date) -> OffsetDateTime {
    debug_assert_eq!(monday.weekday(), Weekday::Monday);
    monday.midnight().assume_utc()
}

// ------------------------------------------------------------------ the week

/// A star named in a letter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    pub id: i32,
    pub name: String,
}

/// What one person's week came to.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Week {
    pub listens: i64,
    /// Minutes of listening, counted the way light counts them.
    pub minutes: i64,
    pub light: i64,
    /// Stars whose first listen ever fell in this week, first opened first.
    pub opened: Vec<Named>,
    /// The stars heard most this week, most first.
    pub loudest: Vec<String>,
    /// The home star at the end of the week, and at its start.
    pub home: Option<Named>,
    pub home_before: Option<Named>,
}

/// Reads one person's week out of the ledger.
pub async fn gather(conn: &mut PgConnection, user_id: i32, monday: Date) -> sqlx::Result<Week> {
    let (from, until) = (start(monday), start(monday) + time::Duration::days(7));

    let rows: Vec<(Option<i32>, Option<i32>)> =
        sqlx::query_as("SELECT seconds, light FROM listen WHERE user_id = $1 AND listened_at >= $2 AND listened_at < $3")
            .bind(user_id)
            .bind(from)
            .bind(until)
            .fetch_all(&mut *conn)
            .await?;
    let mut week = Week {
        listens: i64::try_from(rows.len()).unwrap_or(i64::MAX),
        minutes: rows.iter().map(|(seconds, _)| i64::from(light::minutes(*seconds))).sum(),
        light: rows.iter().filter_map(|(_, light)| *light).map(i64::from).sum(),
        ..Week::default()
    };
    if week.listens == 0 {
        return Ok(week);
    }

    let after = scrobbling::placed(&mut *conn, user_id, Some(until)).await?;
    let before = scrobbling::placed(&mut *conn, user_id, Some(from)).await?;
    let mut opened: Vec<&scrobbling::Placed> = after.iter().filter(|star| star.first_at >= from).collect();
    opened.sort_by_key(|star| (star.first_at, star.id));
    week.opened = opened
        .into_iter()
        .map(|star| Named {
            id: star.id,
            name: star.name.clone(),
        })
        .collect();

    let home_of = |stars: &[scrobbling::Placed]| {
        home::home(&stars.iter().map(scrobbling::Placed::heard).collect::<Vec<_>>())
            .and_then(|home| stars.iter().find(|star| star.id == home.id))
            .map(|star| Named {
                id: star.id,
                name: star.name.clone(),
            })
    };
    week.home = home_of(&after);
    week.home_before = home_of(&before);

    week.loudest = sqlx::query_scalar(
        "SELECT a.name
         FROM listen l, unnest(l.artist_mbids) AS artist
         JOIN artist a ON a.mbid = artist
         WHERE l.user_id = $1 AND l.listened_at >= $2 AND l.listened_at < $3
         GROUP BY a.id, a.name
         ORDER BY count(*) DESC, a.name
         LIMIT $4",
    )
    .bind(user_id)
    .bind(from)
    .bind(until)
    .bind(LOUDEST)
    .fetch_all(conn)
    .await?;
    Ok(week)
}

/// The letter for a week, as a subject and a body -- or `None` for a week
/// with no listening in it.
pub fn compose(locale: &Locale, week: &Week, monday: Date, public_url: &str, token: &str) -> Option<(String, String)> {
    if week.listens == 0 {
        return None;
    }
    let star = |named: &Named| format!("{public_url}/star/{}", named.id);
    let label = locale.week(monday);
    let subject = locale.fill("subject", &[("week", &label)]);

    let mut body = vec![locale.fill("heading", &[("week", &label)]), String::new()];

    let listens = locale.count("listens", week.listens);
    let time = if week.minutes < 120 {
        locale.count("minutes", week.minutes)
    } else {
        locale.count("hours", (week.minutes + 30) / 60)
    };
    body.push(if week.light > 0 {
        let light = locale.fill("light", &[("number", &locale.number(week.light))]);
        locale.fill("tally", &[("listens", &listens), ("time", &time), ("light", &light)])
    } else {
        // A week of only history or a file's listening gathered nothing,
        // and "0 light gathered" would read as a fault.
        locale.fill("tally_unpaid", &[("listens", &listens), ("time", &time)])
    });
    body.push(String::new());

    if week.opened.is_empty() {
        body.push(locale.get("nothing_opened").to_string());
    } else {
        body.push(locale.count("opened", i64::try_from(week.opened.len()).unwrap_or(i64::MAX)));
        for named in week.opened.iter().take(NAMED) {
            body.push(format!("  {} — {}", named.name, star(named)));
        }
        let more = week.opened.len().saturating_sub(NAMED);
        if more > 0 {
            body.push(format!("  {}", locale.count("more", i64::try_from(more).unwrap_or(i64::MAX))));
        }
    }

    if !week.loudest.is_empty() {
        body.push(String::new());
        body.push(locale.fill("loudest", &[("names", &week.loudest.join(", "))]));
    }

    if let Some(home) = &week.home {
        body.push(String::new());
        body.push(match &week.home_before {
            Some(before) if before.id != home.id => locale.fill("home_moved", &[("from", &before.name), ("to", &home.name), ("link", &star(home))]),
            _ => locale.fill("home", &[("name", &home.name), ("link", &star(home))]),
        });
    }

    body.push(String::new());
    body.push(locale.fill("sky", &[("link", public_url)]));
    body.push(String::new());
    body.push("--".to_string());
    body.push(locale.fill("footer", &[("link", &stop_page(public_url, token))]));
    body.push(locale.get("signature").to_string());
    Some((subject, body.join("\n")))
}

/// The page a person lands on from the link at the bottom of a letter.
pub fn stop_page(public_url: &str, token: &str) -> String {
    format!("{public_url}/letter/stop?token={token}")
}

/// The address a mail client posts to when its own "unsubscribe" is pressed.
pub fn stop_address(public_url: &str, token: &str) -> String {
    format!("{public_url}/api/letter/stop?token={token}")
}

// ------------------------------------------------------------------ asking

/// Asks for the letter, or changes the language of one already asked for.
///
/// The first letter is about the week now under way: the week that ended is
/// marked as considered, so asking on a Thursday does not bring a letter about
/// a week that was over before the asking.
pub async fn subscribe(db: impl PgExecutor<'_>, user_id: i32, language: &str, now: OffsetDateTime) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO weekly_letter (user_id, language, sent_week, token) VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id) DO UPDATE SET language = EXCLUDED.language",
    )
    .bind(user_id)
    .bind(language)
    .bind(last_week(now))
    .bind(crate::auth::link_token())
    .execute(db)
    .await?;
    Ok(())
}

/// Stops the letter from the account.
pub async fn unsubscribe(db: impl PgExecutor<'_>, user_id: i32) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM weekly_letter WHERE user_id = $1").bind(user_id).execute(db).await?;
    Ok(())
}

/// Stops the letter a link's token belongs to. Whether it did is not said:
/// either way no letter will come under that token, which is all the link
/// promised -- and a link that answered "no such letter" would let anyone
/// test tokens.
pub async fn stop(db: impl PgExecutor<'_>, token: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM weekly_letter WHERE token = $1").bind(token).execute(db).await?;
    Ok(())
}

// ------------------------------------------------------------------ sending

/// A letter taken for sending.
#[derive(Debug)]
pub struct Due {
    pub user_id: i32,
    pub language: String,
    pub token: String,
    pub email: String,
    pub confirmed: bool,
}

/// Takes the next letter due for the week beginning `week`, marking the week
/// as considered in the same statement.
pub async fn claim(db: impl PgExecutor<'_>, week: Date) -> sqlx::Result<Option<Due>> {
    let row: Option<(i32, String, String, String, bool)> = sqlx::query_as(
        "UPDATE weekly_letter w SET sent_week = $1
         FROM app_user u
         WHERE w.user_id = (
                 SELECT user_id FROM weekly_letter
                 WHERE sent_week < $1
                 ORDER BY user_id
                 LIMIT 1
                 FOR UPDATE SKIP LOCKED
             )
           AND u.id = w.user_id
         RETURNING w.user_id, w.language, w.token, u.email, u.email_confirmed_at IS NOT NULL",
    )
    .bind(week)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|(user_id, language, token, email, confirmed)| Due {
        user_id,
        language,
        token,
        email,
        confirmed,
    }))
}

/// Sends every letter due for the week that ended before `now`, and says how
/// many went.
pub async fn send_due(pool: &PgPool, mailer: &Mailer, public_url: &str, now: OffsetDateTime) -> sqlx::Result<usize> {
    let week = last_week(now);
    let mut sent = 0;
    let mut conn = pool.acquire().await?;
    while let Some(due) = claim(&mut *conn, week).await? {
        // Asking needed a confirmed address; this holds it to that if the
        // address ever stops being one.
        if !due.confirmed {
            continue;
        }
        let gathered = gather(&mut conn, due.user_id, week).await?;
        let Some((subject, body)) = compose(locale(&due.language), &gathered, week, public_url, &due.token) else {
            continue;
        };
        match mailer.send_asked_for(&due.email, &subject, &body, &stop_address(public_url, &due.token)).await {
            Ok(()) => sent += 1,
            // Lost rather than retried: the week is already taken, and a
            // letter that might arrive twice is worse than one that did not.
            Err(error) => tracing::warn!(user = due.user_id, error = format!("{error:#}"), "a weekly letter could not be sent"),
        }
    }
    Ok(sent)
}

/// Sends the weekly letters, forever: looks once an hour.
pub async fn run(pool: PgPool, mailer: Mailer, public_url: String) {
    loop {
        match send_due(&pool, &mailer, &public_url, OffsetDateTime::now_utc()).await {
            Ok(0) => {}
            Ok(sent) => tracing::info!(sent, "weekly letters sent"),
            Err(error) => tracing::warn!(%error, "weekly letters could not be gathered"),
        }
        tokio::time::sleep(LOOK_EVERY).await;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use time::{Month, PrimitiveDateTime, Time};

    use super::*;

    fn date(year: i32, month: u8, day: u8) -> Date {
        Date::from_calendar_date(year, Month::try_from(month).unwrap(), day).unwrap()
    }

    fn at(year: i32, month: u8, day: u8, hour: u8, minute: u8) -> OffsetDateTime {
        PrimitiveDateTime::new(date(year, month, day), Time::from_hms(hour, minute, 0).unwrap()).assume_utc()
    }

    /// A key with its plural suffix folded away.
    fn base_of(key: &str) -> &str {
        ["_one", "_few", "_many", "_other"]
            .iter()
            .find_map(|suffix| key.strip_suffix(suffix))
            .unwrap_or(key)
    }

    fn bases(locale: &Locale) -> BTreeSet<&str> {
        locale.strings.keys().map(|key| base_of(key)).collect()
    }

    fn placeholders(text: &str) -> BTreeSet<&str> {
        text.split("{{")
            .skip(1)
            .filter_map(|rest| rest.split_once("}}").map(|(name, _)| name))
            .collect()
    }

    #[test]
    fn every_language_says_everything_english_says_with_the_same_blanks() {
        let english = locale("en");
        for language in LANGUAGES {
            let other = locale(language);
            assert_eq!(bases(other), bases(english), "{language} has other keys than English");
            for (key, text) in &other.strings {
                let english_text = english
                    .strings
                    .get(key)
                    .or_else(|| english.strings.get(&format!("{}_other", base_of(key))))
                    .unwrap();
                assert_eq!(placeholders(text), placeholders(english_text), "{language}: {key}");
            }
        }
    }

    #[test]
    fn every_plural_form_a_language_needs_is_written() {
        for (language, forms) in [("en", vec!["one", "other"]), ("ru", vec!["one", "few", "many"])] {
            let words = locale(language);
            for base in bases(words).iter().filter(|base| words.strings.contains_key(&format!("{base}_other"))) {
                for form in &forms {
                    assert!(words.strings.contains_key(&format!("{base}_{form}")), "{language}: {base}_{form}");
                }
            }
        }
    }

    #[test]
    fn russian_counts_by_its_own_rule() {
        let ru = locale("ru");
        assert_eq!(ru.count("listens", 1), "1 прослушивание");
        assert_eq!(ru.count("listens", 3), "3 прослушивания");
        assert_eq!(ru.count("listens", 11), "11 прослушиваний");
        assert_eq!(ru.count("listens", 21), "21 прослушивание");
        assert_eq!(ru.count("listens", 1_214), "1\u{a0}214 прослушиваний");
        assert_eq!(locale("en").count("listens", 1_214), "1,214 listens");
        assert_eq!(locale("en").count("listens", 1), "1 listen");
    }

    #[test]
    fn a_week_is_named_across_a_month_end_too() {
        assert_eq!(locale("en").week(date(2026, 10, 5)), "5–11 October");
        assert_eq!(locale("en").week(date(2026, 9, 28)), "28 September – 4 October");
        assert_eq!(locale("ru").week(date(2026, 9, 28)), "28 сентября – 4 октября");
    }

    #[test]
    fn the_letter_is_about_the_last_whole_week() {
        // Tuesday 13 October 2026: the last week that has ended began on
        // Monday the 5th.
        assert_eq!(last_week(at(2026, 10, 13, 9, 0)), date(2026, 10, 5));
        // Monday, a minute past midnight: the week of the 5th has just ended.
        assert_eq!(last_week(at(2026, 10, 12, 0, 1)), date(2026, 10, 5));
        // Sunday night: it has not.
        assert_eq!(last_week(at(2026, 10, 11, 23, 59)), date(2026, 9, 28));
    }

    fn named(id: i32, name: &str) -> Named {
        Named { id, name: name.to_string() }
    }

    #[test]
    fn a_week_with_no_listening_sends_nothing() {
        assert_eq!(compose(locale("en"), &Week::default(), date(2026, 10, 5), "https://lyrid.example", "t"), None);
    }

    #[test]
    fn the_letter_names_what_opened_links_each_star_and_can_be_stopped() {
        let week = Week {
            listens: 214,
            minutes: 780,
            light: 1_320,
            opened: (1..=10).map(|id| named(id, &format!("Star {id}"))).collect(),
            loudest: vec!["Air".to_string(), "Portishead".to_string()],
            home: Some(named(42, "Massive Attack")),
            home_before: Some(named(7, "Portishead")),
        };
        let (subject, body) = compose(locale("en"), &week, date(2026, 10, 5), "https://lyrid.example", "secret").unwrap();
        assert_eq!(subject, "Your week in the sky: 5–11 October");
        assert!(body.contains("214 listens, about 13 hours, 1,320 light gathered."), "{body}");
        assert!(body.contains("10 stars opened for the first time:"), "{body}");
        assert!(body.contains("Star 1 — https://lyrid.example/star/1"), "{body}");
        assert!(!body.contains("Star 9 —"), "eight are named: {body}");
        assert!(body.contains("and 2 more"), "{body}");
        assert!(body.contains("Heard most: Air, Portishead."), "{body}");
        assert!(
            body.contains("moved from Portishead to Massive Attack: https://lyrid.example/star/42"),
            "{body}"
        );
        assert!(body.contains("https://lyrid.example/letter/stop?token=secret"), "{body}");
    }

    #[test]
    fn a_week_that_gathered_no_light_does_not_say_it_gathered_none() {
        let week = Week {
            listens: 3,
            minutes: 9,
            ..Week::default()
        };
        let (_, body) = compose(locale("ru"), &week, date(2026, 10, 5), "https://lyrid.example", "t").unwrap();
        assert!(body.contains("3 прослушивания, 9 минут."), "{body}");
        assert!(!body.contains("свет"), "{body}");
        assert!(body.contains("Новых звёзд на этой неделе нет"), "{body}");
    }

    // ------------------------------------------------------------ database

    async fn database() -> Option<PgPool> {
        let _ = dotenvy::dotenv();
        let Ok(url) = std::env::var("LYRID_TEST_DATABASE_URL") else {
            eprintln!("LYRID_TEST_DATABASE_URL is not set: the weekly letter was NOT checked against a database");
            return None;
        };
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("test database");
        sqlx::migrate!().run(&pool).await.expect("migrations apply");
        Some(pool)
    }

    /// A star of a sky made for the test: the newest layout inside the
    /// transaction, so it is the current one there.
    async fn star(tx: &mut PgConnection, layout: i16, name: &str, x: f32) -> uuid::Uuid {
        let mbid = uuid::Uuid::from_u128(rand::random());
        let id: i32 = rand::random_range(1_500_000_000..2_000_000_000);
        sqlx::query("INSERT INTO artist (id, mbid, name, sort_name) VALUES ($1, $2, $3, $3)")
            .bind(id)
            .bind(mbid)
            .bind(name)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO artist_position (layout_id, artist_id, x, y) VALUES ($1, $2, $3, 0)")
            .bind(layout)
            .bind(id)
            .bind(x)
            .execute(&mut *tx)
            .await
            .unwrap();
        mbid
    }

    /// How many letters for `user` the week `week` gives out before none is
    /// due.
    async fn taken(tx: &mut PgConnection, week: Date, user: i32) -> usize {
        let mut ours = 0;
        while let Some(due) = claim(&mut *tx, week).await.unwrap() {
            ours += usize::from(due.user_id == user);
        }
        ours
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn each_week_is_taken_once_and_tells_what_opened_in_it() {
        let Some(pool) = database().await else { return };
        let mut tx = pool.begin().await.unwrap();

        let user: i32 = sqlx::query_scalar("INSERT INTO app_user (email, password_hash, email_confirmed_at) VALUES ($1, 'x', now()) RETURNING id")
            .bind(format!("{}@letter.test", uuid::Uuid::from_u128(rand::random())))
            .fetch_one(&mut *tx)
            .await
            .unwrap();

        // Asked for on Tuesday 13 October: the week of the 5th is already
        // over, so the first letter is about the week of the 12th.
        subscribe(&mut *tx, user, "en", at(2026, 10, 13, 9, 0)).await.unwrap();
        assert_eq!(taken(&mut tx, date(2026, 10, 5), user).await, 0, "the week before the asking");
        assert_eq!(taken(&mut tx, date(2026, 10, 12), user).await, 1);
        assert_eq!(taken(&mut tx, date(2026, 10, 12), user).await, 0, "a week is taken once");

        // The week of the 12th: one star heard before it, one opened in it.
        let metric: i16 = sqlx::query_scalar("INSERT INTO similarity_metric (key, description) VALUES ($1, 'test') RETURNING id")
            .bind(format!("letter-{}", uuid::Uuid::from_u128(rand::random())))
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        let layout: i16 = sqlx::query_scalar("INSERT INTO sky_layout (key, metric_id, description, seed, stars) VALUES ($1, $2, 'test', 1, 0) RETURNING id")
            .bind(format!("letter-{}", uuid::Uuid::from_u128(rand::random())))
            .bind(metric)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        let old = star(&mut tx, layout, "Old Friend", 0.0).await;
        let new = star(&mut tx, layout, "New Light", 1.0).await;
        for (when, artist, light) in [
            (at(2026, 10, 1, 12, 0), old, Some(5)),
            (at(2026, 10, 13, 12, 0), old, Some(3)),
            (at(2026, 10, 14, 12, 0), new, Some(15)),
            (at(2026, 10, 18, 23, 59), new, None),
            (at(2026, 10, 19, 0, 0), new, Some(15)),
        ] {
            sqlx::query(
                "INSERT INTO listen (user_id, listened_at, recording, artist_mbids, seconds, light, source)
                 VALUES ($1, $2, $3, ARRAY[$4], 180, $5, 'listenbrainz')",
            )
            .bind(user)
            .bind(when)
            .bind(uuid::Uuid::from_u128(rand::random()))
            .bind(artist)
            .bind(light)
            .execute(&mut *tx)
            .await
            .unwrap();
        }

        let week = gather(&mut tx, user, date(2026, 10, 12)).await.unwrap();
        assert_eq!(week.listens, 3, "Monday to Sunday night, not the Monday after");
        assert_eq!(week.minutes, 9);
        assert_eq!(week.light, 18);
        assert_eq!(week.opened.iter().map(|star| star.name.as_str()).collect::<Vec<_>>(), vec!["New Light"]);
        assert_eq!(week.loudest, vec!["New Light".to_string(), "Old Friend".to_string()]);
        assert_eq!(week.home_before.map(|home| home.name), Some("Old Friend".to_string()));
        assert!(week.home.is_some());

        // Stopped by its link: no row, so no letter, and the account says so.
        let token: String = sqlx::query_scalar("SELECT token FROM weekly_letter WHERE user_id = $1")
            .bind(user)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        stop(&mut *tx, &token).await.unwrap();
        assert_eq!(taken(&mut tx, date(2026, 10, 19), user).await, 0);

        tx.rollback().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_letter_keeps_a_monday_and_leaves_with_the_account() {
        let Some(pool) = database().await else { return };
        let mut tx = pool.begin().await.unwrap();
        let user: i32 = sqlx::query_scalar("INSERT INTO app_user (email, password_hash) VALUES ($1, 'x') RETURNING id")
            .bind(format!("{}@letter.test", uuid::Uuid::from_u128(rand::random())))
            .fetch_one(&mut *tx)
            .await
            .unwrap();

        let mut savepoint = sqlx::Connection::begin(&mut *tx).await.unwrap();
        let tuesday = sqlx::query("INSERT INTO weekly_letter (user_id, language, sent_week, token) VALUES ($1, 'en', '2026-10-13', 't-tuesday')")
            .bind(user)
            .execute(&mut *savepoint)
            .await;
        assert!(tuesday.is_err(), "a week that does not begin on a Monday");
        savepoint.rollback().await.unwrap();

        subscribe(&mut *tx, user, "ru", at(2026, 10, 13, 9, 0)).await.unwrap();
        sqlx::query("DELETE FROM app_user WHERE id = $1").bind(user).execute(&mut *tx).await.unwrap();
        let left: i64 = sqlx::query_scalar("SELECT count(*) FROM weekly_letter WHERE user_id = $1")
            .bind(user)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(left, 0, "the letter outlived its account");
        tx.rollback().await.unwrap();
    }
}
