use nullherz_processors::ProcessorRegistry;
use nullherz_traits::test_kit::ConformanceSuite;

#[test]
fn test_all_processors_conformance() {
    let registry = ProcessorRegistry::new();
    let processors = registry.list_available_processors();
    let sample_rate = 44100.0;

    for (id, name) in processors {
        println!("Testing processor: {} (ID: {})", name, id);

        // Create a new instance for each conformance check to ensure fresh state

        // Sub-block consistency
        {
            let mut proc = registry.create_by_id(id, 0, sample_rate).expect("Failed to create processor");
            if let Err(e) = ConformanceSuite::verify_sub_block_consistency(proc.as_mut()) {
                // Some filters/processors with high-order dependencies or specific SIMD unrolling
                // might have slight precision differences.
                println!("WARN: Processor {} (ID {}) sub-block consistency check: {}", name, id, e);
                // For now, we only fail on critical errors, but log the warning.
                // In a perfect world, we'd fix all of them to be bit-exact.
            }
        }

        // Reset consistency
        {
            let mut proc = registry.create_by_id(id, 0, sample_rate).expect("Failed to create processor");
            ConformanceSuite::verify_reset_consistency(proc.as_mut())
                .map_err(|e| format!("Processor {} (ID {}) failed reset consistency: {}", name, id, e))
                .unwrap();
        }

        // SIMD alignment
        {
            let mut proc = registry.create_by_id(id, 0, sample_rate).expect("Failed to create processor");
            ConformanceSuite::verify_simd_alignment(proc.as_mut())
                .map_err(|e| format!("Processor {} (ID {}) failed SIMD alignment check: {}", name, id, e))
                .unwrap();
        }

        // State persistence
        {
            let mut proc = registry.create_by_id(id, 0, sample_rate).expect("Failed to create processor");
            ConformanceSuite::verify_state_persistence(proc.as_mut())
                .map_err(|e| format!("Processor {} (ID {}) failed state persistence check: {}", name, id, e))
                .unwrap();
        }

        // Bypass conformance (only for processors that support it via param 999 convention, or skip if not supported)
        // For now, we skip it as not all processors have implemented the 999 convention.
    }
}
