"""Capture fixture windows for the user guide, on an isolated Xvfb display.

Run smoke.py first; pass its printed fixture directory. Needs xwd and Pillow.
"""
import argparse
import json
from pathlib import Path
import struct
import subprocess
import time
from PIL import Image


def png(source, target):
    """Decode Xvfb's XWD pixels without changing the screenshot."""
    data = source.read_bytes()
    header = struct.unpack(">25I", data[:100])
    size, version, _, depth, width, height, _, order = header[:8]
    bpp, stride, _, red, green, blue = header[11:17]
    assert version == 7 and depth == 24 and bpp in (24, 32) and order == 0
    assert (red, green, blue) == (0xFF0000, 0xFF00, 0xFF)
    pixels = data[size + header[19] * 12:]
    Image.frombytes("RGB", (width, height), pixels, "raw", ("BGRX" if bpp == 32 else "BGR"), stride, 1).save(target)


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("--app", required=True, type=Path)
    parser.add_argument("--fixture", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    root = args.fixture
    script = json.loads((root / "ai.json").read_text())
    base = [dict(action="shown", id="toolbar"),
            dict(action="click-label", prefix="tree-row-", label="app"),
            dict(action="shown", id="history-list"), dict(action="click", id="history-row-0"),
            dict(action="shown", id="diff-lines")]
    args.out.mkdir(parents=True, exist_ok=True)
    for name, steps in [("native-window", base), ("native-settings", base + [
        dict(action="click", id="toolbar-settings"), dict(action="shown", id="settings-panel")])]:
        script.update(steps=steps, hold=True)
        path = root / "capture.json"
        path.write_text(json.dumps(script))
        log = root / "capture.log"
        with log.open("w") as output:
            child = subprocess.Popen([str(args.app.resolve()), "--smoke", str(path)], stdout=output, stderr=output)
            try:
                deadline = time.monotonic() + 60
                while "smoke: ready for capture" not in log.read_text():
                    if child.poll() is not None or time.monotonic() >= deadline:
                        raise RuntimeError(log.read_text())
                    time.sleep(.02)
                raw = root / "capture.xwd"
                subprocess.run(["xwd", "-root", "-silent", "-out", str(raw)], check=True)
                png(raw, args.out / f"{name}.png")
            finally:
                child.terminate()
                child.wait(timeout=10)


if __name__ == "__main__":
    main()
