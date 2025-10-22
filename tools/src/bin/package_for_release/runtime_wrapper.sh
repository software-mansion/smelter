#!/usr/bin/env bash

#
#   Runtime wrapper which provides paths to native libs used by the web renderer
#  

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

export SMELTER_PROCESS_HELPER_PATH="$SCRIPT_DIR/smelter_process_helper"

"$SCRIPT_DIR/dependency_check"
exec "$SCRIPT_DIR/smelter_main" "$@"
