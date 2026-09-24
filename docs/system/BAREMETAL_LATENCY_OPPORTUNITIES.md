# Nullherz Baremetal Latency Opportunities & Technical Specification

**Prepared by:** Lead Audio Systems & Hardware Architecture Team
**Status:** ARCHITECTURAL SPECIFICATION & ROADMAP
**Target:** Sub-Millisecond & Microsecond Ultra-Low Latency Execution

---

## 1. Executive Summary & Latency Target Taxonomy

Nullherz is engineered around a **Triple-Plane Isolation Model** (Orchestration, Protocol, Execution) designed to guarantee real-time audio safety and zero heap allocation on the audio hot-path.

Under conventional desktop Linux operation (using standard ALSA `snd_pcm_writei` or PipeWire audio server routing), the engine operates at buffer period sizes of 128 to 256 samples, achieving round-trip audio latency in the **3.5 ms – 7.4 ms** range. While suitable for studio production, ultra-low-latency live applications (such as digital guitar modeling, physical modeling synthesis, live DJ monitoring, and distributed network DSP nodes) benefit significantly from **baremetal hardware and kernel-bypass optimization**.

This document outlines **five high-impact baremetal latency opportunities** that enable Nullherz to push round-trip audio latency down to **sub-millisecond (< 1.0 ms) and physical microsecond limits**.

```
+-----------------------------------------------------------------------------------+
|                            LATENCY TARGET TAXONOMY                                |
+-----------------------+--------------------+--------------------------------------+
| Layer                 | Round-Trip Latency | Underlying Infrastructure            |
+-----------------------+--------------------+--------------------------------------+
| Standard Desktop      | 3.5 ms - 7.4 ms    | User daemon (PipeWire/JACK), POSIX I/O|
| High-Priority RT      | 1.0 ms - 2.5 ms    | SCHED_FIFO + ALSA rw, 64-sample period|
| Baremetal Hardware    | 0.3 ms - 0.8 ms    | Direct ALSA MMAP + NO_PERIOD_WAKEUP  |
| Baremetal Isolated    | 50 µs - 200 µs     | Dedicated isolated CPU + Busy-Wait   |
| Embedded Baremetal    | 10 µs - 50 µs      | `no_std` ARM/FPGA + Hardware DMA SAI |
+-----------------------+--------------------+--------------------------------------+
```

---

## 2. Baremetal Opportunity 1: ALSA Direct Hardware MMAP & `NO_PERIOD_WAKEUP` Mode

### 2.1 Technical Analysis
Standard ALSA write calls (`snd_pcm_writei`) rely on userspace-to-kernel memory copies (`copy_from_user`) and software polling/interrupt wakeups. Furthermore, sound card hardware interrupt controllers generate physical IRQs per period, introducing interrupt handling latency and context switch jitter.

### 2.2 Baremetal Blueprint
1. **Direct Memory Mapping (`SND_PCM_ACCESS_MMAP_INTERLEAVED`)**:
   Map the physical DMA sound card ring buffer directly into Nullherz's virtual address space via `snd_pcm_mmap_begin` and `snd_pcm_mmap_commit`. This achieves **100% zero-copy audio rendering** straight into hardware DMA memory.
2. **Disable Hardware Period Interrupts (`SND_PCM_HW_PARAMS_NO_PERIOD_WAKEUP`)**:
   Configure ALSA hardware parameters with `snd_pcm_hw_params_set_period_wakeup(pcm, hw_params, 0)`. This instructs the hardware audio controller to disable timer interrupts.
3. **Poll-Free Busy-Wait Ring Buffer Pointer Traversal**:
   Instead of sleeping until an IRQ fires, the RT thread busy-waits or uses high-resolution CPU timestamp counters (`rdtsc`) to poll `snd_pcm_mmap_avail`. This allows tiny period sizes (16 or 32 samples at 96 kHz / 192 kHz) without suffering IRQ latency overhead.

