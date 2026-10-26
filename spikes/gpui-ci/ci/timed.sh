#!/usr/bin/env bash
# Runs a command and records how long it took in the job summary and in
# metrics/<label>.txt.
#
#   ci/timed.sh <label> <command>...
set -euo pipefail
label=$1
shift
mkdir -p metrics
start=$(date +%s)
"$@"
seconds=$(( $(date +%s) - start ))
echo "$label: ${seconds}s" | tee "metrics/$label.txt"
printf '| step | time |\n| --- | --- |\n| %s | %s s |\n' "$label" "$seconds" >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
