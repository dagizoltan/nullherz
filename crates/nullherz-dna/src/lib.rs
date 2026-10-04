// Non-RT plane (gossip/discovery network threads): thread spawn/sleep are sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]
// Module split (July 2026): one domain per file, public API preserved via re-exports.

pub mod consensus;
pub mod library;
pub mod asset_db;
pub mod network;
pub mod curation;
pub mod registry;
pub mod transfusion;
pub mod matchmaker;
pub mod presets;
#[cfg(test)]
mod tests;

pub use consensus::*;
pub use library::*;
pub use asset_db::*;
pub use network::*;
pub use curation::*;
pub use registry::*;
pub use transfusion::*;
pub use matchmaker::*;
pub use presets::*;

pub use nullherz_traits::{RegisteredSample, SampleBuffer, MmapBuffer};
