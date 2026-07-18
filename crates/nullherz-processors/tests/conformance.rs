use nullherz_processors::registry::ProcessorRegistry;
use nullherz_traits::test_kit::{ConformanceSuite, StabilityTester};
use nullherz_traits::AudioProcessor;

#[test]
fn test_all_processors_conformance() {
    let registry = ProcessorRegistry::new();
    let processors = registry.list_available_processors();

    for (type_id, name) in processors {
        if let Some(mut processor) = registry.create_by_id(type_id, 0, 44100.0) {
            println!("Testing Conformance for Processor: {} (Type ID: {})", name, type_id);

            let proc_ref: &mut dyn AudioProcessor = processor.as_mut();

            // 1. Signal Stability (Impulse Response & Bounds)
            StabilityTester::verify_signal_bounds(proc_ref, 10).unwrap_or_else(|_| panic!("Stability check failed for {}", name));

            // 2. Reset Determinism
            ConformanceSuite::verify_reset_consistency(proc_ref).unwrap_or_else(|_| panic!("Reset consistency failed for {}", name));

            // 3. Sub-block Consistency (Phase-locking across automation boundaries)
            ConformanceSuite::verify_sub_block_consistency(proc_ref).unwrap_or_else(|_| panic!("Sub-block consistency failed for {}", name));

            // 4. SIMD Alignment
            ConformanceSuite::verify_simd_alignment(proc_ref).unwrap_or_else(|_| panic!("SIMD alignment check failed for {}", name));

            // 5. Parameter Bounds (NaN/Inf robustness)
            // We test the first 16 potential parameters
            for p_id in 0..16 {
                ConformanceSuite::verify_parameter_bounds(proc_ref, p_id).unwrap_or_else(|_| panic!("Parameter robustness failed for {} on param {}", name, p_id));
            }
        }
    }
}
