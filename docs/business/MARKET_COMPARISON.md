# Nullherz: Market Competitor & Performance Comparison

**Last Updated:** July 18, 2026
**Status:** Living Document (Continuously Updated)

---

> **How to read this document:** §1–§3 benchmark against the legacy DJ/DAW incumbents — an *engineering yardstick*, not a market map; per the [Strategic Assessment](./STRATEGIC_ASSESSMENT_2026_07.md) we do not intend to meet Traktor/Ableton/Rekordbox in their own categories. **§4–§6 are the comparisons that actually matter**: one competitive set per candidate identity. Claims in the Nullherz columns are tagged **[V]** when backed by tests/CI in this repo, **[M]** when measured, and **[D]** when design-intent not yet proven on hardware.

## 1. Legacy Landscape (Engineering Yardstick)

| Competitor | Category | Target Audience | Core Technology |
| :--- | :--- | :--- | :--- |
| **Traktor Pro** | DJ Performance | Touring DJs / Pros | C++ (Legacy) |
| **Mixxx** | Open Source DJ | OSS Community / Hobbyists | C++ / Qt |
| **Ableton Live** | Studio / Live | Producers / Performers | C++ (Legacy) |
| **SuperCollider**| Programmatic DSP | Researchers / Advanced Devs | C++ / SClang |
| **Nullherz** | **Engine + Instrument (identity pending validation)** | **Tech-Forward Producers / Rust Devs** | **Rust / Triple-Plane** |

## 2. Technical Performance Comparison

| Metric | Traktor Pro | Mixxx | Ableton Live | SuperCollider | **Nullherz** |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **RT Safety** | High | Medium | High | High | **No-Alloc hot path, lint-enforced [V]** |
| **Memory Model**| Manual/Arc | Manual | Manual | Manual | **Memory-Safe (Rust) [V]** |
| **Parallelism** | Coarse | Single-Threaded | Stage-Based | Multi-Server | **Graph task pool + SIMD [V]** |
| **Jitter Floor** | < 1ms | ~2-5ms | < 1ms | < 0.1ms | **Pending hardware Survival/RTL runs [D]** [1] |
| **Plugin Isolation**| None (Crashes) | None | Sandbox (v11+) | External Process | **Sidecar processes, cgroups + heartbeat fallback [V]** |
| **Modulation** | Fixed | Scriptable | Clip-Based | Dynamic | **Modulation Matrix (test-pinned) [V]** |

## 3. Feature Set Deep-Dive (unchanged from June assessment)

### 3.1 Performance & Intelligence
| Feature | Traktor Pro | Mixxx | Ableton | **Nullherz** |
| :--- | :---: | :---: | :---: | :---: |
| **BPM Analysis** | Offline | Offline/Online | Online/Warp | **Concurrent Analysis [V]**|
| **Key Detection** | Proprietary | Analyzer | Complex | **12-Bin Chromagram [V]** |
| **Transient Sync** | Beat-Grid | Beat-Grid | Warp-Markers | **Phase-Locked RT [V]** |
| **Live Looping** | 4-8 slots | 8 slots | Clip-Grid | **Sidecar-extensible [D]**|

### 3.2 Studio & Arrangement
| Feature | Traktor Pro | Mixxx | Ableton | **Nullherz** |
| :--- | :---: | :---: | :---: | :---: |
| **Sequencing** | None | Basic | Industry Std | **16x64 Step Grid [V]** |
| **Automation** | Basic | Mapping | Complex/MPE | **Ramped Macro Bus [V]** |
| **Project State** | Library DB | SQLite | .als Project | **redb + JSON/rkyv round-trip [V]** |
| **Modularity** | Fixed FX | Scripted | Max4Live | **Sidecar SDK (contract test-pinned) [V]** |

---

## 4. Identity 1 — Rust Audio-Engine Infrastructure

*The bet: become the embeddable, crash-isolated, verification-friendly audio engine the Rust ecosystem lacks ("the Bevy of audio").*

| | JUCE | Tracktion Engine | CLAP (ABI) | cpal / rodio / fundsp | SuperCollider (server) | **Nullherz engine** |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **Language / safety** | C++ | C++ | C ABI spec | Rust | C++ | **Rust end-to-end [V]** |
| **Scope** | Full app framework | DAW engine | Plugin ABI only | I/O / playback / DSP pieces | Synthesis server | **Graph engine + supervisor + IPC + persistence [V]** |
| **Crash isolation** | None | None | Host-dependent | None | Client/server split | **Per-node process isolation, heartbeat fallback, safe mode [V]** |
| **Formal/machine verification** | — | — | — | — | — | **Kani proofs on servo/jitter/parallel-exec invariants [V]** |
| **License / cost** | Dual GPL/commercial | Dual | MIT | MIT/Apache | GPL | Undecided — **decisive for this identity** |
| **Maturity / adoption** | Industry standard | Shipping products | Fast-growing | Fragmented, hobby-heavy | Decades of research use | **Pre-adoption** |
| **Docs / examples** | Extensive | Good | Good | Uneven | Extensive | **Thin — top gap for this identity** |

