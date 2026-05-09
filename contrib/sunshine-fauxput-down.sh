#!/usr/bin/env bash
# fauxput "down" wrapper for Sunshine prep-cmd undo.

set -euo pipefail

logger -t fauxput-sunshine "down: ${SUNSHINE_CLIENT_NAME:-unknown}"

exec fauxput down
