#!/bin/sh
# Source me: sets env for rootless AT-SPI (extracted debs in .syslibs) and starts the a11y bus if needed.
D=${PINSHOT_DIR:-$(cd "$(dirname "$0")" 2>/dev/null && pwd)}
S="$D/.syslibs"
export GI_TYPELIB_PATH="$S/usr/lib/x86_64-linux-gnu/girepository-1.0${GI_TYPELIB_PATH:+:$GI_TYPELIB_PATH}"
export PYTHONPATH="$S/usr/lib/python3/dist-packages${PYTHONPATH:+:$PYTHONPATH}"
export XDG_DATA_DIRS="$S/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
# Session bus: reuse the desktop's (the one Chrome uses) when we don't have one
if [ -z "$DBUS_SESSION_BUS_ADDRESS" ]; then
  A=$(tr '\0' '\n' < /proc/$(pgrep -o -f 'chrome.*--class=box-chrome' 2>/dev/null)/environ 2>/dev/null | sed -n 's/^DBUS_SESSION_BUS_ADDRESS=//p')
  [ -n "$A" ] && export DBUS_SESSION_BUS_ADDRESS="$A"
fi
CONF=/home/box/.local/a11y/atspi/accessibility.conf   # path length matches patched launcher
if [ ! -f "$CONF" ]; then
  mkdir -p "$(dirname "$CONF")"
  sed "s|<servicedir>.*</servicedir>|<servicedir>$S/usr/share/dbus-1/accessibility-services</servicedir>|" \
    "$S/usr/share/defaults/at-spi2/accessibility.conf" > "$CONF"
fi
if [ -n "$DBUS_SESSION_BUS_ADDRESS" ] && ! pgrep -f "$S/usr/libexec/at-spi-bus-launcher" >/dev/null; then
  LD_LIBRARY_PATH="$S/usr/lib/x86_64-linux-gnu:$LD_LIBRARY_PATH" "$S/usr/libexec/at-spi-bus-launcher" --launch-immediately --a11y=1 >/dev/null 2>&1 &
  sleep 1
fi
export QT_LOGGING_RULES="${QT_LOGGING_RULES:+$QT_LOGGING_RULES;}qt.accessibility.atspi=false"
