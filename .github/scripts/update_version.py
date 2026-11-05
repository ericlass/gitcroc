"""PLAT-4 (M6-28c): the update test's B, the workspace version A with the
patch number raised by one.

    python update_version.py          prints B
    python update_version.py --bump   sets the workspace to B and prints it

--bump rewrites the version in the workspace Cargo.toml and the workspace
members' entries in Cargo.lock, so the builds of B still pass --locked.
"""

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]


def newer(version):
    """The version with the patch number raised by one, its pre-release and
    build parts kept: 1.0.0-rc.1 gives 1.0.1-rc.1."""
    match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)([-+].*)?", version)
    if not match:
        raise ValueError(f"{version!r} is not a version")
    major, minor, patch, rest = match.groups()
    return f"{major}.{minor}.{int(patch) + 1}{rest or ''}"


def workspace_version(root=ROOT):
    """[workspace.package]'s version in `root`'s Cargo.toml. Read without
    tomllib, which ubuntu-22.04's Python 3.10 lacks."""
    return manifest_version((root / "Cargo.toml").read_text(encoding="utf-8"))


def manifest_version(text):
    match = re.search(r'\[workspace\.package\][^\[]*?\nversion = "([^"]+)"', text)
    if not match:
        raise ValueError("Cargo.toml has no [workspace.package] version")
    return match.group(1)


def bump_manifest(text, old, new):
    """Cargo.toml with [workspace.package]'s version set to `new`."""
    pattern = re.compile(r'(\[workspace\.package\][^\[]*?\nversion = ")' + re.escape(old) + '"')
    text, count = pattern.subn(lambda match: match.group(1) + new + '"', text)
    if count != 1:
        raise ValueError("Cargo.toml has no [workspace.package] version " + old)
    return text


def bump_lock(text, old, new):
    """Cargo.lock with the workspace members' version `old` set to `new`:
    the packages with no `source`."""
    blocks = text.split("\n[[package]]\n")
    changed = 0
    for index, block in enumerate(blocks):
        if index == 0 or "\nsource = " in block:
            continue
        line = f'\nversion = "{old}"'
        if line in block:
            blocks[index] = block.replace(line, f'\nversion = "{new}"', 1)
            changed += 1
    if not changed:
        raise ValueError("Cargo.lock has no workspace member at " + old)
    return "\n[[package]]\n".join(blocks)


def bump(root=ROOT):
    old = workspace_version(root)
    new = newer(old)
    manifest = root / "Cargo.toml"
    manifest.write_text(bump_manifest(manifest.read_text(encoding="utf-8"), old, new), encoding="utf-8")
    lock = root / "Cargo.lock"
    lock.write_text(bump_lock(lock.read_text(encoding="utf-8"), old, new), encoding="utf-8")
    return new


if __name__ == "__main__":
    if sys.argv[1:] == ["--bump"]:
        print(bump())
    elif sys.argv[1:] == []:
        print(newer(workspace_version()))
    else:
        sys.exit(__doc__)
