#!/usr/bin/env bash
# Downloads one file of the v1.0.0-rc.1 release into a directory and checks
# it against the release's SHA256SUMS, for the "upgrade over rc.1" steps
# (M6-31). Usage: download-rc1.sh <file> <directory>
set -euo pipefail
file=$1
dir=$2
base="https://github.com/${GITHUB_REPOSITORY:-ericlass/gitcroc}/releases/download/v1.0.0-rc.1"
mkdir -p "$dir"
curl -fsSL --retry 3 -o "$dir/SHA256SUMS" "$base/SHA256SUMS"
curl -fsSL --retry 3 -o "$dir/$file" "$base/$file"
line=$(grep -E "^[0-9a-f]{64}  \*?${file//./\\.}\$" "$dir/SHA256SUMS")
(cd "$dir" && echo "$line" | sha256sum -c -)
