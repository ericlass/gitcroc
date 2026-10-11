# 13. Milestones

Six milestones, each ending in something runnable, in this order because each one depends on the one before.

| Milestone | Delivers | Done when |
| --- | --- | --- |
| M0 Skeleton | Cargo workspace with all crates, Tauri window, SolidJS app, generated IPC types, light and dark theme switch, git detection (PLAT-3), `just check`, CI on three platforms | An empty three-pane window opens on all three platforms from CI artifacts; QA-1 is green |
| M1 Read-only single repo | Add repo, tree (UI-1 to UI-8 for one repo), history with graph, changes pane with both diff modes, code themes, cache, file watching | The single-repo budgets of section 9 are met on `target-project`, and the stretch results for `big-history` and `big-tree` are reported; no write operation exists yet |
| M2 Single-repo writes | Every operation in section 6, job engine, status bar and job list, askpass helper, conflict state with the manual conflict editor (OPS-1 to OPS-9) | Each operation has integration tests for success, failure and conflict; a merge conflict can be resolved and continued by hand |
| M3 Multi-repo | Workspaces, workspace tabs (UI-27 to UI-33), groups, multi-selection, preflight, group operations including group commit, result report, merged history, summary strip, import and export | The mixed-branch scenario passes as a test; all section 9 budgets are met on `target-project`, and the stretch results for `many-repos` are reported |
| M4 AI conflict resolution | Providers, settings, consent, proposals in the conflict editor, Resolve all, CLI providers | All AI requirements have tests against the mock provider; the fixture set (AI-15) runs against one API provider and one CLI provider |
| M5 Release | Installers, updater, command palette complete, accessibility and keyboard pass, user docs, website or README | v1.0.0 installs and updates on all three platforms; every requirement ID has a test (QA-3) |

Risk is front-loaded on purpose: M1 proves the performance claim on the hardest data before any feature work depends on it. If M1 misses its budgets, the stack decisions D2 and D3 are revisited before M2 starts.
