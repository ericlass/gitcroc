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
