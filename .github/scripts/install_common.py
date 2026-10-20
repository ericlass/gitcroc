"""PLAT-3, PLAT-4, D12: start an installed app with updates off; await its log outcome."""

import os
from pathlib import Path
import re
import subprocess
import sys
import time


def wait_started(process, log, version, system, arch, package):
    marker = f"gitcroc {version} started ({system} {arch}, {package})"
    deadline = time.monotonic() + 300
    while True:
        if process.poll() is not None:
            raise RuntimeError(f"app exited before startup succeeded: {process.returncode}")
        text = log.read_text(encoding="utf-8") if log.exists() else ""
        if sum(line.endswith(marker) for line in text.splitlines()) == 1 and re.search(
            r'git "--version" in [^\n]*, exit 0$', text, re.MULTILINE
        ):
            print(f"PASS: {marker}; git --version exited 0", flush=True)
            return
        if time.monotonic() >= deadline:
            raise TimeoutError(f"startup outcome missing: {marker}\n{text}")
        # Poll for an outcome. The interval is not a success condition, and
        # the deadline only fails a hung app (README 'Timing in CI').
        time.sleep(0.1)


def start(version, system, arch, package, executable):
    home = Path.home()
    if system == "windows":
        config = Path(os.environ["APPDATA"]) / "gitcroc"
        log = Path(os.environ["LOCALAPPDATA"]) / "gitcroc/logs/gitcroc.log"
    elif system == "macos":
        config = home / "Library/Application Support/gitcroc"
        log = home / "Library/Logs/gitcroc/gitcroc.log"
    else:
        config = Path(os.environ.get("XDG_CONFIG_HOME", home / ".config")) / "gitcroc"
        log = Path(os.environ.get("XDG_STATE_HOME", home / ".local/state")) / "gitcroc/gitcroc.log"
    config.mkdir(parents=True, exist_ok=True)
    (config / "settings.toml").write_text("schema = 2\nupdate_check = false\n", encoding="utf-8")
    # Each package must produce its own evidence, never reuse an earlier start.
    log.unlink(missing_ok=True)
    with subprocess.Popen([executable], cwd=os.environ["RUNNER_TEMP"]) as process:
        try:
            wait_started(process, log, version, system, arch, package)
        finally:
            if log.exists():
                evidence = Path(os.environ["INSTALL_LOGS"]) / f"{package}-gitcroc.log"
                evidence.parent.mkdir(parents=True, exist_ok=True)
                evidence.write_bytes(log.read_bytes())
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


if __name__ == "__main__":
    start(*sys.argv[1:])
