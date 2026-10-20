"""PLAT-4, SEC-7: run only in CI; no generated key or overlay is uploaded."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
WORK = Path(os.environ.get("RUNNER_TEMP", tempfile.gettempdir())) / "gitcroc-update-test"


def run(*args, **kwargs):
    return subprocess.run(args, check=True, cwd=ROOT, **kwargs)


def build():
    shutil.rmtree(WORK, ignore_errors=True)
    # Older runs put generated packages in Cargo's cached target directory.
    shutil.rmtree(ROOT / "target/update-test", ignore_errors=True)
    WORK.mkdir(parents=True, exist_ok=True)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    core, separator, suffix = version.partition("-")
    core, metadata_separator, metadata = core.partition("+")
    major, minor, patch = core.split(".")
    newer = f"{major}.{minor}.{int(patch) + 1}{separator}{suffix}{metadata_separator}{metadata}"
    system = os.environ["RUNNER_OS"]
    target = "universal-apple-darwin" if system == "macOS" else ""
    bundles = {"Windows": "nsis", "macOS": "app,dmg", "Linux": "appimage,deb"}[system]
    key = WORK / "updater.key"
    config = ROOT / "target/update-test-overlay.json"
    pnpm = shutil.which("pnpm")
    try:
        run(pnpm, "--dir", "ui", "exec", "tauri", "signer", "generate", "--ci", "--write-keys", str(key),
            stdout=subprocess.DEVNULL)
        public = Path(str(key) + ".pub").read_text().strip()
        for name, number in [("a", version), ("b", newer)]:
            overlay = {"version": number, "bundle": {"createUpdaterArtifacts": name == "b"},
                       "plugins": {"updater": {"pubkey": public,
                           "dangerousInsecureTransportProtocol": True,
                           "endpoints": ["http://127.0.0.1:8765/latest.json"]}}}
            config.write_text(json.dumps(overlay))
            env = os.environ.copy()
            env.update(TAURI_SIGNING_PRIVATE_KEY=str(key), TAURI_SIGNING_PRIVATE_KEY_PASSWORD="")
            args = ["just", "bundle", "--features", "update-test", "--bundles", bundles,
                    "--config", "../target/update-test-overlay.json"]
            if target:
                args += ["--target", target]
            run(*args, env=env)
            output = WORK / name
            output.mkdir()
            bundle = ROOT / "target" / target / "release/bundle"
            for extension in ["*-setup.exe", "*.dmg", "*.app.tar.gz", "*.AppImage", "*.deb", "*.sig"]:
                for file in bundle.rglob(extension):
                    shutil.copy2(file, output / file.name)
            # Do not mistake A's bundles for B's output on the next build.
            shutil.rmtree(bundle)
        run("cargo", "build", "--locked", "-p", "release-manifest")
        (WORK / "versions.json").write_text(json.dumps([version, newer]))
    finally:
        key.unlink(missing_ok=True)
        Path(str(key) + ".pub").unlink(missing_ok=True)
        config.unlink(missing_ok=True)


if __name__ == "__main__":
    build()
