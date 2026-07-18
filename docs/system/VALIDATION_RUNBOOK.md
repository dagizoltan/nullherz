# Validation Runbook: Survival & RTL Numbers

**Purpose:** step-by-step procedures for the two *blocking* tests of the Validation Gate ([STRATEGIC_ASSESSMENT_2026_07.md](../business/STRATEGIC_ASSESSMENT_2026_07.md) §3). Both require real hardware; both produce artifacts that go into this repo.

---

## 1. Survival Test (1 hour, 0 xruns)

**Harness:** `crates/nullherz-conductor/src/bin/survival.rs` — headless; boots the full 4-channel DJ topology, loads the first two analyzed tracks onto decks A/B, plays them, tracks xruns and block times from live telemetry, writes a markdown report, and exits non-zero on any xrun.

### Procedure

```bash
# 1. Make sure at least two WAVs are present (repo ships tracks/track_a.wav, track_b.wav)
ls tracks/

# 2. Full run on each backend that matters (release build!):
cargo run --release -p nullherz-conductor --bin survival -- --minutes 60 --backend pipewire
cargo run --release -p nullherz-conductor --bin survival -- --minutes 60 --backend alsa

# Smoke variant (validates the harness itself, not the system):
cargo run --release -p nullherz-conductor --bin survival -- --minutes 1 --backend threaded
```

Options: `--minutes N` (default 60), `--backend alsa|pipewire|jack|threaded|mock`, `--tracks DIR` (default `tracks/`), `--report PATH`.

### Reading the result

- **PASS** = exit code 0, `Xruns: 0` in the report. Anything else is a FAIL — including "just one" xrun; one dropout per hour is audible on stage.
- The report also gives **peak/mean block time vs. the period budget** — headroom under 50% at peak is the comfort zone; over 80% means the machine is one scheduler hiccup away from a dropout even if this run passed.
- Commit each report to `docs/state/survival/` (create the folder on first run) so results accumulate per backend/machine.

### Honest-run conditions

Run on the machine class you actually intend to perform on, with the desktop session running (not a bare console), typical background load, and the release profile. A pass on an idle machine with a stripped session proves less than a pass under realistic conditions.

---

## 2. RTL Numbers Test (< 10 ms on PipeWire)

**What it measures:** round-trip latency — output → physical loopback → input — as seen by the engine's calibration routine (`CoreCommand::CalibrateLatency`).

### Hardware setup

A physical loopback on the interface you intend to use: patch cable from line/headphone out to line in. (USB interfaces: out L → in L is enough; onboard audio works but expect worse numbers.)

### Procedure

1. Configure the backend and period size in `system_config.json` (start with `period_size: 512`, then try 256 and 128).
2. Launch the inspector, open **Settings → Calibration**, and trigger calibration (sends `CoreCommand::CalibrateLatency`). The measured RTL displays as `Current RTL: X.X ms (N samples)` and persists into `system_config.json` (`calibration_samples`).
3. Record one measurement per (backend × period size) combination, three runs each; report the median.

### Reporting

Add results to a table in `docs/state/RTL_NUMBERS.md` (create on first measurement):

| Date | Machine / Interface | Backend | Period | Sample rate | RTL (median of 3) |
| :-- | :-- | :-- | --: | --: | --: |
| — | — | — | — | — | — |

**Pass bar:** < 10 ms on PipeWire at a period size the Survival test also passes with. Publishing an RTL that only holds at a period size that xruns is self-deception — the two tests constrain each other.

---

## 3. What the results decide

Per the Strategic Assessment: **no further feature work until Survival and Numbers have both been run.** Their outcomes re-rank the three candidate identities:

- Survival FAIL → stability work is the only roadmap.
- Survival PASS + RTL ≥ 10 ms → period/backend tuning before any latency-sensitive positioning.
- Both PASS → proceed to the Stranger test (Breeder demo) and the identity decision.
