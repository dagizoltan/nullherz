# Embedded ARM & Akai MPC Live 1 Multi-Target Architecture Specification

This specification documents Nullherz's architecture for cross-compiling, assembling, and executing on embedded ARM Linux platforms, specifically targeting the **Akai MPC Live 1** (`armv7-unknown-linux-gnueabihf`).

---

## 1. Executive Summary & Hardware Context

The Akai MPC Live 1 is a standalone hardware groovebox running an Embedded Linux kernel on a quad-core ARM Cortex-A17 SoC with ARM NEON SIMD acceleration, 2 GB RAM, a 7" 1280x800 capacitive touchscreen, a Cirrus Logic CS4272 audio codec, 16 velocity/pressure-sensitive RGB pad grid, 4 endless Q-Link encoders, and physical navigation buttons.

Nullherz's 100% native Rust architecture (`audio-core`, `nullherz-conductor`, `nullherz-processors`, `nullherz-topology`) is inherently portable to ARM. It operates within a modest ~150–200 MB RSS footprint, leaving ample headroom inside the MPC Live 1's 2 GB RAM.

---

## 2. Cross-Compilation Target & Portable Type System

### Target Triple
* **Architecture:** `armv7-unknown-linux-gnueabihf` (ARMv7 32-bit Hard Float with NEON)

### Cross-Compilation Verification
The primary DSP audio graph and IPC crates cross-compile cleanly on ARMv7:
- `audio-core`, `audio-dsp`, `nullherz-topology`, `nullherz-traits`, `nullherz-processors`, `ipc-layer`, `sidecar-sdk`, and all sidecars.

### 32-bit vs 64-bit Type System Hardening
On 32-bit ARM Linux targets:
1. **Resource Limits (`libc::rlimit`):** `rlim_cur` is `u32` (vs `u64` on x86_64). `crates/ipc-layer/src/lib.rs` safely converts `lim.rlim_cur as u64` to prevent type mismatches when querying `RLIMIT_RTPRIO` and `RLIMIT_MEMLOCK`.
2. **ALSA Frame Counts (`snd_pcm_uframes_t`):** `snd_pcm_uframes_t` maps to `c_ulong` (`u32` on 32-bit ARM vs `u64` on 64-bit x86_64). FFI parameter passed to ALSA functions use `.try_into().unwrap()` or `as snd_pcm_uframes_t`.
3. **C String Pointer Representation:** `c_char` on ARM is `u8` (unsigned). C string literals `c"..."` passed to `dlopen` FFI bindings are cast as `*const _` or `*const std::ffi::c_char`.

---

## 3. Composable Multi-Target Repository Layout

Nullherz employs a composable assembly pattern where shared domain crates (`audio-core`, `nullherz-conductor`, `nullherz-ui-hal`) are instantiated into distinct target binaries:

```text
nullherz/
├── crates/
│   ├── audio-core / audio-dsp / nullherz-topology   <-- Pure DSP & Graph Engine
│   ├── nullherz-conductor                           <-- Engine Orchestration
│   ├── nullherz-ui-hal                              <-- UI Abstraction & Touch Widgets
│   │
│   ├── nullherz-inspector (Desktop Assembly)         <-- 5-Column DJ Studio, 64-Step Composer
│   │   ├── Renderer: wgpu / Glow (OpenGL)
│   │   └── Input: Mouse, QWERTY Virtual MIDI, Generic MIDI Controllers
│   │
│   └── nullherz-mpc (MPC Live 1 Standalone Target)   <-- Standalone Touch UI & Hardware IO
│       ├── Renderer: DRM/KMS Framebuffer or EGL
│       ├── Resolution: Fixed 1280x800 (7" Touchscreen)
│       ├── Audio Driver: Direct ALSA MMAP (`cs4272` codec)
│       └── Hardware Controls: 4x4 Pad Grid, 4 Q-Link Encoders, Evdev Buttons
```

---

## 4. Hardware Controller & Evdev Interfacing

The standalone MPC binary interacts directly with the Linux kernel input subsystem (`/dev/input/event*`):

* **Pad Grid:** 16 velocity & pressure (aftertouch) pad events mapped to `PerformanceCommand::TriggerSlice` / sampler voices.
* **Q-Link Encoders:** 4 optical encoders mapped to active `MixerCommand::SetParam` / macro modulators.
* **Physical Buttons:** Hardware switches (`MAIN`, `MENU`, `SHIFT`, `TRACK MUTE`, `ERASE`) drive screen navigation and context toggles in `nullherz-ui-hal`.

---

## 5. Performance Metrics & Expected Latency Budget on MPC Live 1

| Metric | MPC Live 1 Target Profile |
| :--- | :--- |
| **CPU Target** | Quad-core ARM Cortex-A17 @ 1.8 GHz |
| **Audio Format** | 48 kHz / 24-bit PCM via CS4272 codec |
| **Block Size / Latency** | 128 samples @ 48 kHz = **2.66 ms period latency** |
| **DSP Vector Acceleration** | ARM NEON 128-bit SIMD via `wide::f32x4` |
| **RAM Footprint** | ~150–200 MB RSS (out of 2,000 MB available) |
| **GUI Rendering** | 60 FPS direct framebuffer rendering (`/dev/fb0` or EGL) |

---

## 6. Build Command Reference

```bash
# Install ARMv7 standard library
rustup target add armv7-unknown-linux-gnueabihf

# Cross-compile Nullherz core engine for MPC Live 1
cargo check --target armv7-unknown-linux-gnueabihf

# Build release target binary
cargo build --bin nullherz-mpc \
    --target armv7-unknown-linux-gnueabihf \
    --release
```
