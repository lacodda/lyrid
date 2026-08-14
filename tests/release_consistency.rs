//! Guards the facts that must agree before a version is published.
//!
//! lyrid ships to several storefronts - GitHub, crates.io, the docs site -
//! and each renders its own copy of the README or carries its own manifest.
//! Drift is only visible after publishing, when it is too late to take back,
//! so these checks run in CI instead.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: impl AsRef<Path>) -> String {
    let path = repo_root().join(path);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Pulls `"version": "x.y.z"` out of a package.json without a JSON dependency:
/// the field is the only thing these checks care about.
fn package_json_version(path: &str) -> String {
    let manifest = read(path);
    let key = "\"version\"";
    let at = manifest.find(key).unwrap_or_else(|| panic!("{path} has no version field"));
    let rest = &manifest[at + key.len()..];
    let open = rest.find('"').unwrap_or_else(|| panic!("{path}: malformed version field"));
    let rest = &rest[open + 1..];
    let close = rest.find('"').unwrap_or_else(|| panic!("{path}: unterminated version string"));
    rest[..close].to_string()
}

#[test]
fn readme_links_resolve_off_github() {
    // The same file is rendered on crates.io, where a relative path has no
    // repository to resolve against: the banner turns into a broken image
    // and the links 404.
    let readme = read("README.md");

    for (line_no, line) in readme.lines().enumerate() {
        for (marker, kind) in [("src=\"", "image"), ("](", "link")] {
            let mut rest = line;
            while let Some(at) = rest.find(marker) {
                let target = &rest[at + marker.len()..];
                let end = if marker == "](" { ')' } else { '"' };
                let target = &target[..target.find(end).unwrap_or(target.len())];

                let relative = !target.starts_with("http") && !target.starts_with('#') && !target.is_empty();
                assert!(
                    !relative,
                    "README line {}: relative {kind} `{target}` breaks on crates.io; use an absolute URL",
                    line_no + 1
                );

                rest = &rest[at + marker.len()..];
            }
        }
    }
}

#[test]
fn readme_is_not_duplicated() {
    // One README for every storefront. A second copy is where descriptions
    // start to drift; the SPA and the docs site must reuse the root file
    // rather than fork it.
    for candidate in ["web/README.md", "docs/README.md", "docs/site/README.md"] {
        let duplicate = repo_root().join(candidate);
        assert!(
            !duplicate.exists(),
            "{candidate} exists; it will drift from the root README, which is the single source"
        );
    }
}

#[test]
fn every_manifest_carries_the_same_version() {
    // Three manifests, one release. A stale number in web/ or docs/ ships a
    // build that claims a version it is not.
    let crate_version = env!("CARGO_PKG_VERSION");

    for manifest in ["web/package.json", "docs/site/package.json"] {
        if !repo_root().join(manifest).exists() {
            continue;
        }
        assert_eq!(
            package_json_version(manifest),
            crate_version,
            "{manifest} disagrees with Cargo.toml about the version"
        );
    }
}
