//! Streaming reader for the Discogs XML dumps.
//!
//! Each dump is one gzip stream holding one giant document: a root element
//! wrapping millions of records. Nothing may be buffered whole, so records are
//! yielded one at a time as the parser walks past them.
//!
//! The three files this reads are shaped differently on purpose, and the
//! differences are exactly where a careless parser goes quietly wrong:
//!
//! - `<artist>` and `<label>` carry their id as a **child element**, while
//!   `<master>` carries it as an **attribute**. Reading the wrong one yields
//!   zero ids without failing.
//! - Genres nest twice: `<genres><genre>Rock</genre></genres>`. Matching on
//!   `<genre>` alone would also match the container's siblings in `<styles>`.
//! - Inside `<members>`, an id appears **twice** — once as a bare `<id>` and
//!   again as an attribute on the following `<name>`. A reader that collects
//!   every `<id>` under an artist picks up its members' ids as its own.
//! - `<parentLabel>` is camelCase, alone among the element names.
//!
//! So this module does not offer a generic "find element" helper. It walks a
//! record's subtree with the path in hand, which is the only way the cases
//! above stay distinguishable.

use std::io::BufRead;

use anyhow::{Context, Result};
use quick_xml::Reader;
use quick_xml::events::Event;

/// Streams records out of one Discogs dump.
///
/// `T` is what a record turns into; the caller supplies the shape by way of a
/// [`Record`] implementation.
pub struct Records<R: BufRead, T> {
    reader: Reader<R>,
    buffer: Vec<u8>,
    /// Reused across records so a multi-million-record pass does not allocate
    /// a fresh text buffer per element.
    text: String,
    marker: std::marker::PhantomData<T>,
}

/// One kind of Discogs record: which element opens it, and how its subtree is
/// read.
pub trait Record: Sized {
    /// The element that opens one record: `artist`, `label`, `master`.
    const ELEMENT: &'static str;

    /// Builds the record from the opening tag's attributes. `<master>` keeps
    /// its id there; `<artist>` and `<label>` have none and ignore this.
    fn open(attributes: Attributes<'_>) -> Self;

    /// Called for each text-bearing element inside the record, with the path
    /// from the record element down to it (excluding the record element
    /// itself) and any attributes the element carries.
    ///
    /// A path rather than a name: `["genres", "genre"]` and
    /// `["styles", "style"]` are different facts, and `["members", "id"]` is
    /// somebody else's id.
    fn field(&mut self, path: &[&str], text: &str, attributes: Attributes<'_>);
}

/// The attributes of one element.
///
/// Only the few attributes these dumps actually use are ever asked for -- `id`
/// on nested names, `id` on `<master>` -- so this is a decoded slice rather
/// than a parser handle: an element's text and its attributes are reported
/// together at its closing tag, by which point the parser has moved on.
pub struct Attributes<'a> {
    pairs: &'a [(String, String)],
}

impl Attributes<'_> {
    /// The value of `name`, or `None` when absent.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.pairs.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }

    /// `name` parsed as `T`. A malformed number reads as absent: a dump is
    /// upstream data, and one bad value must not abort a ten-million-record
    /// pass.
    pub fn parse<T: std::str::FromStr>(&self, name: &str) -> Option<T> {
        self.get(name)?.parse().ok()
    }
}

impl<R: BufRead, T: Record> Records<R, T> {
    pub fn new(input: R) -> Self {
        let mut reader = Reader::from_reader(input);
        // The dumps are not always strictly well-formed -- profile text has
        // carried stray control characters and unescaped entities for years --
        // and a whole-file abort over one bad record would be the wrong trade.
        reader.config_mut().check_end_names = false;
        reader.config_mut().trim_text(false);
        Self {
            reader,
            buffer: Vec::with_capacity(1 << 16),
            text: String::new(),
            marker: std::marker::PhantomData,
        }
    }

