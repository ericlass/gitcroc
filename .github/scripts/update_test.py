"""PLAT-4, SEC-7, D12 (M6-28c): installed apps update from a local server.

    python update_test.py gpui <a> <b> <version>
    python update_test.py rc1 <rc1> <b> <version>

<a> holds packages.yml's packages of A, the workspace version <version>,
built with the `update-test` feature; <b> holds B, A with the patch number
raised by one: its update package, the package's .sig and the throwaway
public key, update-test.pub. <rc1> holds 1.0.0-rc.1 built from its tag with
its own `update-test` feature and that key (update_rc1_build.py).

`gpui`: A installed updates itself to B and starts as B; B refuses a C
whose signature does not match and stays B; on Linux, A's .deb only
notifies about B. `rc1`: rc.1 installed updates itself to the GPUI B, which
starts as the same package; on Windows there is still one install directory
and one uninstall key.

Both apps' only endpoint is http://127.0.0.1:8765/latest.json, served here
(D12). Each step waits for a log line, never for a time; the deadline only
ends a hung app (README 'Timing in CI'). The logs go to $UPDATE_LOGS.
"""

import functools
import hashlib
import http.server
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from urllib.parse import urlsplit

from install_common import config_and_log
from update_version import newer

ROOT = Path(__file__).resolve().parents[2]
WORK = Path(os.environ.get("RUNNER_TEMP", tempfile.gettempdir())) / "gitcroc-update-test"
PORT = 8765
BASE = f"http://127.0.0.1:{PORT}"
DEADLINE = 600
# Retain and reap apps we launch: the Unix restart helper waits for the
# old PID to disappear, which includes waiting for its parent to reap it.
PROCESSES = []


def reap_processes():
    PROCESSES[:] = [process for process in PROCESSES if process.poll() is None]

SYSTEM = {"Windows": "windows", "Darwin": "macos", "Linux": "linux"}[platform.system()]
# The manifest's platform key, the start line's OS and arch, and the
# package that installs updates.
PLATFORM, OS, ARCH, PACKAGE = {
    "windows": ("windows-x86_64", "windows", "x86_64", "installer"),
    "macos": ("darwin-aarch64", "macos", "aarch64", "app"),
    "linux": ("linux-x86_64", "linux", "x86_64", "AppImage"),
}[SYSTEM]


def update_package(version):
    """The file name of `version`'s update package on this platform."""
    return {
        "windows": f"gitcroc_{version}_x64-setup.exe",
        "macos": "gitcroc.app.tar.gz",
        "linux": f"gitcroc_{version}_amd64.AppImage",
    }[SYSTEM]


def start_line(version, package=PACKAGE):
    """M5-14's start line."""
    return f"gitcroc {version} started ({OS} {ARCH}, {package})"


def installed(text, version, package=PACKAGE):
    """The update to `version` was installed and `version` started."""
    return f"update: installed {version}" in text and start_line(version, package) in text


def refused(text):
    """An update was refused for its signature."""
    return re.search(r"update: refused \([^\n]*[Ss]ignature", text) is not None


def localize(manifest, base=BASE):
    """Points each package's URL at the local server (D12)."""
    if urlsplit(base).hostname != "127.0.0.1":
        raise ValueError(f"{base} is not the loopback address")
    for entry in manifest["platforms"].values():
        entry["url"] = base + "/" + urlsplit(entry["url"]).path.rsplit("/", 1)[-1]
    return manifest


def wait(log, predicate, what):
    """The log's text once `predicate` holds for it."""
    deadline = time.monotonic() + DEADLINE
    while True:
        reap_processes()
        text = log.read_text(encoding="utf-8", errors="replace") if log.exists() else ""
        if predicate(text):
            return text
        if time.monotonic() >= deadline:
            raise TimeoutError(f"{what} is missing from the log:\n{text}")
        # Polls for the line; the interval is no success condition.
        time.sleep(0.1)


def digest(path):
    """The SHA-256 of a file, or of every file in a folder."""
    hasher = hashlib.sha256()
    files = sorted(p for p in path.rglob("*") if p.is_file()) if path.is_dir() else [path]
    for file in files:
        hasher.update(str(file.relative_to(path) if path.is_dir() else file.name).encode())
        hasher.update(file.read_bytes())
    return hasher.hexdigest()


