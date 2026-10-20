#!/usr/bin/env bash
# D6, D13, PLAT-3/4, DATA-3: AppImage and installed deb start off-repo and find git;
# Fedora resolves the rpm dependencies and rpm -q proves it installed (no display).
set -euo pipefail
artifacts=$1
version=$2
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-0 libxdo3 xvfb
appimages=("$artifacts"/*.AppImage)
chmod +x "${appimages[0]}"
APPIMAGE_EXTRACT_AND_RUN=1 xvfb-run -a python3 "$script_dir/install_common.py" \
  "$version" linux x86_64 AppImage "${appimages[0]}"
sudo apt-get install -y "$artifacts"/*.deb
xvfb-run -a python3 "$script_dir/install_common.py" "$version" linux x86_64 deb /usr/bin/gitcroc
sudo apt-get remove -y gitcroc
docker run --rm -v "$artifacts:/packages:ro" fedora:latest \
  bash -euc 'dnf install -y /packages/*.rpm; rpm -q gitcroc'
