//! Turns a Wikipedia article's wikitext into the plain lead paragraphs.
//!
//! There is no dump of ready-made extracts any more -- the Enterprise HTML
//! mirror stopped replicating in March 2025 and the abstracts dump is gone --
//! so the lead is parsed out of the article source here.
//!
//! The shapes this has to survive were measured against 867 real articles
//! rather than guessed:
//!
//! - **97% carry wikilinks**, in both `[[Target]]` and `[[Target|words]]` form.
//! - **64% carry `<ref>` footnotes**, most with templates nested inside them.
//! - **13% carry nested templates** -- `{{a|{{b|c}}}}`. This is why templates
//!   are removed by counting braces rather than by a regular expression: a
//!   pattern that stops at the first `}}` truncates one lead in eight.
//! - **11% open with a parenthesis holding only pronunciation**, so removing
//!   the pronunciation templates leaves `Name (; born 1982)` unless the
//!   punctuation left behind is cleaned up.
//! - **5% have an HTML comment in the middle of the first sentence** -- editors
//!   arguing about "was" versus "were".
//!
//! Not every template may be deleted. `{{spaced ndash}}` between two years and
//! `{{convert|103.9|sqmi|sqkm}}` are part of the sentence; dropping them makes
//! prose that reads as broken rather than as shortened.

/// The lead paragraphs of one article, or `None` when the article has no
/// usable prose (a redirect, a stub of nothing but templates).
#[must_use]
pub fn lead(wikitext: &str) -> Option<String> {
    // A redirect is a pointer, not an article.
    //
    // Compared as bytes rather than by slicing the string: an article opening
    // with a multi-byte character puts no boundary at byte 9, and `&s[..9]`
    // panics there. Found on the real dump twice, at two different fixed
    // widths -- so the rule here is that a fixed byte count never indexes a
    // str, only a &[u8].
    let trimmed = wikitext.trim_start();
    if trimmed.len() >= 9 && trimmed.as_bytes()[..9].eq_ignore_ascii_case(b"#redirect") {
        return None;
    }

    // The lead is everything before the first section heading.
    let body = match find_heading(wikitext) {
        Some(at) => &wikitext[..at],
        None => wikitext,
    };

    let cleaned = clean(body);
    let lead = first_paragraphs(&cleaned);
    (!lead.is_empty()).then_some(lead)
}

/// Where the first section heading starts (`== History ==` at line start).
fn find_heading(text: &str) -> Option<usize> {
    let mut at = 0usize;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with("==") {
            return Some(at);
        }
        at += line.len();
    }
    None
}

/// Strips wikitext markup down to plain prose.
fn clean(text: &str) -> String {
    let without_comments = remove_html_comments(text);
    let without_refs = remove_refs(&without_comments);
    let without_templates = remove_templates(&without_refs);
    let without_tags = remove_html_tags(&without_templates);
    let without_files = remove_file_links(&without_tags);
    let flattened = flatten_wikilinks(&without_files);
    let unformatted = remove_formatting(&flattened);
    let decoded = decode_entities(&unformatted);
    tidy(&decoded)
}

/// Removes `<!-- … -->`, which appears mid-sentence in 5% of leads.
fn remove_html_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            // An unclosed comment swallows the rest of the article rather than
            // leaking markup into the prose.
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Removes `<ref>…</ref>` and self-closing `<ref … />`.
fn remove_refs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<ref") {
        // `<references>` is a different tag that happens to share the prefix.
        let after = &rest[start + 4..];
        if after.starts_with("erences") {
            let cut = start + 4;
            out.push_str(&rest[..cut]);
            rest = &rest[cut..];
            continue;
        }

        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        // Self-closing (`<ref name="x" />`) ends at the first `>`.
        let Some(open_end) = tail.find('>') else { return out };
        if tail[..open_end].ends_with('/') {
            rest = &tail[open_end + 1..];
            continue;
        }
        match tail.find("</ref>") {
            Some(close) => rest = &tail[close + 6..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Removes remaining HTML tags, keeping the text between them.
fn remove_html_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0usize;
    for ch in text.chars() {
        match ch {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

/// Removes `[[File:…]]` and `[[Image:…]]`, which nest brackets for captions.
fn remove_file_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"[[") {
            // Compared as bytes: a link target may open with a multi-byte
            // character, and slicing a string by a byte length that lands
            // inside one panics.
            let inner = &bytes[i + 2..];
            let is_media = [b"file:".as_slice(), b"image:".as_slice(), b"media:".as_slice()]
                .iter()
                .any(|prefix| inner.len() >= prefix.len() && inner[..prefix.len()].eq_ignore_ascii_case(prefix));
            if is_media {
                match matching_bracket(text, i) {
                    Some(end) => {
                        i = end;
                        continue;
                    }
                    None => break,
                }
            }
        }
        let ch = text[i..].chars().next().unwrap_or('\0');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// The index just past the `]]` matching the `[[` at `start`.
fn matching_bracket(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = start;
    while i + 1 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            depth += 1;
            i += 2;
        } else if bytes[i] == b']' && bytes[i + 1] == b']' {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return Some(i);
            }
        } else {
            i += 1;
        }
    }
    None
}