**Honest read:** no Rust competitor offers an integrated engine of this scope — the niche is genuinely open, and crash isolation + machine-checked invariants are differentiators none of the column has. But JUCE-level docs and a chosen license are prerequisites to compete for adoption at all. The [Adoption Probe](./STRATEGIC_ASSESSMENT_2026_07.md) (extract `nullherz-engine`, publish, measure a quarter) is the falsifier.

## 5. Identity 2 — The Genetic Instrument

*The bet: SoundDNA breeding/transfusion as a novel instrument experience, dropping the DJ/DAW pretense.*

| | VCV Rack | TidalCycles | Endlesss | Koala Sampler | **Nullherz Breeder** |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **One-line idea** | Modular rack in software | Live-coded patterns | Collaborative jam loops | Pocket sampler | **Breed sounds like organisms** |
| **Idea originality** | Port of hardware paradigm | High | High | Low (execution win) | **High — no direct equivalent found** |
| **Barrier to first joy** | Medium (patching) | High (code) | Low | Very low | **Unknown — Stranger test pending [D]** |
| **Platform** | Win/mac/Linux | Any (terminal) | iOS/desktop (†2024) | iOS/Android/desktop | **Linux only — ceiling** |
| **Community moat** | Huge module ecosystem | Academic + live-coding scene | Died with the company | Casual mass market | **None yet; gossip/P2P DNA exchange is the seed [V]** |
| **Monetization** | Paid modules | None (OSS) | Subscription (failed) | One-time purchase | Undecided |

**Honest read:** niche instruments live or die on the 15-minute experience, and ours is untested on outsiders — that's the whole reason the Stranger test gates this identity. Endlesss is the cautionary tale in this table: a genuinely original collaborative idea, VC-funded, subscription-priced — and it still shut down in 2024 when the novelty didn't convert to retention. The lesson we take: keep the cost base at zero (OSS core), let the idea prove retention before any monetization architecture.

## 6. Identity 3 — Distributed Live Audio (Installations / Performance)

*The bet: clock-synced multi-machine DSP over commodity networks, below Dante's price and above JackTrip's integration depth.*

| | Dante | AES67 / Ravenna | AVB / Milan | JackTrip | SonoBus | **Nullherz distributed** |
| :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **Cost model** | Per-device licensing | Standard (impl. varies) | Certified hardware | Free (OSS) | Free (OSS) | **Free, commodity NICs** |
| **Special hardware** | Licensed chips/software | PTP-capable network | AVB switches required | None | None | **None** |
| **Clock discipline** | Proprietary + PTP | PTP (IEEE 1588) | gPTP (802.1AS) | None (buffer-based) | Adaptive resampling | **PTP-style measured path delay, PI servo (Kani-proved clamp) [V][M]** |
| **Timestamping** | Hardware | Hardware | Hardware | Software | Software | **Software now; SO_TIMESTAMPING groundwork done [D]** |
| **DSP on remote nodes** | Transport only | Transport only | Transport only | Transport only | Transport only | **Remote *processing* (sidecar offload), not just transport [V]** |
| **Ecosystem trust** | Industry standard | Broadcast standard | Automotive/pro-AV | Academic/community | Musician community | **None yet** |

**Honest read:** the genuinely differentiated cell in this table is *remote DSP* — every incumbent moves audio; none offload processing graphs to remote machines as a first-class concept. Against that: Dante's moat is certification and trust, not technology, and hardware timestamping (which we lack, software-only for now) is what "pro" means in this market. The RTL/Survival numbers decide whether this identity is credible at the installation/art tier, which does not require certification.

---

## 7. Roadmap vs Market Trends

| Market Trend | Competitor Status | Nullherz Response |
| :--- | :--- | :--- |
| **Distributed DSP** | Transport-only (Dante/AES67) | Remote sidecar *processing* is implemented [V]; RDMA parked as research. |
| **AI Integration** | VST-bolted | Core-level off-thread analysis workers [V]; true neural latent space is R&D (see [R&D Strategy](./R_AND_D_STRATEGY.md)). |
| **Plugin safety** | CLAP/VST3 in-process | Process isolation + supervisor is shipped and test-pinned [V]. |
| **Mobile/Embedded**| iPad apps | Rust portability is real, but untested on ARM targets [D]. |

---

[1] **Jitter/latency claims — MEASURED 2026-07-29, policy retired.** See §8.

