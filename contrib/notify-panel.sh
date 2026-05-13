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
#
#   3. Mode switching:
#      ./notify-panel.sh --mode <mode-name>
#      ./notify-panel.sh --next     (advance to next mode)
#      ./notify-panel.sh --prev     (go to previous mode)
#      ./notify-panel.sh --return   (return from custom text / image / slideshow)
#
#   4. Runtime panel settings:
#      ./notify-panel.sh --quality <1-100>
#      ./notify-panel.sh --orient  <0|90|180|270>
#      ./notify-panel.sh --brightness <0-100>
#      ./notify-panel.sh --blank
#      ./notify-panel.sh --cancel             (cancel running countdown)
#
#   5. Stopwatch:
#      ./notify-panel.sh --sw-start           (start or resume stopwatch)
#      ./notify-panel.sh --sw-stop            (pause stopwatch)
#      ./notify-panel.sh --sw-lap             (record a lap)
#      ./notify-panel.sh --sw-reset           (reset stopwatch)
#      ./notify-panel.sh --sw-status          (print current stopwatch state)
#
#   6. Display freeze:
#      ./notify-panel.sh --freeze             (freeze display)
#      ./notify-panel.sh --unfreeze           (resume display)
#
#   7. Custom metrics:
#      ./notify-panel.sh --metrics "Title" "Label1:Value1" "Label2:Value2" ...
#      Example: ./notify-panel.sh --metrics "CI" "Build:OK" "Tests:1234" "Cover:92%"
#
#   8. Display profiles:
#      ./notify-panel.sh --profile-save <name>    (save current state)
#      ./notify-panel.sh --profile-apply <name>   (restore saved state)
#      ./notify-panel.sh --profile-list           (list saved profiles)
#
#   9. Big text banner:
#      ./notify-panel.sh --banner <line1> [line2] [line3] [color]
#
#  10. Weather and notes:
#      ./notify-panel.sh --weather <city>         (fetch wttr.in weather & push)
#      ./notify-panel.sh --notes "Title" "Note 1" "Note 2" ...
#      ./notify-panel.sh --notes-add "Append one note"
#
#  11. Settings backup:
#      ./notify-panel.sh --export [outfile.json]  (export all settings)
#      ./notify-panel.sh --import <file.json>     (restore settings)
#
#  12. Status and diagnostics:
#      ./notify-panel.sh --status             (print daemon status JSON)
#      ./notify-panel.sh --snapshot           (print full system snapshot)
#      ./notify-panel.sh --frozen             (check if display is frozen)
#      ./notify-panel.sh --history [N]        (show last N notifications)
#      ./notify-panel.sh --version
#      ./notify-panel.sh --uptime
#      ./notify-panel.sh --panel-info
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

# ── Brightness: --brightness <0-100> ─────────────────────────────────────────
if [[ "${1:-}" == "--brightness" ]]; then
    B="${2:?'--brightness requires a value (0-100)'}"
    result=$(curl -sf -X POST "${DAEMON_URL}/api/brightness" \
         -H 'Content-Type: application/json' \
         -d "{"brightness":${B}}")
    echo "Brightness → ${result}"
    exit 0
fi

# ── Cancel countdown: --cancel ────────────────────────────────────────────────
if [[ "${1:-}" == "--cancel" ]]; then
    result=$(curl -sf -X DELETE "${DAEMON_URL}/api/countdown")
    echo "${result}"
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

# ── Test pattern: --test [seconds] ───────────────────────────────────────────
if [[ "${1:-}" == "--test" ]]; then
    SECS="${2:-5}"
    curl -sf \
         -X POST "${DAEMON_URL}/api/test" \
         -H 'Content-Type: application/json' \
         -d "{"duration_secs":${SECS}}" \
         -o /dev/null \
      && echo "Test pattern: ${SECS}s"
    exit 0
fi

# ── Slideshow shuffle: --shuffle ──────────────────────────────────────────────
if [[ "${1:-}" == "--shuffle" ]]; then
    result=$(curl -sf -X POST "${DAEMON_URL}/api/slideshow/shuffle")
    echo "${result}"
    exit 0
fi

# ── Slideshow image list: --images ────────────────────────────────────────────
if [[ "${1:-}" == "--images" ]]; then
    curl -sf "${DAEMON_URL}/api/slideshow/images" | python3 -c "
import json, sys
d = json.load(sys.stdin)
print(f'Slideshow: {d["count"]} images, current idx={d["current"]}')
for i, name in enumerate(d['images']):
    marker = ' ◀' if i == d['current'] else ''
    print(f'  [{i}] {name}{marker}')
"
    exit 0
fi

