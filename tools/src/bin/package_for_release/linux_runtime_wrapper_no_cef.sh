#!/usr/bin/env bash

#
#   Runtime wrapper which provides paths to native libs used by the web renderer
#  

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

"$SCRIPT_DIR/dependency_check"
exec "$SCRIPT_DIR/smelter_main" "$@"