/// `[[Target]]` -> `Target`, `[[Target|words]]` -> `words`.
fn flatten_wikilinks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find("]]") else {
            // Unbalanced markup: keep what is left rather than lose the text.
            out.push_str(&rest[start..]);
            return out;
        };
        let inner = &rest[start + 2..start + end];
        // The display text is the last pipe-separated part; a link with no
        // pipe displays its target.
        out.push_str(inner.rsplit('|').next().unwrap_or(inner));
        rest = &rest[start + end + 2..];
    }
    out.push_str(rest);
    out
}

/// Removes `'''bold'''` and `''italics''` while keeping the words.
fn remove_formatting(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("''") {
        out.push_str(&rest[..at]);
        let quotes = rest[at..].chars().take_while(|&c| c == '\'').count();
        rest = &rest[at + quotes..];
    }
    out.push_str(rest);
    out
}

/// Removes templates, resolving the ones that carry sentence content.
///
/// Brace counting rather than pattern matching: 13% of leads nest templates,
/// and a pattern that stops at the first `}}` truncates them.
fn remove_templates(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"{{") {
            match template_end(text, i) {
                Some(end) => {
                    let inner = &text[i + 2..end - 2];
                    out.push_str(&resolve_template(inner));
                    i = end;
                    continue;
                }
                // Unclosed template: everything after it is markup, not prose.
                None => break,
            }
        }
        let ch = text[i..].chars().next().unwrap_or('\0');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// The index just past the `}}` matching the `{{` at `start`.
fn template_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = start;
    while i + 1 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 1] == b'{' {
            depth += 1;
            i += 2;
        } else if bytes[i] == b'}' && bytes[i + 1] == b'}' {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return Some(i);
            }
        } else {
            i += 1;
        }
    }
    None
}

/// What a template contributes to the prose: usually nothing, sometimes words.
fn resolve_template(inner: &str) -> String {
    let mut parts = inner.splitn(2, '|');
    let name = parts.next().unwrap_or("").trim().to_ascii_lowercase();
    let arguments = parts.next().unwrap_or("");

    match name.as_str() {
        // Part of the sentence: a dash between two years.
        "spaced ndash" | "snd" => " – ".to_string(),
        "ndash" => "–".to_string(),
        "mdash" => "—".to_string(),
        // "circa 930", written as an abbreviation the way the article renders it.
        "circa" | "c." => {
            let year = positional(arguments).next().unwrap_or_default();
            if year.is_empty() { String::new() } else { format!("c. {year}") }
        }
        // {{convert|103.9|sqmi|sqkm|1}} -> "103.9 sqmi": the value and its
        // first unit, which is what the sentence is saying.
        "convert" | "cvt" => {
            let mut positions = positional(arguments);
            let value = positions.next().unwrap_or_default();
            let unit = positions.next().unwrap_or_default();
            format!("{value} {unit}").trim().to_string()
        }
        // The foreign word itself, which is the last positional argument.
        "lang" | "langx" | "transliteration" | "transl" => positional(arguments).last().unwrap_or_default(),
        // Wrappers that exist for layout: keep what they wrap.
        "nowrap" | "nobr" | "small" | "big" | "em" | "strong" => positional(arguments).next().unwrap_or_default(),
        // Everything else -- pronunciation, citations, infoboxes, maintenance
        // banners, list formatting -- contributes no prose.
        _ => String::new(),
    }
}