# ── Big text banner: --banner <line1> [line2] [line3] [color] ───────────────
if [[ "${1:-}" == "--banner" ]]; then
    L1="${2:?'--banner requires at least one line'}"
    L2="${3:-}"
    L3="${4:-}"
    COLOR="${5:-green}"
    payload=$(python3 -c "
import json, sys
lines = [l for l in [sys.argv[1], sys.argv[2], sys.argv[3]] if l]
print(json.dumps({'lines': lines, 'color': sys.argv[4], 'show_time': True}))
" "$L1" "$L2" "$L3" "$COLOR")
    curl -sf \
         -X POST "${DAEMON_URL}/api/banner" \
         -H 'Content-Type: application/json' \
         -d "$payload" \
         -o /dev/null \
      && echo "Banner: ${L1}"
    exit 0
fi

# ── Version and uptime: --version / --uptime ─────────────────────────────────
if [[ "${1:-}" == "--version" ]]; then
    curl -sf "${DAEMON_URL}/api/version"
    echo
    exit 0
fi

if [[ "${1:-}" == "--uptime" ]]; then
    curl -sf "${DAEMON_URL}/api/uptime"
    echo
    exit 0
fi

# ── Panel info: --panel-info ──────────────────────────────────────────────────
if [[ "${1:-}" == "--panel-info" ]]; then
    curl -sf "${DAEMON_URL}/api/panel/info"
    echo
    exit 0
fi

# ── Stopwatch: --sw-start / --sw-stop / --sw-lap / --sw-reset / --sw-status ──
if [[ "${1:-}" == "--sw-start" ]]; then
    curl -sf -X POST "${DAEMON_URL}/api/stopwatch/start" && echo "Stopwatch started"
    exit 0
fi
if [[ "${1:-}" == "--sw-stop" ]]; then
    curl -sf -X POST "${DAEMON_URL}/api/stopwatch/stop" && echo "Stopwatch paused"
    exit 0
fi
if [[ "${1:-}" == "--sw-lap" ]]; then
    result=$(curl -sf -X POST "${DAEMON_URL}/api/stopwatch/lap")
    echo "${result}"
    exit 0
fi
if [[ "${1:-}" == "--sw-reset" ]]; then
    curl -sf -X POST "${DAEMON_URL}/api/stopwatch/reset" && echo "Stopwatch reset"
    exit 0
fi
if [[ "${1:-}" == "--sw-status" ]]; then
    curl -sf "${DAEMON_URL}/api/stopwatch"
    echo
    exit 0
fi

# ── Display freeze: --freeze / --unfreeze ────────────────────────────────────
if [[ "${1:-}" == "--freeze" ]]; then
    curl -sf -X POST "${DAEMON_URL}/api/display/freeze" && echo "Display frozen"
    exit 0
fi
if [[ "${1:-}" == "--unfreeze" ]]; then
    curl -sf -X POST "${DAEMON_URL}/api/display/unfreeze" && echo "Display unfrozen"
    exit 0
fi

# ── Custom metrics: --metrics <title> <key:val> ... ──────────────────────────
if [[ "${1:-}" == "--metrics" ]]; then
    TITLE="${2:?'--metrics requires a title as second arg'}"
    shift 2
    payload=$(python3 -c "
import json, sys
title = sys.argv[1]
pairs = sys.argv[2:]
metrics = []
for p in pairs:
    if ':' in p:
        label, _, value = p.partition(':')
        metrics.append({'label': label.strip(), 'value': value.strip(), 'color': 'blue'})
print(json.dumps({'title': title, 'metrics': metrics, 'show': True}))
" "$TITLE" "$@")
    curl -sf -X POST "${DAEMON_URL}/api/metrics"          -H 'Content-Type: application/json'          -d "$payload"          -o /dev/null       && echo "Metrics pushed: ${TITLE}"
    exit 0
fi

# ── Display profiles: --profile-save / --profile-apply / --profile-list ───────
if [[ "${1:-}" == "--profile-save" ]]; then
    NAME="${2:?'--profile-save requires a name'}"
    curl -sf -X POST "${DAEMON_URL}/api/profiles"          -H 'Content-Type: application/json'          -d "{\"name\":\"${NAME}\"}"          -o /dev/null       && echo "Profile '${NAME}' saved"
    exit 0
fi
if [[ "${1:-}" == "--profile-apply" ]]; then
    NAME="${2:?'--profile-apply requires a name'}"
    curl -sf -X POST "${DAEMON_URL}/api/profiles/${NAME}/apply"          -o /dev/null       && echo "Profile '${NAME}' applied"
    exit 0
fi
if [[ "${1:-}" == "--profile-list" ]]; then
    curl -sf "${DAEMON_URL}/api/profiles" | python3 -c "
import json, sys
profiles = json.load(sys.stdin)
if not profiles:
    print('No saved profiles.')
for p in profiles:
    print(f'  {p["name"]:20s} mode={p["mode"]:16s} bright={p["brightness"]*100:.0f}%')
"
    exit 0
fi

# ── Weather: --weather <city> ─────────────────────────────────────────────────
if [[ "${1:-}" == "--weather" ]]; then
    CITY="${2:-London}"
    # Fetch from wttr.in and push to daemon
    json=$(curl -sf "https://wttr.in/${CITY}?format=j1" 2>/dev/null) || {
        echo "Error: could not reach wttr.in" >&2; exit 1; }
    payload=$(python3 -c "
import json, sys, time
d = json.loads(sys.stdin.read())
cur   = d['current_condition'][0]
cast  = d['weather'][0] if d['weather'] else {}
obj = {
  'city':          sys.argv[1],
  'temp_c':        float(cur['temp_C']),
  'feels_like_c':  float(cur['FeelsLikeC']),
  'condition':     cur['weatherDesc'][0]['value'] if cur.get('weatherDesc') else '',
  'humidity_pct':  int(cur['humidity']),
  'wind_kmh':      float(cur['windspeedKmph']),
  'wind_dir':      cur.get('winddir16Point', ''),
  'visibility_km': float(cur.get('visibility', 0)),
  'high_c':        float(cast['maxtempC']) if 'maxtempC' in cast else None,
  'low_c':         float(cast['mintempC']) if 'mintempC' in cast else None,
  'show':          True,
}
print(json.dumps(obj))
" "$CITY" <<< "$json")
    curl -sf -X POST "${DAEMON_URL}/api/weather"          -H 'Content-Type: application/json'          -d "$payload" -o /dev/null
    echo "Weather pushed for ${CITY}"
    exit 0
fi

# ── Notes: --notes <title> <item1> [item2] ... ────────────────────────────────
if [[ "${1:-}" == "--notes" ]]; then
    TITLE="${2:?'--notes requires a title'}"
    shift 2
    payload=$(python3 -c "
import json, sys
print(json.dumps({'title': sys.argv[1], 'items': list(sys.argv[2:]), 'append': False, 'show': True}))
" "$TITLE" "$@")
    curl -sf -X POST "${DAEMON_URL}/api/notes"          -H 'Content-Type: application/json'          -d "$payload" -o /dev/null       && echo "Notes pushed: ${TITLE}"
    exit 0
fi

# ── Notes append: --notes-add <item> ─────────────────────────────────────────
if [[ "${1:-}" == "--notes-add" ]]; then
    ITEM="${2:?'--notes-add requires an item string'}"
    curl -sf -X POST "${DAEMON_URL}/api/notes"          -H 'Content-Type: application/json'          -d "$(python3 -c "import json, sys; print(json.dumps({'items':[sys.argv[1]],'append':True,'show':True}))" "$ITEM")"          -o /dev/null       && echo "Note added: ${ITEM}"
    exit 0
fi

# ── Settings export: --export [outfile] ──────────────────────────────────────
if [[ "${1:-}" == "--export" ]]; then
    OUTFILE="${2:-ttlcd-settings.json}"
    curl -sf "${DAEMON_URL}/api/settings/export" -o "$OUTFILE"
    echo "Settings exported to ${OUTFILE}"
    exit 0
fi

# ── Settings import: --import <file> ─────────────────────────────────────────
if [[ "${1:-}" == "--import" ]]; then
    FILE="${2:?'--import requires a filename'}"
    result=$(curl -sf -X POST "${DAEMON_URL}/api/settings/import"              -H 'Content-Type: application/json'              -d "@${FILE}")
    echo "Imported: ${result}"
    exit 0
fi

# ── Freeze status: --frozen ───────────────────────────────────────────────────
if [[ "${1:-}" == "--frozen" ]]; then
    curl -sf "${DAEMON_URL}/api/display/frozen"
    echo
    exit 0
fi

# ── Snapshot: --snapshot ──────────────────────────────────────────────────────
if [[ "${1:-}" == "--snapshot" ]]; then
    curl -sf "${DAEMON_URL}/api/sys/snapshot" | python3 -c "
import json, sys
d = json.load(sys.stdin)
cpu = d['cpu']
mem = d['memory']
print(f"Host: {d['hostname']}  OS: {d['os']}  Kernel: {d['kernel']}")
print(f"CPU:  {cpu['pct']:.1f}%  Load: {cpu['load_1']:.2f}/{cpu['load_5']:.2f}/{cpu['load_15']:.2f}")
used_gb = mem['used'] / 1073741824
total_gb = mem['total'] / 1073741824
print(f"RAM:  {used_gb:.1f}/{total_gb:.1f} GB")
print(f"Uptime: {d['uptime_secs']//3600}h {(d['uptime_secs']%3600)//60}m")
print(f"Daemon: mode={d['daemon']['mode']}  fps={d['daemon']['fps']:.1f}  frame={d['daemon']['frame_seq']}")
"
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
