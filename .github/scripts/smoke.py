"""QA-2/QA-3, section 12: the e2e GPUI binary, real window and real git.

No network, personal state or fixed-time waits. Keep all artifacts on failure.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile


def call(args, **kwargs):
    return subprocess.check_output([str(arg) for arg in args], text=True, **kwargs).strip()


def click(id):
    return {"action": "click", "id": id}


def labelled(prefix, label):
    return {"action": "click-label", "prefix": prefix, "label": label}


def shown(id):
    return {"action": "shown", "id": id}


def input(text):
    return {"action": "input", "text": text}


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("--app", required=True, type=Path)
    parser.add_argument("--cli", required=True, type=Path)
    options = parser.parse_args()
    root = Path(tempfile.mkdtemp(prefix="gitcroc-smoke-", dir=os.environ.get("RUNNER_TEMP")))
    print(f"smoke artifacts: {root}", flush=True)
    env = dict(os.environ, GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
               GIT_TERMINAL_PROMPT="0", LC_ALL="C")
    repos = root / "repos"
    repos.mkdir()

    def git(repo, *args):
        return call(["git", "-C", repo, *args], env=env)

    for name in ["app", "lib"]:
        repo = repos / name
        repo.mkdir()
        git(repo, "init", "--quiet", "--initial-branch=main")
        git(repo, "config", "user.name", "Smoke Test")
        git(repo, "config", "user.email", "smoke@example.com")
        git(repo, "config", "core.autocrlf", "false")
        (repo / "a.txt").write_text("base\n")
        git(repo, "add", "--all")
        git(repo, "commit", "--quiet", "-m", "Base")
    app = repos / "app"
    git(app, "switch", "--quiet", "-c", "feature")
    (app / "a.txt").write_text("feature\n")
    git(app, "commit", "--quiet", "-am", "Feature")
    git(app, "switch", "--quiet", "main")
    (app / "a.txt").write_text("main\n")
    git(app, "commit", "--quiet", "-am", "Main")
    (app / "notes.txt").write_text("notes\n")
    config = root / "config"
    call([options.cli, "workspace", "--config-dir", config, "create", "Smoke"], env=env)

    def run(name, steps):
        script = root / f"{name}.json"
        script.write_text(json.dumps(dict(home=str(root), folder=str(repos), steps=steps)))
        with (root / f"{name}.log").open("w") as log:
            subprocess.run([str(options.app.resolve()), "--smoke", str(script)], env=env,
                           stdout=log, stderr=log, timeout=300, check=True)

    # Add through the UI, show history, stage and commit through actual controls.
    run("commit", [shown("toolbar"), click("tree-add"), labelled("menu-item", "Add folder"),
        shown("tree-add-report"), click("tree-add-report-close"),
        labelled("tree-row-", "app"), shown("history-list"), click("history-uncommitted"),
        labelled("changes-row-", "Stage all"), click("commit-subject"), input("Smoke commit"),
        click("commit-commit"),
        {"action": "label", "prefix": "history-row-", "label": "Smoke commit"}])
    assert git(app, "log", "-1", "--format=%s") == "Smoke commit"
    assert git(app, "status", "--porcelain") == ""
    # Merge through the UI; resolve through the native conflict editor.
    run("manual", [shown("toolbar"), labelled("tree-row-", "app"),
        labelled("toolbar-", "Merge"), shown("op-revision"), click("op-revision"), input("feature"),
        click("dialog-submit"), shown("conflict-banner"), click("history-uncommitted"),
        labelled("changes-row-", "a.txt"), click("conflict-0-take-theirs"),
        click("conflict-mark-resolved"), click("conflict-banner-continue"),
        {"action": "gone", "id": "conflict-banner"},
        {"action": "label", "prefix": "history-row-", "label": "Merge branch"}])
    assert not (app / ".git" / "MERGE_HEAD").exists()
    assert len(git(app, "rev-list", "--parents", "-n", "1", "HEAD").split()) == 3
    assert (app / "a.txt").read_text() == "feature\n"
    assert git(app, "status", "--porcelain") == ""
    # A second workspace, both repos, a group checkout whose missing branch
    # policy creates feature in lib. The preflight and report stay visible.
    call([options.cli, "workspace", "--config-dir", config, "create", "Second"], env=env)
    call([options.cli, "workspace", "--config-dir", config, "add", repos], env=env)
    run("group", [shown("toolbar"), labelled("tree-row-", "app"),
        {"action":"press", "keys":"shift-down"}, {"action":"press", "keys":"shift-f10"},
        labelled("menu-item", "Add to group"), labelled("submenu-item", "New group"),
        click("name-field"), input("Both"), click("dialog-submit"),
        labelled("tree-row-", "Both"),
        {"action":"press", "keys":"ctrl-k"}, shown("palette-input"), input("Check out branch"),
        {"action":"press", "keys":"enter"}, shown("group-name-field"),
        click("group-name-field"), input("feature"),
        labelled("group-name-missing", "Create the branch"), click("group-name-submit"),
        click("preflight-run"), {"action":"job-ended", "label":"Checkout feature"},
        {"action":"label", "prefix":"tree-row-", "label":"Both"}])
    for repo in [app, repos / "lib"]:
        assert git(repo, "branch", "--show-current") == "feature"
    # A new conflict in feature, then a mock proposal, explicit acceptance,
    # Mark resolved and Continue. The provider never contacts the network.
    git(app, "switch", "--quiet", "-c", "topic")
    (app / "a.txt").write_text("topic\n")
    git(app, "commit", "--quiet", "-am", "Topic")
    git(app, "switch", "--quiet", "feature")
    (app / "a.txt").write_text("ours\n")
    git(app, "commit", "--quiet", "-am", "Ours")
    replies = root / "ai-replies.json"
    replies.write_text(json.dumps([json.dumps({"hunks":[{"id":1,"status":"resolved",
        "resolution":"feature and topic\n", "explanation":"Keeps both sides.","confidence":"high"}]})]))
    env["GITCROC_E2E_AI"] = str(replies)
    run("ai", [shown("toolbar"), labelled("tree-row-", "app"), labelled("toolbar-", "Merge"),
        click("op-revision"), input("topic"), click("dialog-submit"), shown("conflict-banner"),
        click("history-uncommitted"), labelled("changes-row-", "a.txt"),
        click("toolbar-settings"), click("settings-tab-ai"), click("ai-add"),
        labelled("provider-kind", "Mock"), click("provider-name"), input("Smoke mock"),
        click("provider-save"), labelled("ai-provider-default", "Set"),
        click("ai-features"), {"action":"gone", "id":"ai-add"}, click("settings-close"),
        {"action":"gone", "id":"conflict-resolve-ai"}, click("toolbar-settings"),
        click("settings-tab-ai"), click("ai-features"), shown("ai-add"), click("settings-close"),
        click("conflict-resolve-ai"), click("consent-allow"), shown("conflict-proposal-0"),
        labelled("conflict-", "Accept all high-confidence"),
        click("conflict-mark-resolved"), click("conflict-banner-continue"),
        {"action":"gone", "id":"conflict-banner"},
        {"action":"label", "prefix":"history-row-", "label":"Merge branch"}])
    assert (app / "a.txt").read_text() == "feature and topic\n"
    assert git(app, "status", "--porcelain") == ""
    assert not (app / ".git" / "MERGE_HEAD").exists()
    print("smoke: commit, manual conflict, group checkout and mock AI passed", flush=True)


if __name__ == "__main__":
    main()