/// The positional (unnamed) arguments of a template, in order.
///
/// Splitting has to respect nesting: `{{a|{{b|c}}|d}}` has two arguments, not
/// three.
fn positional(arguments: &str) -> impl Iterator<Item = String> + '_ {
    split_arguments(arguments)
        .into_iter()
        .filter(|argument| !argument.contains('='))
        .map(|argument| argument.trim().to_string())
        .filter(|argument| !argument.is_empty())
}

/// Splits template arguments on `|`, ignoring pipes inside nested templates
/// and links.
fn split_arguments(arguments: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut braces = 0usize;
    let mut brackets = 0usize;
    let mut chars = arguments.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '{' if chars.peek() == Some(&'{') => {
                braces += 1;
                current.push(ch);
                current.push(chars.next().unwrap_or('{'));
            }
            '}' if chars.peek() == Some(&'}') => {
                braces = braces.saturating_sub(1);
                current.push(ch);
                current.push(chars.next().unwrap_or('}'));
            }
            '[' if chars.peek() == Some(&'[') => {
                brackets += 1;
                current.push(ch);
                current.push(chars.next().unwrap_or('['));
            }
            ']' if chars.peek() == Some(&']') => {
                brackets = brackets.saturating_sub(1);
                current.push(ch);
                current.push(chars.next().unwrap_or(']'));
            }
            '|' if braces == 0 && brackets == 0 => parts.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    parts.push(current);
    parts
}

/// Cleans up what removal leaves behind.
///
/// Deleting a template out of the middle of a sentence leaves punctuation with
/// nothing before it -- `Name (; born 1982)` where the parenthesis held only a
/// pronunciation. This is why the removal steps do not try to be clever: it is
/// simpler to tidy afterwards than to guess in advance.
fn tidy(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let mut line = collapse_spaces(line);

        line = tidy_parentheses(&line);
        let line = collapse_spaces(&line);
        let line = line.replace(" ,", ",").replace(" ;", ";").replace(" .", ".");

        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// Resolves the HTML entities editors write into wikitext by hand.
///
/// These are not the dump's own XML escaping -- that is undone before the text
/// reaches here. These are written in the article source, and measured against
/// real leads: `&nbsp;` appears 86 times in 867 articles, `&ndash;` six, and a
/// numeric `&#124;` twice. Left alone they show up verbatim in the prose, as
/// "200 to 300&nbsp;million records".
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        // An entity is short; anything longer is a stray ampersand.
        //
        // The window is measured in bytes but must land on a character
        // boundary: slicing to a fixed byte count panics when the twelfth byte
        // falls inside a multi-byte character, and leads are full of them.
        // Found on the real dump, where "Bee Gees &ndash; 1958–2003" ends an
        // en dash astride the limit.
        let window = tail.len().min(12);
        let window = (0..=window).rev().find(|n| tail.is_char_boundary(*n)).unwrap_or(0);
        let Some(end) = tail[..window].find(';') else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };

        let name = &tail[1..end];
        let resolved = match name {
            "nbsp" => Some(' '),
            "ndash" => Some('–'),
            "mdash" => Some('—'),
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => name
                .strip_prefix('#')
                .and_then(|digits| match digits.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => digits.parse().ok(),
                })
                .and_then(char::from_u32),
        };

        if let Some(ch) = resolved {
            out.push(ch);
            rest = &tail[end + 1..];
        } else {
            // Not an entity we know: keep it as written rather than delete it.
            out.push('&');
            rest = &tail[1..];
        }
    }

    out.push_str(rest);
    out
}

/// Repairs the parentheses that template removal empties out.
///
/// In 11% of leads the parenthesis after the name holds only pronunciation, so
/// removing those templates leaves either `Name ()` or `Name ( ; born 1982)`.
/// Both are handled by looking at what the parenthesis actually contains now,
/// rather than by matching the particular strings that happen to be left --
/// the spacing varies with how many templates were removed.
fn tidy_parentheses(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;

    while let Some(start) = rest.find('(') {
        let Some(offset) = rest[start..].find(')') else { break };
        let inside = &rest[start + 1..start + offset];
        let trimmed = inside.trim_matches(|c: char| c.is_whitespace() || matches!(c, ';' | ',' | '/' | '-' | '–'));

        out.push_str(&rest[..start]);
        if trimmed.is_empty() {
            // Nothing of substance survived: drop the parenthesis entirely,
            // along with the space that preceded it.
            while out.ends_with(' ') {
                out.pop();
            }
        } else {
            out.push('(');
            out.push_str(trimmed);
            out.push(')');
        }
        rest = &rest[start + offset + 1..];
    }

    out.push_str(rest);
    out
}

