//! Rebuilds whatever embeds the migrations when a migration is added.
//!
//! `sqlx::migrate!()` reads `migrations/` at compile time, and cargo does not
//! know that the macro depends on that directory. A new migration therefore
//! reached the binaries whose own sources changed and not the ones whose
//! sources did not: the integration suites kept the old list, met a database
//! the server had already moved on, and failed every test with
//! `VersionMissing`. This line is sqlx's own documented remedy.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
