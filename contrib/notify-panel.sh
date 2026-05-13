#!/usr/bin/env bash
# notify-panel.sh — push a notification to the ttlcd panel
#
# MODES:
#
#   1. Full custom text (default):
#      Usage: ./notify-panel.sh "Title" "Subtitle" "Line1" "Line2" "Line3" [progress] [color]
#      Arguments:
#        1  Title      (large heading, max 22 chars)
#        2  Subtitle   (smaller line, max 38 chars; pass "" to omit)
#        3  Line 1     (body text; pass "" to omit)
#        4  Line 2
#        5  Line 3
#        6  Progress   0.0–1.0 (pass "" to omit progress bar)
#        7  Color      blue|green|yellow|red|teal|mauve|peach|white  (default: blue)
#
#   2. Severity alert (--alert flag):
#      Usage: ./notify-panel.sh --alert <level> "Message" ["Detail"] [progress]
#      Levels: error | warning | info | success | critical
#      Example:
#        ./notify-panel.sh --alert error "Disk full" "/dev/sda1 at 100%"
#        ./notify-panel.sh --alert success "Deploy done" "" 1.0
#
#   3. Mode switching:
#      ./notify-panel.sh --mode <mode-name>
#      ./notify-panel.sh --next     (advance to next mode)
#      ./notify-panel.sh --prev     (go to previous mode)
#      ./notify-panel.sh --return   (return from custom text / image / slideshow)
#
#   4. Runtime panel settings:
#      ./notify-panel.sh --quality <1-100>   (change JPEG quality)
#      ./notify-panel.sh --orient  <0|90|180|270>  (rotate output)
#      ./notify-panel.sh --blank              (push blank/dark frame)
#
#   5. Status and diagnostics:
#      ./notify-panel.sh --status             (print daemon status JSON)
#      ./notify-panel.sh --history [N]        (show last N notifications, default 10)
#
# Environment:
#   TTLCD_URL   base URL of the daemon (default: http://localhost:7070)
#
# Examples:
#   # Build success / failure
#   ./notify-panel.sh "Build OK" "ttlcd-rs v0.3" "" "" "" 1.0 green
#   ./notify-panel.sh --alert error "Build FAILED" "check CI logs"
#
#   # Deployment with progress bar
#   ./notify-panel.sh --alert info "Deploying" "Rolling update…" 0.5
#   ./notify-panel.sh --alert success "Deploy complete" "" 1.0
#
#   # Simple one-liner
#   ./notify-panel.sh "Hello World"
#
#   # From a Makefile / CI pipeline:
#   make build \
#     && ./notify-panel.sh --alert success "Build OK" \
#     || ./notify-panel.sh --alert error "Build FAILED" "see make output"
#
#   # Mode navigation
#   ./notify-panel.sh --mode temperature
#   ./notify-panel.sh --next
#   ./notify-panel.sh --return
#
#   # Runtime panel adjustments
#   ./notify-panel.sh --quality 95    # higher quality for photo slides
#   ./notify-panel.sh --orient 90     # rotate display 90 degrees
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

# ── Mode: --next / --prev / --return / --mode <name> ──────────────────────────
if [[ "${1:-}" == "--next" ]]; then
    result=$(curl -sf -X POST "${DAEMON_URL}/api/mode/next")
    echo "Mode → ${result}"
    exit 0
fi

if [[ "${1:-}" == "--prev" ]]; then
    result=$(curl -sf -X POST "${DAEMON_URL}/api/mode/prev")
    echo "Mode → ${result}"
    exit 0
fi

if [[ "${1:-}" == "--return" ]]; then
    curl -sf -X POST "${DAEMON_URL}/api/clear" && echo "Returned to previous mode"
    exit 0
fi

if [[ "${1:-}" == "--mode" ]]; then
    MODE="${2:?'--mode requires a mode name'}"
    curl -sf -X POST "${DAEMON_URL}/api/mode" \
         -H 'Content-Type: application/json' \
         -d "{\"mode\":\"${MODE}\"}" \
      && echo "Mode → ${MODE}"
    exit 0
fi

