# Privacy

gitcroc has no telemetry: it collects no usage data, crash reports or statistics, and has no account or sign-in. It makes only three kinds of network connection.

## Git operations

Fetch, pull, push, clone, and the operations that include them (such as deleting a branch on the remote or pushing a tag), connect to your remotes. git makes these connections, not gitcroc, using your git config, credential helpers and SSH agent, exactly as on the command line. gitcroc stores no git passwords or tokens (see [Credentials](troubleshooting.md#credentials-and-password-prompts)).

## AI requests you set up

Only when you have set up an AI provider, allowed it in the consent dialog, and asked for a resolution, gitcroc sends the conflicted file's details to that provider: an API host, or a command-line tool on your computer, which may in turn connect to its own service. [What is sent](conflicts-and-ai.md#what-is-sent) lists every item. Turning off **AI features** in Settings, or **AI for this workspace** in a tab's menu, stops all of it.

## The update check

This is the only connection gitcroc makes on its own. Once a day while it runs, starting shortly after it opens, gitcroc downloads a small file from gitcroc's GitHub releases page that names the latest version. It sends nothing about you or your repositories; GitHub sees the request as it sees any download. When you install an update, the new version is downloaded from the same place.

To turn it off, open **Settings** and uncheck **Check for updates automatically** in the **Updates** section. **Check now** still checks when you ask. A build you made yourself never checks.

## Nothing else

gitcroc loads no web pages, fonts, scripts or images from the internet; everything its window shows comes with the app. Its files stay on your computer (see [Settings and files](settings-and-files.md#files)).
