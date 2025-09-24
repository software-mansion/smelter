#!/usr/bin/env bash

#
# Runtime wrapper which runs `dependency_check` before smelter.
#

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

"$SCRIPT_DIR/dependency_check"
exec "$SCRIPT_DIR/smelter_main"
