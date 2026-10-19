# Prompt fixtures (AI-15)

Real-world style conflicts, written for this repo, that `just ai-eval` sends
to a provider to compare prompt versions. Nothing here runs in `just check`
or CI except the check that every fixture loads.

Each folder `conflicts/<nn>-<name>/` holds plain text only (QA-2):

- `meta.toml`: `path`, `operation` (`merge`, `rebase`, `cherry-pick`,
  `revert` or `stash-apply`), `ours_label`, `theirs_label`, and the commit
  messages per side, newest first, in `ours_commits` and `theirs_commits`.
- `base`, `ours`, `theirs`: the whole file on each side. The hunks are what
  the conflict editor would show for them.
- `accept/1.txt`, `accept/2.txt`, …: each an acceptable resolution of the
  whole file. Only the hunks may differ from the merged text, since the
  editor changes nothing else.
- or `accept/UNRESOLVED`: no resolution is right, and why. Such a fixture
  has one hunk.

A hunk *matches* when its proposal equals its part of an accepted resolution,
after line endings are normalized and trailing whitespace is trimmed. A file
matches when the whole file, with the proposals applied, equals one.
