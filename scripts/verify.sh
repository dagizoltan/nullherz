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

bold "==> cargo check --workspace --all-targets (-D warnings)"
RUSTFLAGS="-D warnings" cargo check --workspace --all-targets --quiet

bold "==> cargo test --workspace"
# Tests run on the Mock backend; no audio hardware required.
cargo test --workspace --quiet 2>&1 | awk '
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
cargo test --workspace --release --quiet 2>&1 | awk '
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
scripts/smoke.sh "${SMOKE_ARGS[@]}"

if [[ "${1:-}" == "--full" ]]; then
    bold "==> cargo clippy (advisory — style backlog, target: 0)"
    count=$(cargo clippy --workspace --all-targets --message-format=short 2>&1 \
        | grep -c ": warning" || true)
    echo "    ${count} clippy warnings remaining"
fi

bold "PASS — safe to push."