def run(*args, **kwargs):
    print("+", " ".join(map(str, args)), flush=True)
    return subprocess.run([str(arg) for arg in args], check=True, **kwargs)


class Server:
    """The local update server: `folder` on 127.0.0.1:8765."""

    def __init__(self, folder):
        self.folder = folder
        folder.mkdir(parents=True, exist_ok=True)
        handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(folder))
        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", PORT), handler)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def publish(self, manifest):
        temporary = self.folder / "latest.json.tmp"
        temporary.write_text(json.dumps(manifest), encoding="utf-8")
        temporary.replace(self.folder / "latest.json")

    def close(self):
        self.server.shutdown()
        self.server.server_close()


class Logs:
    def __init__(self):
        self.folder = Path(os.environ.get("UPDATE_LOGS", WORK / "logs"))
        self.folder.mkdir(parents=True, exist_ok=True)
        self.config, self.log = config_and_log(SYSTEM)

    def reset(self, update_check=False):
        """Settings with the scheduled check off, and no log or pending
        install from an earlier start."""
        self.config.mkdir(parents=True, exist_ok=True)
        value = "true" if update_check else "false"
        (self.config / "settings.toml").write_text(f"schema = 2\nupdate_check = {value}\n", encoding="utf-8")
        (self.config / "update-test.pending").unlink(missing_ok=True)
        self.log.unlink(missing_ok=True)

    def keep(self, name):
        if self.log.exists():
            shutil.copy2(self.log, self.folder / f"{name}.log")


def start(executable, logs):
    print(f"starting {executable}", flush=True)
    with (logs.folder / "output.log").open("ab") as output:
        PROCESSES.append(subprocess.Popen(
            [str(executable)], cwd=WORK, stdout=output, stderr=subprocess.STDOUT
        ))


def stop():
    """Ends every gitcroc process and waits until none is left."""
    if SYSTEM == "windows":
        subprocess.run(["taskkill", "/IM", "gitcroc.exe", "/T", "/F"], capture_output=True)
        running = lambda: b"gitcroc.exe" in subprocess.run(
            ["tasklist", "/FI", "IMAGENAME eq gitcroc.exe"], capture_output=True
        ).stdout
    else:
        subprocess.run(["pkill", "-x", "gitcroc"], capture_output=True)
        running = lambda: subprocess.run(["pgrep", "-x", "gitcroc"], capture_output=True).returncode == 0
    deadline = time.monotonic() + 60
    while True:
        reap_processes()
        if not running():
            break
        if time.monotonic() >= deadline:
            raise TimeoutError("gitcroc did not stop")
        time.sleep(0.1)


