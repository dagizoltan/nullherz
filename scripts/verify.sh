#!/usr/bin/env bash
# Local CI gate — the same checks as .github/workflows/ci.yml, runnable
# without GitHub Actions. Run before pushing; wire it up as a pre-push hook
# with:  git config core.hooksPath .githooks
#
# Usage:
#   scripts/verify.sh          # gate: check + tests + smoke run
#   scripts/verify.sh --full   # gate + advisory clippy report
#   scripts/verify.sh --audio  # gate, with the smoke run on a real ALSA device
set -euo pipefail
cd "$(dirname "$0")/.."

bold() { printf '\033[1m%s\033[0m\n' "$*"; }

# Every step runs under a wall-clock limit.
#
# WHY: the workspace suite used to DEADLOCK, not fail. `Conductor::setup_engine`
# bound a blocking `std::net::UdpSocket` and called `recv_from` inside a
# `tokio::spawn`, parking a runtime worker in the kernel forever, so
# `Runtime::drop` could never complete and the test binary could not exit. With
# no timeout here the gate simply hung — and a gate that hangs is worse than one
# that fails, because CI reports it as a timeout with no failing test to point
# at, and locally it looks like a slow build.
#
# Override per-invocation: NULLHERZ_STEP_TIMEOUT=1200 scripts/verify.sh
STEP_TIMEOUT="${NULLHERZ_STEP_TIMEOUT:-900}"
run_step() {
    local label="$1"; shift
    # `rc=$?` must be captured from the command itself, NOT from inside
    # `if ! cmd; then` — there `$?` is the status of the successful NEGATION,
    # i.e. 0, so a timed-out step reported success and the gate sailed past it.
    # A gate that cannot fail is the defect this whole function exists to fix.
    local rc=0
    timeout --kill-after=10s --foreground "$STEP_TIMEOUT" "$@" || rc=$?
    if [[ $rc -ne 0 ]]; then
        if [[ $rc -eq 124 || $rc -eq 137 ]]; then
            echo
            echo "TIMED OUT after ${STEP_TIMEOUT}s: ${label}"
            echo "A step that does not finish is a failure. If this is a hang, find the"
            echo "stuck thread with:  pgrep -a -f 'target/debug/deps' ; cat /proc/<pid>/task/*/wchan"
        fi
        return "$rc"
    fi
}

bold "==> cargo check --workspace --all-targets (-D warnings)"
RUSTFLAGS="-D warnings" run_step "cargo check" cargo check --workspace --all-targets --quiet

bold "==> cargo test --workspace"
# Tests run on the Mock backend; no audio hardware required.
run_step "cargo test (debug)" cargo test --workspace --quiet 2>&1 | awk '
    /^test result:/ { passed += $4; failed += $6 }
    END {
        printf "    %d passed, %d failed\n", passed, failed
        exit (failed > 0)
    }'

# The debug run above SKIPS every wall-clock assertion (control-path budgets,
# RT block budgets): an unoptimized build misses a 5.8 ms audio block budget on
# merit rather than on regression, and a gate that cries wolf is a gate the team
# learns to bypass. Those assertions are compiled in only when debug_assertions
# is off, so this is the only place they run. The smoke step below already
# builds release, so the marginal cost here is the test targets.
bold "==> cargo test --workspace --release (timing budgets)"
run_step "cargo test (release)" cargo test --workspace --release --quiet 2>&1 | awk '
    /^test result:/ { passed += $4; failed += $6 }
    END {
        printf "    %d passed, %d failed\n", passed, failed
        exit (failed > 0)
    }'

# The unit suite passes green through defects that stop the application dead —
# see the header of scripts/smoke.sh for the list. Running the console is the
# only check that catches them, so it is part of the gate, not an extra.
SMOKE_ARGS=()
for arg in "$@"; do [[ "$arg" == "--audio" ]] && SMOKE_ARGS+=(--audio); done
bold "==> smoke run (console must survive and produce audio)"
run_step "smoke run" scripts/smoke.sh "${SMOKE_ARGS[@]}"

if [[ "${1:-}" == "--full" ]]; then
    bold "==> cargo clippy (advisory — style backlog, target: 0)"
    count=$(cargo clippy --workspace --all-targets --message-format=short 2>&1 \
        | grep -c ": warning" || true)
    echo "    ${count} clippy warnings remaining"
fi

bold "PASS — safe to push."
