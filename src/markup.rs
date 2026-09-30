//! The markup Discogs writes its descriptions in.
//!
//! A label's profile is prose with bracketed references in it: `[a=Carl
//! Craig]` names an artist, `[a674]` points at one by id, `[l=Seasons
//! Recordings]` and `[l123]` do the same for labels, `[r…]` and `[m…]` for a
//! release and a master, `[url=…]text[/url]` is a link, and `[b]`, `[i]`, `[u]`
//! are formatting. Measured over the 20260801 labels file: 81k id-only artist
//! references, 71k by name, 32k of each kind for labels, and a long tail of
//! the rest.
//!
//! Prose also carries square brackets of its own -- `[sic]`, `[also known
//! as …]`, a catalogue number in brackets -- so anything this module does not
//! recognise as a tag stays text, character for character. A reader must never
//! lose words to a parser that guessed.
//!
//! The same grammar serves twice. The importer resolves id-only references
//! into `[a674=Stephan Grieder]` -- this project's own form, an id and a name
//! together -- while every dump file is open anyway, because an id alone
//! cannot be shown and the names live in files nobody wants to read twice.
//! The API then turns the stored text into pieces a page can render, linking
//! what the canon can follow.

use std::fmt::Write as _;

/// What a reference points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Artist,
    Label,
    Release,
    Master,
}

impl Kind {
    fn from_letter(letter: u8) -> Option<Self> {
        match letter {
            b'a' => Some(Self::Artist),
            b'l' => Some(Self::Label),
            b'r' => Some(Self::Release),
            b'm' => Some(Self::Master),
            _ => None,
        }
    }

    fn letter(self) -> char {
        match self {
            Self::Artist => 'a',
            Self::Label => 'l',
            Self::Release => 'r',
            Self::Master => 'm',
        }
    }
}

/// One piece of a profile, in reading order.
#[derive(Debug, PartialEq, Eq)]
pub enum Piece<'a> {
    Text(&'a str),
    /// A reference, with whatever it carried: an id, a name, or both. Neither
    /// is the broken `[lnull]` the dump holds fifty of, and reads as nothing.
    Ref {
        kind: Kind,
        id: Option<i32>,
        name: Option<&'a str>,
    },
    /// A link and the words it was written on.
    Link {
        url: &'a str,
        text: &'a str,
    },
}

/// A tag recognised at the start of a slice, and how many bytes it spans.
#[derive(Debug, PartialEq, Eq)]
enum Tag<'a> {
    /// `[b]`, `[/i]` and the like: carries no words, so it is dropped.
    Format,
    Ref {
        kind: Kind,
        id: Option<i32>,
        name: Option<&'a str>,
    },
    /// `[u=name]`, a Discogs user. Shown as the name.
    User(&'a str),
    /// `[url=…]` or, with `None`, a bare `[url]` whose text is the address.
    UrlOpen(Option<&'a str>),
    UrlClose,
}

/// Reads the tag at the start of `s`, which begins with `[`.
fn tag_at(s: &str) -> Option<(Tag<'_>, usize)> {
    let end = s.find(']')?;
    let inner = &s[1..end];
    let len = end + 1;
    let tag = match inner {
        "b" | "/b" | "i" | "/i" | "u" | "/u" => Tag::Format,
        "url" => Tag::UrlOpen(None),
        "/url" => Tag::UrlClose,
        _ => {
            if let Some(url) = inner.strip_prefix("url=") {
                if url.is_empty() {
                    return None;
                }
                Tag::UrlOpen(Some(url))
            } else if let Some(name) = inner.strip_prefix("u=") {
                if name.trim().is_empty() {
                    return None;
                }
                Tag::User(name)
            } else {
                reference(inner)?
            }
        }
    };
    Some((tag, len))
}

/// `a=Name`, `a674`, `a674=Name`, `lnull` -- or nothing, for anything else.
fn reference(inner: &str) -> Option<Tag<'_>> {
    let kind = Kind::from_letter(*inner.as_bytes().first()?)?;
    let rest = &inner[1..];
    if rest == "null" {
        return Some(Tag::Ref { kind, id: None, name: None });
    }
    let (digits, name) = match rest.split_once('=') {
        Some((digits, name)) => (digits, Some(name)),
        None => (rest, None),
    };
    let name = match name {
        // `[a=]` is not a reference to anyone.
        Some(name) if name.trim().is_empty() => return None,
        other => other,
    };
    let id = if digits.is_empty() {
        None
    } else if digits.bytes().all(|b| b.is_ascii_digit()) {
        Some(digits.parse().ok()?)
    } else {
        // `[also known as]` begins with an `a` and is prose.
        return None;
    };
    if id.is_none() && name.is_none() {
        return None;
    }
    Some(Tag::Ref { kind, id, name })
}

