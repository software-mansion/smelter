#!/usr/bin/env bash

set -eo pipefail
set -x

sleep 2

export DBUS_SESSION_BUS_ADDRESS=unix:path=$XDG_RUNTIME_DIR/bus
sudo service dbus start
dbus-daemon --session --address=$DBUS_SESSION_BUS_ADDRESS --nofork --nopidfile --syslog-only &

xvfb-run "$SMELTER_MAIN_EXECUTABLE_PATH"
