# Nullherz Documentation Hub

Technical, strategic, and business knowledge base for the Nullherz engine.

**Three documents carry the plan of record. Everything else is context.**

1. [System Architecture Reference](./system/ARCHITECTURE.md) — what the system *is*.
2. [Pre-Implementation Design Gate](./system/PRE_IMPLEMENTATION_DESIGN_GATE_2026_07.md) — the measurements and the defect inventory.
3. [Implementation Roadmap, July 2026](./roadmap/IMPLEMENTATION_ROADMAP_2026_07.md) — what happens next, in order, each phase ending in a number or a green test.

Status claims live in the document that also carries the way to disprove them.
Documents that asserted maturity without that are in [`archive/`](./archive/) —
read [`archive/README.md`](./archive/README.md) before citing anything from there.

---

## 🏗 [System Architecture](./system/)
Core technical specifications and engineering principles.
- [System Architecture Reference](./system/ARCHITECTURE.md) — **start here**: reverse-engineered crate map, data flow, protocols, on-disk state
- [Pre-Implementation Design Gate (July 2026)](./system/PRE_IMPLEMENTATION_DESIGN_GATE_2026_07.md) — design decisions, measurements, defect inventory
- [AnaWaves Genetic Schema](./system/ANAWAVES_GENETIC_SCHEMA_RFC.md)
- [Sidecar Protocol v2](./system/SIDECAR_PROTOCOL_V2.md)
- [SDK Developer Guide](./system/SDK_DEVELOPER_GUIDE.md)
- [Engineering Hardening Manifesto](./system/ENGINEERING_HARDENING_MANIFESTO.md)
- [Solution Design & Optimization](./system/SOLUTION_DESIGN_OPTIMIZATION.md)
- [Verification & QA Strategy](./system/VERIFICATION_AND_QA_STRATEGY.md)
- [Validation Runbook (Survival & RTL)](./system/VALIDATION_RUNBOOK.md) — procedures for the Validation Gate's blocking tests

## 📊 [Current State & Health](./state/)
Tracking of system maturity and technical debt.
- [Feature Matrix](./state/FEATURE_MATRIX.md) — a ✅ here means *reachable by a user*, not *a test passes*; the gate is `crates/nullherz-conductor/tests/reachability_gate_test.rs`
- [Technical Debt & Stubs Log](./state/TECHNICAL_DEBT_AND_STUBS.md)
- [Reverse Engineering Evaluation](./state/REVERSE_ENGINEERING_EVALUATION.md)
- [Optimization & Hardening Log](./state/TECHNICAL_OPTIMIZATION_LOG.md)

## 🗺 [Roadmaps](./roadmap/)
Strategic direction and detailed task orchestration.
- [Implementation Roadmap (July 2026)](./roadmap/IMPLEMENTATION_ROADMAP_2026_07.md) — **plan of record**: Phases 0–7 + Track U, each with a measurable gate
- [Strategic Roadmap (3-Month)](./roadmap/STRATEGIC_ROADMAP.md)
- [Detailed Task List](./roadmap/DETAILED_TASKS.md)

## 🗄 [Archive](./archive/)
Superseded documents, kept for history. Not maintained, not citable.
- [Why these were archived](./archive/README.md)

## 💼 [Business & Strategy](./business/)
Financial planning, market analysis, and ecosystem growth.
- [Strategic Assessment: Where the True Potential Is](./business/STRATEGIC_ASSESSMENT_2026_07.md) — **read first**: candid verdict, three candidate identities, validation tests
- [Financial Plan: Eatbrain Strategic Proposal](./business/FINANCIAL_PLAN_EATBRAIN.md)
- [Market Comparison & Evaluation](./business/MARKET_COMPARISON.md)
- [Market Viability Strategy](./business/MARKET_VIABILITY_STRATEGY.md)
- [R&D Strategy](./business/R_AND_D_STRATEGY.md)
- [Community & Ecosystem Strategy](./business/COMMUNITY_AND_ECOSYSTEM_STRATEGY.md)
- [Developer & Artist Experience Strategy](./business/DEVELOPER_AND_ARTIST_EXPERIENCE_STRATEGY.md)

## 📝 [Templates](./templates/)
Reusable blueprints for future expansion.
- [Processor RFC Template](./templates/PROCESSOR_RFC_TEMPLATE.md)
- [Strategic Pre-Flight Checklist](./templates/STRATEGIC_PRE_FLIGHT_CHECKLIST.md)

---

*“It is better to have one mathematically verified kernel than ten un-hardened features.”*