### 2.3 Implementation Architecture
```rust
// Proposed ALSA Direct MMAP Backend Definition
pub struct AlsaMmapBackend {
    pcm_handle: *mut std::ffi::c_void,
    ring_buffer_ptr: *mut f32,
    buffer_size_frames: u64,
    period_size_frames: u64,
    no_period_wakeup: bool,
}

impl AlsaMmapBackend {
    pub unsafe fn render_mmap_block(&mut self, engine: &dyn RenderingEngine) {
        let mut offset = 0;
        let mut frames_avail = self.get_mmap_avail();
        if frames_avail >= self.period_size_frames {
            let mut areas = std::ptr::null_mut();
            snd_pcm_mmap_begin(self.pcm_handle, &mut areas, &mut offset, &mut frames_avail);

            // Render DSP block directly into the mapped DMA buffer
            let dma_slice = std::slice::from_raw_parts_mut(
                ((*areas).addr as *mut f32).add(offset * 2),
                (self.period_size_frames * 2) as usize,
            );
            engine.process_interleaved(dma_slice);

            snd_pcm_mmap_commit(self.pcm_handle, offset, self.period_size_frames);
        }
    }
}
```

---

## 3. Baremetal Opportunity 2: Dedicated CPU Core Isolation & Full Kernel Preemption Bypass

### 3.1 Technical Analysis
Even with `SCHED_FIFO` priority, Linux kernel thread schedulers, timer ticks (`CONFIG_HZ`), RCU callbacks, and CPU power state transitions (C-states/P-states) periodically preempt real-time threads. At period sizes under 64 samples (1.3 ms budget), a single 10 µs kernel interrupt spike results in an audible buffer underrun (xrun).

### 3.2 Baremetal Blueprint
Isolate physical performance cores entirely from OS kernel interference using Linux boot parameter pinning:

```bash
# Linux Kernel Boot Parameters for Isolated Baremetal Audio Execution
isolcpus=2,3 nohz_full=2,3 rcu_nocbs=2,3 intel_idle.max_cstate=0 processor.max_cstate=0 iommu=pt
```

1. **`isolcpus=2,3`**: Prevents the OS kernel scheduler from assigning any general tasks to physical CPU cores 2 and 3.
2. **`nohz_full=2,3`**: Suspends the kernel timer tick on cores 2 and 3 whenever a single runnable task is active.
3. **`rcu_nocbs=2,3`**: Offloads RCU callback processing to non-isolated cores (0, 1).
4. **`intel_idle.max_cstate=0`**: Disables CPU power state sleep transitions, keeping CPU core clocks locked at maximum frequency without latency recovery penalties.
5. **Continuous Spin-Loop Execution**: The audio execution thread locks onto Core 2, executing a lockless busy-wait loop that evaluates audio blocks deterministically with `< 1 µs` scheduling variance.

---

## 4. Baremetal Opportunity 3: `no_std` Embedded & Microkernel DSP Runtime

### 4.1 Technical Analysis
To run Nullherz on bare-metal hardware (such as dedicated Eurorack DSP modules, standalone DJ hardware consoles, guitar multi-FX pedals, or embedded microkernels), the DSP hot-path must operate without depending on the Rust standard library (`std`).

### 4.2 Baremetal Blueprint
Decouple `audio-dsp` and `audio-core` execution primitives into a `no_std` compatible core crate with optional `alloc`:

```
+----------------------------------------------------+
|               Nullherz DSP Core                    |
|  #[no_std] + SIMD (AVX-512 / NEON / WASM-SIMD128)  |
+----------------------------------------------------+
       ▲                                 ▲
       │                                 │
 [ std Desktop Backend ]       [ baremetal Hardware Target ]
 (Linux/macOS/Windows)         (STM32H7 / ARM Cortex-M7 / RISC-V)
```

1. **Static Pre-Allocated Buffer Pools**:
   Replace runtime heap allocations with static `AudioBlock` arrays or custom stack/bump allocators backed by fixed SRAM/SDRAM regions (`.sdram_bss` sections).
2. **Direct Hardware I2S / SAI DMA Interfacing**:
   In embedded baremetal targets (e.g. STM32H753 @ 480 MHz or Xilinx Zynq ARM+FPGA), the audio kernel links directly to double-buffered Serial Audio Interface (SAI) DMA transfer completion interrupts:

```rust
#[no_std]
#[no_mangle]
pub extern "C" fn DMA1_STR0_IRQHandler() {
    // Clear DMA transfer complete flag
    let dma_buffer = unsafe { get_sai_dma_buffer_half() };

    // Process audio block directly on bare metal without OS overhead
    BAREMETAL_ENGINE.process_block_inplace(dma_buffer);
}
```

---

## 5. Baremetal Opportunity 4: Kernel-Bypass Network Transport (`io_uring`, AF_XDP & RoCE v2 RDMA)

