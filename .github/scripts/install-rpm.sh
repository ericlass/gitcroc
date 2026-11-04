#!/usr/bin/env bash
# D6, D13, PLAT-3, PLAT-4 (M6-28), in a fedora:latest container: with no
# font installed beforehand, dnf installs the .rpm and the font it requires;
# rpm -q shows both, and the app starts under Xvfb as `rpm`.
set -euo pipefail
artifacts=$1
version=$2
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
mkdir -p "$INSTALL_LOGS"
grep PRETTY_NAME /etc/os-release
shopt -s globstar nullglob
fonts=(/usr/share/fonts/**/*.*)
if [ ${#fonts[@]} -ne 0 ]; then
  echo "a font is installed before the .rpm: ${fonts[*]}"
  exit 1
fi

dnf install -y "$artifacts/gitcroc-${version}-1.x86_64.rpm"
rpm -q gitcroc
rpm -qR gitcroc | tee "$INSTALL_LOGS/rpm-files.txt"
rpm -ql gitcroc | tee -a "$INSTALL_LOGS/rpm-files.txt"
# The font came with the .rpm.
rpm -q dejavu-sans-fonts
fonts=(/usr/share/fonts/**/*.*)
echo "fonts: ${fonts[*]}"
[ ${#fonts[@]} -ne 0 ]

# xvfb-run, and Mesa's Vulkan and GL drivers to show the window.
dnf install -y /usr/bin/xvfb-run xorg-x11-xauth mesa-vulkan-drivers mesa-dri-drivers
xvfb-run -a python3 "$script_dir/install_common.py" "$version" linux x86_64 rpm /usr/bin/gitcroc
