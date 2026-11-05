# Native application screenshots

Captured from the real GPUI window on Linux under a private 1280×800
Xvfb display, from the generated smoke fixture, with default appearance
settings. Xvfb has no window manager, so system window controls are absent.
No personal repositories, credentials or settings are used.

To refresh (Pillow and `xwd` are needed only for documentation capture):

1. Run `xvfb-run -a just smoke` and keep its printed fixture directory.
2. Run `xvfb-run -a -s '-screen 0 1280x800x24' python .github/scripts/capture_smoke.py --app target/e2e/debug/gitcroc-next --fixture <fixture-directory> --out docs/user/images`.
3. Inspect both PNGs before committing them. The capture reads the private
   X server's framebuffer directly, rather than a desktop screenshot service.

The `e2e` script's `hold` option leaves the scene open for capture; regular
smoke scripts quit after their assertions.
