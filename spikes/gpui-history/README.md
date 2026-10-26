# GPUI history spike (throwaway)

The history pane and a commit diff in GPUI + gpui-kit 0.8.1, reading through
`gitcroc-git`. It exists to measure scrolling against the Tauri UI; it is not
part of the workspace and nothing depends on it.

```
cargo run --release                                   # target-project repo-01
cargo run --release -- --repo <path>                  # any repo
cargo run --release -- --commit <hash>                # open a commit's diff at start
cargo run --release -- --bench --frames 3000          # scripted run, prints JSON, quits
```

Keys: `b` runs the benchmark, `m` measures your own scrolling (mouse wheel,
either pane) and shows fps, frame-time percentiles and CPU time per frame in
the top bar.

The benchmark scrolls like `bench-ui`'s `scroll-frame`: 100 px a frame from
the top. History runs to the end; the diff of the last loaded commit (the root
commit of `target-project`, about 18,000 files, 216,000 lines) scrolls for
`--frames` frames while it is still loading. Per scenario it reports the frame
interval (vsync-paced, so 6.94 ms at 144 Hz), the share of frames over 1.5
intervals, CPU time per frame (start of `render` to the end of painting) and
the longest time a placeholder row was on screen.

## Results, 2026-10-26

RTX 4070, X11 (Cinnamon), 1920×1080 at 144 Hz, window 1920×1003, dark theme.
`results/` holds the JSON and screenshots.

| Scenario | fps | p50 | p99 | max | dropped | CPU p95 | blank |
| --- | --- | --- | --- | --- | --- | --- | --- |
| target-project history, 5,000 commits, 3 runs | 144.0 | 6.94 ms | 6.96 ms | 7.31 ms | 0 | 4.2–4.3 ms | 0 |
| target-project diff, 216k lines, 3 runs | 144.0 | 6.94 ms | 6.96 ms | 7.00 ms | 0 | 3.6–3.7 ms | 0 |
| Linux kernel history, 21,000 commits | 143.8 | 6.94 ms | 6.96 ms | 62.5 ms | 1 of 5,000 | 3.4 ms | 0 |

For comparison, the Tauri UI's `bench-ui` `scroll-frame` median on this
machine is 16 ms: WebKitGTK paces frames at 60 Hz on a 144 Hz display.

The first kernel run reached only 70 fps (CPU 9 ms a frame): every graph edge
was tessellated as a vector path on every frame. Drawing vertical edges as
quads and skipping lanes past the column's edge fixed it.

Linking: the build script links against `libxkbcommon-x11.so.0` directly,
because this machine has no `libxkbcommon-x11-dev`.