def manifest_for(version, folder):
    """B's latest.json from `release-manifest local`, pointed at the server."""
    tool = ROOT / "target/debug" / ("release-manifest.exe" if SYSTEM == "windows" else "release-manifest")
    output = subprocess.run(
        [str(tool), "local", "--version", version, "--dir", str(folder), "--pub-date", "2026-11-05T12:00:00Z",
         "--platform", PLATFORM, "--package", update_package(version)],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout
    return localize(json.loads(output))


def serve_b(b_dir, version):
    """B's update package and .sig on the local server, offered in its
    latest.json."""
    b = newer(version)
    server = Server(WORK / "serve")
    for name in (update_package(b), update_package(b) + ".sig"):
        shutil.copy2(b_dir / name, server.folder / name)
    manifest = manifest_for(b, server.folder)
    server.publish(manifest)
    return server, manifest, b


# Where the app is installed, per platform: its executable, and the file or
# folder an update replaces.
WINDOWS_DIR = Path(os.environ.get("LOCALAPPDATA", ".")) / "gitcroc"
MAC_APPS = WORK / "Applications"


def install_windows(installer):
    run(installer, "/S")
    executable = WINDOWS_DIR / "gitcroc.exe"
    return executable, executable


def uninstall_windows():
    uninstaller = WINDOWS_DIR / "uninstall.exe"
    if uninstaller.exists():
        run(uninstaller, "/S", f"_?={WINDOWS_DIR}")
    shutil.rmtree(WINDOWS_DIR, ignore_errors=True)


def install_mac(app):
    MAC_APPS.mkdir(parents=True, exist_ok=True)
    target = MAC_APPS / "gitcroc.app"
    shutil.rmtree(target, ignore_errors=True)
    shutil.copytree(app, target, symlinks=True)
    return target / "Contents/MacOS/gitcroc", target


def install_mac_dmg(dmg):
    mount = WORK / "dmg"
    mount.mkdir(parents=True, exist_ok=True)
    run("hdiutil", "attach", dmg, "-readonly", "-nobrowse", "-mountpoint", mount)
    try:
        return install_mac(mount / "gitcroc.app")
    finally:
        run("hdiutil", "detach", mount)


def install_appimage(appimage):
    target = WORK / "Apps/gitcroc.AppImage"
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(appimage, target)
    target.chmod(0o755)
    return target, target


def update(executable, replaced, logs, a, b, name, package=PACKAGE):
    """Starts the installed `a`, waits until it has installed `b` and `b`
    has started, and checks the installed files changed."""
    before = digest(replaced)
    logs.reset()
    start(executable, logs)
    try:
        wait(logs.log, lambda text: f"update: installed {b}" in text, f"update: installed {b}")
        text = wait(logs.log, lambda text: start_line(b, package) in text, start_line(b, package))
    finally:
        logs.keep(name)
    if start_line(a, package) not in text:
        raise AssertionError(f"{a} did not start first:\n{text}")
    stop()
    if digest(replaced) == before:
        raise AssertionError(f"{replaced} was not replaced")
    print(f"PASS: {a} installed {b} and restarted as {b} ({package})", flush=True)


def refuse(executable, replaced, logs, server, manifest, b):
    """Offers a C whose package does not match its signature; the
    installed B refuses it and stays."""
    good = server.folder / update_package(b)
    bad = server.folder / ("tampered-" + good.name)
    shutil.copy2(good, bad)
    with bad.open("ab") as file:
        file.write(b"tampered")
    c = newer(b)
    manifest = json.loads(json.dumps(manifest))
    manifest["version"] = c
    manifest["platforms"][PLATFORM]["url"] = f"{BASE}/{bad.name}"
    server.publish(manifest)
    before = digest(replaced)
    logs.reset()
    start(executable, logs)
    try:
        wait(logs.log, lambda text: start_line(b) in text and refused(text), "a refused signature")
    finally:
        logs.keep("refused")
    stop()
    if digest(replaced) != before:
        raise AssertionError(f"{replaced} changed although C was refused")
    print(f"PASS: {b} refused C ({c}), whose signature does not match, and stayed {b}", flush=True)


def notify_deb(deb, logs, server, manifest, a, b):
    """A's .deb only says that B is out (Linux)."""
    server.publish(manifest)
    run("sudo", "apt-get", "install", "-y", deb)
    executable = Path("/usr/bin/gitcroc")
    try:
        before = digest(executable)
        logs.reset()
        start(executable, logs)
        try:
            wait(logs.log, lambda text: f"update: notify only ({b})" in text and start_line(a, "deb") in text,
                 f"update: notify only ({b})")
        finally:
            logs.keep("notify")
        stop()
        if digest(executable) != before:
            raise AssertionError("the .deb's gitcroc was changed by the updater")
    finally:
        run("sudo", "apt-get", "remove", "-y", "gitcroc")
    print(f"PASS: the .deb of {a} only notified about {b}", flush=True)


def gpui(a_dir, b_dir, version):
    """GPUI A to B, then the refusal, then (Linux) the .deb's notice."""
    a = version
    logs = Logs()
    server, manifest, b = serve_b(b_dir, version)
    try:
        if SYSTEM == "windows":
            executable, replaced = install_windows(a_dir / f"gitcroc_{a}_x64-setup.exe")
        elif SYSTEM == "macos":
            executable, replaced = install_mac_dmg(a_dir / f"gitcroc_{a}_universal.dmg")
        else:
            executable, replaced = install_appimage(a_dir / f"gitcroc_{a}_amd64.AppImage")
        update(executable, replaced, logs, a, b, "installed")
        refuse(executable, replaced, logs, server, manifest, b)
        if SYSTEM == "linux":
            notify_deb(a_dir / f"gitcroc_{a}_amd64.deb", logs, server, manifest, a, b)
    finally:
        stop()
        logs.keep("last")
        server.close()
        if SYSTEM == "windows":
            uninstall_windows()


def rc1(rc1_dir, b_dir, version):
    """rc.1 to the GPUI B, as the same package."""
    rc1_version = json.loads((rc1_dir / "version.json").read_text(encoding="utf-8"))
    logs = Logs()
    server, _, b = serve_b(b_dir, version)
    try:
        if SYSTEM == "windows":
            uninstall_windows()
            executable, replaced = install_windows(next(rc1_dir.glob("*-setup.exe")))
        elif SYSTEM == "macos":
            executable, replaced = install_mac(rc1_dir / "gitcroc.app")
        else:
            executable, replaced = install_appimage(next(rc1_dir.glob("*.AppImage")))
        update(executable, replaced, logs, rc1_version, b, "rc1-installed")
        if SYSTEM == "windows":
            one_installation(b)
    finally:
        stop()
        logs.keep("rc1-last")
        server.close()
        if SYSTEM == "windows":
            uninstall_windows()


def one_installation(version):
    """Windows: one install directory, %LOCALAPPDATA%\\gitcroc, and one
    uninstall key, HKCU's, for `version`, as M6-28a's upgrade check."""
    import winreg

    path = r"Software\Microsoft\Windows\CurrentVersion\Uninstall"
    roots = [
        ("HKCU", winreg.HKEY_CURRENT_USER, path),
        ("HKLM", winreg.HKEY_LOCAL_MACHINE, path),
        ("HKLM", winreg.HKEY_LOCAL_MACHINE, r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
    ]
    found = []
    for name, root, path_ in roots:
        try:
            parent = winreg.OpenKey(root, path_)
        except OSError:
            continue
        with parent:
            index = 0
            while True:
                try:
                    sub = winreg.EnumKey(parent, index)
                except OSError:
                    break
                index += 1
                with winreg.OpenKey(parent, sub) as key:
                    try:
                        display = winreg.QueryValueEx(key, "DisplayName")[0]
                    except OSError:
                        continue
                    if display == "gitcroc":
                        values = {}
                        for value in ("DisplayVersion", "InstallLocation"):
                            try:
                                values[value] = winreg.QueryValueEx(key, value)[0]
                            except OSError:
                                values[value] = None
                        found.append((name, sub, values))
    if [(name, sub) for name, sub, _ in found] != [("HKCU", "gitcroc")]:
        raise AssertionError(f"expected one uninstall key, HKCU\\{path}\\gitcroc: {found}")
    values = found[0][2]
    if values["DisplayVersion"] != version:
        raise AssertionError(f"DisplayVersion is {values['DisplayVersion']}, not {version}")
    if Path(str(values["InstallLocation"]).strip('"')) != WINDOWS_DIR:
        raise AssertionError(f"InstallLocation is {values['InstallLocation']}")
    others = [
        Path(os.environ["LOCALAPPDATA"]) / "Programs/gitcroc",
        Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "gitcroc",
        Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "gitcroc",
    ]
    if any(other.exists() for other in others):
        raise AssertionError(f"another install directory: {[o for o in others if o.exists()]}")
    exes = sorted(p.name for p in WINDOWS_DIR.rglob("*.exe"))
    if exes != ["gitcroc.exe", "uninstall.exe"]:
        raise AssertionError(f"the install directory holds {exes}")
    print(f"PASS: one install directory, {WINDOWS_DIR}, and one uninstall key, for {version}", flush=True)


if __name__ == "__main__":
    WORK.mkdir(parents=True, exist_ok=True)
    match sys.argv[1:]:
        case ["gpui", a_dir, b_dir, version]:
            gpui(Path(a_dir).resolve(), Path(b_dir).resolve(), version)
        case ["rc1", rc1_dir, b_dir, version]:
            rc1(Path(rc1_dir).resolve(), Path(b_dir).resolve(), version)
        case _:
            sys.exit(__doc__)
