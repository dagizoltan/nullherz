# Nullherz Community & Ecosystem Strategy

**Focus:** Social Proof, Modular Growth, and the "Horde" Effect.

---

## 1. The Eatbrain "Lighthouse" Strategy
We will use the Eatbrain partnership as the primary vehicle for social proof in the technical music community.
*   **Artist-Driven Feedback:** Top neurofunk producers (e.g., Burr Oak, MNDSCP) will be the "Alpha Cohort," providing the aesthetic direction for new kernels.
*   **The "Eatbrain Edition":** Create a dedicated UI layout and DSP preset pack that embodies the "Premier Neurofunk" sound, serving as a marketing flagship.

---

## 2. The Sidecar Marketplace (Genetic Traits)
Nullherz wins when third parties contribute to the ecosystem.
*   **AnaWaves DNA Store:** A platform where users can trade or sell **AnaWaves Genetic Traits** (Spectral Personality files and Rhythmic DNA masks). **Status update (July 18, 2026): the security layer this depended on is shipped** — ed25519-signed DNA payloads (`GOSSIP_SIGNED`), lineage consensus, and per-peer identity pinning (trust-on-first-use with key-change rejection) are implemented and regression-tested. Remaining before public trade: out-of-band identity verification/revocation (tracked as TOFU limitation in the debt log) and the `libp2p` migration.
*   **Rust Sidecar SDK:** Provide a one-command "Project Template" (`cargo generate nullherz-sidecar`) to lower the barrier for Rust developers to build high-performance DSP plugins. **Status update: the SDK's runtime contract (SHM data path, command ordering, heartbeat liveness, extension routing) is now pinned by tests** — the prerequisite for freezing and versioning it for third parties. The `sidecars/nullherz-template` crate is the seed of the project template.

---

## 3. Open Documentation Standard
To foster an ecosystem, the "Sound DNA" bit-layout (ANAWAVES_GENETIC_SCHEMA_RFC) must be treated as an open standard.
*   **Interoperability:** Encourage other software to export spectral analysis data in our schema, making Nullherz the central hub for Sound Transfusion.
*   **Technical Content:** Publish deep-dives on the Triple-Plane architecture and lock-free Rust DSP to attract systems engineers to the platform.

---

## 4. Community Events & "The Gauntlet"
*   **Transfusion Challenges:** Monthly production competitions where participants must start with a single "Seed Sample" (e.g., a simple sinewave) and evolve it through multiple cycles of Transfusion to create a complete track.
*   **Developer Hangouts:** Technical sessions focused on SIMD optimization and RT-safe Rust patterns.

---

**Ecosystem Goal:** *To transform Nullherz from a piece of software into the standard environment for evolutionary audio synthesis.*
