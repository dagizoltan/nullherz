#!/usr/bin/env bash
# Local CI gate — the same checks as .github/workflows/ci.yml, runnable
# without GitHub Actions. Run before pushing; wire it up as a pre-push hook
# with:  git config core.hooksPath .githooks
#
# Usage:
#   scripts/verify.sh          # gate: check (-D warnings) + full test suite
#   scripts/verify.sh --full   # gate + advisory clippy report
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

if [[ "${1:-}" == "--full" ]]; then
    bold "==> cargo clippy (advisory — style backlog, target: 0)"
    count=$(cargo clippy --workspace --all-targets --message-format=short 2>&1 \
        | grep -c ": warning" || true)
    echo "    ${count} clippy warnings remaining"
fi

bold "PASS — safe to push."
