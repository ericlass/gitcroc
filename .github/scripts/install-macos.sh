#!/usr/bin/env bash
# D6, SEC-7, PLAT-3/4: the dmg's ad-hoc signed universal app runs on Apple silicon.
set -euo pipefail
artifacts=$1
version=$2
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
test "$(uname -m)" = arm64
mount="$RUNNER_TEMP/gitcroc-dmg"
applications="$RUNNER_TEMP/Applications"
mkdir -p "$mount" "$applications"
dmgs=("$artifacts"/*.dmg)
hdiutil attach "${dmgs[0]}" -readonly -nobrowse -mountpoint "$mount"
trap 'hdiutil detach "$mount"' EXIT
cp -R "$mount/gitcroc.app" "$applications/"
app="$applications/gitcroc.app"
codesign --verify --deep --strict "$app"
codesign -dv --verbose=4 "$app" 2>&1 | tee "$RUNNER_TEMP/codesign.txt"
grep -Fx 'Signature=adhoc' "$RUNNER_TEMP/codesign.txt"
lipo "$app/Contents/MacOS/gitcroc" -verify_arch x86_64 arm64
python3 "$script_dir/install_common.py" "$version" macos aarch64 app "$app/Contents/MacOS/gitcroc"
