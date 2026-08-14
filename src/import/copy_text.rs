//! Reader for `PostgreSQL`'s COPY TEXT format, which is what `MusicBrainz` dump
//! files are: one table per file, tab-separated, no header, `\N` for NULL.
//!
//! The format is not TSV with extra steps -- a tab or newline inside a value
//! is escaped rather than quoted, so splitting on tabs without unescaping
//! silently corrupts every value that contains one. Artist names contain both.

use std::io::{self, BufRead};

/// One parsed row, holding its own decoded bytes.
///
/// Fields are `None` for `\N`. Values are unescaped, so a caller never sees
/// the backslash sequences.
pub struct Row {
    /// Byte ranges into `decoded`, or `None` for NULL.
    fields: Vec<Option<(usize, usize)>>,
    decoded: Vec<u8>,
}

impl Row {
    /// The field at `index`, or `None` when it is NULL or beyond the row.
    ///
    /// Invalid UTF-8 also reads as `None`: the dump is UTF-8, and a byte
    /// sequence that is not should be skipped rather than abort the import.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&str> {
        let (start, end) = (*self.fields.get(index)?)?;
        std::str::from_utf8(&self.decoded[start..end]).ok()
    }

    /// The field at `index` parsed as `T`, or `None` when NULL, absent or
    /// unparseable. Malformed numbers are treated as missing: a dump is
    /// upstream data, and one broken value should not abort a 3M-row import.
    pub fn parse<T: std::str::FromStr>(&self, index: usize) -> Option<T> {
        self.get(index)?.parse().ok()
    }

    /// How many fields the row has. Used by tests and by callers that want to
    /// check a dump's column count before trusting fixed indices.
    #[must_use]
    #[cfg_attr(not(test), expect(dead_code, reason = "part of the reader's API; only tests need it so far"))]
    pub fn len(&self) -> usize {
        self.fields.len()
    }
}

/// Streams rows out of a COPY TEXT file.
pub struct Reader<R> {
    input: R,
    line: Vec<u8>,
}

impl<R: BufRead> Reader<R> {
    pub fn new(input: R) -> Self {
        Self { input, line: Vec::new() }
    }

    /// Reads the next row, or `Ok(None)` at end of file.
    ///
    /// A value may contain an escaped newline (`\n`) but never a literal one,
    /// so one line is always exactly one row.
    pub fn next_row(&mut self) -> io::Result<Option<Row>> {
        self.line.clear();
        if self.input.read_until(b'\n', &mut self.line)? == 0 {
            return Ok(None);
        }

        // Trailing CR appears when a dump has been through a Windows tool.
        let mut line = self.line.as_slice();
        if line.last() == Some(&b'\n') {
            line = &line[..line.len() - 1];
        }
        if line.last() == Some(&b'\r') {
            line = &line[..line.len() - 1];
        }

        // A lone `\.` terminates a COPY stream. Dump files do not carry it,
        // but a file pasted out of psql does.
        if line == b"\\." {
            return Ok(None);
        }

        Ok(Some(parse_line(line)))
    }
}

