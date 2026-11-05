"""PLAT-4, SEC-7, D12 (M6-28c): 1.0.0-rc.1, built from its tag to update
itself from the local server.

    python update_rc1_build.py <rc1> <public-key-file> <out>

<rc1> is a checkout of v1.0.0-rc.1 with its toolchain ready: its Rust
toolchain, Node, pnpm and the UI's packages, and on Linux WebKitGTK. As
rc.1's own update_build.py built its A, Tauri builds the NSIS installer,
the .app or the AppImage at rc.1's version, with rc.1's M5-15 `update-test`
feature and an overlay that sets the updater's only endpoint to
http://127.0.0.1:8765/latest.json and its key to the throwaway key in
<public-key-file>. The released rc.1 has the real endpoint and key built
in, so it cannot be pointed at a local server. No update package is made,
so no secret key is needed.

<out> gets the package and version.json, rc.1's version.
"""

import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

from update_version import workspace_version

ENDPOINT = "http://127.0.0.1:8765/latest.json"


def overlay(version, public_key):
    """The Tauri config overlay: rc.1's version, the local endpoint over
    plain http and the throwaway key."""
    return {
        "version": version,
        "bundle": {"createUpdaterArtifacts": False},
        "plugins": {
            "updater": {
                "pubkey": public_key,
                "dangerousInsecureTransportProtocol": True,
                "endpoints": [ENDPOINT],
            }
        },
    }


def build(rc1, public_key_file, out):
    version = workspace_version(rc1)
    public_key = public_key_file.read_text(encoding="utf-8").strip()
    system = platform.system()
    bundles = {"Windows": "nsis", "Darwin": "app", "Linux": "appimage"}[system]
    config = rc1 / "target/update-test-overlay.json"
    config.parent.mkdir(parents=True, exist_ok=True)
    config.write_text(json.dumps(overlay(version, public_key)), encoding="utf-8")
    # rc.1's `just bundle`.
    env = dict(os.environ, TAURI_APP_PATH=str(rc1 / "crates/gitcroc-app"))
    env.pop("TAURI_SIGNING_PRIVATE_KEY", None)
    pnpm = shutil.which("pnpm")
    args = [pnpm, "--dir", "ui", "exec", "tauri", "build", "--features", "update-test",
            "--bundles", bundles, "--config", str(config)]
    print("+", " ".join(map(str, args)), flush=True)
    subprocess.run(args, cwd=rc1, env=env, check=True)

    bundle = rc1 / "target/release/bundle"
    out.mkdir(parents=True, exist_ok=True)
    if system == "Windows":
        (installer,) = bundle.glob("nsis/*-setup.exe")
        shutil.copy2(installer, out / installer.name)
    elif system == "Darwin":
        shutil.copytree(bundle / "macos/gitcroc.app", out / "gitcroc.app", symlinks=True)
    else:
        (appimage,) = bundle.glob("appimage/*.AppImage")
        shutil.copy2(appimage, out / appimage.name)
    (out / "version.json").write_text(json.dumps(version), encoding="utf-8")
    print(f"rc.1 ({version}) built for {ENDPOINT} into {out}", flush=True)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    build(*(Path(arg).resolve() for arg in sys.argv[1:]))
