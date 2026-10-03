# Nullherz Workstation — UI Architecture & Design System Specification

## 1. Executive Summary & Design Principles

Nullherz is a native Rust real-time DJ/DAW music workstation built on `egui` and `WGPU`. The user interface is modeled after high-end hardware performance instruments and studio workstations.

### Global Design Pillars
1. **Triple-Plane Spatial Isolation**: Control/Orchestration plane, Real-Time Audio/DSP plane, and Presentation/egui plane are decoupled via IPC & ring buffers.
2. **Hardware-Exact Ergonomics**:
   - Deck A Accent: Cyan `#00DCFF` (`egui::Color32::from_rgb(0, 220, 255)`)
   - Deck B Accent: Gold `#FFD700` (`egui::Color32::from_rgb(255, 215, 0)`)
   - Deck C Accent: Magenta `#FF69B4` (`egui::Color32::from_rgb(255, 105, 180)`)
   - Deck D Accent: Violet `#9370DB` (`egui::Color32::from_rgb(147, 112, 219)`)
   - Minimum tactile target size: `44px` for performance-critical transport and cue controls.
3. **Needle Waveform Model**: Center-fixed playhead alignment with frame-accurate playback position and constant visible horizon (~8s).
4. **Multi-Viewport Architecture**: Detachable views (Console, Mixer, Analyzer, Visuals, Sidecars) with shared state and theme synchronization.
5. **SoundDNA & Stem Matrix**: 12-stem matrix (Acapella, Instrumental, Drums+Bass, Reset) and spectral/perceptual music descriptors integrated into performance workflows.
6. **State & Intent Transparency**: Every control explicitly distinguishes between `DISCOVERED`, `SELECTED`, `CONFIGURED`, `ACTIVE`, `FAILED`, and `UNAVAILABLE`.

---

## 2. Component Hierarchy & Module Organization

```text
crates/nullherz-inspector/src/
├── main.rs                  # Window entrypoint, top bar, status bar, viewport manager
├── state.rs                 # Central UI state wrapper & telemetry smoothing
├── ui_harness.rs            # Headless UI testing & integration harness
└── views/
    ├── dj_studio/
    │   ├── mod.rs           # Multi-deck console view
    │   ├── render.rs        # Deck panel & stem matrix rendering
    │   ├── waveform.rs      # Spectral WGPU / raw waveform canvas & playhead
    │   └── dna.rs           # SoundDNA genetic visualization & shaping controls
    ├── player.rs            # Dedicated 4-deck transport & playhead inspector
    ├── mixer.rs             # 4-channel DJ mixer, EQ, filters, crossfader, VU meters
    ├── analyzer.rs          # ISO 226 loudness, spectral tilt, Camelot pitch detector
    ├── visuals.rs           # Audio-reactive shader viewports & sidecar pipelines
    ├── composer.rs          # Step sequencer / timeline workflow
    ├── sampler.rs           # Performance pad sampler
    ├── store.rs / sidecars  # Sidecar plugin registry
    └── settings/
        ├── audio.rs         # Device discovery, routing, sample rate, latency diagnostics
        ├── midi.rs          # Hardware controller mappings
        ├── preferences.rs   # Performance mode, frame rate, theme configuration
        └── mod.rs
```

---

## 3. UI Control & Telemetry Propagation Path

```text
User Interaction (egui Widget)
    ↓
Inspector App Command Queue (`InspectorAppCommand`)
    ↓
Conductor IPC Channel (`NullherzConductor`)
    ↓
Audio Core / Real-time Engine Processing
    ↓
Engine Telemetry Packet (`DeckTelemetry`, `MixerTelemetry`)
    ↓
Inspector State Synchronization (`state.rs`)
    ↓
egui Surface Re-render (~60Hz performance / ~5Hz idle)
```

---

## 4. State & Intent Transparency Matrix

| Component | Requested State | Engine Telemetry | Rendered Status Badge | Actionable Feedback |
| :--- | :--- | :--- | :--- | :--- |
| Audio Device | User-selected device ID | Active ALSA/Jack/PA Handle | `ACTIVE` / `FAILED` / `SELECTED` | Fallback device notice & Retry prompt |
| Deck Transport | Play/Pause Command | Telemetry position advancing | `PLAYING` / `PAUSED` / `STALLED` | Auto-recovery from momentary buffer stalls |
| Performance Mode | Preset requested | RT priority, thread affinity | `REALTIME 48kHz 64f (2.7ms)` | Resource utilization & buffer diagnostics |
| Stem Separation | DNA shape requested | Stem volume gains | `12-STEM ACTIVE` | Individual stem peak meter & solo indicators |

---

## 5. Visual Hierarchy Levels

- **Level 1 (Performance Critical)**: Center needle playhead, deck transport buttons (Play/Cue/Sync), master/channel VU meters, crossfader, BPM.
- **Level 2 (Musical Context)**: Key / Camelot tag, SoundDNA descriptors (tilt, syncopation), 12-stem matrix, track title & artist.
- **Level 3 (Technical & Configuration)**: Audio buffer sizes, DSP graph load %, sidecar socket paths, ALSA backend handles, network sync status.