/// Splits a line on unescaped tabs and unescapes each field.
///
/// Decoding happens at the byte level: escapes and delimiters are all ASCII,
/// and multi-byte characters pass through untouched, so the result is valid
/// UTF-8 whenever the input was.
fn parse_line(line: &[u8]) -> Row {
    let mut decoded = Vec::with_capacity(line.len());
    let mut fields = Vec::new();
    let mut start = 0usize;
    let mut is_null = false;
    let mut field_len = 0usize;
    let mut bytes = line.iter().copied().peekable();

    while let Some(byte) = bytes.next() {
        match byte {
            b'\t' => {
                fields.push(if is_null { None } else { Some((start, decoded.len())) });
                start = decoded.len();
                is_null = false;
                field_len = 0;
            }
            b'\\' => {
                let escaped = bytes.next();
                match escaped {
                    // `\N` is NULL, but only as the whole field: a value may
                    // legitimately contain a literal N elsewhere.
                    Some(b'N') if field_len == 0 && matches!(bytes.peek(), None | Some(b'\t')) => is_null = true,
                    Some(b'n') => decoded.push(b'\n'),
                    Some(b't') => decoded.push(b'\t'),
                    Some(b'r') => decoded.push(b'\r'),
                    Some(b'b') => decoded.push(0x08),
                    Some(b'f') => decoded.push(0x0c),
                    Some(b'v') => decoded.push(0x0b),
                    // `\\` is a literal backslash; any other escaped byte
                    // stands for itself, as Postgres itself treats it on input.
                    Some(other) => decoded.push(other),
                    // A trailing backslash is malformed; keep the byte rather
                    // than lose it.
                    None => decoded.push(b'\\'),
                }
                field_len += 1;
            }
            other => {
                decoded.push(other);
                field_len += 1;
            }
        }
    }

    fields.push(if is_null { None } else { Some((start, decoded.len())) });
    Row { fields, decoded }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(input: &str) -> Vec<Vec<Option<String>>> {
        let mut reader = Reader::new(input.as_bytes());
        let mut out = Vec::new();
        while let Some(row) = reader.next_row().unwrap() {
            out.push((0..row.len()).map(|i| row.get(i).map(str::to_string)).collect());
        }
        out
    }

    #[test]
    fn splits_plain_fields() {
        assert_eq!(
            rows("1\tNirvana\tGroup\n"),
            vec![vec![Some("1".into()), Some("Nirvana".into()), Some("Group".into())]]
        );
    }

    #[test]
    fn reads_null_as_none() {
        assert_eq!(rows("1\t\\N\tx\n"), vec![vec![Some("1".into()), None, Some("x".into())]]);
    }

    #[test]
    fn unescapes_tabs_and_newlines_inside_values() {
        // Without unescaping, this row would split into four fields and the
        // artist name would be truncated.
        let parsed = rows("1\ta\\tb\\nc\n");
        assert_eq!(parsed, vec![vec![Some("1".into()), Some("a\tb\nc".into())]]);
    }

    #[test]
    fn unescapes_backslashes() {
        assert_eq!(rows("AC\\\\DC\n"), vec![vec![Some("AC\\DC".into())]]);
    }

    #[test]
    fn keeps_values_that_merely_start_with_n() {
        // `\N` means NULL only as a whole field; "Nirvana" must survive.
        assert_eq!(rows("Nirvana\tN\n"), vec![vec![Some("Nirvana".into()), Some("N".into())]]);
    }

    #[test]
    fn keeps_empty_string_distinct_from_null() {
        assert_eq!(rows("\t\\N\n"), vec![vec![Some(String::new()), None]]);
    }

    #[test]
    fn preserves_non_ascii() {
        assert_eq!(
            rows("Гражданская оборона\tSigur Rós\n"),
            vec![vec![Some("Гражданская оборона".into()), Some("Sigur Rós".into())]]
        );
    }

    #[test]
    fn handles_crlf_line_endings() {
        assert_eq!(rows("a\tb\r\n"), vec![vec![Some("a".into()), Some("b".into())]]);
    }

    #[test]
    fn stops_at_the_copy_terminator() {
        assert_eq!(rows("a\n\\.\nb\n"), vec![vec![Some("a".into())]]);
    }

    #[test]
    fn parses_numbers_and_ignores_malformed_ones() {
        let mut reader = Reader::new("42\tnot-a-number\t\\N\n".as_bytes());
        let row = reader.next_row().unwrap().unwrap();
        assert_eq!(row.parse::<i32>(0), Some(42));
        assert_eq!(row.parse::<i32>(1), None);
        assert_eq!(row.parse::<i32>(2), None);
        assert_eq!(row.parse::<i32>(9), None);
    }
}