    /// Reads the next record, or `Ok(None)` at end of document.
    pub fn next_record(&mut self) -> Result<Option<T>> {
        // Skip forward to the next record element.
        loop {
            self.buffer.clear();
            let event = match self.reader.read_event_into(&mut self.buffer) {
                Ok(event) => event,
                // As in `read_subtree`: a truncated document ends the pass
                // rather than failing it.
                Err(quick_xml::Error::Syntax(_)) => return Ok(None),
                Err(error) => return Err(error).context("failed to read the dump"),
            };
            match event {
                Event::Eof => return Ok(None),
                Event::Start(start) if start.name().as_ref() == T::ELEMENT => {
                    let attributes = collect_attributes(&start);
                    let mut record = T::open(Attributes { pairs: &attributes });
                    self.read_subtree(&mut record)?;
                    return Ok(Some(record));
                }
                // A record with no children at all: `<artist/>` appears in the
                // dumps where an entry has been emptied.
                Event::Empty(empty) if empty.name().as_ref() == T::ELEMENT => {
                    let attributes = collect_attributes(&empty);
                    return Ok(Some(T::open(Attributes { pairs: &attributes })));
                }
                _ => {}
            }
        }
    }

    /// Walks one record's subtree, reporting every text-bearing element with
    /// its path.
    fn read_subtree(&mut self, record: &mut T) -> Result<()> {
        // Owned rather than borrowed: the event buffer is reused every step,
        // so a path of `&str` into it would dangle on the next read.
        let mut path: Vec<String> = Vec::with_capacity(4);
        // The attributes of the element currently open, kept alongside its
        // text until the closing tag reports both together.
        let mut open_attributes: Vec<Vec<(String, String)>> = Vec::with_capacity(4);
        // The deepest path length that has reported a field since the element
        // now open was opened. Greater than the current depth means something
        // inside reported, which makes the element a container.
        let mut depth_reported = 0usize;
        self.text.clear();

        loop {
            self.buffer.clear();
            // A document cut mid-tag reports a syntax error rather than EOF.
            // That is the shape of a truncated download, and the millions of
            // records already read are still good, so the pass ends here
            // instead of throwing them away.
            let event = match self.reader.read_event_into(&mut self.buffer) {
                Ok(event) => event,
                Err(quick_xml::Error::Syntax(_)) => return Ok(()),
                Err(error) => return Err(error).context("failed to read the dump"),
            };
            match event {
                Event::Start(start) => {
                    path.push(start.name().as_ref().to_owned());
                    open_attributes.push(collect_attributes(&start));
                    self.text.clear();
                }
                Event::Empty(empty) => {
                    // A self-closing element has attributes but no text --
                    // `<image .../>` and, in emptied records, `<name/>`.
                    let name = empty.name().as_ref().to_owned();
                    path.push(name);
                    let attributes = collect_attributes(&empty);
                    report(record, &path, "", &attributes);
                    depth_reported = path.len();
                    path.pop();
                }
                Event::Text(text) => {
                    // Text arrives in pieces: an entity in the middle of a
                    // value splits it, so this accumulates rather than
                    // replaces.
                    self.text.push_str(&text.into_inner());
                }
                // An entity reference is its own event, not part of the text
                // around it. Ignoring it silently deletes a character from
                // every value that has one -- "AC&DC" becomes "ACDC" and
                // "Sigur Rós" becomes "Sigur Rs" -- and these dumps are full
                // of both: `&amp;` in band names, `&#243;` in non-ASCII ones.
                Event::GeneralRef(reference) => {
                    self.text.push_str(&resolve_entity(reference.as_ref()));
                }
                Event::CData(data) => {
                    self.text.push_str(&data.into_inner());
                }
                Event::End(end) => {
                    if end.name().as_ref() == T::ELEMENT && path.is_empty() {
                        return Ok(());
                    }
                    let attributes = open_attributes.pop().unwrap_or_default();
                    if !path.is_empty() {
                        // A container -- `<members>`, `<genres>` -- holds
                        // elements rather than a value, and reporting it as an
                        // empty field would put a meaningless fact next to the
                        // real ones. It is recognised by having had a child:
                        // its close comes when something was already reported
                        // deeper than it.
                        let is_container = depth_reported > path.len();
                        if !is_container {
                            report(record, &path, self.text.trim(), &attributes);
                        }
                        path.pop();
                        // This element's close is itself "something reported
                        // deeper" as far as its parent is concerned, so the
                        // parent will be recognised as a container too.
                        depth_reported = path.len() + 1;
                    }
                    self.text.clear();
                }
                // Truncated document: the last record is incomplete rather
                // than the file being unreadable, so the records already read
                // stand.
                Event::Eof => return Ok(()),
                _ => {}
            }
        }
    }
}

