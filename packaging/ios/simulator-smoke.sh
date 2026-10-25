#!/usr/bin/env bash
# Smoke-test the iPad app on the simulator: boot an iPad simulator, install CADCraft.app, launch
# it, check it is still running after a while, and keep a screenshot, the app's log and any crash
# report. Run after `cargo xtask ios --bundle --sim` (macOS with Xcode; .github/workflows/ipad.yml).
#
# Usage: packaging/ios/simulator-smoke.sh [app] [out-dir]
#   app      default target/ios/iphonesimulator/CADCraft.app
#   out-dir  default target/ios/smoke (launch.png, console.log, app.log, crash reports)
# Env: SMOKE_SECONDS (default 30) how long the app must stay up; SIM_UDID picks the simulator;
#      SMOKE_PRINT_SCREENSHOT=1 also prints a small JPEG of the screenshot to the log, base64
#      between BEGIN/END SCREENSHOT lines (CI, where artifacts may not be reachable).
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"

APP="${1:-${CARGO_TARGET_DIR:-$ROOT/target}/ios/iphonesimulator/CADCraft.app}"
OUT="${2:-${CARGO_TARGET_DIR:-$ROOT/target}/ios/smoke}"
BUNDLE_ID=ai.storyteller.cadcraft
SECONDS_UP="${SMOKE_SECONDS:-30}"

[ -d "$APP" ] || { echo "no app at $APP: run cargo xtask ios --bundle --sim first" >&2; exit 1; }
mkdir -p "$OUT"

# The newest available iPad simulator, an iPad Pro if there is one.
UDID="${SIM_UDID:-$(xcrun simctl list devices available --json | python3 -c '
import json, re, sys
devices = json.load(sys.stdin)["devices"]
def version(runtime):
    m = re.search(r"iOS-([0-9-]+)$", runtime)
    return [int(x) for x in m.group(1).split("-")] if m else []
found = [(version(rt), "iPad Pro" in d["name"], d["udid"], d["name"], rt)
         for rt, ds in devices.items() if ".iOS-" in rt
         for d in ds if d.get("isAvailable") and d["name"].startswith("iPad")]
found.sort()
if found:
    print(found[-1][2])
    print("simulator:", found[-1][3], found[-1][4], file=sys.stderr)
')}"
[ -n "$UDID" ] || { echo "no iPad simulator available (xcrun simctl list devices)" >&2; exit 1; }

xcrun simctl boot "$UDID" 2>/dev/null || true # already booted is fine
xcrun simctl bootstatus "$UDID" -b
xcrun simctl install "$UDID" "$APP"
# With --console-pty the launch stays attached and streams the app's standard output and error
# (its log) into console.log; its first line is "ai.storyteller.cadcraft: <pid>". Simulator apps
# are host processes, so the PID can be checked from here.
xcrun simctl launch --console-pty "$UDID" "$BUNDLE_ID" >"$OUT/console.log" 2>&1 &
CONSOLE=$!
sleep "$SECONDS_UP"
PID="$(sed -n "s/^$BUNDLE_ID: \([0-9][0-9]*\).*/\1/p" "$OUT/console.log" | head -n 1)"
echo "launched: pid ${PID:-unknown}"

xcrun simctl io "$UDID" screenshot "$OUT/launch.png" || true
xcrun simctl spawn "$UDID" log show --last 5m --style compact --predicate 'process == "cadcraft-ios"' >"$OUT/app.log" 2>&1 || true
find "$HOME/Library/Logs/DiagnosticReports" -name 'cadcraft-ios*' -exec cp {} "$OUT/" \; 2>/dev/null || true

if [ "${SMOKE_PRINT_SCREENSHOT:-0}" = 1 ] && [ -f "$OUT/launch.png" ]; then
  sips -s format jpeg -s formatOptions 55 -Z 1100 "$OUT/launch.png" --out "$OUT/launch-small.jpg" >/dev/null
  echo "BEGIN SCREENSHOT launch-small.jpg"
  base64 -i "$OUT/launch-small.jpg" | tr -d '\n'
  echo
  echo "END SCREENSHOT"
fi

ALIVE=0
case "$PID" in
  '' | *[!0-9]*) ;; # launch printed no PID
  *) if kill -0 "$PID" 2>/dev/null; then ALIVE=1; fi ;;
esac
kill "$CONSOLE" 2>/dev/null || true
echo "--- console (the app's own log) ---"
tail -n 80 "$OUT/console.log" || true
if [ "$ALIVE" != 1 ]; then
  echo "CADCraft is not running ${SECONDS_UP}s after launch; see $OUT/console.log and $OUT/app.log" >&2
  exit 1
fi
echo "CADCraft running in the simulator (pid $PID) after ${SECONDS_UP}s; screenshot $OUT/launch.png"
