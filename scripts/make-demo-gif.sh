#!/usr/bin/env bash
# Turn a screen recording into an optimised demo GIF for the README.
#
#   ./scripts/make-demo-gif.sh ~/Desktop/recording.mov [start] [duration]
#
# Two-pass palette encoding keeps the UI's flat colours crisp at a fraction of
# the size a naive conversion produces.
set -euo pipefail

SRC="${1:?usage: make-demo-gif.sh <recording.mov> [start-seconds] [duration-seconds]}"
START="${2:-0}"
DURATION="${3:-30}"
OUT="media/demo.gif"
WIDTH=1000
FPS=12

command -v ffmpeg >/dev/null || { echo "error: ffmpeg not found (brew install ffmpeg)"; exit 1; }
[ -f "$SRC" ] || { echo "error: no such file: $SRC"; exit 1; }

mkdir -p media
PALETTE="$(mktemp -t cbnb-palette).png"
trap 'rm -f "$PALETTE"' EXIT

FILTERS="fps=${FPS},scale=${WIDTH}:-1:flags=lanczos"

echo "1/2 building colour palette…"
ffmpeg -v error -y -ss "$START" -t "$DURATION" -i "$SRC" \
  -vf "${FILTERS},palettegen=max_colors=192:stats_mode=diff" "$PALETTE"

echo "2/2 encoding gif…"
ffmpeg -v error -y -ss "$START" -t "$DURATION" -i "$SRC" -i "$PALETTE" \
  -lavfi "${FILTERS} [x]; [x][1:v] paletteuse=dither=bayer:bayer_scale=3:diff_mode=rectangle" \
  "$OUT"

SIZE=$(du -h "$OUT" | cut -f1)
echo "done: $OUT ($SIZE)"
[ "$(du -k "$OUT" | cut -f1)" -gt 10240 ] && cat <<'HINT'

The GIF is over 10 MB — GitHub renders it, but it will feel slow. Trim it:
  ./scripts/make-demo-gif.sh <recording.mov> <start> <shorter-duration>
or lower FPS/WIDTH at the top of this script.
HINT
exit 0