/// Turns an entity reference's name into the text it stands for.
///
/// The reference arrives as what sat between `&` and `;`: `amp`, `#243`,
/// `#x1F600`. An unknown name is kept in its original form rather than
/// dropped -- Discogs profiles carry HTML entities the XML spec does not
/// define, and a visible `&hellip;` is better than a silent deletion.
fn resolve_entity(name: &str) -> String {
    if let Some(digits) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
        return u32::from_str_radix(digits, 16)
            .ok()
            .and_then(char::from_u32)
            .map_or_else(|| format!("&{name};"), String::from);
    }
    if let Some(digits) = name.strip_prefix('#') {
        return digits
            .parse::<u32>()
            .ok()
            .and_then(char::from_u32)
            .map_or_else(|| format!("&{name};"), String::from);
    }
    match name {
        "amp" => "&".to_string(),
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "quot" => "\"".to_string(),
        "apos" => "'".to_string(),
        other => format!("&{other};"),
    }
}

fn collect_attributes(start: &quick_xml::events::BytesStart<'_>) -> Vec<(String, String)> {
    start
        .attributes()
        .with_checks(false)
        .filter_map(Result::ok)
        .map(|attribute| (attribute.key.as_ref().to_owned(), attribute.value.into_owned()))
        .collect()
}

