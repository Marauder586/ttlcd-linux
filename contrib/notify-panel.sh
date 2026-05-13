#!/usr/bin/env bash
# notify-panel.sh — push a notification to the ttlcd panel via POST /api/text
#
# Usage:
#   ./notify-panel.sh "Title" "Subtitle" "Line1" "Line2" [progress 0.0-1.0] [color]
#
# Examples:
#   # Show a build success:
#   ./notify-panel.sh "Build OK" "ttlcd-rs v0.2" "$(date '+%H:%M:%S')" "" 1.0 green
#
#   # Show a build failure:
#   ./notify-panel.sh "Build FAILED" "ttlcd-rs" "See CI logs" "" 0 red
#
#   # From a Makefile or CI script:
#   make build && ./notify-panel.sh "Build OK" "" "" "" 1.0 green \
#             || ./notify-panel.sh "Build FAILED" "check logs" "" "" 0 red

set -euo pipefail

DAEMON_URL="${TTLCD_URL:-http://localhost:7070}"

TITLE="${1:-}"
SUBTITLE="${2:-}"
LINE1="${3:-}"
LINE2="${4:-}"
LINE3="${5:-}"
PROGRESS="${6:-}"
COLOR="${7:-blue}"

# Build the JSON payload
payload=$(python3 -c "
import json, sys

title    = sys.argv[1]
subtitle = sys.argv[2]
line1    = sys.argv[3]
line2    = sys.argv[4]
line3    = sys.argv[5]
progress = sys.argv[6]
color    = sys.argv[7]

obj = {'color': color, 'show_time': True}
if title:    obj['title']    = title
if subtitle: obj['subtitle'] = subtitle

lines = [l for l in [line1, line2, line3] if l]
if lines: obj['lines'] = lines

if progress != '':
    obj['progress'] = float(progress)

print(json.dumps(obj))
" "$TITLE" "$SUBTITLE" "$LINE1" "$LINE2" "$LINE3" "$PROGRESS" "$COLOR")

curl -sf \
     -X POST "${DAEMON_URL}/api/text" \
     -H 'Content-Type: application/json' \
     -d "$payload" \
  && echo "Panel updated"
