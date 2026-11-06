#!/usr/bin/env bash
# D6, D13, PLAT-3, PLAT-4 (M6-28): on ubuntu-22.04 under Xvfb, with Mesa's
# lavapipe, the AppImage starts as `AppImage`, and the .deb, installed with
# apt so its dependencies resolve, starts as `deb`.
set -euo pipefail
artifacts=$1
version=$2
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
mkdir -p "$INSTALL_LOGS"
sudo apt-get update
# Xvfb and Mesa's Vulkan driver to show the window (M6-03); libfuse2 to
# mount the AppImage.
sudo apt-get install -y --no-install-recommends xvfb xauth mesa-vulkan-drivers libfuse2

appimage="$artifacts/gitcroc_${version}_amd64.AppImage"
chmod +x "$appimage"
xvfb-run -a python3 "$script_dir/install_common.py" "$version" linux x86_64 AppImage "$appimage"

sudo apt-get install -y "$artifacts/gitcroc_${version}_amd64.deb"
dpkg -L gitcroc > "$INSTALL_LOGS/deb-files.txt"
dpkg -s gitcroc | tee -a "$INSTALL_LOGS/deb-files.txt"
xvfb-run -a python3 "$script_dir/install_common.py" "$version" linux x86_64 deb /usr/bin/gitcroc
sudo apt-get remove -y gitcroc

# M6-31: rc.2's .deb upgrades an installed rc.1. rc.1's .deb has no epoch
# and sorts after 1.0.0, so the new one carries epoch 1 (`1:<version with ~>`).
# apt-get refuses a downgrade under -y without --allow-downgrades.
rc1_dir="$RUNNER_TEMP/rc1"
"$script_dir/download-rc1.sh" gitcroc_1.0.0-rc.1_amd64.deb "$rc1_dir"
sudo apt-get install -y "$rc1_dir/gitcroc_1.0.0-rc.1_amd64.deb"
test "$(dpkg-query -W -f '${Version}' gitcroc)" = 1.0.0-rc.1
new_version="1:${version//-/\~}"
dpkg --compare-versions "$new_version" gt 1.0.0-rc.1
# A pre-release sorts before its release.
case $version in *-*) dpkg --compare-versions "$new_version" lt "1:${version%%-*}" ;; esac
sudo apt-get install -y "$artifacts/gitcroc_${version}_amd64.deb"
test "$(dpkg-query -W -f '${Version}' gitcroc)" = "$new_version"
echo "PASS: the .deb upgraded rc.1 to $new_version"
xvfb-run -a python3 "$script_dir/install_common.py" "$version" linux x86_64 deb /usr/bin/gitcroc
sudo apt-get remove -y gitcroc
