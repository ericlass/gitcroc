#!/usr/bin/env bash
# Records file sizes in the job summary and in metrics/sizes-<label>.txt.
#
#   ci/sizes.sh <label> <file>...
set -euo pipefail
label=$1
shift
mkdir -p metrics
printf '| file | size |\n| --- | --- |\n' >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
for file in "$@"; do
  bytes=$(wc -c < "$file" | tr -d ' ')
  echo "$(basename "$file"): $bytes bytes ($(( bytes / 1024 / 1024 )) MiB)" | tee -a "metrics/sizes-$label.txt"
  echo "| $(basename "$file") | $(( bytes / 1024 )) KiB |" >> "${GITHUB_STEP_SUMMARY:-/dev/null}"
done
