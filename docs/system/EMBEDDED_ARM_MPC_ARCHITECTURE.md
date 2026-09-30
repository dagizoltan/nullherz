# Embedded ARM, Akai MPC & Standalone CDJ Multi-Target Architecture Specification

This specification documents Nullherz's architecture for cross-compiling, assembling, and deploying standalone hardware targets on embedded Linux platforms, specifically targeting **Akai MPC Live 1/2/One** and **Embedded CDJ/XDJ-style ARM hardware** (`armv7-unknown-linux-gnueabihf`).

---

## 1. Executive Summary & Hardware Context

Standalone music hardware units (such as Akai MPC Live/X/One grooveboxes or Pioneer CDJ/XDJ media players) run custom Embedded Linux operating systems on ARM SoCs with integrated touchscreen displays, physical pads/jogwheels, and hardware audio codecs.

Nullherz's **100% native Rust architecture** (`audio-core`, `nullherz-conductor`, `nullherz-processors`, `nullherz-topology`, `nullherz-ui-hal`) is specifically designed to deploy onto standalone embedded hardware:
* **Modest Memory Footprint**: Operates within a lightweight **150–200 MB RSS footprint**, leaving ample headroom inside 2 GB embedded RAM units.
* **Direct DRM/KMS Framebuffer Rendering**: Bypasses heavy desktop X11/Wayland window managers to render UI directly to the display framebuffer (`/dev/fb0` or DRM/KMS EGL).
* **Direct Evdev Control Interfacing**: Bypasses desktop event loops to parse physical pad grid velocities, rotary encoders, and jogwheel scratch movements directly from kernel input events (`/dev/input/event*`).

---

## 2. Embedded Hardware Deployability Matrix

| Standalone Hardware Target | CPU & SIMD Architecture | RAM Capacity | Audio Codec & Format | Display & Rendering | Input Hardware Driver | Target Latency Profile |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Akai MPC Live 1 / 2 / One** | Quad-core ARM Cortex-A17 @ 1.8 GHz (NEON SIMD) | 2 GB DDR3 | Cirrus Logic CS4272 (48 kHz / 24-bit PCM) | 7" 1280x800 Capacitive Touch (DRM/KMS EGL) | 16 RGB Pad Grid, 4 Q-Link Encoders, Evdev Buttons | **2.66 ms** (128 frames @ 48k) |
| **Embedded CDJ / XDJ Unit** | Quad-core ARM Cortex-A53 / A72 (NEON SIMD) | 2 GB - 4 GB DDR4 | High-End 192 kHz / 24-bit DAC (AKM / ESS) | 7" - 10" Touchscreen + Jog Wheel LCD | Optical Jogwheel Encoder, Pitch Fader, Hot Cues | **0.82 ms** (64 frames @ 96k / 192k) |
| **Embedded Micro-Mixer / FX Box** | Dual-core ARM Cortex-A7 (NEON SIMD) | 1 GB DDR3 | Multi-channel I2S Codec (48 kHz / 24-bit) | OLED / Low-res TFT Display (`/dev/fb0`) | Rotary Encoders & Push Switches | **1.33 ms** (64 frames @ 48k) |

---

## 3. Cross-Compilation Target & Portable Type System

### Target Triple
* **Architecture:** `armv7-unknown-linux-gnueabihf` (ARMv7 32-bit Hard Float with NEON SIMD) or `aarch64-unknown-linux-gnu` (64-bit ARM)

### 32-Bit vs 64-Bit Type Hardening
On 32-bit ARM embedded Linux targets:
1. **Resource Limits (`libc::rlimit`):** `rlim_cur` is `u32` (vs `u64` on x86_64). `crates/ipc-layer/src/lib.rs` safely converts `lim.rlim_cur as u64` to prevent type mismatches when querying `RLIMIT_RTPRIO` and `RLIMIT_MEMLOCK`.
2. **ALSA Frame Counts (`snd_pcm_uframes_t`):** `snd_pcm_uframes_t` maps to `c_ulong` (`u32` on 32-bit ARM vs `u64` on 64-bit x86_64). FFI parameters passed to ALSA functions use `.try_into().unwrap()` or `as snd_pcm_uframes_t`.
3. **C String Pointer Representation:** `c_char` on ARM is `u8` (unsigned). C string literals `c"..."` passed to FFI bindings are cast as `*const std::ffi::c_char`.

---

## 4. Composable Multi-Target Repository Layout

Nullherz employs a composable assembly pattern where shared domain crates (`audio-core`, `nullherz-conductor`, `nullherz-ui-hal`) are instantiated into distinct target binaries:

```text
nullherz/
├── crates/
│   ├── audio-core / audio-dsp / nullherz-topology   <-- Pure DSP & Graph Engine
│   ├── nullherz-conductor                           <-- Engine Orchestration
│   ├── nullherz-ui-hal                              <-- UI Abstraction & Touch Widgets
│   │
│   ├── nullherz-inspector (Desktop Assembly)         <-- Desktop DJ Studio & DAW
│   │   ├── Renderer: wgpu / Glow (OpenGL)
│   │   └── Input: Mouse, QWERTY Virtual MIDI, Generic MIDI Controllers
│   │
│   └── nullherz-mpc (Embedded Standalone Target)     <-- Standalone Touch UI & Hardware IO
│       ├── Renderer: DRM/KMS Framebuffer or Direct EGL
│       ├── Audio Driver: Direct ALSA MMAP
│       └── Hardware Controls: Evdev 16-Pad Grid, Optical Jogwheel Encoders, Evdev Buttons
```

---

## 5. Hardware Controller & Evdev Interfacing

The standalone embedded binary interacts directly with the Linux kernel input subsystem (`/dev/input/event*`):

* **Pad Grid:** 16 velocity & pressure (aftertouch) pad events mapped to `PerformanceCommand::TriggerSlice` / sampler voices.
* **Optical Jogwheel Encoder:** High-resolution tick counter mapped to `PerformanceCommand::ScratchGesture` in `audio-core`.
* **Q-Link Encoders:** Optical encoders mapped to active `MixerCommand::SetParam` / macro modulators.
* **Physical Buttons:** Hardware switches (`MAIN`, `MENU`, `SHIFT`, `HOT CUE`, `LOOP`) drive screen navigation and context toggles in `nullherz-ui-hal`.

---

## 6. Build Command Reference

```bash
# Install ARMv7 standard library
rustup target add armv7-unknown-linux-gnueabihf

# Cross-compile Nullherz core engine for standalone ARM hardware
cargo check --target armv7-unknown-linux-gnueabihf

# Build release target binary for standalone Akai MPC / CDJ hardware
cargo build --bin nullherz-mpc \
    --target armv7-unknown-linux-gnueabihf \
    --release
```
