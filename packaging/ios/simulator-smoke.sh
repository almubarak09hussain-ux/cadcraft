#!/usr/bin/env bash
# Smoke-test the iPad app on the simulator: boot an iPad simulator, install CADCraft.app, launch
# it, check it is still running after a while, and keep a screenshot, the app's log and any crash
# report. Run after `cargo xtask ios --bundle --sim` (macOS with Xcode; .github/workflows/ipad.yml).
#
# Usage: packaging/ios/simulator-smoke.sh [app] [out-dir]
#   app      default target/ios/iphonesimulator/CADCraft.app
#   out-dir  default target/ios/smoke (launch.png, app.log, crash reports)
# Env: SMOKE_SECONDS (default 30) how long the app must stay up; SIM_UDID picks the simulator.
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
xcrun simctl launch "$UDID" "$BUNDLE_ID"
sleep "$SECONDS_UP"

xcrun simctl io "$UDID" screenshot "$OUT/launch.png" || true
xcrun simctl spawn "$UDID" log show --last 5m --style compact --predicate 'process == "cadcraft-ios"' >"$OUT/app.log" 2>&1 || true
find "$HOME/Library/Logs/DiagnosticReports" -name 'cadcraft-ios*' -exec cp {} "$OUT/" \; 2>/dev/null || true

# launchctl lists the app with its PID while it runs ("-" once it has exited).
PID="$(xcrun simctl spawn "$UDID" launchctl list | awk -v id="UIKitApplication:$BUNDLE_ID" 'index($3, id) == 1 { print $1; exit }')"
if [ -z "$PID" ] || [ "$PID" = "-" ]; then
  echo "CADCraft is not running ${SECONDS_UP}s after launch; see $OUT/app.log" >&2
  tail -n 50 "$OUT/app.log" >&2 || true
  exit 1
fi
echo "CADCraft running in the simulator (pid $PID) after ${SECONDS_UP}s; screenshot $OUT/launch.png"
