#!/usr/bin/env bash
# Screenshots for docs/guide from ui/guide/mock.html (made-up data, fake backend), in Korean
# and English: docs/guide/images/<ko|en>/<screen>.png. Needs ui/node_modules and Chrome.
#   scripts/guide-shots.sh [screen ...]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
PORT=5199
SCREENS=("$@"); [ ${#SCREENS[@]} -gt 0 ] || SCREENS=(stack changes locks restack conflict fold artist setup)
cd "$ROOT/ui"
npx vite --port $PORT --strictPort >/dev/null 2>&1 &
VITE=$!
trap 'kill $VITE 2>/dev/null' EXIT
sleep 3
for lang in ko en; do
  mkdir -p "$ROOT/docs/guide/images/$lang"
  for screen in "${SCREENS[@]}"; do
    size=1280,760
    case "$screen" in restack) size=1280,980 ;; esac
    "$CHROME" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 --window-size=$size \
      --virtual-time-budget=9000 --screenshot="$ROOT/docs/guide/images/$lang/$screen.png" \
      "http://localhost:$PORT/guide/mock.html?lang=$lang&screen=$screen" 2>/dev/null
    echo "$lang/$screen.png"
  done
done
