use ipc_layer::Producer;
use nullherz_traits::Command;
use crate::engine::metrics::EngineMetrics;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub struct ResourceRecycler {
    pub bundle_garbage_producer: Option<Producer<Vec<Command>>>,
    pub bundle_overflow_producer: Option<Producer<Vec<Command>>>,
}

impl ResourceRecycler {
    pub fn new(
        bundle_garbage_producer: Option<Producer<Vec<Command>>>,
        bundle_overflow_producer: Option<Producer<Vec<Command>>>,
    ) -> Self {
        Self {
            bundle_garbage_producer,
            bundle_overflow_producer,
        }
    }

    /// Recycles a command bundle, ensuring that any contained heavy resources (like Arc<Vec<f32>>)
    /// are not dropped on the real-time thread.
    pub fn recycle_bundle(
        &mut self,
        bundle: Vec<Command>,
        metrics: &EngineMetrics,
        health_signal: &Arc<AtomicBool>,
    ) {
        nullherz_traits::assert_rt_safe!();

        if let Some(ref mut prod) = self.bundle_garbage_producer {
            if let Err(b) = prod.push(bundle) {
                if let Some(ref mut overflow) = self.bundle_overflow_producer {
                    if let Err(leak) = overflow.push(b) {
                        metrics.report_resource_leak(health_signal);
                        // RT-HARDENING: Use forget to avoid participation in drop sequence
                        std::mem::forget(leak);
                    }
                } else {
                    metrics.report_resource_leak(health_signal);
                    std::mem::forget(b);
                }
            }
        } else if let Some(ref mut overflow) = self.bundle_overflow_producer {
            if let Err(b) = overflow.push(bundle) {
                metrics.report_resource_leak(health_signal);
                std::mem::forget(b);
            }
        } else {
            // CRITICAL: If no garbage channels are available, we must forget the bundle
            // rather than dropping it and risking an expensive deallocation on the audio thread.
            metrics.report_resource_leak(health_signal);
            std::mem::forget(bundle);
        }
    }
}