# ── Countdown: --countdown <seconds> [label] ──────────────────────────────────
if [[ "${1:-}" == "--countdown" ]]; then
    SECS="${2:?'--countdown requires seconds (e.g. 300)'}"
    LABEL="${3:-}"
    payload=$(python3 -c "
import json, sys
obj = {'seconds': int(sys.argv[1])}
if sys.argv[2]:
    obj['label'] = sys.argv[2]
print(json.dumps(obj))
" "$SECS" "$LABEL")
    curl -sf \
         -X POST "${DAEMON_URL}/api/countdown" \
         -H 'Content-Type: application/json' \
         -d "$payload" \
         -o /dev/null \
      && echo "Countdown: ${SECS}s${LABEL:+ \"${LABEL}\"}"
    exit 0
fi

# ── Panel settings: --quality / --orient ──────────────────────────────────────
if [[ "${1:-}" == "--quality" ]]; then
    Q="${2:?'--quality requires a value (1-100)'}"
    result=$(curl -sf -X POST "${DAEMON_URL}/api/quality" \
         -H 'Content-Type: application/json' \
         -d "{\"quality\":${Q}}")
    echo "Quality → ${result}"
    exit 0
fi

if [[ "${1:-}" == "--orient" ]]; then
    R="${2:?'--orient requires 0, 90, 180, or 270'}"
    result=$(curl -sf -X POST "${DAEMON_URL}/api/orient" \
         -H 'Content-Type: application/json' \
         -d "{\"rotate\":${R}}")
    echo "Orientation → ${result}"
    exit 0
fi

# ── Panel blank ──────────────────────────────────────────────────────────────────
if [[ "${1:-}" == "--blank" ]]; then
    curl -sf -X POST "${DAEMON_URL}/api/blank" -o /dev/null && echo "Panel blanked"
    exit 0
fi

# ── Status and history ────────────────────────────────────────────────────────
if [[ "${1:-}" == "--status" ]]; then
    curl -sf "${DAEMON_URL}/api/status"
    echo
    exit 0
fi

if [[ "${1:-}" == "--history" ]]; then
    LIMIT="${2:-10}"
    curl -sf "${DAEMON_URL}/api/history?limit=${LIMIT}" | python3 -c "
import json, sys, datetime
items = json.load(sys.stdin)
for item in items:
    ts = datetime.datetime.fromtimestamp(item['timestamp']).strftime('%H:%M:%S')
    print(f'[{ts}] ({item["kind"]}) {item["summary"]}')
"
    exit 0
fi

# ── /api/notify shorthand: --notify <title> [color] [progress] ───────────────
if [[ "${1:-}" == "--notify" ]]; then
    TITLE="${2:?'--notify requires a title'}"
    COLOR="${3:-blue}"
    PROGRESS="${4:-}"
    payload=$(python3 -c "
import json, sys
obj = {'title': sys.argv[1], 'color': sys.argv[2], 'show_time': True}
if sys.argv[3]:
    obj['progress'] = float(sys.argv[3])
print(json.dumps(obj))
" "$TITLE" "$COLOR" "$PROGRESS")
    curl -sf \
         -X POST "${DAEMON_URL}/api/notify" \
         -H 'Content-Type: application/json' \
         -d "$payload" \
         -o /dev/null \
      && echo "Notify: ${TITLE}"
    exit 0
fi

# ── Webhook test: --webhook <status> <message> ───────────────────────────────
if [[ "${1:-}" == "--webhook" ]]; then
    STATUS="${2:-success}"
    MESSAGE="${3:-Webhook triggered}"
    payload=$(python3 -c "
import json, sys
print(json.dumps({'status': sys.argv[1], 'message': sys.argv[2]}))
" "$STATUS" "$MESSAGE")
    curl -sf \
         -X POST "${DAEMON_URL}/api/webhook" \
         -H 'Content-Type: application/json' \
         -d "$payload" \
         -o /dev/null \
      && echo "Webhook: ${STATUS}: ${MESSAGE}"
    exit 0
fi

# ── Mode: --alert <level> <message> [<detail>] [<progress>] ───────────────────
if [[ "${1:-}" == "--alert" ]]; then
    LEVEL="${2:?'--alert requires a level: error|warning|info|success|critical'}"
    MESSAGE="${3:?'--alert requires a message string'}"
    DETAIL="${4:-}"
    PROGRESS="${5:-}"

    payload=$(python3 -c "
import json, sys
obj = {'level': sys.argv[1], 'message': sys.argv[2]}
if sys.argv[3]:
    obj['detail'] = sys.argv[3]
if sys.argv[4]:
    obj['progress'] = float(sys.argv[4])
print(json.dumps(obj))
" "$LEVEL" "$MESSAGE" "$DETAIL" "$PROGRESS")

    curl -sf \
         -X POST "${DAEMON_URL}/api/alert" \
         -H 'Content-Type: application/json' \
         -d "$payload" \
         -o /dev/null \
      && echo "Alert [${LEVEL}]: ${MESSAGE}"
    exit 0
fi

# ── Schedule: --schedule <seconds> <title> [color] ────────────────────────────
if [[ "${1:-}" == "--schedule" ]]; then
    SECS="${2:?'--schedule requires seconds'}"
    TITLE="${3:?'--schedule requires a title'}"
    COLOR="${4:-blue}"
    payload=$(python3 -c "
import json, sys
obj = {
    'in_secs': int(sys.argv[1]),
    'payload': {'title': sys.argv[2], 'color': sys.argv[3], 'show_time': True},
    'label': sys.argv[2]
}
print(json.dumps(obj))
" "$SECS" "$TITLE" "$COLOR")
    result=$(curl -sf \
         -X POST "${DAEMON_URL}/api/schedule" \
         -H 'Content-Type: application/json' \
         -d "$payload")
    echo "${result}"
    exit 0
fi

# ── Default mode: full custom text ────────────────────────────────────────────
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
     -o /dev/null \
  && echo "Panel updated: ${TITLE:-<no title>}"
