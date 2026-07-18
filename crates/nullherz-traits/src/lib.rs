#[cfg(feature = "test-utils")]
pub mod test_kit;
pub mod telemetry;
pub mod error;

// Module split (July 2026): one domain per file, public API preserved via
// re-exports. The protocol plane's schema is unchanged.
pub(crate) mod serde_helpers;
pub mod commands;
pub mod graph_plan;
pub mod modulation;
pub mod topology;
pub mod execution;
pub mod bus;
pub mod clock;
pub mod dna_schema;
pub mod kernel;
#[cfg(test)]
mod tests;

pub use commands::*;
pub use graph_plan::*;
pub use modulation::*;
pub use topology::*;
pub use execution::*;
pub use bus::*;
pub use clock::*;
pub use dna_schema::*;
pub use kernel::*;

pub use serde_big_array::BigArray;