**Correction to the previous text of this footnote**, which read *"the survival harness … and RTL calibration exist and are verified in CI [V]"*. The survival harness is real and does gate xruns. **RTL calibration is not.** `calibration_samples` is a config field plumbed through `SystemConfig` with no measurement behind it — there is no round-trip latency routine anywhere in the tree. A `[V]` tag on a thing that does not exist is the exact failure this document's tagging system is meant to prevent, so it is called out rather than quietly edited.

Consequence: the converter/interface term in §8 is an **estimate**, not a measurement, and will stay that way until a real loopback RTL exists.

---

## 8. Latency — measured, 2026-07-29

**This section replaces the "qualitative by policy" rule in [1].** Nullherz figures are measured on the reference laptop (4-core, ALC298 onboard codec, ALSA, 48 kHz). **Competitor figures are typical operating ranges from published specs and community reporting — NOT measured here**, and methodology across vendors is inconsistent (most quote buffer size, not round-trip). Treat the competitor column as an order-of-magnitude sanity check, not a benchmark.

Latency decomposes into three terms, and only the last is hardware:

| term | set by | Nullherz status |
| :--- | :--- | :--- |
| Signal path (DSP) | code — identical on any machine | **[M]** measured per block size |
| Output buffer | period × buffer-periods | **[M]** measured on the ALC298 |
| Converter (DAC) | the interface | **[D]** estimated; no RTL exists (see [1]) |

**Signal path, measured** (impulse fed to a deck, observed at master; `conductor/examples/probe_deck_latency.rs`):

| block | RAW | + KeySync (realtime) |
| ---: | ---: | ---: |
| 256 | 7.33 ms | 28.67 ms |
| 64 | **3.33 ms** | 24.67 ms |
| 32 | 2.67 ms | 24.00 ms |

Each FFT node adds a flat **21.33 ms** irrespective of block size — that is the analysis window, not the workload. Verified by detaching KeySync and re-measuring (352 and 160 samples, matching the arithmetic exactly).

**Total output latency:**

| configuration | Nullherz (measured + est. DAC) | typical competitor range [unverified] |
| :--- | ---: | :--- |
| **Shipping default** (period 256 × 8 buffers) | **~51–53 ms** | — |
| Tuned (period 64 × 3), RAW | **~8–10 ms** | Traktor / Serato / rekordbox: ~5–15 ms on a good interface |
| Tuned, DNA via **pre-rendered** key shift | **~8–10 ms** | — (no competitor does this) |
| Tuned, realtime spectral (key lock class) | ~30–32 ms | Key-lock engaged costs competitors a window too |
| Aggressive (period 32 × 2), RAW | **~5–7 ms** | Dedicated hardware (CDJ/SC6000): ~2–5 ms |

**Three honest readings of this table:**

1. **The engine is competitive; the shipping default is not.** ~8–10 ms tuned sits inside the range serious DJ software operates in. **~52 ms out of the box does not**, and it is an environment variable away (`NULLHERZ_BUFFER_PERIODS`, period size). Fix the default before quoting any of this externally.
2. **The 21 ms FFT is not a Nullherz weakness — it is physics everyone pays.** Traktor, Serato and Mixxx all use phase-vocoder-class pitch shifting and all incur window latency when key lock is on. The difference today is that **Nullherz pays it unconditionally, for a feature that defaults to off.**
3. **Pre-rendering is a genuine structural advantage, and it exists only because of a gap.** A static key shift can be rendered at load (~6 s for a 6-minute stereo track, ~120× realtime — `probe_prerender_keyshift.rs`), giving DNA-with-key at **zero** added latency. Competitors cannot do this for *key lock*, because key lock is dynamic. Nullherz can only do it because it has no key lock — see §9.

---

## 9. The gap this comparison does not currently show: key lock

**No row in §2 or §3 covers master tempo / key lock, and it is table stakes.** Traktor, Serato, rekordbox and Mixxx all hold pitch constant while tempo varies. Nullherz does not: tempo sync changes `playback_rate`, which resamples, so **changing tempo changes pitch** — turntable behaviour. `KeySync` is written to only from the KEY latch and is never driven by tempo.

That is a defensible *design* position (some DJs want vinyl behaviour) but it must be a stated choice, not an omission. As of this document it was neither stated nor visible in any comparison table, which meant the feature matrix implied parity that does not exist.

If key lock is ever added it is **inherently realtime** — the correction changes every block, so it cannot be pre-rendered, and it will cost a window like everyone else's.

---

**Comparison Integrity:** *Maintained by the Nullherz Engineering & Product Strategy Team. Every [V] tag is backed by a test or CI check in this repository; challenge any tag that isn't.* Footnote [1] records a `[V]` tag that did not survive that challenge.
