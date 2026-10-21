# Conflicts and AI

## When git stops on conflicts

A merge, rebase, cherry-pick, revert, stash apply or pull can stop with conflicts: files that git could not merge on its own. The repository is then in conflict state. gitcroc sees this from the repository itself, so a conflict you started in a terminal looks the same as one started in gitcroc.

A repository in conflict state shows a conflict icon in the tree. When you select it, a banner above the history names the operation and what it was doing, such as "Merge in progress: merging feature/x into main" or "Rebase in progress: rebasing feature/x onto main, step 3 of 7". The banner has:

- **Abort**: stops the operation and goes back to where it started. Edits you have not marked resolved are dropped.
- **Continue**: carries on once every conflicted file is resolved and staged. It stays greyed out until then. A rebase or a cherry-pick of several commits may stop again at a later commit.
- **Resolve all with AI**, while AI is set up (see [AI help](#ai-help)).

A stash apply (or pop) that stops on conflicts has no Continue: it ends when you mark its last file resolved. Its **Abort** puts the files it changed back as they were; the stash and your other local changes are kept.

While a repository is in conflict state, other operations that change it are blocked until you continue or abort. Fetch still works.

## The conflict editor

In the changes pane, conflicted files come first, under **Unmerged**, each with how the two sides changed it (for example "both modified"). Select one, and the conflict editor takes the place of the diff.

The editor shows the file as it will be written, with each conflict highlighted. Above each conflict, the two sides are labelled with the branch or commit they really come from, such as **Ours: main** and **Theirs: feature/x**. The labels matter because the meaning of "ours" and "theirs" flips between a merge and a rebase. Each conflict has:

- **Take ours**, **Take theirs** and **Take both**: put that side's text, or both, in place of the conflict.
- **Show base**: shows the text both sides started from.

You can also edit the text freely, like in any editor. The toolbar above the editor shows how many conflicts are left, with **Previous conflict** and **Next conflict**.

Nothing is written to the file while you work. Your edits stay as they are when you switch to another file, repository or tab, and the file list marks such a file with **unsaved edits**. Closing the tab or restarting for an update loses them, and gitcroc warns you first.

When the file is right, click **Mark resolved** (or press Ctrl+Enter, Cmd+Enter on macOS). gitcroc writes the file and stages it. If a line still looks like a conflict marker (`<<<<<<<`, `=======`, `>>>>>>>`), it refuses and names the line; for a file that really contains such lines, click **Mark resolved anyway**.

When every file is resolved, the banner says "All conflicts are resolved and staged." Click **Continue**.

### Files that cannot be merged line by line

Some conflicts are not text: a binary file, a file deleted on one side and changed on the other, a file whose mode both sides changed differently, a submodule, a symbolic link, a file that is not UTF-8 text or is too large to edit. For these, gitcroc shows why and the two versions side by side, with their size or what they are. Click **Use this version** under the one to keep.

### A file edited outside gitcroc

If the conflicted file was changed on disk since the conflict began, for example in your code editor, gitcroc asks before showing it: **Keep the file as it is**, or **Start over from the conflict**.

## AI help

gitcroc can ask an AI model to propose a resolution for each conflict. You use your own account: an API key, or an AI command-line tool you already have installed. The AI only proposes. It never writes a file, stages, commits or continues an operation; each of those still takes your click.

### Setting up a provider

Open **Settings** and scroll to the **AI** section. **AI features** must be checked (it is by default). Under **Providers**, click **Add provider…** and pick the **Kind**:

| Kind | You enter |
| --- | --- |
| **Anthropic** | **API key** and **Model** |
| **OpenAI-compatible** | **Base URL** (such as `https://api.openai.com/v1`, or the address of a local Ollama or LM Studio server), **API key (optional)** and **Model**. This covers OpenAI, OpenRouter, Ollama, LM Studio and similar services |
| **Claude Code** | **Program** (gitcroc finds it if it is installed) and **Model (optional)** |
| **Codex** | **Program** (gitcroc finds it if it is installed) and **Model (optional)** |

Give the provider a **Name**. **Fetch models** lists the models the provider offers, where it offers a list; otherwise type the model's name. **Test** checks that the provider can be reached and used, and shows the result. **Context budget in tokens** (100,000 by default) is how much text one request may hold; gitcroc splits larger files across several requests. Click **Save**.

If gitcroc finds Claude Code or Codex on your computer, the AI section offers it ("Found Claude Code at …") with **Add it…**.

You can set up several providers. One is the default, marked **(default)**, and that is the one gitcroc uses; **Set as default** changes it. **Edit…** and **Remove…** change or remove a provider.

API keys are kept only in your system's keychain: Keychain on macOS, Credential Manager on Windows, the Secret Service (GNOME Keyring, KWallet and others) on Linux. They are never written to a file, a log or an error message. If the keychain cannot be used, the AI section says so, and keys cannot be stored (see [Troubleshooting](troubleshooting.md#the-keychain-cannot-be-used)). A provider that needs no key, such as a local model or a command-line tool, works without it.

The command-line tools run in an empty temporary folder, never in your repository, with all their tools turned off. They receive the same request as the API providers.

### The first time: consent

The first time a provider would be used, gitcroc asks **Send to &lt;host or program&gt;?** It names the host the provider sends to, or the program it runs, and lists exactly what is sent. Nothing is sent until you click **Allow**. If you later change the provider's host or program, gitcroc asks again.

### What is sent

For each file, gitcroc sends exactly this and nothing else:

- the file's path;
- the kind of operation (merge, rebase and so on);
- the labels of the two sides, such as `main` and `feature/x`;
- for each conflict not yet resolved: the text of the base, ours and theirs, with 40 lines around it;
- the messages of up to 10 commits per side that changed the file since the two sides split, each cut at 2,000 characters.

A file is never sent when it is not a text conflict the editor can show, is larger than 1 MB, or matches **Never send files matching (one pattern per line)**. That list starts as `.env*`, `*.pem`, `*.key`, `*.p12` and `id_rsa*`; add your own patterns. A file that is not sent says why.

The requests and answers are kept in memory only. They are written to the log only while **AI debug logging** is on, and then the log holds your code, so turn it off again when you are done. Keys are never logged.

### Resolving with AI

In the conflict editor, click **Resolve with AI**. gitcroc sends the file and fills in a proposal for each conflict it has an answer for, showing:

- the proposed text;
- a short explanation of what it kept from each side and why;
- the confidence: **High confidence**, **Medium confidence** or **Low confidence**.

For each proposal choose **Accept** (Alt+4), **Reject** (Alt+5) or **Edit** (Alt+6). Edit lets you change the text first; then **Accept** (Ctrl+Enter) or cancel (Escape). **Accept all high-confidence** accepts every untouched proposal of this file that has high confidence. There is no way to accept every proposal of every file at once. A conflict the AI could not resolve says **Not resolved**, with its reason.

Accepting puts the text into the editor and nowhere else. As always, **Mark resolved** writes and stages the file, and **Continue** carries on with the operation.

If the provider's answer is unusable, gitcroc asks once more, then shows **AI failed** with the reason and **Try again**. A running request can be cancelled. When the provider reports it, the tokens each request used show above the editor, such as "Request 1: 1,450 input tokens, 120 output tokens".

**Resolve all with AI**, in the banner, does this for every conflicted file of the repository, four files at a time, with each file's progress. Cancelling keeps the proposals that arrived; files not yet done stay untouched. You still review each file.

Proposals are never applied on their own, and they disappear if you abort.

### Turning AI off

- **For one workspace:** uncheck **AI for this workspace** in the tab's menu. The workspace then sends nothing to any AI.
- **Everywhere:** uncheck **AI features** in Settings. gitcroc then sends no AI request, does not look for or run the command-line tools, and hides every AI button and the rest of the AI settings. Running AI requests are cancelled and proposals dropped. Your providers, keys and consents are kept for when you turn it on again.
