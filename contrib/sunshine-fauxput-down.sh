#!/usr/bin/env bash
# fauxput "down" wrapper for Sunshine prep-cmd undo.

set -euo pipefail

logger -t fauxput-sunshine "down: ${SUNSHINE_CLIENT_NAME:-unknown}"

# Mirror the up wrapper: pipe stderr to journald under the same tag so
# teardown errors are visible alongside setup output.
exec systemd-cat -t fauxput-sunshine --level-prefix=false \
    fauxput down -vv
