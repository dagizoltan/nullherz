# IPC Strategy: Zero-Copy Shared Memory

To achieve ultra-low latency between the RT Core and DSP Sidecars, we utilize a zero-copy shared memory approach.

## 1. Shared Memory Buffers
- Audio data is stored in pre-allocated shared memory segments (`/dev/shm` on Linux).
- Both the RT Core and Sidecar processes map the same memory region.

## 2. Synchronization (Lock-Free)
- We use circular buffers (ring buffers) with atomic head/tail pointers.
- No mutexes or condition variables are used in the RT path.
- Event notification (if needed for non-spinning sidecars) can use `eventfd` or similar Linux-specific low-latency primitives.

## 3. Command Queue
- Control commands are also passed via atomic ring buffers.
- Commands are timestamped to ensure deterministic application at the correct sample offset.

## 4. Safety
- Shared memory regions are managed to prevent out-of-bounds access.
- Each sidecar has its own dedicated input/output buffers to avoid contention.
