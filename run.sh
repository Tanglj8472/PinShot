#!/bin/sh
# Launcher: adds locally-extracted Qt system libs (if present) for root-less boxes.
D=$(cd "$(dirname "$0")" && pwd)
[ -d "$D/.syslibs" ] && export LD_LIBRARY_PATH="$D/.syslibs/usr/lib/x86_64-linux-gnu:$LD_LIBRARY_PATH"
PY="$D/.venv/bin/python"; [ -x "$PY" ] || PY=python3
export PYTHONUNBUFFERED=1
[ -f "$D/a11y_env.sh" ] && PINSHOT_DIR="$D" . "$D/a11y_env.sh"
cd "$D" && exec "$PY" -m pinshot "$@"