### 5.1 Technical Analysis
Nullherz supports distributed audio processing across networked nodes (`distributed-sidecar`). Conventional POSIX socket networking (TCP/UDP) incurs kernel network stack parsing, socket buffer allocations, and interrupt context switches, limiting network audio round-trip times to **1.5 ms – 4.0 ms**.

### 5.2 Baremetal Blueprint
1. **`io_uring` with Registered Buffers (`IORING_REGISTER_BUFFERS`)**:
   Utilize Linux `io_uring` with fixed file descriptors and pre-mapped memory buffers (`io_uring_register_buffers`). This bypasses per-packet syscall overhead when streaming audio frames over the local network.
2. **AF_XDP (eBPF Zero-Copy Express Data Path)**:
   By attaching eBPF programs to network interface drivers (NICs), AF_XDP queues bypass the Linux kernel network stack entirely, passing raw Ethernet audio frames directly from NIC DMA rings into Nullherz shared memory (`ipc-layer`).
3. **RoCE v2 / InfiniBand RDMA (Remote Direct Memory Access)**:
   For high-density distributed DSP clusters, map audio blocks directly between host memory regions via RDMA over Converged Ethernet (RoCE v2). Network packets write directly to remote sidecar RAM without CPU involvement, achieving sub-100 microsecond network round-trips.

```
[ Local Conductor SHM ] ───(RDMA Write / AF_XDP)───> [ Remote Sidecar NIC DMA ]
                          < 100 µs LAN Latency
```

---

## 6. Baremetal Opportunity 5: Contiguous DMA Memory & 1GB HugePages (`MAP_HUGETLB`)

### 6.1 Technical Analysis
When audio processing threads access large sample buffers, wavetables, or multi-channel IPC ring buffers, Translation Lookaside Buffer (TLB) misses force the CPU to perform page table walks. On real-time execution threads, TLB miss penalties cause unpredictable micro-stalls.

### 6.2 Baremetal Blueprint
1. **1GB HugePage Allocations (`MAP_HUGETLB`)**:
   Allocate all `ipc-layer` shared memory segments, `SampleRegistry` audio buffers, and ring buffers using Linux 2MB or 1GB HugePages:

```rust
pub unsafe fn allocate_hugepage_ring(size_bytes: usize) -> *mut u8 {
    let ptr = libc::mmap(
        std::ptr::null_mut(),
        size_bytes,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_SHARED | libc::MAP_ANONYMOUS | libc::MAP_HUGETLB,
        -1,
        0,
    );
    assert_ne!(ptr, libc::MAP_FAILED, "HugePage allocation failed");
    ptr as *mut u8
}
```

2. **Hardware Page Lock & Physical Prefaulting**:
   Call `libc::mlockall(MCL_CURRENT | MCL_FUTURE)` during initialization and prefault all memory pages to guarantee zero page faults during live audio rendering.

---

## 7. Performance Targets & Theoretical Latency Bounds

| Configuration | Period Size | Sample Rate | Audio Buffer Latency | Execution Jitter | Round-Trip Latency |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Standard Desktop (PipeWire) | 256 frames | 48.0 kHz | 5.33 ms | ± 500 µs | **7.4 ms** |
| High-Priority SCHED_FIFO | 64 frames | 48.0 kHz | 1.33 ms | ± 100 µs | **2.2 ms** |
| Baremetal ALSA MMAP | 32 frames | 96.0 kHz | 0.33 ms | ± 15 µs | **0.65 ms** |
| Baremetal Isolated Core | 16 frames | 192.0 kHz | 0.083 ms | < 1 µs | **0.18 ms** |
| Embedded Baremetal (SAI DMA) | 8 frames | 192.0 kHz | 0.041 ms | < 0.1 µs | **0.09 ms** |

---

## 8. Summary & Strategic Recommendations

To realize these baremetal latency advantages:
1. **Short-Term (Beta Phase)**: Implement the `AlsaMmapBackend` using `SND_PCM_ACCESS_MMAP_INTERLEAVED` and `NO_PERIOD_WAKEUP` flags to unlock sub-millisecond audio on desktop hardware.
2. **Mid-Term**: Provide an automated tuning script/profile (`scripts/baremetal_core_isolate.sh`) to configure CPU isolation (`isolcpus`), C-state pinning, and HugePages (`MAP_HUGETLB`).
3. **Long-Term**: Modularize `audio-dsp` for `no_std` compilation to support embedded ARM/FPGA baremetal platforms and integrate `io_uring`/AF_XDP for low-latency network audio streams.