/// Hands one element to the record, with its path as `&str` slices.
fn report<T: Record>(record: &mut T, path: &[String], text: &str, attributes: &[(String, String)]) {
    let borrowed: Vec<&str> = path.iter().map(String::as_str).collect();
    record.field(&borrowed, text, Attributes { pairs: attributes });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record that simply records what it was told, so the walker's own
    /// behaviour is what gets asserted.
    struct Spy {
        fields: Vec<(String, String, Option<String>)>,
    }

    impl Record for Spy {
        const ELEMENT: &'static str = "artist";

        fn open(_: Attributes<'_>) -> Self {
            Self { fields: Vec::new() }
        }

        fn field(&mut self, path: &[&str], text: &str, attributes: Attributes<'_>) {
            self.fields.push((path.join("/"), text.to_string(), attributes.get("id").map(str::to_string)));
        }
    }

    fn read(xml: &str) -> Vec<Spy> {
        let mut records = Records::<_, Spy>::new(xml.as_bytes());
        let mut out = Vec::new();
        while let Some(record) = records.next_record().unwrap() {
            out.push(record);
        }
        out
    }

    fn paths(record: &Spy) -> Vec<(&str, &str)> {
        record.fields.iter().map(|(p, t, _)| (p.as_str(), t.as_str())).collect()
    }

    #[test]
    fn reads_child_elements_with_their_path() {
        let records = read("<artists><artist><id>1</id><name>The Persuader</name></artist></artists>");
        assert_eq!(records.len(), 1);
        assert_eq!(paths(&records[0]), vec![("id", "1"), ("name", "The Persuader")]);
    }

    #[test]
    fn distinguishes_genres_from_styles_by_path() {
        // Both are a `<genre>`-shaped leaf; only the container tells them
        // apart, which is why the path matters.
        let records = read("<artists><artist><genres><genre>Electronic</genre></genres><styles><style>Techno</style></styles></artist></artists>");
        assert_eq!(paths(&records[0]), vec![("genres/genre", "Electronic"), ("styles/style", "Techno")]);
    }

    #[test]
    fn keeps_member_ids_distinct_from_the_records_own_id() {
        // `<members>` repeats a bare `<id>` for each member. Collecting every
        // `<id>` under an artist would make a member's id the artist's.
        let records = read("<artists><artist><id>2</id><members><id>26</id><name id=\"26\">Alexi Delano</name></members></artist></artists>");
        assert_eq!(paths(&records[0]), vec![("id", "2"), ("members/id", "26"), ("members/name", "Alexi Delano")]);
    }

    #[test]
    fn reads_the_id_attribute_on_nested_names() {
        let records = read("<artists><artist><aliases><name id=\"239\">Jesper Dahlbäck</name></aliases></artist></artists>");
        let alias = records[0].fields.iter().find(|(p, _, _)| p == "aliases/name").unwrap();
        assert_eq!(alias.2.as_deref(), Some("239"));
        assert_eq!(alias.1, "Jesper Dahlbäck");
    }

    #[test]
    fn reads_an_id_carried_as_an_attribute_on_the_record() {
        // This is `<master id="...">`, the one record shaped that way.
        struct Master {
            id: Option<i32>,
        }
        impl Record for Master {
            const ELEMENT: &'static str = "master";
            fn open(attributes: Attributes<'_>) -> Self {
                Self { id: attributes.parse("id") }
            }
            fn field(&mut self, _: &[&str], _: &str, _: Attributes<'_>) {}
        }

        let mut records = Records::<_, Master>::new("<masters><master id=\"18500\"><title>New Soil</title></master></masters>".as_bytes());
        let record = records.next_record().unwrap().unwrap();
        assert_eq!(record.id, Some(18500));
    }

    #[test]
    fn yields_every_record_in_sequence() {
        let records = read("<artists><artist><id>1</id></artist><artist><id>2</id></artist><artist><id>3</id></artist></artists>");
        let ids: Vec<&str> = records.iter().map(|r| r.fields[0].1.as_str()).collect();
        assert_eq!(ids, vec!["1", "2", "3"]);
    }

    #[test]
    fn preserves_non_ascii_and_entities() {
        let records = read("<artists><artist><name>Sigur R&#243;s</name><realname>AC&amp;DC</realname></artist></artists>");
        assert_eq!(paths(&records[0]), vec![("name", "Sigur Rós"), ("realname", "AC&DC")]);
    }

    #[test]
    fn survives_an_empty_record() {
        let records = read("<artists><artist/><artist><id>2</id></artist></artists>");
        assert_eq!(records.len(), 2);
        assert!(records[0].fields.is_empty());
    }

    #[test]
    fn survives_a_truncated_document() {
        // A download cut short must not lose the records already parsed.
        let records = read("<artists><artist><id>1</id></artist><artist><id>2</id");
        assert_eq!(records.len(), 2);
        assert_eq!(paths(&records[0]), vec![("id", "1")]);
    }

    #[test]
    fn reports_self_closing_elements_without_text() {
        let records = read("<artists><artist><images><image height=\"450\" id=\"7\" uri=\"\"/></images></artist></artists>");
        let image = records[0].fields.iter().find(|(p, _, _)| p == "images/image").unwrap();
        assert_eq!(image.1, "");
        assert_eq!(image.2.as_deref(), Some("7"));
    }

    #[test]
    fn keeps_nested_paths_separate_across_siblings() {
        // `parentLabel` is camelCase and sits beside `sublabels/label`; both
        // are label-shaped leaves and must not collapse into one fact.
        let records =
            read("<artists><artist><parentLabel id=\"4711\">Goldhead</parentLabel><sublabels><label id=\"2437\">Birdy</label></sublabels></artist></artists>");
        assert_eq!(paths(&records[0]), vec![("parentLabel", "Goldhead"), ("sublabels/label", "Birdy")]);
    }
}
