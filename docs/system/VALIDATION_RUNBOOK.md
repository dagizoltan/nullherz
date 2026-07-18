# Validation Runbook: Survival & RTL Numbers

**Purpose:** step-by-step procedures for the two *blocking* tests of the Validation Gate ([STRATEGIC_ASSESSMENT_2026_07.md](../business/STRATEGIC_ASSESSMENT_2026_07.md) §3). Both require real hardware; both produce artifacts that go into this repo.

---

## 0. Getting Sound At All (First-Run Checklist)

Diagnosed on the reference machine (x270, July 18, 2026): the stack is
**PipeWire** (pipewire + wireplumber); Nullherz's ALSA backend opens
`default`, which routes through PipeWire's ALSA layer — this works and is
the supported default. "No sound" is almost never the driver; check in
order:

1. **Release build.** Debug DSP is 10–30× slower and blows the period
   budget every block (the endless `snd_pcm_writei error: -32` loop):
   `cargo run --release --bin nullherz-inspector -- --gui ./graph.json`
2. **Something must be playing.** The engine outputs silence until a track
   is loaded onto a deck and the deck plays (telemetry `peak_L=0.000000`
   means exactly this). Library panel → select a track → load to Deck A →
   press play in the Player/DJ view.
3. **Fresh analysis after changing demo tracks.** The library caches
   analysis per path; after regenerating `tracks/*.wav`, back up and delete
   `library.redb` so the folder monitor re-analyzes (BPM, transients,
   waveform mips).
4. **OS-level check** if still silent: `aplay /usr/share/sounds/alsa/Front_Center.wav`
   and `pactl get-sink-mute @DEFAULT_SINK@`.

Demo tracks are generated, deterministic, and regenerable with
`python3 scripts/gen_demo_tracks.py` — track_a is a 174 BPM neurofunk loop
(two-step kick/snare, 1/16 hats, reese + sub), track_b a 128 BPM house
loop (four-on-the-floor, offbeat hats, sub bassline, stabs). Both have
real transients so waveform LOD, BPM detection, and transient analysis
have material to work with.

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
