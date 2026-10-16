#!/usr/bin/env bash
# Start the iPad app in a fresh iPad simulator with the sample drawing and keep what happened in
# dist/ios/simulator: screenshots, the app's output and any crash report. Build the app first
# (packaging/ios/build.sh --simulator). Exits non-zero when the app is not running at the end.
#
# Usage: packaging/ios/run-simulator.sh
#
# Env: CADCRAFT_SIM_WAIT   seconds to let the app run before the last screenshot (default 30)
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
APP="$ROOT/dist/ios/CADCraft-simulator.app"
OUT="$ROOT/dist/ios/simulator"
WAIT="${CADCRAFT_SIM_WAIT:-30}"

if [ ! -d "$APP" ]; then
  echo "error: $APP is missing (packaging/ios/build.sh --simulator)" >&2
  exit 1
fi
rm -rf "$OUT"
mkdir -p "$OUT"
BUNDLE_ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Info.plist")"

# The newest iOS runtime, and the newest 13-inch iPad Pro it has (else any iPad Pro, else any iPad).
RUNTIME="$(xcrun simctl list runtimes -j | python3 -c '
import json, sys
rts = [r for r in json.load(sys.stdin)["runtimes"]
       if r.get("isAvailable") and (r.get("platform") == "iOS" or r.get("name", "").startswith("iOS"))]
rts.sort(key=lambda r: [int(p) for p in r["version"].split(".") if p.isdigit()])
print(rts[-1]["identifier"] if rts else "")
')"
if [ -z "$RUNTIME" ]; then
  echo "error: no iOS simulator runtime is installed" >&2
  exit 1
fi
DEVICE_TYPE="$(xcrun simctl list runtimes -j | python3 -c '
import json, sys
runtime = sys.argv[1]
rt = next(r for r in json.load(sys.stdin)["runtimes"] if r["identifier"] == runtime)
ipads = [d for d in rt.get("supportedDeviceTypes", []) if d.get("productFamily") == "iPad" or "iPad" in d["name"]]
for want in ("iPad Pro 13-inch", "iPad Pro (12.9-inch)", "iPad Pro", "iPad"):
    match = [d for d in ipads if want in d["name"]]
    if match:
        print(match[-1]["identifier"])
        break
' "$RUNTIME")"
if [ -z "$DEVICE_TYPE" ]; then
  echo "error: runtime $RUNTIME has no iPad simulator" >&2
  exit 1
fi
echo "Simulator: $DEVICE_TYPE on $RUNTIME"

UDID="$(xcrun simctl create "CADCraft iPad" "$DEVICE_TYPE" "$RUNTIME")"
cleanup() {
  xcrun simctl shutdown "$UDID" >/dev/null 2>&1 || true
  xcrun simctl delete "$UDID" >/dev/null 2>&1 || true
}
trap cleanup EXIT
xcrun simctl boot "$UDID"
xcrun simctl bootstatus "$UDID" -b >/dev/null
xcrun simctl install "$UDID" "$APP"

STARTED="$(date +%s)"
LAUNCH="$(xcrun simctl launch --stdout="$OUT/stdout.txt" --stderr="$OUT/stderr.txt" "$UDID" "$BUNDLE_ID" --sample)"
echo "$LAUNCH"
PID="${LAUNCH##*: }"

sleep 10
xcrun simctl io "$UDID" screenshot "$OUT/1-after-10s.png" >/dev/null
sleep "$WAIT"
xcrun simctl io "$UDID" screenshot "$OUT/2-after-$((WAIT + 10))s.png" >/dev/null

# Simulator apps are processes on this Mac.
RUNNING=0
if kill -0 "$PID" 2>/dev/null; then RUNNING=1; fi

# Crash reports written since the launch.
for f in "$HOME"/Library/Logs/DiagnosticReports/CADCraft*; do
  if [ -f "$f" ] && [ "$(stat -f %m "$f")" -ge "$STARTED" ]; then cp "$f" "$OUT/"; fi
done

echo "--- app output (stderr) ---"
tail -n 40 "$OUT/stderr.txt" 2>/dev/null || true
if [ "$RUNNING" = 1 ]; then
  echo "CADCraft is running after $((WAIT + 10)) s; screenshots in $OUT"
else
  if [ -n "${GITHUB_ACTIONS:-}" ]; then echo "::error::CADCraft quit or crashed in the simulator; see the ipad-simulator artifact"; fi
  echo "error: CADCraft is not running after $((WAIT + 10)) s" >&2
  exit 1
fi
