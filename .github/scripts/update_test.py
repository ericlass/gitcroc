"""PLAT-4, SEC-7, D12: installed-app proof; loopback HTTP only, outcome waits."""
import functools
import hashlib
import http.server
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import threading
import time
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[2]
WORK = ROOT / "target/update-test"
BASE = "http://127.0.0.1:8765"


def installed(text, version, start):
    marker = f"update: installed {version}"
    return marker in text and start in text[text.index(marker) + len(marker):]


def refused(text, before, after):
    return bool(re.search(r"update: refused \([^\n]*(?:signature|Signature)", text)) and before == after


def localize(value, base):
    assert urlsplit(base).hostname == "127.0.0.1"
    for entry in value["platforms"].values():
        entry["url"] = base + "/" + urlsplit(entry["url"]).path.rsplit("/", 1)[-1]


def wait(log, predicate):
    deadline = time.monotonic() + 300
    while True:
        text = log.read_text(encoding="utf-8") if log.exists() else ""
        if predicate(text):
            return text
        if time.monotonic() >= deadline:
            raise TimeoutError(f"update outcome missing:\n{text}")
        time.sleep(0.1)


def run(*args):
    result = subprocess.run(args, cwd=ROOT, capture_output=True, text=True)
    if result.returncode:
        print(result.stdout, result.stderr, flush=True)
        result.check_returncode()
    return result.stdout


def stop(system):
    args = ["taskkill", "/IM", "gitcroc.exe", "/T", "/F"] if system == "Windows" else ["pkill", "-x", "gitcroc"]
    subprocess.run(args, capture_output=True)
    if system != "Windows":
        deadline = time.monotonic() + 30
        while subprocess.run(["pgrep", "-x", "gitcroc"], capture_output=True).returncode == 0:
            if time.monotonic() >= deadline:
                raise TimeoutError("gitcroc did not stop")
            time.sleep(0.1)


def exercise():
    a, b = json.loads((WORK / "versions.json").read_text())
    system = os.environ["RUNNER_OS"]
    home = Path.home()
    config, log = {
        "Windows": (Path(os.environ.get("APPDATA", ".")) / "gitcroc",
                    Path(os.environ.get("LOCALAPPDATA", ".")) / "gitcroc/logs/gitcroc.log"),
        "macOS": (home / "Library/Application Support/gitcroc", home / "Library/Logs/gitcroc/gitcroc.log"),
        "Linux": (home / ".config/gitcroc", home / ".local/state/gitcroc/gitcroc.log"),
    }[system]
    config.mkdir(parents=True, exist_ok=True)
    (config / "settings.toml").write_text("schema = 2\nupdate_check = true\n")
    (config / "update-test.pending").unlink(missing_ok=True)
    logs = WORK / "logs"
    logs.mkdir(exist_ok=True)
    platform, rust_os, arch, package, pattern = {
        "Windows": ("windows-x86_64", "windows", "x86_64", "installer", "*-setup.exe"),
        "macOS": ("darwin-aarch64", "macos", "aarch64", "app", "*.app.tar.gz"),
        "Linux": ("linux-x86_64", "linux", "x86_64", "AppImage", "*.AppImage"),
    }[system]
    payload = next((WORK / "b").glob(pattern))
    tool = ROOT / "target/debug" / ("release-manifest.exe" if system == "Windows" else "release-manifest")
    manifest = json.loads(run(str(tool), "local", "--version", b, "--dir", str(WORK / "b"),
                             "--pub-date", "2026-10-20T12:00:00Z", "--platform", platform,
                             "--package", payload.name))
    localize(manifest, BASE)

    def publish(value):
        temporary = WORK / "b/latest.tmp"
        temporary.write_text(json.dumps(value), encoding="utf-8")
        temporary.replace(WORK / "b/latest.json")

    publish(manifest)
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(WORK / "b"))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 8765), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    app = None
    try:
        if system == "Windows":
            folder = Path(os.environ["LOCALAPPDATA"]) / "gitcroc-update-test"
            run(str(next((WORK / "a").glob(pattern))), "/S", f"/D={folder}")
            app = folder / "gitcroc.exe"
        elif system == "macOS":
            mount = WORK / "mount"
            mount.mkdir()
            run("hdiutil", "attach", str(next((WORK / "a").glob("*.dmg"))), "-readonly", "-nobrowse", "-mountpoint", str(mount))
            try:
                shutil.copytree(mount / "gitcroc.app", WORK / "gitcroc.app")
            finally:
                run("hdiutil", "detach", str(mount))
            app = WORK / "gitcroc.app/Contents/MacOS/gitcroc"
        else:
            app = WORK / "gitcroc.AppImage"
            shutil.copy2(next((WORK / "a").glob(pattern)), app)
            app.chmod(0o755)

        def start():
            log.unlink(missing_ok=True)
            with (logs / "launcher.log").open("ab") as output:
                subprocess.Popen([str(app)], cwd=os.environ["RUNNER_TEMP"], stdout=output, stderr=subprocess.STDOUT)

        def start_line(version, kind=package):
            return f"gitcroc {version} started ({rust_os} {arch}, {kind})"

        old_binary = hashlib.sha256(app.read_bytes()).digest()
        start()
        text = wait(log, lambda text: installed(text, b, start_line(b)))
        assert start_line(a) in text and text.index(start_line(a)) < text.index(f"update: installed {b}")
        assert hashlib.sha256(app.read_bytes()).digest() != old_binary, "A's executable was not replaced"
        shutil.copy2(log, logs / "installed.log")
        stop(system)
        print(f"PASS: installed {b} and restarted as {b}", flush=True)

        # Keep a valid signature but change the package it authenticates.
        bad = payload.with_name("bad-" + payload.name)
        shutil.copy2(payload, bad)
        with bad.open("ab") as file:
            file.write(b"tampered")
        core = b.split("-", 1)[0].split("+", 1)[0].split(".")
        manifest["version"] = f"{core[0]}.{core[1]}.{int(core[2]) + 1}"
        manifest["platforms"][platform]["url"] = BASE + "/" + bad.name
        publish(manifest)
        digest = lambda: hashlib.sha256(app.read_bytes()).digest()
        before = digest()
        start()
        wait(log, lambda text: start_line(b) in text and refused(text, before, digest()))
        shutil.copy2(log, logs / "refused.log")
        stop(system)
        print("PASS: bad signature refused; installed B unchanged", flush=True)

        if system == "Linux":
            manifest["version"] = b
            manifest["platforms"][platform]["url"] = BASE + "/" + payload.name
            publish(manifest)
            run("sudo", "dpkg", "-i", str(next((WORK / "a").glob("*.deb"))))
            app = Path("/usr/bin/gitcroc")
            before = digest()
            start()
            wait(log, lambda text: f"update: notify only ({b})" in text and start_line(a, "deb") in text)
            assert digest() == before, "deb was changed by the updater"
            shutil.copy2(log, logs / "notify.log")
            stop(system)
            run("sudo", "dpkg", "-r", "gitcroc")
            print(f"PASS: deb notified about {b}; installed A unchanged", flush=True)
    finally:
        stop(system)
        if log.exists():
            shutil.copy2(log, logs / "last.log")
        server.shutdown()
        server.server_close()
        if system == "Windows" and app and app.exists():
            run(str(app.parent / "uninstall.exe"), "/S", f"_?={app.parent}")


if __name__ == "__main__":
    exercise()
