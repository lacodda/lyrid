//! Guards the brand mark against being cut from the wrong level.
//!
//! The line's identity has three masters — S, M and L — and which one a raster
//! or a placement uses is decided by how big it is, never by what it is for.
//! Getting that wrong is invisible in a diff and obvious on screen: the S tile
//! is a hex filled with colour, which is the only thing that reads at 16 px
//! and a coloured blob at 48.
//!
//! It has shipped twice in this line already — kasl's docs header and kilna's
//! desktop icon — and both times the owner's eye caught it, several releases
//! late. lyrid had it in three places at once when v0.11 went looking. Hence
//! a test rather than a note.
//!
//! These read files in the repository on purpose. An earlier attempt in the
//! line compared the SVGs against a registry kept in a notes vault, which CI
//! does not have: the check passed by finding nothing to check.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: impl AsRef<Path>) -> Vec<u8> {
    let path = repo_root().join(path);
    fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The three masters, smallest detail first.
const S: &str = "assets/logo-s.svg";
const M: &str = "assets/logo-m.svg";
const L: &str = "assets/logo.svg";

/// The level a raster or a placement of this many pixels should come from.
///
/// The bands are the line's, not this project's: S up to 27 px, M to 63, L
/// above. Kept beside the assertions rather than imported, because the
/// generator that writes the files is JavaScript and this is the independent
/// reading of the same rule.
fn level_for(size: u32) -> &'static str {
    match size {
        0..=27 => S,
        28..=63 => M,
        _ => L,
    }
}

#[test]
fn the_header_mark_is_cut_for_the_size_it_is_drawn_at() {
    // The SPA header draws it at 2.25rem -- 36 px, the M band. It held the S
    // tile until v0.11, which at that size is a filled hexagon where the mark
    // should be. The stylesheet is read rather than assumed, so changing the
    // size without changing the file fails here instead of on screen.
    let css = String::from_utf8(read("web/src/styles.css")).expect("the stylesheet is UTF-8");
    let rule = css.split(".app__mark {").nth(1).expect("the stylesheet still styles the header mark");
    let width = rule
        .lines()
        .find_map(|line| line.trim().strip_prefix("width:"))
        .and_then(|value| value.trim().strip_suffix("rem;"))
        .and_then(|value| value.trim().parse::<f32>().ok())
        .expect("the header mark has a width in rem");

    // 1rem is 16 px unless the page says otherwise, and this page does not.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a mark is never a fraction of a pixel wide")]
    let pixels = (width * 16.0).round() as u32;

    assert_eq!(
        read("web/public/mark.svg"),
        read(level_for(pixels)),
        "the header mark is drawn at {pixels} px and should be cut from {}",
        level_for(pixels)
    );
}

#[test]
fn the_favicon_is_the_small_tile() {
    // An SVG favicon is drawn at 16 px in a tab, whatever its viewBox says.
    assert_eq!(read("web/public/favicon.svg"), read(S));
    assert_eq!(read("docs/site/public/favicon.svg"), read(S));
}

#[test]
fn the_docs_header_is_the_full_mark() {
    // The docs site draws it at bar height, well into the L band. This is the
    // exact placement that shipped wrong in kasl.
    assert_eq!(read("docs/site/src/assets/logo.svg"), read(L));
}

#[test]
fn every_icon_in_the_ico_is_cut_for_its_own_size() {
    // An .ico holds sizes from 16 to 256, and there is room in it for all
    // three levels. Taking every entry from S is what put a filled blob on
    // kilna's desktop at 48 px.
    //
    // Checked by size rather than by content: the entries are PNGs of
    // different dimensions, so they cannot be compared to a master byte for
    // byte. What can be compared is how much of the master survived -- a 48 px
    // rendering of the plain filled hex compresses far smaller than the same
    // size cut from the detailed mark.
    let ico = read("assets/icon.ico");
    let entries = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    assert_eq!(entries, 7, "the .ico should hold 16, 24, 32, 48, 64, 128 and 256");

    let mut sizes = Vec::new();
    for i in 0..entries {
        let at = 6 + i * 16;
        // A width byte of 0 means 256: the field is one byte and 256 does not
        // fit in it.
        let width = if ico[at] == 0 { 256_u32 } else { u32::from(ico[at]) };
        let bytes = u32::from_le_bytes([ico[at + 8], ico[at + 9], ico[at + 10], ico[at + 11]]);
        sizes.push((width, bytes));
    }

    // Largest first. Windows picks by nearest size and ignores the order, but
    // tauri-codegen takes entries()[0] literally as the window icon -- a 16 px
    // entry at the front is a title bar stretched from sixteen pixels. lyrid
    // is not a Tauri app; the .ico is the line's shared artefact and the order
    // is part of what makes it one.
    let widths: Vec<u32> = sizes.iter().map(|(width, _)| *width).collect();
    let mut descending = widths.clone();
    descending.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(widths, descending, "the .ico lists its images smallest-first: {widths:?}");

    // The levels really differ. If every entry came from one master, the bytes
    // per pixel would follow one curve; they do not, because the 48 px image
    // carries detail the 16 px one does not have.
    let at = |want: u32| sizes.iter().find(|(width, _)| *width == want).map_or(0, |(_, bytes)| *bytes);
    assert!(
        at(48) > at(32),
        "the 48 px image is not richer than the 32 px one, so both came from the same master"
    );
}

#[test]
fn the_touch_icon_is_the_full_mark_at_touch_size() {
    // 180 px is a home-screen tile, not an icon in a list. It was cut from S
    // until v0.11, which is a coloured square on a phone's home screen.
    let png = read("assets/apple-touch-icon.png");
    // PNG header: width and height are big-endian at byte 16.
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    assert_eq!(width, 180, "the touch icon is not 180 px");
    assert_eq!(level_for(width), L, "180 px should be the L band");

    // And it is the same file both sites serve, so they cannot drift.
    assert_eq!(read("web/public/apple-touch-icon.png"), png);
    assert_eq!(read("docs/site/public/apple-touch-icon.png"), png);
}

#[test]
fn the_spa_offers_the_icons_it_points_at() {
    // A `<link rel>` to a file that is not there is a broken icon everywhere
    // at once, and nothing in a build says so.
    let html = String::from_utf8(read("web/index.html")).expect("index.html is UTF-8");
    for (attribute, file) in [
        ("/favicon.svg", "web/public/favicon.svg"),
        ("/apple-touch-icon.png", "web/public/apple-touch-icon.png"),
        ("/social-preview.png", "web/public/social-preview.png"),
    ] {
        assert!(html.contains(attribute), "index.html does not reference {attribute}");
        assert!(
            repo_root().join(file).exists(),
            "index.html points at {attribute} but {file} is not in the repository"
        );
    }
}
