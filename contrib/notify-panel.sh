#!/usr/bin/env bash
# notify-panel.sh — push a notification to the ttlcd panel via POST /api/text
#
# Usage:
#   ./notify-panel.sh "Title" "Subtitle" "Line1" "Line2" "Line3" [progress] [color]
#
# Arguments:
#   1  Title      (large heading, max 22 chars)
#   2  Subtitle   (smaller line, max 38 chars; pass "" to omit)
#   3  Line 1     (body text; pass "" to omit)
#   4  Line 2
#   5  Line 3
#   6  Progress   0.0–1.0 (pass "" to omit progress bar)
#   7  Color      blue|green|yellow|red|teal|mauve|peach|white  (default: blue)
#
# Environment:
#   TTLCD_URL   base URL of the daemon (default: http://localhost:7070)
#
# Examples:
#   # Build success
#   ./notify-panel.sh "Build OK" "ttlcd-rs v0.3" "" "" "" 1.0 green
#
#   # Build failure
#   ./notify-panel.sh "Build FAILED" "check CI logs" "" "" "" 0.0 red
#
#   # Deployment started
#   ./notify-panel.sh "Deploying" "production" "Rolling update…" "" "" 0.3 teal
#
#   # Simple one-liner (most args optional)
#   ./notify-panel.sh "Hello World"
#
#   # From a Makefile / CI pipeline:
#   make build && ./notify-panel.sh "Build OK"  "" "" "" "" 1.0 green \
#             || ./notify-panel.sh "Build FAIL" "" "" "" "" 0.0 red
#
# Preset management (requires curl + jq):
#   # Save current payload as a preset
#   curl -sf -X POST $TTLCD_URL/api/presets \
#     -H 'Content-Type: application/json' \
#     -d '{"name":"build-ok","payload":{"title":"Build OK","color":"green","progress":1.0,"show_time":true}}'
#
#   # Apply a saved preset
#   curl -sf -X POST $TTLCD_URL/api/presets/build-ok
#
# Auto-rotation:
#   # Start rotating through modes every 30 seconds
#   curl -sf -X POST $TTLCD_URL/api/rotation \
#     -H 'Content-Type: application/json' \
#     -d '{"modes":["clock","sysinfo","cpu_detail","network"],"interval":30}'
#
#   # Stop rotation
#   curl -sf -X POST $TTLCD_URL/api/rotation \
#     -H 'Content-Type: application/json' \
#     -d '{"modes":[],"interval":0}'

set -euo pipefail

DAEMON_URL="${TTLCD_URL:-http://localhost:7070}"

TITLE="${1:-}"
SUBTITLE="${2:-}"
LINE1="${3:-}"
LINE2="${4:-}"
LINE3="${5:-}"
PROGRESS="${6:-}"
COLOR="${7:-blue}"

# Build the JSON payload using Python (no external deps beyond standard library)
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
  && echo "Panel updated: ${TITLE:-<no title>}"
