#!/usr/bin/env bash
# Runs the spike app, or an installed copy of it, and records the JSON line it
# prints in the job summary and in metrics/<label>.json.
#
#   ci/frames.sh <label> <command>...
#
# Exits with the app's status: non-zero when it presented no frames.
set -uo pipefail
label=$1
shift
mkdir -p metrics
"$@" > "metrics/$label.out" 2> "metrics/$label.err"
status=$?
cat "metrics/$label.err" >&2
cat "metrics/$label.out"
json=$(grep '^{' "metrics/$label.out" | tail -n 1)
echo "${json:-null}" > "metrics/$label.json"
printf '| run | exit | report |\n| --- | --- | --- |\n| %s | %s | `%s` |\n' \
  "$label" "$status" "${json:-no report}" >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
exit $status
