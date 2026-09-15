#!/usr/bin/env bash
# Boot check (phase 6 fix-round): the REAL main scene must start headless
# with zero script/wiring errors. Per-panel tests boot nodes
# programmatically; this catches breakage in the assembled scene.
#
# Usage (repo root or godot/): bash godot/tests/boot_check.sh
set -o pipefail

cd "$(dirname "$0")/.."   # -> godot/

GODOT_BIN="${GODOT:-godot}"
LOG="$(mktemp)"

"$GODOT_BIN" --headless --quit-after 180 2>&1 | tee "$LOG"
code=$?

echo "--- boot check: exit=$code, forbidden-line scan ---"
if grep -nE "SCRIPT ERROR|Node not found|no SimNode|Invalid call|Invalid access|Invalid assignment" "$LOG"; then
    echo "BOOT CHECK FAILED: forbidden lines above"
    exit 1
fi
if [ $code -ne 0 ]; then
    echo "BOOT CHECK FAILED: godot exited with $code"
    exit 1
fi
echo "BOOT CHECK OK"
