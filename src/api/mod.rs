//! What the browser asks the server for.
//!
//! The map itself is static tiles and never reaches these routes; this is
//! what a click, a search box and a card need.

pub mod accounts;
pub mod admin;
pub mod artists;
pub mod changes;
pub mod dossiers;
pub mod letter;
pub mod listening;
pub mod metrics;
pub mod relations;
pub mod scrobbling;
pub mod spectrum;
pub mod youth;

use serde::Serialize;

/// A star, and where the current sky puts it: what every list of stars a
/// person can fly to is made of.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlacedStar {
    pub id: i32,
    pub name: String,
    pub x: f32,
    pub y: f32,
}