/// Splits a profile into pieces, in order.
///
/// Adjacent text is not merged: a piece boundary where a dropped `[b]` stood
/// costs a renderer nothing, and merging would mean allocating.
#[must_use]
pub fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut plain_from = 0;
    let mut at = 0;
    while let Some(offset) = text[at..].find('[') {
        let start = at + offset;
        let Some((tag, len)) = tag_at(&text[start..]) else {
            // Not a tag: the bracket is prose, and scanning resumes after it.
            at = start + 1;
            continue;
        };
        if plain_from < start {
            out.push(Piece::Text(&text[plain_from..start]));
        }
        let mut next = start + len;
        match tag {
            Tag::Format | Tag::UrlClose => {}
            Tag::Ref { kind, id, name } => out.push(Piece::Ref { kind, id, name }),
            Tag::User(name) => out.push(Piece::Text(name)),
            Tag::UrlOpen(address) => {
                // The link runs to its closing tag. Without one, the opening
                // tag is dropped and what follows it is read as ordinary text.
                if let Some(close) = text[next..].find("[/url]") {
                    let words = &text[next..next + close];
                    let url = address.unwrap_or(words);
                    out.push(Piece::Link {
                        url: url.trim(),
                        text: if words.trim().is_empty() { url.trim() } else { words },
                    });
                    next += close + "[/url]".len();
                }
            }
        }
        plain_from = next;
        at = next;
    }
    if plain_from < text.len() {
        out.push(Piece::Text(&text[plain_from..]));
    }
    out
}

/// The id-only references in a profile: the ones that need a name looked up.
pub fn unnamed(text: &str) -> impl Iterator<Item = (Kind, i32)> + '_ {
    pieces(text).into_iter().filter_map(|piece| match piece {
        Piece::Ref {
            kind,
            id: Some(id),
            name: None,
        } => Some((kind, id)),
        _ => None,
    })
}

