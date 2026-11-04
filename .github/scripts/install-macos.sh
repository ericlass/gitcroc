#!/usr/bin/env bash
# D6, SEC-7, PLAT-3, PLAT-4 (M6-28): the .dmg's app, copied to an
# Applications folder, is ad hoc signed, universal (x86_64 and arm64), and
# starts as `app` on Apple silicon.
set -euo pipefail
artifacts=$1
version=$2
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
mkdir -p "$INSTALL_LOGS"
test "$(uname -m)" = arm64
mount="$RUNNER_TEMP/gitcroc-dmg"
applications="$RUNNER_TEMP/Applications"
mkdir -p "$mount" "$applications"
hdiutil attach "$artifacts/gitcroc_${version}_universal.dmg" -readonly -nobrowse -mountpoint "$mount"
trap 'hdiutil detach "$mount"' EXIT
cp -R "$mount/gitcroc.app" "$applications/"
app="$applications/gitcroc.app"
ls -lR "$app" > "$INSTALL_LOGS/app-files.txt"
codesign --verify --deep --strict "$app"
codesign -dv --verbose=4 "$app" 2>&1 | tee "$INSTALL_LOGS/codesign.txt"
grep -Fx 'Signature=adhoc' "$INSTALL_LOGS/codesign.txt"
lipo "$app/Contents/MacOS/gitcroc" -verify_arch x86_64 arm64
lipo -info "$app/Contents/MacOS/gitcroc"
python3 "$script_dir/install_common.py" "$version" macos aarch64 app "$app/Contents/MacOS/gitcroc"