fn collapse_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_was_space = false;
    for ch in text.chars() {
        let is_space = ch == ' ' || ch == '\t';
        if is_space && last_was_space {
            continue;
        }
        out.push(if is_space { ' ' } else { ch });
        last_was_space = is_space;
    }
    out
}

/// The prose paragraphs of the cleaned lead.
///
/// Lines that survive as list items, table remnants or stray punctuation are
/// dropped: what is wanted is sentences.
fn first_paragraphs(text: &str) -> String {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            if !current.trim().is_empty() {
                paragraphs.push(current.trim().to_string());
            }
            current.clear();
            continue;
        }
        // Markup that is not prose: list items, indents, table rows, headings.
        if line.starts_with(['*', '#', ':', ';', '|', '!', '=']) {
            continue;
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(line);
    }
    if !current.trim().is_empty() {
        paragraphs.push(current.trim().to_string());
    }

    // A "paragraph" of a few characters is a leftover, not a sentence.
    paragraphs.retain(|paragraph| paragraph.len() > 40 && paragraph.contains(' '));
    paragraphs.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_entities_editors_write_by_hand() {
        // Measured on real leads: &nbsp; appears in 86 of 867 articles. Left
        // alone it reads as "300&nbsp;million records" in the finished prose.
        let text = "'''Led Zeppelin''' were an English rock band with an estimated 200 to 300&nbsp;million records sold worldwide.";
        let lead = lead(text).unwrap();
        assert!(lead.contains("300 million records"), "got: {lead}");
        assert!(!lead.contains("&nbsp;"), "got: {lead}");
    }

    #[test]
    fn decodes_numeric_and_dash_entities() {
        let text = "'''Some act''' released records between 1968&ndash;1980 and toured widely across Europe and America.";
        let lead = lead(text).unwrap();
        assert!(lead.contains("1968–1980"), "got: {lead}");
    }

    #[test]
    fn keeps_a_stray_ampersand_as_written() {
        // "R&B" is not an entity, and deleting the ampersand would rename a
        // genre.
        let text = "'''Some act''' were an American R&B group formed in Detroit in 1965 by four singers.";
        let lead = lead(text).unwrap();
        assert!(lead.contains("American R&B group"), "got: {lead}");
    }

    #[test]
    fn survives_a_multi_byte_character_after_a_link_opener() {
        // Found by running the parser over real articles rather than by
        // reasoning: a link target opening with a multi-byte character used to
        // panic the media-link check, which sliced a string mid-character.
        let text = "'''Some act''' took their name from [[Ṭahmāsp I]], a Safavid ruler, and formed in London in 1968.";
        let lead = lead(text).unwrap();
        assert!(lead.contains("Ṭahmāsp I"), "got: {lead}");
    }

    #[test]
    fn returns_nothing_for_a_redirect() {
        assert!(lead("#REDIRECT [[Nirvana (band)]]").is_none());
        assert!(lead("#redirect [[Nirvana]]").is_none());
    }

    #[test]
    fn stops_at_the_first_section_heading() {
        let text = "'''Nirvana''' was an American rock band formed in Aberdeen in 1987.\n\n== History ==\nThe band began when...";
        let lead = lead(text).unwrap();
        assert!(lead.contains("American rock band"));
        assert!(!lead.contains("The band began"));
    }

    #[test]
    fn drops_the_templates_that_open_an_article() {
        let text = concat!(
            "{{short description|American rock band (1987–1994)}}\n",
            "{{Use mdy dates|date=February 2020}}\n",
            "'''Nirvana''' was an American rock band formed in Aberdeen, Washington, in 1987.\n"
        );
        let lead = lead(text).unwrap();
        assert!(lead.starts_with("Nirvana was an American rock band"), "got: {lead}");
        assert!(!lead.contains("short description"));
    }

    #[test]
    fn removes_a_nested_infobox_whole() {
        // 13% of real leads nest templates. A pattern that stops at the first
        // `}}` would leave the tail of the infobox in the prose.
        let text = concat!(
            "{{Infobox musical artist\n",
            "| name = Nirvana\n",
            "| genre = {{flatlist|\n* [[Grunge]]\n* [[alternative rock]]\n}}\n",
            "| years_active = 1987–1994\n",
            "}}\n",
            "'''Nirvana''' was an American rock band formed in Aberdeen, Washington, in 1987.\n"
        );
        let lead = lead(text).unwrap();
        assert_eq!(lead, "Nirvana was an American rock band formed in Aberdeen, Washington, in 1987.");
    }

    #[test]
    fn flattens_wikilinks_to_their_display_words() {
        let text = "'''Pixies''' are an American [[alternative rock]] band formed in [[Boston]], Massachusetts, in 1986 by [[Rock music|rock]] musicians.";
        let lead = lead(text).unwrap();
        assert!(lead.contains("an American alternative rock band"), "got: {lead}");
        assert!(lead.contains("formed in Boston,"));
        // The piped link shows its second half, not its target.
        assert!(lead.contains("by rock musicians"), "got: {lead}");
        assert!(!lead.contains("Rock music"));
    }

    #[test]
    fn removes_a_comment_from_the_middle_of_a_sentence() {
        // Both Led Zeppelin and The Beatles carry an editors' argument inside
        // the first sentence.
        let text = "'''Led Zeppelin''' were <!-- DO NOT change \"WERE\" to \"WAS\". --> an English rock band formed in London in 1968.";
        let lead = lead(text).unwrap();
        assert_eq!(lead, "Led Zeppelin were an English rock band formed in London in 1968.");
    }

    #[test]
    fn removes_references_including_nested_templates() {
        let text = concat!(
            "'''Miles Davis''' was an American trumpeter and composer",
            "<ref>{{Cite web |title=Did you know Miles Davis was also a painter? |url=https://example.org}}</ref>",
            " who shaped the sound of jazz over five decades of recording."
        );
        let lead = lead(text).unwrap();
        assert_eq!(
            lead,
            "Miles Davis was an American trumpeter and composer who shaped the sound of jazz over five decades of recording."
        );
    }

    #[test]
    fn removes_a_self_closing_reference() {
        let text = "'''Kate Bush''' is an English singer and songwriter<ref name=\"bio\" /> known for her eclectic style and dance.";
        let lead = lead(text).unwrap();
        assert_eq!(lead, "Kate Bush is an English singer and songwriter known for her eclectic style and dance.");
    }

    #[test]
    fn keeps_the_dash_that_separates_two_years() {
        // Deleting this template would join the years into one number.
        let text = "'''Miles Dewey Davis III''' (May 26, 1926{{spaced ndash}}September 28, 1991) was an American trumpeter and bandleader.";
        let lead = lead(text).unwrap();
        assert!(lead.contains("May 26, 1926 – September 28, 1991"), "got: {lead}");
    }

    #[test]
    fn resolves_the_templates_that_carry_sentence_content() {
        let text = concat!(
            "'''Lincoln''' is the capital of [[Nebraska]]. The city covers {{convert|103.9|sqmi|sqkm|1}} ",
            "and was founded {{circa|1856}} under the name {{lang|de|Lancaster}} before renaming."
        );
        let lead = lead(text).unwrap();
        assert!(lead.contains("covers 103.9 sqmi"), "got: {lead}");
        assert!(lead.contains("founded c. 1856"), "got: {lead}");
        assert!(lead.contains("the name Lancaster"), "got: {lead}");
    }

    #[test]
    fn cleans_up_a_parenthesis_left_holding_only_punctuation() {
        // 11% of leads open with a parenthesis of pure pronunciation. Removing
        // it naively leaves "Kirsten Caroline Dunst (; born April 30, 1982)".
        let text = "'''Kirsten Caroline Dunst''' ({{IPAc-en|ˈ|k|ɪər|s|t|ən}} {{respell|KEER|stən}}; born April 30, 1982) is an American actress and singer.";
        let lead = lead(text).unwrap();
        assert!(!lead.contains("(;"), "orphaned punctuation left in: {lead}");
        assert!(
            lead.starts_with("Kirsten Caroline Dunst (born April 30, 1982) is an American actress"),
            "got: {lead}"
        );
    }

    #[test]
    fn drops_a_parenthesis_that_becomes_empty() {
        let text = "'''Mieszko I''' ({{IPA|pl|ˈmjɛʂkɔ}}) was the ruler of Poland and the founder of the first Polish state.";
        let lead = lead(text).unwrap();
        assert!(!lead.contains("()"), "empty parenthesis left in: {lead}");
        assert!(lead.starts_with("Mieszko I was the ruler"), "got: {lead}");
    }

    #[test]
    fn removes_file_links_with_their_captions() {
        // A file link nests brackets for its caption, so naive bracket
        // flattening would leak the caption into the prose.
        let text = concat!(
            "[[File:Nirvana around 1992.jpg|thumb|Kurt Cobain (front) and [[Krist Novoselic]] live]]\n",
            "'''Nirvana''' was an American rock band formed in Aberdeen, Washington, in 1987.\n"
        );
        let lead = lead(text).unwrap();
        assert_eq!(lead, "Nirvana was an American rock band formed in Aberdeen, Washington, in 1987.");
    }

    #[test]
    fn keeps_several_paragraphs_of_the_lead() {
        let text = concat!(
            "'''Nirvana''' was an American rock band formed in Aberdeen, Washington, in 1987 by Kurt Cobain.\n",
            "\n",
            "In the late 1980s, Nirvana established itself as part of the Seattle grunge scene of that decade.\n",
            "\n",
            "== History ==\nMore text.\n"
        );
        let lead = lead(text).unwrap();
        assert!(lead.contains("formed in Aberdeen"));
        assert!(lead.contains("Seattle grunge scene"));
        assert!(!lead.contains("More text"));
        assert!(lead.contains("\n\n"), "paragraphs should stay separated");
    }

    #[test]
    fn drops_list_lines_that_are_not_prose() {
        let text = concat!(
            "'''Some band''' were an English rock band formed in London in 1968 by four musicians.\n",
            "* [[Robert Plant]]\n",
            "* [[Jimmy Page]]\n"
        );
        let lead = lead(text).unwrap();
        assert_eq!(lead, "Some band were an English rock band formed in London in 1968 by four musicians.");
    }

    #[test]
    fn returns_nothing_when_there_is_no_prose_left() {
        // A stub of nothing but an infobox has no lead to show.
        assert!(lead("{{Infobox musical artist\n| name = X\n}}\n").is_none());
        assert!(lead("").is_none());
    }

    #[test]
    fn survives_unbalanced_markup() {
        // Upstream data is not always well-formed; the text before the damage
        // is still worth keeping.
        let text = "'''Nirvana''' was an American rock band formed in Aberdeen in 1987. {{unclosed template";
        let lead = lead(text).unwrap();
        assert!(lead.starts_with("Nirvana was an American rock band"), "got: {lead}");
    }

    #[test]
    fn keeps_an_article_that_never_bolds_its_name() {
        // 19 of 867 sampled articles have no bold name at all; the prose is
        // still prose.
        let text = "This article covers the history of recorded music in the twentieth century and beyond.";
        let lead = lead(text).unwrap();
        assert!(lead.starts_with("This article covers"));
    }

    #[test]
    fn an_article_opening_with_a_multi_byte_character_does_not_panic() {
        // The redirect check compares a fixed nine bytes. An article whose
        // opening puts a character across that boundary panicked the second
        // full prose run, after the first had already been fixed at a
        // different fixed width.
        for filler in 0..12 {
            let text = format!("{}á is a Spanish singer who recorded eleven albums between 1970 and 1994.", "x".repeat(filler));
            let lead = lead(&text);
            assert!(lead.is_some(), "the lead should survive (filler {filler}): {text:?}");
        }
        // The check itself must still work.
        assert!(lead("#REDIRECT [[Somewhere]]").is_none());
        assert!(lead("#redirect [[Somewhere]]").is_none());
    }

    #[test]
    fn a_multi_byte_character_at_the_entity_window_does_not_panic() {
        // The entity scan looks ahead a fixed number of bytes, and this lead
        // puts an en dash astride that limit. Slicing to the byte count panics
        // -- which is exactly how the first full prose run died, after locating
        // 181,491 articles.
        for filler in 0..16 {
            let text = format!("&{}–2003 was a year", "x".repeat(filler));
            let decoded = decode_entities(&text);
            assert!(decoded.contains("2003"), "the text should survive: {decoded}");
        }
    }

    #[test]
    fn resolves_the_entities_editors_write_by_hand() {
        assert_eq!(decode_entities("200&nbsp;million"), "200 million");
        assert_eq!(decode_entities("1958&ndash;2003"), "1958–2003");
        assert_eq!(decode_entities("a &#124; b"), "a | b");
        // A stray ampersand is not an entity and must survive as itself.
        assert_eq!(decode_entities("Fun & Games"), "Fun & Games");
    }
}
