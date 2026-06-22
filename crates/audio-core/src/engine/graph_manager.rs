use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, Ordering};
use ipc_layer::Producer;
use nullherz_traits::AudioProcessor;
use crate::engine::metrics::EngineMetrics;
use crate::rt_logging::RtLogger;

/// Manages the active and pending audio graphs, handling atomic swaps and
/// real-time safe deallocation of replaced graphs.
///
/// NOTE: The `active_graph` and `pending_graph` use `AtomicPtr<Box<dyn AudioProcessor>>`.
/// We use a double-box (`Box<Box<dyn AudioProcessor>>`) before converting to a raw pointer.
/// This is necessary because `Box<dyn AudioProcessor>` is a fat pointer (data pointer + vtable pointer),
/// and `AtomicPtr` can only store thin pointers. By boxing the fat pointer, we get a thin pointer
/// to the box that can be safely used with `AtomicPtr`.
pub struct GraphManager {
    active_graph: AtomicPtr<Box<dyn AudioProcessor>>,
    pending_graph: AtomicPtr<Box<dyn AudioProcessor>>,
    garbage_producer: Producer<Box<dyn AudioProcessor>>,
    overflow_garbage_producer: Option<Producer<Box<dyn AudioProcessor>>>,
    logger: Arc<RtLogger>,
}

impl GraphManager {
    pub fn new(
        initial_graph: Box<dyn AudioProcessor>,
        garbage_producer: Producer<Box<dyn AudioProcessor>>,
        overflow_garbage_producer: Option<Producer<Box<dyn AudioProcessor>>>,
        logger: Arc<RtLogger>,
    ) -> Self {
        Self {
            active_graph: AtomicPtr::new(Box::into_raw(Box::new(initial_graph))),
            pending_graph: AtomicPtr::new(std::ptr::null_mut()),
            garbage_producer,
            overflow_garbage_producer,
            logger,
        }
    }

    /// Checks if a new graph is pending and swaps it in if so.
    /// Returns a reference to the active graph.
    /// SAFETY: This must be called from the real-time thread.
    pub unsafe fn swap_if_pending(&mut self, metrics: &EngineMetrics, health_signal: &Arc<std::sync::atomic::AtomicBool>) -> &mut dyn AudioProcessor {
        let pending = self.pending_graph.swap(std::ptr::null_mut(), Ordering::Acquire);
        if !pending.is_null() {
            let old = self.active_graph.swap(pending, Ordering::AcqRel);
            if !old.is_null() {
                // Reconstruct the Box<Box<dyn AudioProcessor>> from the raw pointer.
                let old_graph_box = unsafe { Box::from_raw(old) };
                // Dereference once to get the inner Box<dyn AudioProcessor>.
                let old_graph = *old_graph_box;
                if let Err(leaked) = self.garbage_producer.push(old_graph) {
                    if let Some(ref mut overflow) = self.overflow_garbage_producer {
                        if let Err(leaked) = overflow.push(leaked) {
                            self.logger.log(crate::rt_logging::RtLogLevel::Error, "CRITICAL: Resource leak - garbage producers full", 0);
                            metrics.report_resource_leak(health_signal);
                            // leaked is Box<dyn AudioProcessor>. forget it to avoid dropping on RT thread.
                            std::mem::forget(leaked);
                        }
                    } else {
                        self.logger.log(crate::rt_logging::RtLogLevel::Error, "CRITICAL: Resource leak - garbage producer full", 0);
                        metrics.report_resource_leak(health_signal);
                        std::mem::forget(leaked);
                    }
                }
            }
        }
        let graph_ptr = self.active_graph.load(Ordering::Acquire);
        unsafe { &mut **graph_ptr }
    }

    /// Provides a mutable reference to the active graph.
    /// SAFETY: The caller must ensure exclusive access, typically by being on the RT thread.
    pub unsafe fn get_active_graph_mut(&self) -> &mut dyn AudioProcessor {
        let graph_ptr = self.active_graph.load(Ordering::Acquire);
        unsafe { &mut **graph_ptr }
    }

    pub fn set_pending_graph(&self, graph: Box<dyn AudioProcessor>) {
        let ptr = Box::into_raw(Box::new(graph));
        let old = self.pending_graph.swap(ptr, Ordering::AcqRel);
        if !old.is_null() {
             // If there was already a pending graph that wasn't swapped yet,
             // we should probably drop it or send it to garbage.
             // For now, simple drop as it hasn't reached RT yet.
             unsafe { drop(Box::from_raw(old)); }
        }
    }
}

impl Drop for GraphManager {
    fn drop(&mut self) {
        let ptr = self.active_graph.load(Ordering::Acquire);
        if !ptr.is_null() { unsafe { drop(Box::from_raw(ptr)); } }
        let pending = self.pending_graph.load(Ordering::Acquire);
        if !pending.is_null() { unsafe { drop(Box::from_raw(pending)); } }
    }
}
