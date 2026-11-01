# M6-19b1 Conflict code colours and syntax

First implementation PR of [M6-19b](M6-19b-conflict-presentation.md), split
under DEV-4. Read the parent's sources, design, scope and cited requirements.

Implements: UI-24, UI-25 (the conflict editor's colours and syntax)
Respects: ARCH-6, ARCH-7, UI-26, UI-29, D15, QA-3, QA-4, QA-5

Draw the kept buffer in the selected code theme: its text, caret,
selection, gutter and background, whatever the app theme, over a transparent
editor. Highlight syntax with the diff's syntect classes through the
editor's highlighter seam, parsing off the UI thread; unknown languages stay
plain text. Theme changes recolour without replacing the buffer.

Write first: `ui_24_conflict_syntax_and_theme_changes` and
`ui_25_conflict_text_contrast_meets_four_point_five`.

Acceptance: these tests and `just check` pass. Rows, headers and controls
remain M6-19b2.
