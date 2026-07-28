#!/usr/bin/env bash
# Run the console for real and check that audio comes out.
#
# WHY THIS EXISTS
#
# The unit suite is large and green, and it has not caught a single one of the
# defects that actually stopped the application working:
#
#   * a per-telemetry-frame `get_track()` (61 ms) starved the conductor thread,
#     so full-length tracks would not play      — 267 tests green
#   * a per-telemetry-frame `enumerate_devices()` (74 ms) did it again, and the
#     decks stopped responding entirely         — 325 tests green
#   * the library scanner re-decoded every track every 10 s   — 299 tests green
#   * the graph overran its buffers on any device period > 256 frames, panicking
#     the audio thread                          — 267 tests green
#
# Every one was a COMPOSITION failure: cheap-in-isolation code placed on a hot
# path, or correct code that nothing reached. Unit tests are blind to both,
# because each part passes on its own. Only running the thing finds them.
#
# Exit code is the gate: non-zero means the console did not survive.
#
# Usage:
#   scripts/smoke.sh                # threaded backend, no audio hardware needed
#   scripts/smoke.sh --audio        # real ALSA device (catches backend-only bugs)
#   scripts/smoke.sh --minutes 5    # longer run
set -euo pipefail
cd "$(dirname "$0")/.."

BACKEND=threaded
MINUTES=1
while [[ $# -gt 0 ]]; do
    case "$1" in
        --audio)   BACKEND=alsa; shift ;;
        --minutes) MINUTES="$2"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

REPORT="$(mktemp -t nullherz-smoke-XXXXXX.md)"
trap 'rm -f "$REPORT"' EXIT

echo "==> smoke: ${MINUTES} min on ${BACKEND}"

# The harness itself asserts the things that matter and exits non-zero if any
# fail: underruns counted BY THE BACKEND (the engine's own counter is never
# incremented), audio present in >90% of frames (so a silent graph cannot pass
# with a trivial zero), and telemetry actually flowing.
if ! cargo run --release --quiet -p nullherz-conductor --bin survival -- \
        --backend "$BACKEND" --minutes "$MINUTES" --report "$REPORT"; then
    echo
    echo "SMOKE FAILED — the console did not survive a ${MINUTES}-minute run."
    echo
    sed -n '1,30p' "$REPORT" 2>/dev/null || true
    exit 1
fi

# Surface the numbers even on success: a passing run whose DSP load has crept to
# 80% of budget is a warning that no boolean gate would show.
grep -E 'Xruns \(backend\)|Peak MASTER|Frames with signal|Peak block|Period budget' "$REPORT" || true
echo "smoke OK"
