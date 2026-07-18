# Nullherz: 3-Month Strategic Roadmap (Updated July 2026)

**Timeline:** July 2026 – September 2026
**Focus:** Stability, Federated Intelligence, and Production Scale

---

## Month 1: Stabilization & The 5 Layers of Transfusion [COMPLETED]
*Goal: Finalize the primary control interfaces and ensure total signal reliability.*

*   **MIDI Mapping System [DONE]:** Implemented declarative JSON mapping engine.
*   **Intelligence Perfection [DONE]:** BPM and Root Key detection stabilized and integrated.
*   **Safe-Mode & Recovery [DONE]:** "Soft Fallback" and X-RUN detection operational.
*   **Core Hardening [DONE]**: Formal verification of Graph Executor and Wasm-SIMD integration.

---

## Month 2: Federated Intelligence & DNA Discovery [COMPLETED / BETA]
*Goal: Scale the genetic ecosystem beyond the local machine.*

*   **Federated P2P Genetic Cloud [DONE]:**
    *   Implemented `CloudPeerSync` gossipsub control overlay supporting GRAFT, PRUNE, IHAVE, and IWANT.
    *   Integrate cryptographically signed sound DNA lineages with consensus tracking.
*   **Genetic Sequencer [DONE]:**
    *   DNA-aware MIDI pattern mutation based on rhythmic genetic markers and 12-entry micro-timing.
    *   Groove transfusion copying attributes between sequencer tracks via dedicated command.
*   **WASM SDK Maturity [DONE]:**
    *   Direct zero-copy memory-mapping getters for guest plugins to eliminate serialization.

---

## Month 3: Ecosystem & Scale (Infrastructure for Growth) [IN PROGRESS]
*Goal: Scale the library and prepare for distributed processing.*

*   **Library Optimization [DONE]:** Optimized `redb` queries for >100k entries and non-blocking background loading.
*   **Remote Sidecar Protocol [DONE]:** Type 5 (Send) and Type 6 (UDP Return) operational.
*   **Public Alpha Preparation [DONE]:** Setup Wizard and Conformance Suite verified.
*   **RDMA Audio Path [RESEARCH]**: Prototype zero-copy RDMA return path for sub-100 microsecond network DSP.

---

## Month 4 (August 2026): The Validation Gate [NEXT]
*Goal: replace assumptions with measurements before any further feature work. See the [Strategic Assessment](../business/STRATEGIC_ASSESSMENT_2026_07.md) §3 for full procedures and pass bars.*

*   **Survival Test [BLOCKING]:** 1 hour continuous audio on real hardware (ALSA + PipeWire), full DJ topology, 0 xruns. Prerequisite for everything else.
*   **Numbers Test [BLOCKING]:** measured RTL per backend on ordinary hardware, published in docs. Pass: < 10 ms on PipeWire.
*   **Stranger Test:** 2–3 outside musicians, 15 minutes with the Breeder. Decides whether SoundDNA is the product or a feature.
*   **Adoption Probe:** extract a minimal `nullherz-engine` crate + example, publish, measure interest for one quarter.
*   **Identity Decision:** after the results, commit to ONE identity (engine infrastructure / genetic instrument / distributed live audio) and re-scope the remaining surfaces to serve it.

*Engineering support tasks (code-side, can start immediately): headless survival-test harness with xrun logging; RTL measurement runbook; stripped-down Breeder demo build.*

---

## Milestone Summary
| Month | Primary Objective | Key Deliverable |
| :--- | :--- | :--- |
| **Month 1** | Core Hardening | **Verified Graph Executor** |
| **Month 2** | Federated Intelligence | **Genetic Cloud (GossipSync) & DNA Sequencer** |
| **Month 3** | Scalability | **Ecosystem SDK & RDMA Prototype** |
| **Month 4** | **Validation Gate** | **Survival + RTL numbers + identity decision** |

---

**Strategic Approval:** *Lead Nullherz Architect & Senior Producer*
