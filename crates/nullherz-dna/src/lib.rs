// Non-RT plane (gossip/discovery network threads): thread spawn/sleep are sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]
// Module split (July 2026): one domain per file, public API preserved via re-exports.

pub mod consensus;
pub mod library;
pub mod network;
pub mod curation;
pub mod registry;
pub mod transfusion;
pub mod matchmaker;
#[cfg(test)]
mod tests;

pub use consensus::*;
pub use library::*;
pub use network::*;
pub use curation::*;
pub use registry::*;
pub use transfusion::*;
pub use matchmaker::*;

use std::sync::Arc;

pub type SampleBuffer = Arc<Vec<f32>>;
pub use nullherz_traits::RegisteredSample;