/// Rewrites id-only references to carry their names, `[a674]` becoming
/// `[a674=Stephan Grieder]`, and leaves every other byte as it was.
///
/// A reference whose name cannot be found stays as it is: the id is still
/// true, and a later import with the missing file can still name it.
pub fn named<'a>(text: &str, name_of: impl Fn(Kind, i32) -> Option<&'a str>) -> String {
    let mut out = String::with_capacity(text.len() + 32);
    let mut plain_from = 0;
    let mut at = 0;
    while let Some(offset) = text[at..].find('[') {
        let start = at + offset;
        let Some((tag, len)) = tag_at(&text[start..]) else {
            at = start + 1;
            continue;
        };
        if let Tag::Ref {
            kind,
            id: Some(id),
            name: None,
        } = tag
            && let Some(name) = name_of(kind, id)
        {
            out.push_str(&text[plain_from..start]);
            // A bracket inside the name would end the tag early.
            let name: String = name.chars().filter(|c| *c != '[' && *c != ']').collect();
            // Writing into a String cannot fail.
            let _ = write!(out, "[{}{id}={name}]", kind.letter());
            plain_from = start + len;
        }
        at = start + len;
    }
    out.push_str(&text[plain_from..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_references_by_name_by_id_and_by_both() {
        assert_eq!(
            pieces("[a=Carl Craig]'s label"),
            vec![
                Piece::Ref {
                    kind: Kind::Artist,
                    id: None,
                    name: Some("Carl Craig")
                },
                Piece::Text("'s label"),
            ]
        );
        assert_eq!(
            pieces("started by [a674] in 1996"),
            vec![
                Piece::Text("started by "),
                Piece::Ref {
                    kind: Kind::Artist,
                    id: Some(674),
                    name: None
                },
                Piece::Text(" in 1996"),
            ]
        );
        assert_eq!(
            pieces("[l3=Seasons Recordings]"),
            vec![Piece::Ref {
                kind: Kind::Label,
                id: Some(3),
                name: Some("Seasons Recordings")
            }]
        );
    }

    #[test]
    fn keeps_brackets_that_are_prose() {
        // Every one of these is in the dump, and every one is words.
        for prose in ["[sic]", "[also known as Planet E]", "[g123]", "[a=]", "[url=]x", "[aka Foo]", "[l1x]", "a [ b"] {
            let got = pieces(prose);
            let text: String = got
                .iter()
                .map(|piece| match piece {
                    Piece::Text(text) => *text,
                    other => panic!("{prose:?} produced {other:?}"),
                })
                .collect();
            assert_eq!(text, prose, "{prose:?} lost characters");
        }
    }

    #[test]
    fn drops_formatting_and_keeps_its_words() {
        assert_eq!(pieces("Labelcode [b]LC 6269[/b]"), vec![Piece::Text("Labelcode "), Piece::Text("LC 6269")]);
    }

    #[test]
    fn reads_a_link_in_both_of_its_forms() {
        assert_eq!(
            pieces("an [url=http://x.example/i]interview[/url] with"),
            vec![
                Piece::Text("an "),
                Piece::Link {
                    url: "http://x.example/i",
                    text: "interview"
                },
                Piece::Text(" with"),
            ]
        );
        assert_eq!(
            pieces("[url]https://y.example[/url]"),
            vec![Piece::Link {
                url: "https://y.example",
                text: "https://y.example"
            }]
        );
    }

    #[test]
    fn an_unclosed_link_leaves_its_words_as_text() {
        assert_eq!(pieces("see [url=http://x.example]here"), vec![Piece::Text("see "), Piece::Text("here")]);
    }

    #[test]
    fn a_null_reference_is_nothing() {
        assert_eq!(
            pieces("on [lnull]."),
            vec![
                Piece::Text("on "),
                Piece::Ref {
                    kind: Kind::Label,
                    id: None,
                    name: None
                },
                Piece::Text("."),
            ]
        );
    }

    #[test]
    fn a_user_reads_as_their_name() {
        assert_eq!(pieces("by [u=someone]"), vec![Piece::Text("by "), Piece::Text("someone")]);
    }

    #[test]
    fn lists_only_the_references_that_need_a_name() {
        let found: Vec<(Kind, i32)> = unnamed("[a674] and [a=Name] and [l5=Svek] and [m12] and [lnull]").collect();
        assert_eq!(found, vec![(Kind::Artist, 674), (Kind::Master, 12)]);
    }

    #[test]
    fn names_what_it_can_and_leaves_every_other_byte_alone() {
        let text = "Started by [a674] in 1996 [sic], run with [a239] and [a=Pete]; see [l5] and [b]LC 1[/b].";
        let resolved = named(text, |kind, id| match (kind, id) {
            (Kind::Artist, 674) => Some("Stephan Grieder"),
            (Kind::Label, 5) => Some("Svek [SE]"),
            _ => None,
        });
        assert_eq!(
            resolved,
            "Started by [a674=Stephan Grieder] in 1996 [sic], run with [a239] and [a=Pete]; see [l5=Svek SE] and [b]LC 1[/b]."
        );
        // And the resolved form reads back as what it says.
        assert!(pieces(&resolved).contains(&Piece::Ref {
            kind: Kind::Artist,
            id: Some(674),
            name: Some("Stephan Grieder")
        }));
    }

    #[test]
    fn naming_twice_changes_nothing_the_second_time() {
        let once = named("[a1]", |_, _| Some("One"));
        assert_eq!(named(&once, |_, _| Some("Other")), once);
    }
}
