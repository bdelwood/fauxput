#!/usr/bin/env bash
# fauxput "up" wrapper for Sunshine prep-cmd. Forwards Sunshine's
# client env vars to `fauxput up`. Set FAUXPUT_KEEP_REAL=1 to skip
# the real-output disable.

set -euo pipefail

WIDTH="${SUNSHINE_CLIENT_WIDTH:-1920}"
HEIGHT="${SUNSHINE_CLIENT_HEIGHT:-1080}"
FPS="${SUNSHINE_CLIENT_FPS:-60}"
HDR="${SUNSHINE_CLIENT_HDR:-0}"

logger -t fauxput-sunshine "up: ${SUNSHINE_CLIENT_NAME:-unknown} ${WIDTH}x${HEIGHT}@${FPS} hdr=${HDR}"

DISABLE_FLAG=( --disable-real-outputs )
[[ "${FAUXPUT_KEEP_REAL:-0}" == "1" ]] && DISABLE_FLAG=()

HDR_FLAG=()
[[ "${HDR}" == "true" || "${HDR}" == "1" ]] && HDR_FLAG=( --hdr )

exec fauxput up \
    --width "${WIDTH}" \
    --height "${HEIGHT}" \
    --fps "${FPS}" \
    --primary \
    "${DISABLE_FLAG[@]}" \
    "${HDR_FLAG[@]}"
