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

# M6-31: rc.2's .rpm upgrades an installed rc.1. rc.1's .rpm (version
# 1.0.0-rc.1, release 1, no epoch) sorts after 1.0.0, so the new one carries
# epoch 1. The installed epoch, version and release are checked after dnf,
# which would otherwise skip an older package without failing.
dnf remove -y gitcroc
rc1_dir="${RUNNER_TEMP:-/tmp}/rc1"
"$script_dir/download-rc1.sh" gitcroc-1.0.0-rc.1-1.x86_64.rpm "$rc1_dir"
dnf install -y "$rc1_dir/gitcroc-1.0.0-rc.1-1.x86_64.rpm"
test "$(rpm -q --qf '%{EPOCHNUM}:%{VERSION}-%{RELEASE}' gitcroc)" = 0:1.0.0-rc.1-1
dnf install -y "$artifacts/gitcroc-${version}-1.x86_64.rpm"
test "$(rpm -q --qf '%{EPOCHNUM}:%{VERSION}-%{RELEASE}' gitcroc)" = "1:${version//-/\~}-1"
echo "PASS: the .rpm upgraded rc.1 to 1:${version//-/\~}-1"
xvfb-run -a python3 "$script_dir/install_common.py" "$version" linux x86_64 rpm /usr/bin/gitcroc
