#!/bin/sh
# Runs a command in a throwaway X server and D-Bus session with the AT-SPI
# bus enabled, so GPUI's AccessKit adapter activates without touching the
# desktop's own bus, dconf or screen reader (M6-25a, from M6-04's spike).
# Needs xvfb-run, dbus-run-session and at-spi2-core.
#
#   crates/gitcroc-ui/tests/a11y_tree/session.sh <command> [args...]
set -eu
if [ "${A11Y_SESSION_INNER:-}" != 1 ]; then
    export A11Y_SESSION_INNER=1
    exec xvfb-run -a -s "-screen 0 1920x1200x24" dbus-run-session -- "$0" "$@"
fi
# Keep any GSettings writes (the portal's) out of the user's dconf.
export GSETTINGS_BACKEND=memory
export GITCROC_A11Y=1
exec "$@"
